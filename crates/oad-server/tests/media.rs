//! 第 3 阶段：链接解析（缓存、过期先给旧的、失败退避、同一网址只抓一次、图片转存、后台预取）、上传和文件下发。
//! 外部网站用本机的小 HTTP 服务代替，不联网。
mod common;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use common::*;
use oad_core::{AuthConfig, MediaConfig, NotifyConfig};
use oad_server::ServerConfig;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Hits = Arc<Mutex<HashMap<String, usize>>>;

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
    });
    let mut v = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut v), image::ImageFormat::Png)
        .unwrap();
    v
}

/// 假的外部网站：/page、/page2（Open Graph + 图）、/img.png、/icon.png、/missing（404）、/slow（等 300ms）。记每个路径被请求了几次。
async fn site() -> (String, Hits) {
    let hits: Hits = Arc::default();
    let app = axum::Router::new()
        .route(
            "/{name}",
            axum::routing::get(|State(h): State<Hits>, Path(name): Path<String>| async move {
                *h.lock().unwrap().entry(name.clone()).or_default() += 1;
                let html = |title: &str| {
                    format!(
                        r#"<!doctype html><html><head><title>{title}</title>
<meta property="og:title" content="{title}"><meta property="og:description" content="一段描述">
<meta property="og:image" content="/img.png"><link rel="icon" href="/icon.png"></head><body>正文</body></html>"#
                    )
                };
                match name.as_str() {
                    "img.png" => ([(header::CONTENT_TYPE, "image/png")], png(400, 300)).into_response(),
                    "icon.png" => ([(header::CONTENT_TYPE, "image/png")], png(16, 16)).into_response(),
                    "missing" => StatusCode::NOT_FOUND.into_response(),
                    "slowimg.png" => {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        ([(header::CONTENT_TYPE, "image/png")], png(40, 30)).into_response()
                    }
                    "slowimg-page" => (
                        [(header::CONTENT_TYPE, "text/html")],
                        r#"<html><head><title>图很慢</title><meta property="og:image" content="/slowimg.png"></head></html>"#,
                    )
                        .into_response(),
                    "slow" => {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        ([(header::CONTENT_TYPE, "text/html")], html("慢页面")).into_response()
                    }
                    n => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html(&format!("页面 {n}"))).into_response(),
                }
            }),
        )
        .with_state(hits.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (base, hits)
}

fn media_cfg(max_upload: usize) -> MediaConfig {
    MediaConfig {
        fetch: oad_unfurl::FetchConfig {
            allow_cidrs: vec![oad_unfurl::fetch::Cidr::parse("127.0.0.1/32").unwrap()],
            ..oad_unfurl::FetchConfig::default()
        },
        max_upload,
        ..MediaConfig::default()
    }
}

async fn start(dir: &std::path::Path, auth: AuthConfig, max_upload: usize) -> Server {
    Server::start_media(
        dir,
        auth,
        fast_hub(),
        ServerConfig::default(),
        NotifyConfig::default(),
        media_cfg(max_upload),
    )
    .await
}

fn hits(h: &Hits, name: &str) -> usize {
    h.lock().unwrap().get(name).copied().unwrap_or(0)
}

async fn unfurl(s: &Server, c: &reqwest::Client, url: &str) -> (u16, Value) {
    json(c.get(s.api("/unfurl")).query(&[("url", url)])).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unfurl_cache_and_rehost() {
    let dir = tempfile::tempdir().unwrap();
    let s = start(dir.path(), AuthConfig::None, 100 << 20).await;
    let c = http();
    let (base, h) = site().await;

    // 普通网页 → 书签卡片；图转存到本地，封面带宽高和 blurhash
    let (st, v) = unfurl(&s, &c, &format!("{base}/page?utm_source=x")).await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["kind"], "bookmark");
    assert_eq!(v["title"], "页面 page");
    let thumb = v["thumbnail"]["url"].as_str().unwrap();
    assert!(thumb.starts_with("/api/v1/media/"), "{v}");
    assert_eq!(v["thumbnail"]["width"], 400);
    assert!(v["thumbnail"]["blurhash"].is_string());
    assert!(v["favicon"].as_str().unwrap().starts_with("/api/v1/media/"));
    assert!(v.get("attempts").is_none());
    // 转存的图能取，能要缩略图
    let r = c
        .get(format!("http://{}{thumb}", s.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(r.headers()["content-type"], "image/png");
    let r = c
        .get(format!("http://{}{thumb}?w=160", s.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(r.headers()["content-type"], "image/webp");
    let (st, _) = json(
        c.get(s.api("/media/0000000000000000000000000000000000000000000000000000000000000000")),
    )
    .await;
    assert_eq!(st, 404);

    // 再要：缓存里有（去掉跟踪参数后是同一个），不再抓
    let (_, v2) = unfurl(&s, &c, &format!("{base}/page")).await;
    assert_eq!(v2["title"], "页面 page");
    assert_eq!(hits(&h, "page"), 1);
    assert_eq!(hits(&h, "img.png"), 1);

    // 过期了：先给旧的（stale），后台刷新
    let key = oad_unfurl::cache_key(&format!("{base}/page"));
    let mut row = s.core.storage.unfurl(&key).await.unwrap().unwrap();
    row.expires_at = 0;
    s.core.storage.put_unfurl(row).await.unwrap();
    let (_, v3) = unfurl(&s, &c, &format!("{base}/page")).await;
    assert_eq!(v3["stale"], true);
    eventually("后台刷新了", async || hits(&h, "page") == 2).await;
    eventually("刷新后不再是旧的", async || {
        unfurl(&s, &c, &format!("{base}/page"))
            .await
            .1
            .get("stale")
            .is_none()
    })
    .await;
    // 转存的图按原地址记着，不重复下载
    assert_eq!(hits(&h, "img.png"), 1);

    // 失败：退避期间不再抓
    let (st, e) = unfurl(&s, &c, &format!("{base}/missing")).await;
    assert_eq!(st, 502, "{e}");
    assert_eq!(e["error"]["code"], "bad_gateway");
    let (st, _) = unfurl(&s, &c, &format!("{base}/missing")).await;
    assert_eq!(st, 502);
    assert_eq!(hits(&h, "missing"), 1);

    // 同一个网址同时只抓一次
    let url = format!("{base}/slow");
    let tasks: Vec<_> = (0..5)
        .map(|_| {
            let (c, u, api) = (c.clone(), url.clone(), s.api("/unfurl"));
            tokio::spawn(async move { json(c.get(api).query(&[("url", u)])).await })
        })
        .collect();
    for t in tasks {
        let (st, v) = t.await.unwrap();
        assert_eq!(st, 200);
        assert_eq!(v["title"], "慢页面");
    }
    assert_eq!(hits(&h, "slow"), 1);

    // 内网地址：默认的 SSRF 检查拦着（这个服务器只放行了 127.0.0.1）
    let (st, _) = unfurl(&s, &c, "http://10.0.0.1/").await;
    assert_eq!(st, 502);
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_images_do_not_block_the_card() {
    let dir = tempfile::tempdir().unwrap();
    let s = start(dir.path(), AuthConfig::None, 100 << 20).await;
    let c = http();
    let (base, _h) = site().await;
    let url = format!("{base}/slowimg-page");
    let t = std::time::Instant::now();
    let (st, v) = unfurl(&s, &c, &url).await;
    assert_eq!(st, 200);
    assert!(
        t.elapsed() < Duration::from_millis(2500),
        "{:?}",
        t.elapsed()
    );
    assert_eq!(
        v["thumbnail"]["url"],
        format!("{base}/slowimg.png"),
        "没转完先给原地址"
    );
    // 转完以后缓存里是本地地址
    eventually("转存完更新缓存", async || {
        unfurl(&s, &c, &url).await.1["thumbnail"]["url"]
            .as_str()
            .is_some_and(|u| u.starts_with("/api/v1/media/"))
    })
    .await;
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn prefetch_when_embed_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let s = start(dir.path(), AuthConfig::None, 100 << 20).await;
    let c = http();
    let (base, h) = site().await;
    let page = page_with(&s, &c, "预取", "第一段\n").await;
    // Agent 写一行网址
    let agents = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (_, tok, _) = make_agent(c.post(&agents), "写手", "write", None).await;
    let (st, _) = json(
        c.post(s.api(&format!("/pages/{page}/blocks")))
            .bearer_auth(&tok)
            .json(&json!({ "markdown": format!("{base}/agent-link") })),
    )
    .await;
    assert_eq!(st, 200);
    eventually("后台解析了", async || hits(&h, "agent-link") == 1).await;
    // 网页打开时已经在缓存里
    let (_, v) = unfurl(&s, &c, &format!("{base}/agent-link")).await;
    assert_eq!(v["title"], "页面 agent-link");
    assert_eq!(hits(&h, "agent-link"), 1);

    // 浏览器插入的 embed 块也一样
    let mut y = YClient::new(55);
    y.connect(&s.ws(&page), &[]).await;
    eventually("同步上", async || y.synced()).await;
    y.push_block(
        "embed",
        &[("id", "brwsr001"), ("url", &format!("{base}/browser-link"))],
    );
    eventually("后台解析了浏览器写的", async || {
        hits(&h, "browser-link") == 1
    })
    .await;
    // 不是单独一行网址的不解析；改别的段落不会重复解析
    y.type_at(&block_id(&s.server_doc(&page).await, 0), 0, "字");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(hits(&h, "browser-link"), 1);
    s.stop().await;
}

async fn upload(
    s: &Server,
    req: reqwest::RequestBuilder,
    name: &str,
    data: Vec<u8>,
) -> (u16, Value) {
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(data).file_name(name.to_string()),
    );
    let _ = s;
    json(req.multipart(form)).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uploads_and_file_serving() {
    let dir = tempfile::tempdir().unwrap();
    let s = start(
        dir.path(),
        AuthConfig::Header {
            secret: "sec".into(),
        },
        1 << 20,
    )
    .await;
    let c = http();
    let as_user = |r: reqwest::RequestBuilder, id: &str| {
        r.header("X-OAD-Secret", "sec").header("X-OAD-User-Id", id)
    };
    json(as_user(c.get(s.api("/me")), "alice")).await;
    let files = s.api(&format!("/workspaces/{}/files", s.ws_id()));

    // 按内容判断类型（名字叫 .txt 也是 PNG），带宽高、blurhash
    let img = png(800, 600);
    let (st, f) = upload(
        &s,
        as_user(c.post(&files), "alice"),
        "照片 1.txt",
        img.clone(),
    )
    .await;
    assert_eq!(st, 201, "{f}");
    assert_eq!(f["mime"], "image/png");
    assert_eq!(
        (f["width"].as_i64(), f["height"].as_i64()),
        (Some(800), Some(600))
    );
    assert!(f["blurhash"].is_string());
    assert_eq!(f["size"], img.len());
    let url = f["url"].as_str().unwrap().to_string();
    assert!(
        url.starts_with(&format!("/api/v1/files/{}/", f["id"].as_str().unwrap())),
        "{url}"
    );
    let full = format!("http://{}{url}", s.addr);

    // 下载：长缓存、ETag、304、Range、缩略图
    let r = as_user(c.get(&full), "alice").send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers()["cache-control"],
        "private, max-age=31536000, immutable"
    );
    assert_eq!(r.headers()["accept-ranges"], "bytes");
    let etag = r.headers()["etag"].to_str().unwrap().to_string();
    assert_eq!(r.bytes().await.unwrap().to_vec(), img);
    let r = as_user(c.get(&full), "alice")
        .header("if-none-match", &etag)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 304);
    let r = as_user(c.get(&full), "alice")
        .header("range", "bytes=0-9")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(
        r.headers()["content-range"],
        format!("bytes 0-9/{}", img.len())
    );
    assert_eq!(r.bytes().await.unwrap().to_vec(), img[..10]);
    let r = as_user(c.get(&full), "alice")
        .header("range", format!("bytes={}-", img.len()))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 416);
    let r = as_user(c.get(format!("{full}?w=300")), "alice")
        .send()
        .await
        .unwrap();
    assert_eq!(r.headers()["content-type"], "image/webp");
    let thumb = r.bytes().await.unwrap();
    let t = image::load_from_memory(&thumb).unwrap();
    assert_eq!(t.width(), 320, "取最近的一档");
    assert!(thumb.len() < img.len());

    // 原始正文 + X-Filename；SVG 当附件、带 sandbox；HTML 当纯文本
    let r = as_user(c.post(&files), "alice")
        .header("x-filename", "%E5%9B%BE.svg")
        .body("<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script></svg>")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);
    let svg: Value = r.json().await.unwrap();
    assert_eq!(svg["name"], "图.svg");
    assert_eq!(svg["mime"], "image/svg+xml");
    let r = as_user(
        c.get(format!("http://{}{}", s.addr, svg["url"].as_str().unwrap())),
        "alice",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(r.headers()["content-security-policy"], "sandbox");
    assert!(
        r.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment")
    );
    let (_, html) = upload(
        &s,
        as_user(c.post(&files), "alice"),
        "x.html",
        b"<html><script>alert(1)</script></html>".to_vec(),
    )
    .await;
    assert_eq!(html["mime"], "text/plain");

    // 太大：413
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(vec![7u8; 2 << 20]).file_name("big.bin"),
    );
    match as_user(c.post(&files), "alice")
        .multipart(form)
        .send()
        .await
    {
        Ok(r) => assert_eq!(r.status(), 413),
        Err(e) => assert!(e.is_request(), "{e}"),
    }
    // 原始正文超了：服务器读到上限就回 413，客户端还在写的话可能看到的是连接断了
    match as_user(c.post(&files), "alice")
        .body(vec![7u8; 2 << 20])
        .send()
        .await
    {
        Ok(r) => assert_eq!(r.status(), 413),
        Err(e) => assert!(e.is_request(), "{e}"),
    }

    // 上传的文件也能解析：直接从库里给
    let (st, card) = json(as_user(
        c.get(s.api("/unfurl")).query(&[("url", &url)]),
        "alice",
    ))
    .await;
    assert_eq!(st, 200, "{card}");
    assert_eq!(card["kind"], "image");
    assert_eq!(card["name"], "照片 1.txt");
    assert_eq!(card["width"], 800);
    assert!(card["blurhash"].is_string());
    let (_, txt) = upload(
        &s,
        as_user(c.post(&files), "alice"),
        "说明.md",
        "# 标题\n".as_bytes().to_vec(),
    )
    .await;
    let (_, card) = json(as_user(
        c.get(s.api("/unfurl"))
            .query(&[("url", txt["url"].as_str().unwrap())]),
        "alice",
    ))
    .await;
    assert_eq!(card["kind"], "file");
    assert_eq!(card["mime"], "text/markdown");

    // 别的工作区的人看不到；没登录的看不到
    let other = s.core.storage.create_workspace("别人的").await.unwrap();
    let (_, bob) = json(as_user(c.get(s.api("/me")), "bob")).await;
    let bob_id = bob["user"]["id"].as_str().unwrap().to_string();
    // bob 自动进了默认工作区；把他挪到别的工作区
    s.core
        .storage
        .set_member(&other.id, &bob_id, oad_core::Role::Owner)
        .await
        .unwrap();
    s.core
        .storage
        .set_member(&s.ws_id(), &bob_id, oad_core::Role::Viewer)
        .await
        .unwrap();
    let r = as_user(c.get(&full), "bob").send().await.unwrap();
    assert_eq!(r.status(), 200, "只读成员能看");
    let (st, _) = upload(&s, as_user(c.post(&files), "bob"), "x.png", png(2, 2)).await;
    assert_eq!(st, 403, "只读成员不能传");
    let (_, theirs) = upload(
        &s,
        as_user(
            c.post(s.api(&format!("/workspaces/{}/files", other.id))),
            "bob",
        ),
        "x.png",
        png(2, 2),
    )
    .await;
    let r = as_user(
        c.get(format!(
            "http://{}{}",
            s.addr,
            theirs["url"].as_str().unwrap()
        )),
        "alice",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(r.status(), 404, "不是那个工作区的成员");
    let (st, _) = json(as_user(
        c.get(s.api("/unfurl"))
            .query(&[("url", theirs["url"].as_str().unwrap())]),
        "alice",
    ))
    .await;
    assert_eq!(st, 404);
    let r = c.get(&full).send().await.unwrap();
    assert_eq!(r.status(), 401);
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcp_upload_file() {
    use base64::Engine;
    let dir = tempfile::tempdir().unwrap();
    let s = start(dir.path(), AuthConfig::None, 100 << 20).await;
    let c = http();
    let agents = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (_, tok, _) = make_agent(c.post(&agents), "画图", "write", None).await;
    let m = mcp_client(&s, &tok).await.unwrap();
    let img = png(64, 32);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&img);
    let (t, err) = call(
        &m,
        "upload_file",
        json!({ "name": "chart.png", "content_base64": b64 }),
    )
    .await;
    assert!(!err, "{t}");
    assert!(t.contains("64×32"), "{t}");
    let md = t.lines().last().unwrap().to_string();
    assert!(
        md.starts_with("![](/api/v1/files/") && md.ends_with("chart.png)"),
        "{md}"
    );
    let url = md.trim_start_matches("![](").trim_end_matches(')');
    let r = c
        .get(format!("http://{}{url}", s.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(r.bytes().await.unwrap().to_vec(), img);
    // 别的文件给单独一行的网址
    let b64 = base64::engine::general_purpose::STANDARD.encode(b"a,b\n1,2\n");
    let (t, _) = call(
        &m,
        "upload_file",
        json!({ "name": "data.csv", "content_base64": b64 }),
    )
    .await;
    assert!(
        t.lines().last().unwrap().starts_with("/api/v1/files/"),
        "{t}"
    );
    let (t, err) = call(
        &m,
        "upload_file",
        json!({ "name": "x", "content_base64": "不是 base64" }),
    )
    .await;
    assert!(err, "{t}");
    let _ = m.cancel().await;
    s.stop().await;
}

/// 联网：真的去解析几个网站，经过整个服务（缓存、图片转存）。`cargo test -p oad-server --test media live -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn live_unfurl_through_service() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start_media(
        dir.path(),
        AuthConfig::None,
        fast_hub(),
        ServerConfig::default(),
        NotifyConfig::default(),
        MediaConfig {
            fetch: oad_unfurl::FetchConfig::from_env(),
            ..MediaConfig::default()
        },
    )
    .await;
    let c = http();
    for url in [
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://github.com/rust-lang/rust",
        "https://www.rust-lang.org/",
    ] {
        let t = std::time::Instant::now();
        let (st, v) = unfurl(&s, &c, url).await;
        let first = t.elapsed();
        let t = std::time::Instant::now();
        let (st2, _) = unfurl(&s, &c, url).await;
        eprintln!(
            "{url}: {st} {} / {}，第一次 {first:?}，缓存 {:?}，封面 {}",
            v["kind"],
            v["provider"],
            t.elapsed(),
            v["thumbnail"]["url"]
        );
        assert_eq!((st, st2), (200, 200), "{v}");
        assert!(v["title"].is_string());
    }
    s.stop().await;
}
