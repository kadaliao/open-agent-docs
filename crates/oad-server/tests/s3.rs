//! S3 兼容的文件存储：对着本机一个假的 S3（路径式地址，核对签名头和内容哈希）存、取、取一段、看大小、删；
//! 整个服务用它存上传的文件和缩略图。有 MinIO 时设 OAD_TEST_S3_URL=s3://bucket?endpoint=http://..（加 AWS_* 密钥）再测真的。
mod common;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use common::*;
use oad_core::s3::{S3Blobs, S3Config};
use oad_core::{AuthConfig, BlobStore, MediaConfig, NotifyConfig};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type Store = Arc<Mutex<HashMap<String, Vec<u8>>>>;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// 假的 S3：只认 bucket “oad”，要有 SigV4 的 Authorization、x-amz-date、和正文对得上的 x-amz-content-sha256。
async fn object(
    State(st): State<Store>,
    Path((bucket, key)): Path<(String, String)>,
    method: axum::http::Method,
    h: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = h
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !auth.starts_with("AWS4-HMAC-SHA256 Credential=test-key/")
        || !auth.contains("SignedHeaders=host;")
        || !auth.contains("x-amz-content-sha256;x-amz-date")
        || !auth.contains("Signature=")
        || h.get("x-amz-date").is_none()
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if h.get("x-amz-content-sha256").and_then(|v| v.to_str().ok())
        != Some(&hex(&Sha256::digest(&body)))
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if bucket != "oad" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut m = st.lock().unwrap();
    match method.as_str() {
        "PUT" => {
            m.insert(key, body.to_vec());
            StatusCode::OK.into_response()
        }
        "DELETE" => {
            m.remove(&key);
            StatusCode::NO_CONTENT.into_response()
        }
        "HEAD" => match m.get(&key) {
            Some(d) => ([("content-length", d.len().to_string())], StatusCode::OK).into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        },
        _ => match m.get(&key) {
            None => StatusCode::NOT_FOUND.into_response(),
            Some(d) => match h
                .get("range")
                .and_then(|v| v.to_str().ok())
                .and_then(|r| r.strip_prefix("bytes="))
            {
                Some(r) => {
                    let (a, b) = r.split_once('-').unwrap();
                    let a: usize = a.parse().unwrap();
                    if a >= d.len() {
                        return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
                    }
                    let b: usize = b.parse::<usize>().unwrap().min(d.len() - 1);
                    (StatusCode::PARTIAL_CONTENT, d[a..=b].to_vec()).into_response()
                }
                None => d.clone().into_response(),
            },
        },
    }
}

async fn fake_s3() -> (String, Store) {
    let st: Store = Arc::default();
    let app = Router::new()
        .route("/{bucket}/{*key}", axum::routing::any(object))
        .with_state(st.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (format!("http://{addr}"), st)
}

fn blobs(endpoint: &str, bucket: &str) -> Arc<S3Blobs> {
    Arc::new(
        S3Blobs::new(S3Config {
            bucket: bucket.into(),
            prefix: "tenant-a/blobs".into(),
            region: "us-east-1".into(),
            endpoint: Some(endpoint.into()),
            access_key: "test-key".into(),
            secret_key: "test-secret".into(),
            session_token: None,
        })
        .unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blob_store_ops() {
    let (ep, store) = fake_s3().await;
    let b = blobs(&ep, "oad");
    assert_eq!(b.get("abcdef").await.unwrap(), None);
    assert_eq!(b.len("abcdef").await.unwrap(), None);
    b.put("abcdef", b"hello world".to_vec()).await.unwrap();
    b.put("abcdef", b"ignored".to_vec()).await.unwrap();
    assert_eq!(b.get("abcdef").await.unwrap().unwrap(), b"hello world");
    assert_eq!(b.len("abcdef").await.unwrap(), Some(11));
    assert_eq!(b.range("abcdef", 6, 11).await.unwrap().unwrap(), b"world");
    assert_eq!(b.range("abcdef", 6, 100).await.unwrap().unwrap(), b"world");
    assert_eq!(b.range("abcdef", 50, 60).await.unwrap().unwrap(), b"");
    assert!(
        store.lock().unwrap().contains_key("tenant-a/blobs/abcdef"),
        "前缀"
    );
    b.delete("abcdef").await.unwrap();
    b.delete("abcdef").await.unwrap();
    assert_eq!(b.get("abcdef").await.unwrap(), None);
    assert!(b.get("../etc").await.is_err(), "键不对");
    // bucket 不存在：存的时候报错
    assert!(
        blobs(&ep, "nope")
            .put("abcdef", b"x".to_vec())
            .await
            .is_err()
    );
}

/// 整个服务用 S3 存上传的文件和缩略图。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn server_uploads_to_s3() {
    let (ep, store) = fake_s3().await;
    let dir = tempfile::tempdir().unwrap();
    let media = MediaConfig {
        blobs: blobs(&ep, "oad"),
        ..Default::default()
    };
    let s = Server::start_media(
        dir.path(),
        AuthConfig::None,
        fast_hub(),
        Default::default(),
        NotifyConfig::default(),
        media,
    )
    .await;
    let c = http();
    let img = {
        let i = image::RgbImage::from_pixel(1200, 800, image::Rgb([1, 2, 3]));
        let mut out = std::io::Cursor::new(Vec::new());
        i.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    };
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(img.clone()).file_name("图.png"),
    );
    let (st, up) = json(
        c.post(s.api(&format!("/workspaces/{}/files", s.ws_id())))
            .multipart(form),
    )
    .await;
    assert_eq!(st, 201, "{up}");
    let url = up["url"]
        .as_str()
        .unwrap()
        .trim_start_matches("/api/v1")
        .to_string();
    let got = c
        .get(s.api(&url))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(got.to_vec(), img);
    let r = c
        .get(format!("{}?w=320", s.api(&url)))
        .send()
        .await
        .unwrap();
    assert_eq!(r.headers()["content-type"], "image/webp");
    let n = store
        .lock()
        .unwrap()
        .keys()
        .filter(|k| k.contains(".w"))
        .count();
    assert!(n >= 1, "缩略图也存进 S3");
    s.stop().await;
}

/// 有真的 S3 / MinIO 时（OAD_TEST_S3_URL）再测一遍基本操作。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_s3_if_configured() {
    let Ok(u) = std::env::var("OAD_TEST_S3_URL") else {
        return;
    };
    let b = S3Blobs::new(S3Config::from_url(&u).unwrap()).unwrap();
    let key = format!("oadtest{}", oad_core::ids::new_id().to_lowercase());
    b.put(&key, b"real".to_vec()).await.unwrap();
    assert_eq!(b.get(&key).await.unwrap().unwrap(), b"real");
    assert_eq!(b.range(&key, 1, 3).await.unwrap().unwrap(), b"ea");
    b.delete(&key).await.unwrap();
    assert_eq!(b.len(&key).await.unwrap(), None);
}
