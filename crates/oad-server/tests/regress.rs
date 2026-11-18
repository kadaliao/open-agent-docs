//! 评审发现的问题的回归测试。
mod common;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::*;
use futures_util::StreamExt;
use oad_core::AuthConfig;
use oad_server::ServerConfig;
use serde_json::json;
use std::time::Duration;
use yrs::sync::{Message, SyncMessage};
use yrs::updates::encoder::Encode;
use yrs::{ClientID, IdSet, Snapshot, StateVector};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crafted_base_is_rejected_and_page_survives() {
    let dir = tempfile::tempdir().unwrap();
    // 攒很久才存：确认过但还没存的改动在内存里
    let hub = oad_core::HubConfig {
        flush_window: Duration::from_secs(60),
        ..oad_core::HubConfig::default()
    };
    let c = http();
    let s = Server::start(dir.path(), AuthConfig::None, hub.clone()).await;
    let page = page_with(&s, &c, "p", "第一段\n").await;
    let para = block_id(&s.server_doc(&page).await, 0);
    let mut human = YClient::new(3001);
    human.connect(&s.ws(&page), &[]).await;
    eventually("同步上", async || human.synced()).await;
    human.type_at(&para, 0, "人打的");
    eventually("服务器收到", async || {
        s.server_doc(&page).await.plain_text().contains("人打的")
    })
    .await;
    // 时钟为 0 的版本：以前会让 actor panic 退出
    let crafted = Snapshot::new(
        StateVector::from_iter([(ClientID::new(3001), 0)]),
        IdSet::new(),
    )
    .encode_v1();
    let (st, e) = json(
        c.put(s.api(&format!("/pages/{page}/blocks/{para}")))
            .json(&json!({ "markdown": "x", "base": URL_SAFE_NO_PAD.encode(&crafted) })),
    )
    .await;
    assert_eq!(st, 400, "{e}");
    // 连接还在，接着能改
    human.type_at(&para, 0, "又打的");
    eventually("服务器收到", async || {
        s.server_doc(&page)
            .await
            .plain_text()
            .starts_with("又打的人打的")
    })
    .await;
    assert!(!human.state.lock().unwrap().closed);
    s.stop().await;
    let s = Server::start(dir.path(), AuthConfig::None, hub).await;
    assert_eq!(s.server_doc(&page).await.plain_text(), "又打的人打的第一段");
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn host_and_origin_checks() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = ServerConfig {
        public_url: Some("https://docs.example.com".into()),
        allowed_origins: vec!["https://embed.example.org".into()],
        ..ServerConfig::default()
    };
    let s = Server::start_with(dir.path(), AuthConfig::None, fast_hub(), cfg).await;
    let c = http();
    let pages = s.api(&format!("/workspaces/{}/pages", s.ws_id()));
    // DNS rebinding：Host 是别人的域名
    let (st, e) = json(c.get(s.api("/me")).header("host", "evil.example")).await;
    assert_eq!(st, 403, "{e}");
    let (st, _) = json(c.get(s.api("/me")).header("host", "localhost:8848")).await;
    assert_eq!(st, 200);
    let (st, _) = json(c.get(s.api("/me")).header("host", "docs.example.com")).await;
    assert_eq!(st, 200);
    // 改数据：别的网站的 Origin 拒绝；同源、配置里的、本机开发、不带 Origin 都行
    let post = |origin: Option<&str>| {
        let r = c.post(&pages).json(&json!({ "title": "t" }));
        match origin {
            Some(o) => r.header("origin", o),
            None => r,
        }
    };
    assert_eq!(json(post(Some("https://evil.example"))).await.0, 403);
    assert_eq!(json(post(Some("null"))).await.0, 403);
    assert_eq!(json(post(None)).await.0, 201);
    assert_eq!(json(post(Some(&format!("http://{}", s.addr)))).await.0, 201);
    assert_eq!(json(post(Some("http://localhost:5173"))).await.0, 201);
    assert_eq!(json(post(Some("https://embed.example.org"))).await.0, 201);
    assert_eq!(json(post(Some("https://docs.example.com"))).await.0, 201);
    // 读不检查 Origin
    let (st, _) = json(c.get(&pages).header("origin", "https://evil.example")).await;
    assert_eq!(st, 200);
    // WebSocket 握手同样检查 Origin
    let (_, p) = json(post(None)).await;
    let page = p["id"].as_str().unwrap();
    let mut bad = YClient::new(1);
    assert!(
        bad.try_connect(&s.ws(page), &[("origin", "https://evil.example")])
            .await
            .is_err()
    );
    let mut ok = YClient::new(2);
    ok.connect(&s.ws(page), &[("origin", "https://embed.example.org")])
        .await;
    eventually("同步上", async || ok.synced()).await;
    s.stop().await;

    // 有登录的模式没配 allowed_hosts 时不查 Host（换了域名 cookie 带不过去），Origin 照样查
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::Local, fast_hub()).await;
    let (st, _) = json(c.get(s.api("/auth/config")).header("host", "docs.internal")).await;
    assert_eq!(st, 200);
    let (st, _) = json(
        c.post(s.api("/auth/login"))
            .header("origin", "https://evil.example")
            .json(&json!({ "email": "a@b.c", "password": "x" })),
    )
    .await;
    assert_eq!(st, 403);
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn open_sse_and_ws_do_not_block_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let page = page_with(&s, &c, "p", "一段\n").await;
    let sse = c
        .get(s.api(&format!("/workspaces/{}/events", s.ws_id())))
        .send()
        .await
        .unwrap();
    let mut events = sse.bytes_stream();
    let mut y = YClient::new(5);
    y.connect(&s.ws(&page), &[]).await;
    eventually("同步上", async || y.synced()).await;
    // stop() 里断言 HTTP 服务 5 秒内停下
    s.stop().await;
    let end = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(Ok(_)) = events.next().await {}
    })
    .await;
    assert!(end.is_ok(), "SSE 没结束");
    eventually("WebSocket 被关", async || y.state.lock().unwrap().closed).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stale_cookie_does_not_shadow_token() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::Local, fast_hub()).await;
    s.core
        .ensure_admin("admin@example.com", "admin-password")
        .await
        .unwrap();
    let c = http();
    let res = c
        .post(s.api("/auth/login"))
        .json(&json!({ "email": "admin@example.com", "password": "admin-password" }))
        .send()
        .await
        .unwrap();
    let cookie = res.headers()["set-cookie"].to_str().unwrap().to_string();
    let token = cookie
        .split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_string();
    let stale = "oad_session=0000000000000000";
    let (st, _) = json(c.get(s.api("/me")).header("cookie", stale)).await;
    assert_eq!(st, 401);
    let (st, _) = json(
        c.get(s.api("/me"))
            .header("cookie", stale)
            .bearer_auth(&token),
    )
    .await;
    assert_eq!(st, 200);
    let (st, p) = json(
        c.post(s.api(&format!("/workspaces/{}/pages", s.ws_id())))
            .bearer_auth(&token)
            .json(&json!({ "title": "t" })),
    )
    .await;
    assert_eq!(st, 201);
    let page = p["id"].as_str().unwrap();
    let mut y = YClient::new(6);
    y.connect(
        &format!("{}?token={token}", s.ws(page)),
        &[("cookie", stale)],
    )
    .await;
    eventually("token 连上", async || y.synced()).await;
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn step1_flood_is_limited() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let page = page_with(&s, &c, "p", "一段\n").await;
    let mut y = YClient::new(7);
    y.connect(&s.ws(&page), &[]).await;
    eventually("同步上", async || y.synced()).await;
    // 一样的 state vector 刷 200 次：不再重复算差量
    let sv = StateVector::default();
    for _ in 0..200 {
        y.send_raw(Message::Sync(SyncMessage::SyncStep1(sv.clone())).encode_v1());
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let n = y.state.lock().unwrap().step2;
    assert!(n <= 3, "回了 {n} 次 step2");
    assert!(!y.state.lock().unwrap().closed);
    // 每次换一个 state vector：超过每分钟上限就断开
    for i in 0..100u64 {
        let sv = StateVector::from_iter([(ClientID::new(1000 + i), 1)]);
        y.send_raw(Message::Sync(SyncMessage::SyncStep1(sv)).encode_v1());
    }
    eventually("被断开", async || y.state.lock().unwrap().closed).await;
    let n = y.state.lock().unwrap().step2;
    assert!(n <= 25, "回了 {n} 次 step2");
    // 服务器还好好的
    let mut z = YClient::new(8);
    z.connect(&s.ws(&page), &[]).await;
    eventually("别的连接照常", async || z.synced()).await;
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn login_rate_limit() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = ServerConfig {
        login_max_failures: 3,
        ..ServerConfig::default()
    };
    let s = Server::start_with(dir.path(), AuthConfig::Local, fast_hub(), cfg).await;
    s.core
        .ensure_admin("admin@example.com", "admin-password")
        .await
        .unwrap();
    let c = http();
    let login = async |email: &str, pw: &str| {
        let r = c
            .post(s.api("/auth/login"))
            .json(&json!({ "email": email, "password": pw }))
            .send()
            .await
            .unwrap();
        (
            r.status().as_u16(),
            r.headers().get("retry-after").is_some(),
        )
    };
    for _ in 0..3 {
        assert_eq!(login("admin@example.com", "wrong").await.0, 401);
    }
    // 达到上限：密码对了也先拒绝
    assert_eq!(
        login("Admin@Example.com", "admin-password").await,
        (429, true)
    );
    // 别的邮箱不受影响
    assert_eq!(login("other@example.com", "wrong").await.0, 401);
    s.stop().await;
}

/// 不经过任何 HTTP 客户端库（它们可能自己解压）：原样发一个 GET，拿状态、头、正文。
async fn raw_get(
    addr: std::net::SocketAddr,
    path: &str,
    headers: &[(&str, &str)],
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let mut req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).await.unwrap();
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&buf[..split]).to_string();
    let mut body = buf[split + 4..].to_vec();
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let hs: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    // 分块编码的拼回来
    if hs
        .iter()
        .any(|(k, v)| k == "transfer-encoding" && v.contains("chunked"))
    {
        let mut out = Vec::new();
        let mut rest = &body[..];
        loop {
            let nl = rest.windows(2).position(|w| w == b"\r\n").unwrap();
            let n = usize::from_str_radix(std::str::from_utf8(&rest[..nl]).unwrap().trim(), 16)
                .unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&rest[nl + 2..nl + 2 + n]);
            rest = &rest[nl + 2 + n + 2..];
        }
        body = out;
    }
    (status, hs, body)
}

fn hdr<'a>(hs: &'a [(String, String)], k: &str) -> Option<&'a str> {
    hs.iter().find(|(x, _)| x == k).map(|(_, v)| v.as_str())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn static_web_precompressed() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let (_, hs, body) = raw_get(s.addr, "/", &[]).await;
    let body = String::from_utf8_lossy(&body).to_string();
    if !body.contains("<script") {
        eprintln!("网页没构建（apps/web/dist），跳过");
        return;
    }
    // 不支持压缩的客户端拿到原文
    assert!(body.to_ascii_lowercase().starts_with("<!doctype html"));
    assert_eq!(hdr(&hs, "content-encoding"), None);
    let etag = hdr(&hs, "etag").unwrap().to_string();
    let (_, hs, br) = raw_get(s.addr, "/", &[("Accept-Encoding", "gzip, br")]).await;
    assert_eq!(hdr(&hs, "content-encoding"), Some("br"));
    assert_eq!(hdr(&hs, "vary"), Some("Accept-Encoding"));
    assert_eq!(hdr(&hs, "cache-control"), Some("no-cache"));
    assert!(br.len() < body.len());
    let (_, hs, _) = raw_get(s.addr, "/", &[("Accept-Encoding", "gzip")]).await;
    assert_eq!(hdr(&hs, "content-encoding"), Some("gzip"));
    let (st, _, _) = raw_get(s.addr, "/", &[("If-None-Match", &etag)]).await;
    assert_eq!(st, 304);
    // 前端路由回 index.html；带哈希的打包产物长缓存
    let (_, _, b) = raw_get(s.addr, "/p/abc", &[]).await;
    assert!(String::from_utf8_lossy(&b).contains("<script"));
    let asset = body
        .split("src=\"/")
        .nth(1)
        .and_then(|x| x.split('"').next())
        .expect("index.html 里有脚本")
        .to_string();
    let (st, hs, _) = raw_get(s.addr, &format!("/{asset}"), &[("Accept-Encoding", "br")]).await;
    assert_eq!(st, 200);
    assert_eq!(
        hdr(&hs, "cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(hdr(&hs, "content-encoding"), Some("br"));
    assert_eq!(raw_get(s.addr, "/nope.js", &[]).await.0, 404);
    s.stop().await;
}
