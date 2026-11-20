//! 嵌入：`?format=html` 出只读 HTML（白名单、占位、文件地址按 OAD_PUBLIC_URL 拼完整）、`/embed/content.css`（不登录、ETag、缓存）、
//! `/auth/config` 里给网页的 `allowed_origins`。
mod common;
use common::*;
use oad_core::AuthConfig;
use oad_server::ServerConfig;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn content_html() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let md = "# 标题\n\n正文 **加粗** <script>alert(1)</script>\n\n[坏链接](javascript:alert(1))\n\n$$\nE = mc^2\n$$\n\n![图](/api/v1/files/abc123/a.png)\n\n第二段\n\n第三段";
    let id = page_with(&s, &c, "HTML", md).await;

    let (st, v) = json(c.get(s.api(&format!("/pages/{id}/content?format=html")))).await;
    assert_eq!(st, 200, "{v}");
    let html = v["html"].as_str().unwrap();
    assert!(
        html.starts_with("<div class=\"oad-content\"><h1>标题</h1>"),
        "{html}"
    );
    assert!(html.contains("<strong>加粗</strong>"), "{html}");
    assert!(!html.contains("<script"), "{html}");
    assert!(html.contains("&lt;script&gt;"), "{html}");
    assert!(!html.contains("javascript:alert(1)\""), "{html}");
    assert!(html.contains("坏链接"), "{html}");
    assert!(
        html.contains("<div class=\"oad-math-block\" data-latex=\"E = mc^2\">E = mc^2</div>"),
        "{html}"
    );
    // 没设 OAD_PUBLIC_URL：文件是相对地址
    assert!(
        html.contains("<img src=\"/api/v1/files/abc123/a.png\""),
        "{html}"
    );
    assert!(v["version"].as_str().is_some_and(|x| !x.is_empty()), "{v}");
    assert!(v.get("markdown").is_none() && v.get("doc").is_none(), "{v}");
    assert!(v.get("total").is_none(), "{v}");

    // 和 Markdown 版同一个版本
    let (_, m) = json(c.get(s.api(&format!("/pages/{id}/content")))).await;
    assert_eq!(m["version"], v["version"]);
    assert!(m.get("html").is_none(), "{m}");

    // limit 照常
    let (st, v) = json(c.get(s.api(&format!("/pages/{id}/content?format=html&limit=2")))).await;
    assert_eq!(st, 200, "{v}");
    assert!(v["total"].as_u64().unwrap() >= 6, "{v}");
    let html = v["html"].as_str().unwrap();
    assert!(
        html.contains("<h1>标题</h1>") && html.contains("加粗"),
        "{html}"
    );
    assert!(!html.contains("第三段"), "{html}");

    // 不认识的 format
    let (st, _) = json(c.get(s.api(&format!("/pages/{id}/content?format=pdf")))).await;
    assert_eq!(st, 400);
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn html_uses_public_url_and_needs_login() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = ServerConfig {
        public_url: Some("https://docs.example.com/".into()),
        allowed_origins: vec![
            "https://app.example.com/".into(),
            "HTTPS://Other.example.com".into(),
            "https://app.example.com".into(),
        ],
        allowed_hosts: vec!["*".into()],
        ..ServerConfig::default()
    };
    let s = Server::start_with(dir.path(), AuthConfig::None, fast_hub(), cfg).await;
    let c = http();
    let id = page_with(
        &s,
        &c,
        "文件",
        "![图](/api/v1/files/abc123/a.png)\n\n[另一页](/p/xyz)",
    )
    .await;
    let (_, v) = json(c.get(s.api(&format!("/pages/{id}/content?format=html")))).await;
    let html = v["html"].as_str().unwrap();
    assert!(
        html.contains("src=\"https://docs.example.com/api/v1/files/abc123/a.png\""),
        "{html}"
    );
    assert!(
        html.contains("href=\"https://docs.example.com/p/xyz\""),
        "{html}"
    );

    // 允许嵌入的 origin：去重、规范化，加上 public_url 的
    let (_, conf) = json(c.get(s.api("/auth/config"))).await;
    assert_eq!(
        conf["allowed_origins"],
        serde_json::json!([
            "https://app.example.com",
            "https://other.example.com",
            "https://docs.example.com"
        ])
    );
    s.stop().await;

    // 要登录的模式下，HTML 和别的格式一样要登录
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::Local, fast_hub()).await;
    let (st, _) = json(c.get(s.api("/pages/nope/content?format=html"))).await;
    assert_eq!(st, 401);
    let (_, conf) = json(c.get(s.api("/auth/config"))).await;
    assert_eq!(conf["allowed_origins"], serde_json::json!([]));
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn content_css() {
    let dir = tempfile::tempdir().unwrap();
    // 要登录的模式下也不用登录
    let s = Server::start(dir.path(), AuthConfig::Local, fast_hub()).await;
    let c = http();
    let url = format!("http://{}/embed/content.css", s.addr);
    let r = c.get(&url).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 200);
    let h = r.headers().clone();
    assert_eq!(h["content-type"], "text/css; charset=utf-8");
    assert!(h["cache-control"].to_str().unwrap().contains("max-age="));
    assert_eq!(h["access-control-allow-origin"], "*");
    let etag = h["etag"].to_str().unwrap().to_string();
    assert_eq!(r.text().await.unwrap(), oad_markdown::CONTENT_CSS);

    // 带 ETag 再问：304
    let r = c
        .get(&url)
        .header("if-none-match", &etag)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 304);
    assert_eq!(r.headers()["etag"].to_str().unwrap(), etag);
    assert!(r.bytes().await.unwrap().is_empty());

    // 要压缩的给压缩（reqwest 会自己解压、去掉头，用原始的请求看）
    let (st, hs, body) =
        raw_get(s.addr, "/embed/content.css", &[("Accept-Encoding", "gzip")]).await;
    assert_eq!(st, 200);
    assert!(
        hs.iter()
            .any(|(k, v)| k == "content-encoding" && v == "gzip"),
        "{hs:?}"
    );
    assert!(body.len() < oad_markdown::CONTENT_CSS.len());

    // /embed 下别的路径不是它（交给网页）
    let r = c
        .get(format!("http://{}/embed/nope.css", s.addr))
        .send()
        .await
        .unwrap();
    assert_ne!(
        r.headers()
            .get("etag")
            .map(|e| e.to_str().unwrap().to_string()),
        Some(etag)
    );
    s.stop().await;
}
