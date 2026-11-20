//! 客户端的行为（对着本机的假服务器）：重试（S14）、错误、翻页、with_user、请求头、请求体。
mod common;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use common::{Counter, client, me_json, serve};
use futures_util::TryStreamExt;
use oad::Nullable;
use oad::types::PatchPage;
use open_agent_docs as oad;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn rate_limited() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [("retry-after", "1")],
        axum::Json(json!({ "error": { "code": "too_many_requests", "message": "太频繁了" } })),
    )
        .into_response()
}

/// S14：先 429 + Retry-After: 1 再 200 → 成功，等了 ≥ 1 秒；max_retries=0 → RateLimit 错误，retry_after == 1
#[tokio::test]
async fn s14_retry_after() {
    let n = Counter::default();
    let app = Router::new()
        .route(
            "/api/v1/me",
            get(|State(n): State<Counter>| async move {
                if n.bump() % 2 == 0 {
                    rate_limited()
                } else {
                    axum::Json(me_json("王小明")).into_response()
                }
            }),
        )
        .with_state(n.clone());
    let base = serve(app).await;

    let t = Instant::now();
    let me = client(&base).me().get().await.unwrap();
    assert_eq!(me.user.name, "王小明");
    assert!(t.elapsed() >= Duration::from_secs(1), "{:?}", t.elapsed());
    assert_eq!(n.get(), 2);

    let c = oad::Client::builder()
        .base_url(&base)
        .max_retries(0)
        .build()
        .unwrap();
    let e = c.me().get().await.unwrap_err();
    assert!(e.is_rate_limited(), "{e:?}");
    let api = e.api().unwrap();
    assert_eq!(api.retry_after, Some(Duration::from_secs(1)));
    assert_eq!(api.code, "too_many_requests");
    assert_eq!(api.message, "太频繁了");
    assert_eq!(n.get(), 3);
}

/// GET 遇到 503 重试；POST 遇到 503 不重试（可能已经生效了）
#[tokio::test]
async fn retry_5xx_only_idempotent() {
    let gets = Counter::default();
    let posts = Counter::default();
    let app = Router::new()
        .route(
            "/api/v1/pages/{id}",
            get({
                let gets = gets.clone();
                move || async move {
                    if gets.bump() < 2 {
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else {
                        axum::Json(json!({ "id": "p1", "workspace_id": "w1", "position": "a", "title": "T", "created_at": 1, "updated_at": 2 })).into_response()
                    }
                }
            }),
        )
        .route(
            "/api/v1/workspaces/{ws}/pages",
            post({
                let posts = posts.clone();
                move || async move {
                    posts.bump();
                    StatusCode::SERVICE_UNAVAILABLE
                }
            }),
        );
    let base = serve(app).await;
    let c = client(&base);
    let p = c.pages().get("p1").await.unwrap();
    assert_eq!(p.title, "T");
    assert_eq!(gets.get(), 3);

    let e = c
        .pages()
        .create("w1", Default::default())
        .await
        .unwrap_err();
    assert!(e.is_server_error());
    assert_eq!(e.status(), Some(503));
    assert_eq!(posts.get(), 1);
}

#[tokio::test]
async fn errors() {
    let app = Router::new()
        .route(
            "/api/v1/pages/{id}",
            get(|| async {
                (
                    StatusCode::NOT_FOUND,
                    axum::Json(
                        json!({ "error": { "code": "not_found", "message": "找不到这一页" } }),
                    ),
                )
            }),
        )
        .route(
            "/api/v1/me",
            get(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
        )
        .route(
            "/api/v1/pages/{id}/share",
            get(|| async {
                (
                    StatusCode::NOT_FOUND,
                    axum::Json(json!({ "error": { "code": "not_found", "message": "没分享" } })),
                )
            }),
        );
    let base = serve(app).await;
    let c = oad::Client::builder()
        .base_url(&base)
        .max_retries(0)
        .build()
        .unwrap();

    let e = c.pages().get("nope").await.unwrap_err();
    assert!(e.is_not_found());
    let api = e.api().unwrap();
    assert_eq!(
        (api.status, api.code.as_str(), api.message.as_str()),
        (404, "not_found", "找不到这一页")
    );
    assert!(e.to_string().contains("找不到这一页"));

    let e = c.me().get().await.unwrap_err();
    let api = e.api().unwrap();
    assert_eq!(
        (api.status, api.code.as_str(), api.message.as_str()),
        (500, "http_500", "boom")
    );

    // null_on_404：没分享时是 None
    assert_eq!(c.pages().share().get("p1").await.unwrap(), None);
}

#[tokio::test]
async fn connection_refused() {
    // 先占一个端口再放掉：这个端口上没人听
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let c = oad::Client::builder()
        .base_url(format!("http://127.0.0.1:{port}"))
        .max_retries(0)
        .http_client(oad::reqwest::Client::builder().no_proxy().build().unwrap())
        .build()
        .unwrap();
    let e = c.me().get().await.unwrap_err();
    assert!(e.is_connection(), "{e:?}");
}

#[tokio::test]
async fn timeout() {
    let app = Router::new().route(
        "/api/v1/me",
        get(|| async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            "{}"
        }),
    );
    let base = serve(app).await;
    let c = oad::Client::builder()
        .base_url(&base)
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();
    let e = c.me().get().await.unwrap_err();
    assert!(e.is_timeout(), "{e:?}");
}

#[derive(Clone, Default)]
struct Seen(Arc<Mutex<Vec<HashMap<String, String>>>>);

impl Seen {
    fn push_query(&self, q: HashMap<String, String>) {
        self.0.lock().unwrap().push(q);
    }
    fn all(&self) -> Vec<HashMap<String, String>> {
        self.0.lock().unwrap().clone()
    }
}

fn version(id: i64) -> Value {
    json!({ "id": id, "page_id": "p1", "kind": "manual", "authors": [], "created_at": id })
}

/// 翻页：按 next_cursor 取完；停止条件：没有下一页、空页、游标没变
#[tokio::test]
async fn pagination() {
    let seen = Seen::default();
    let app = Router::new()
        .route(
            "/api/v1/pages/{id}/versions",
            get(
                |State(seen): State<Seen>,
                 axum::extract::Path(id): axum::extract::Path<String>,
                 Query(q): Query<HashMap<String, String>>| async move {
                    seen.push_query(q.clone());
                    let body = match (id.as_str(), q.get("cursor").map(String::as_str)) {
                        ("p1", None) => {
                            json!({ "versions": [version(1), version(2)], "next_cursor": "c1" })
                        }
                        ("p1", Some("c1")) => {
                            json!({ "versions": [version(3), version(4)], "next_cursor": "c2" })
                        }
                        ("p1", Some("c2")) => {
                            json!({ "versions": [version(5)], "next_cursor": null })
                        }
                        // 游标不动的服务器
                        ("stuck", _) => json!({ "versions": [version(1)], "next_cursor": "same" }),
                        // 空页
                        _ => json!({ "versions": [], "next_cursor": "more" }),
                    };
                    axum::Json(body)
                },
            ),
        )
        .with_state(seen.clone());
    let base = serve(app).await;
    let c = client(&base);

    let page = c.pages().versions().list("p1").limit(2).await.unwrap();
    assert_eq!(page.versions.len(), 2);
    assert_eq!(page.next_cursor.as_deref(), Some("c1"));

    let all: Vec<_> = c
        .pages()
        .versions()
        .list_all("p1")
        .limit(2)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        all.iter().map(|v| v.id).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    let qs = seen.all();
    assert_eq!(qs.len(), 4); // 一次 list + 三页
    assert!(
        qs.iter()
            .all(|q| q.get("limit").map(String::as_str) == Some("2"))
    );

    // 从中间开始：list(...).cursor(..).into_stream()
    let rest: Vec<_> = c
        .pages()
        .versions()
        .list("p1")
        .cursor("c1")
        .into_stream()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(rest.len(), 3);

    let before = seen.all().len();
    let stuck: Vec<_> = c
        .pages()
        .versions()
        .list_all("stuck")
        .try_collect()
        .await
        .unwrap();
    assert_eq!(stuck.len(), 2);
    assert_eq!(seen.all().len() - before, 2);

    let before = seen.all().len();
    let empty: Vec<_> = c
        .pages()
        .versions()
        .list_all("empty")
        .try_collect()
        .await
        .unwrap();
    assert!(empty.is_empty());
    assert_eq!(seen.all().len() - before, 1);
}

#[derive(Clone, Default)]
struct Headers(Arc<Mutex<Vec<HeaderMap>>>);

async fn record_me(State(h): State<Headers>, headers: HeaderMap) -> axum::Json<Value> {
    h.0.lock().unwrap().push(headers);
    axum::Json(me_json("王小明"))
}

fn h(map: &HeaderMap, k: &str) -> Option<String> {
    map.get(k).map(|v| v.to_str().unwrap().to_string())
}

/// with_user（header 模式）：X-OAD-Secret、X-OAD-User-*，名字百分号编码；原来的客户端不变
#[tokio::test]
async fn with_user_header_mode() {
    let seen = Headers::default();
    let app = Router::new()
        .route("/api/v1/me", get(record_me))
        .with_state(seen.clone());
    let base = serve(app).await;
    let admin = oad::Client::builder()
        .base_url(&base)
        .header_secret("s3cret")
        .token("oad_admin")
        .build()
        .unwrap();
    let u = admin
        .with_user(oad::User::new("u-42").name("王小明").email("x@example.com"))
        .unwrap();
    u.me().get().await.unwrap();
    admin.me().get().await.unwrap();
    let hs = seen.0.lock().unwrap().clone();
    assert_eq!(h(&hs[0], "x-oad-secret").as_deref(), Some("s3cret"));
    assert_eq!(h(&hs[0], "x-oad-user-id").as_deref(), Some("u-42"));
    assert_eq!(
        h(&hs[0], "x-oad-user-name").as_deref(),
        Some("%E7%8E%8B%E5%B0%8F%E6%98%8E")
    );
    assert_eq!(
        h(&hs[0], "x-oad-user-email").as_deref(),
        Some("x%40example.com")
    );
    assert_eq!(h(&hs[0], "authorization"), None);
    // 原来的客户端还是用自己的 token
    assert_eq!(
        h(&hs[1], "authorization").as_deref(),
        Some("Bearer oad_admin")
    );
    assert_eq!(h(&hs[1], "x-oad-user-id"), None);
}

/// with_user（JWT 模式）：签 HS256，缓存（两次请求同一个 token）
#[tokio::test]
async fn with_user_jwt_mode() {
    use base64::Engine;
    let seen = Headers::default();
    let app = Router::new()
        .route("/api/v1/me", get(record_me))
        .with_state(seen.clone());
    let base = serve(app).await;
    let admin = oad::Client::builder()
        .base_url(&base)
        .jwt_secret("jwt-secret")
        .jwt_issuer("https://app.example.com")
        .build()
        .unwrap();
    let u = admin
        .with_user(
            oad::User::new("u9")
                .name("小九")
                .workspaces([oad::JwtWorkspace::with_role("w1", "editor")]),
        )
        .unwrap();
    u.me().get().await.unwrap();
    u.me().get().await.unwrap();
    let hs = seen.0.lock().unwrap().clone();
    let a = h(&hs[0], "authorization").unwrap();
    assert_eq!(Some(a.clone()), h(&hs[1], "authorization"));
    let tok = a.strip_prefix("Bearer ").unwrap();
    let parts: Vec<&str> = tok.split('.').collect();
    assert_eq!(parts.len(), 3);
    let claims: Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .unwrap(),
    )
    .unwrap();
    assert_eq!(claims["sub"], "u9");
    assert_eq!(claims["name"], "小九");
    assert_eq!(claims["iss"], "https://app.example.com");
    assert_eq!(
        claims["workspaces"],
        json!([{ "id": "w1", "role": "editor" }])
    );
    assert_eq!(
        claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(),
        3600
    );
}

/// default_workspace：/me 的第一个，只查一次
#[tokio::test]
async fn default_workspace_cached() {
    let seen = Headers::default();
    let app = Router::new()
        .route("/api/v1/me", get(record_me))
        .with_state(seen.clone());
    let base = serve(app).await;
    let c = client(&base);
    assert_eq!(c.default_workspace().await.unwrap(), "w1");
    assert_eq!(c.default_workspace().await.unwrap(), "w1");
    assert_eq!(seen.0.lock().unwrap().len(), 1);
}

/// 每个请求带 User-Agent、Accept-Language、自己加的头
#[tokio::test]
async fn request_headers() {
    let seen = Headers::default();
    let app = Router::new()
        .route("/api/v1/me", get(record_me))
        .with_state(seen.clone());
    let base = serve(app).await;
    let c = oad::Client::builder()
        .base_url(&base)
        .locale("zh")
        .header("x-trace", "abc")
        .build()
        .unwrap();
    c.me().get().await.unwrap();
    let hs = seen.0.lock().unwrap().clone();
    assert_eq!(
        h(&hs[0], "user-agent").unwrap(),
        format!("open-agent-docs-rust/{}", oad::VERSION)
    );
    assert_eq!(h(&hs[0], "accept-language").as_deref(), Some("zh-CN"));
    assert_eq!(h(&hs[0], "x-trace").as_deref(), Some("abc"));
    assert!(h(&hs[0], "accept-encoding").unwrap().contains("gzip"));
}

#[derive(Clone, Default)]
struct Bodies(Arc<Mutex<Vec<(HeaderMap, Bytes)>>>);

/// 请求体：三态的字段按原样发；上传是原始正文 + X-Filename
#[tokio::test]
async fn request_bodies() {
    let seen = Bodies::default();
    let page = json!({ "id": "p1", "workspace_id": "w1", "position": "a", "title": "T", "created_at": 1, "updated_at": 2 });
    let app = Router::new()
        .route(
            "/api/v1/pages/{id}",
            patch({
                let page = page.clone();
                move |State(s): State<Bodies>, headers: HeaderMap, body: Bytes| async move {
                    s.0.lock().unwrap().push((headers, body));
                    axum::Json(page)
                }
            }),
        )
        .route(
            "/api/v1/workspaces/{ws}/files",
            post(|State(s): State<Bodies>, headers: HeaderMap, body: Bytes| async move {
                s.0.lock().unwrap().push((headers, body.clone()));
                (StatusCode::CREATED, axum::Json(json!({ "id": "f1", "url": "/api/v1/files/f1", "name": "报告 1.txt", "mime": "text/plain", "size": body.len() })))
            }),
        )
        .with_state(seen.clone());
    let base = serve(app).await;
    let c = client(&base);
    c.pages()
        .update(
            "p1",
            PatchPage {
                after: Nullable::Null,
                title: "新标题".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let up = c
        .files()
        .upload("w1", "报告 1.txt", &b"hello"[..])
        .await
        .unwrap();
    assert_eq!(up.size, 5);
    let s = seen.0.lock().unwrap().clone();
    let sent: Value = serde_json::from_slice(&s[0].1).unwrap();
    assert_eq!(sent, json!({ "after": null, "title": "新标题" }));
    assert_eq!(
        h(&s[0].0, "content-type").as_deref(),
        Some("application/json")
    );
    assert_eq!(&s[1].1[..], b"hello");
    assert_eq!(
        h(&s[1].0, "x-filename").as_deref(),
        Some("%E6%8A%A5%E5%91%8A%201.txt")
    );
    assert_eq!(
        h(&s[1].0, "content-type").as_deref(),
        Some("application/octet-stream")
    );
}
