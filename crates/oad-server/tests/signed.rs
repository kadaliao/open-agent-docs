//! 跨域嵌入：`/api/v1` 的 CORS（只对允许的 Origin 回，预检、SSE、429 也带），文件的签名地址
//! （`POST /files/sign`、`?exp=&sig=` 不用登录、过期 / 不对 403、只签看得到的、`format=html` 里直接是签名地址、密钥存在存储里）。
mod common;
use common::*;
use oad_core::{AuthConfig, MediaConfig, NotifyConfig};
use oad_server::ServerConfig;
use serde_json::{Value, json};

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(w, h, image::Rgb([200, 100, 50]))
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

async fn start(dir: &std::path::Path, cfg: ServerConfig) -> Server {
    Server::start_media(
        dir,
        AuthConfig::Header {
            secret: "sec".into(),
        },
        fast_hub(),
        cfg,
        NotifyConfig::default(),
        MediaConfig {
            blobs: std::sync::Arc::new(oad_core::FsBlobs::new(dir.join("blobs")).unwrap()),
            ..MediaConfig::default()
        },
    )
    .await
}

fn as_user(r: reqwest::RequestBuilder, id: &str) -> reqwest::RequestBuilder {
    r.header("X-OAD-Secret", "sec").header("X-OAD-User-Id", id)
}

async fn upload(s: &Server, c: &reqwest::Client, ws: &str, user: &str, name: &str) -> Value {
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(png(400, 300)).file_name(name.to_string()),
    );
    let (st, f) =
        json(as_user(c.post(s.api(&format!("/workspaces/{ws}/files"))), user).multipart(form))
            .await;
    assert_eq!(st, 201, "{f}");
    f
}

fn now_ms() -> i64 {
    oad_core::ids::now_ms()
}

/// HTML 属性里的 `&amp;` 还原。
fn unescape(s: &str) -> String {
    s.replace("&amp;", "&")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signed_urls() {
    let dir = tempfile::tempdir().unwrap();
    let s = start(dir.path(), ServerConfig::default()).await;
    let c = http();
    json(as_user(c.get(s.api("/me")), "alice")).await;
    let ws = s.ws_id();
    let mine = upload(&s, &c, &ws, "alice", "图 1.png").await;
    let id = mine["id"].as_str().unwrap().to_string();
    let url = mine["url"].as_str().unwrap().to_string();

    // bob 的文件在别的工作区，alice 看不到
    let other = s.core.storage.create_workspace("别人的").await.unwrap();
    let (_, bob) = json(as_user(c.get(s.api("/me")), "bob")).await;
    s.core
        .storage
        .set_member(
            &other.id,
            bob["user"]["id"].as_str().unwrap(),
            oad_core::Role::Owner,
        )
        .await
        .unwrap();
    // bob 自动进了默认工作区，挪出去
    s.core
        .storage
        .remove_member(&ws, bob["user"]["id"].as_str().unwrap())
        .await
        .unwrap();
    let theirs = upload(&s, &c, &other.id, "bob", "x.png").await;
    let theirs_url = theirs["url"].as_str().unwrap().to_string();

    // 转存的图片
    let b = s.core.media.store(png(8, 8), "a.png").await.unwrap();
    s.core
        .storage
        .put_media(&b.hash, "https://example.com/a.png")
        .await
        .unwrap();
    let media_url = format!("/api/v1/media/{}", b.hash);

    // 签名：要登录
    let (st, _) = json(c.post(s.api("/files/sign")).json(&json!({ "urls": [url] }))).await;
    assert_eq!(st, 401);
    let before = now_ms();
    let (st, v) = json(
        as_user(c.post(s.api("/files/sign")), "alice").json(&json!({ "urls": [
            url,
            id,
            format!("{url}?w=320"),
            theirs_url,
            "https://example.com/a.png",
            media_url,
            "/api/v1/files/nonexist0000/x.png",
            format!("http://{}{url}", s.addr),
        ] })),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    let urls = v["urls"].as_array().unwrap();
    assert_eq!(urls.len(), 8);
    let signed = urls[0].as_str().unwrap();
    assert!(
        signed.starts_with(&format!("{url}?exp=")) && signed.contains("&sig="),
        "{signed}"
    );
    assert!(
        urls[1]
            .as_str()
            .unwrap()
            .starts_with(&format!("/api/v1/files/{id}?exp=")),
        "{v}"
    );
    assert!(
        urls[2]
            .as_str()
            .unwrap()
            .starts_with(&format!("{url}?w=320&exp=")),
        "{v}"
    );
    assert!(urls[3].is_null(), "看不到的不签：{v}");
    assert!(urls[4].is_null(), "别的网站的不签：{v}");
    assert!(urls[5].as_str().unwrap().starts_with(&media_url), "{v}");
    assert!(urls[6].is_null(), "不存在的不签：{v}");
    assert!(
        urls[7]
            .as_str()
            .unwrap()
            .starts_with(&format!("http://{}{url}?exp=", s.addr)),
        "{v}"
    );
    // 默认 24 小时，往后凑整（最多一小时）
    let exp = v["expires_at"].as_i64().unwrap();
    assert!(
        exp >= before + 86_400_000 && exp <= now_ms() + 90_000_000,
        "{exp}"
    );

    // 带签名的地址不用登录；缓存到过期为止
    let full = |u: &str| format!("http://{}{u}", s.addr);
    let r = c.get(full(signed)).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let cc = r.headers()["cache-control"].to_str().unwrap().to_string();
    let age: i64 = cc
        .strip_prefix("private, max-age=")
        .unwrap()
        .parse()
        .unwrap();
    assert!((86_000..=90_000).contains(&age), "{cc}");
    assert_eq!(
        r.bytes().await.unwrap().len() as i64,
        mine["size"].as_i64().unwrap()
    );
    // 自己加 w=：缩略图，签名照样认
    let r = c.get(full(&format!("{signed}&w=64"))).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["content-type"], "image/webp");
    // 不带文件名的地址、媒体
    let r = c.get(full(urls[1].as_str().unwrap())).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let r = c.get(full(urls[5].as_str().unwrap())).send().await.unwrap();
    assert_eq!(r.status(), 200);

    // 不对的签名：403
    let q = signed.split_once('?').unwrap().1;
    let r = c
        .get(full(&format!("{theirs_url}?{q}")))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403, "别的文件的签名不认");
    let (st, e) = json(c.get(full(&format!("{signed}x")))).await;
    assert_eq!(
        (st, e["error"]["code"].as_str()),
        (403, Some("bad_signature"))
    );
    let exp_s = q.split('&').next().unwrap().trim_start_matches("exp=");
    let later: i64 = exp_s.parse::<i64>().unwrap() + 1;
    let tampered = signed.replace(&format!("exp={exp_s}"), &format!("exp={later}"));
    let (st, _) = json(c.get(full(&tampered))).await;
    assert_eq!(st, 403, "改了过期时间");
    // 过期了：403；登录着、看得到的照样给
    let past = oad_core::signed::now_secs() - 5;
    let old = format!("{url}?{}", s.core.signer.query(&id, past));
    let (st, e) = json(c.get(full(&old))).await;
    assert_eq!(
        (st, e["error"]["code"].as_str()),
        (403, Some("signature_expired"))
    );
    let r = as_user(c.get(full(&old)), "alice").send().await.unwrap();
    assert_eq!(r.status(), 200);
    let r = as_user(c.get(full(&old)), "bob").send().await.unwrap();
    assert_eq!(r.status(), 404, "bob 看不到 alice 的工作区");
    // 什么都没带：照旧 401
    let r = c.get(full(&url)).send().await.unwrap();
    assert_eq!(r.status(), 401);

    // format=html：看得到的文件直接是签名地址，带过期时间；看不到的不签
    let md = format!("![图]({url})\n\n![别人的]({theirs_url})\n\n[下载]({url})");
    let (st, p) = json(
        as_user(c.post(s.api(&format!("/workspaces/{ws}/pages"))), "alice")
            .json(&json!({ "title": "有图", "markdown": md })),
    )
    .await;
    assert_eq!(st, 201, "{p}");
    let pid = p["id"].as_str().unwrap();
    let r = as_user(c.put(s.api(&format!("/pages/{pid}/content"))), "alice")
        .json(&json!({ "markdown": md }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 204);
    let (st, v) = json(as_user(
        c.get(s.api(&format!("/pages/{pid}/content?format=html"))),
        "alice",
    ))
    .await;
    assert_eq!(st, 200, "{v}");
    let html = v["html"].as_str().unwrap();
    let src = html
        .split("<img src=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let src = unescape(src);
    assert!(
        src.starts_with(&format!("{url}?exp=")) && src.contains("&sig="),
        "{html}"
    );
    assert!(html.contains(&format!("src=\"{theirs_url}\"")), "{html}");
    assert!(
        html.contains(&format!("href=\"{}", src.replace('&', "&amp;"))),
        "{html}"
    );
    let expire = v["assets_expire_at"].as_i64().unwrap();
    assert!(expire > now_ms() + 86_000_000, "{v}");
    let r = c.get(full(&src)).send().await.unwrap();
    assert_eq!(r.status(), 200, "HTML 里的地址不登录也打得开");
    // 没有文件：null；别的格式没有这个字段
    let (_, p2) = json(
        as_user(c.post(s.api(&format!("/workspaces/{ws}/pages"))), "alice")
            .json(&json!({ "title": "纯文字", "markdown": "正文" })),
    )
    .await;
    let (_, v) = json(as_user(
        c.get(s.api(&format!(
            "/pages/{}/content?format=html",
            p2["id"].as_str().unwrap()
        ))),
        "alice",
    ))
    .await;
    assert!(
        v["assets_expire_at"].is_null() && v.get("assets_expire_at").is_some(),
        "{v}"
    );
    let (_, v) = json(as_user(
        c.get(s.api(&format!("/pages/{pid}/content"))),
        "alice",
    ))
    .await;
    assert!(v.get("assets_expire_at").is_none(), "{v}");

    // 一次最多 500 个
    let many: Vec<String> = (0..501).map(|_| id.clone()).collect();
    let (st, _) =
        json(as_user(c.post(s.api("/files/sign")), "alice").json(&json!({ "urls": many }))).await;
    assert_eq!(st, 400);

    // 密钥存在存储里：重启后旧的签名地址还认
    s.stop().await;
    let s = start(dir.path(), ServerConfig::default()).await;
    let r = c
        .get(format!("http://{}{signed}", s.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "重启后密钥不变");
    s.stop().await;

    // 另一份数据（多租户时每个租户一份）：另一把密钥
    let dir2 = tempfile::tempdir().unwrap();
    let s2 = start(dir2.path(), ServerConfig::default()).await;
    assert_ne!(
        s2.core.signer.sign(&id, 2_000_000_000),
        oad_core::signed::UrlSigner::load(open_storage(dir.path()).await.as_ref())
            .await
            .unwrap()
            .sign(&id, 2_000_000_000)
    );
    s2.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signed_url_ttl() {
    let dir = tempfile::tempdir().unwrap();
    let s = start(
        dir.path(),
        ServerConfig {
            signed_url_ttl: 60,
            ..ServerConfig::default()
        },
    )
    .await;
    let c = http();
    json(as_user(c.get(s.api("/me")), "alice")).await;
    let f = upload(&s, &c, &s.ws_id(), "alice", "a.png").await;
    let (_, v) =
        json(as_user(c.post(s.api("/files/sign")), "alice").json(&json!({ "urls": [f["id"]] })))
            .await;
    let left = v["expires_at"].as_i64().unwrap() - now_ms();
    assert!((55_000..=65_000).contains(&left), "{v}");
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cors() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = ServerConfig {
        allowed_origins: vec!["https://app.example.com".into()],
        public_url: Some("https://docs.example.com/".into()),
        allowed_hosts: vec!["*".into()],
        rate_limit: 1000,
        ..ServerConfig::default()
    };
    let s = Server::start_with(dir.path(), AuthConfig::None, fast_hub(), cfg).await;
    let c = http();
    let app = "https://app.example.com";

    // 预检
    let r = c
        .request(reqwest::Method::OPTIONS, s.api("/pages/x/content"))
        .header("origin", app)
        .header("access-control-request-method", "PUT")
        .header(
            "access-control-request-headers",
            "authorization, content-type, last-event-id, accept-language, x-filename",
        )
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success(), "{}", r.status());
    let h = r.headers();
    assert_eq!(h["access-control-allow-origin"], app);
    assert_eq!(h["access-control-allow-credentials"], "true");
    let allow = h["access-control-allow-headers"]
        .to_str()
        .unwrap()
        .to_lowercase();
    for want in [
        "authorization",
        "content-type",
        "last-event-id",
        "accept-language",
        "x-filename",
    ] {
        assert!(allow.contains(want), "{allow}");
    }
    assert!(
        h["access-control-allow-methods"]
            .to_str()
            .unwrap()
            .contains("PUT")
    );
    assert!(h.contains_key("access-control-max-age"));

    // 普通请求：回 CORS 头，暴露限流、ETag 的头，Vary: Origin
    let r = c
        .get(s.api("/me"))
        .header("origin", app)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let h = r.headers();
    assert_eq!(h["access-control-allow-origin"], app);
    let expose = h["access-control-expose-headers"]
        .to_str()
        .unwrap()
        .to_lowercase();
    for want in [
        "ratelimit-remaining",
        "ratelimit-reset",
        "retry-after",
        "etag",
    ] {
        assert!(expose.contains(want), "{expose}");
    }
    let vary: Vec<String> = h
        .get_all("vary")
        .iter()
        .map(|v| v.to_str().unwrap().to_lowercase())
        .collect();
    assert!(vary.iter().any(|v| v.contains("origin")), "{vary:?}");
    // OAD_PUBLIC_URL 的 origin 也算；大小写、末尾的 / 不计较
    let r = c
        .get(s.api("/me"))
        .header("origin", "https://Docs.example.com")
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.headers()["access-control-allow-origin"],
        "https://Docs.example.com"
    );

    // 不在列表里的：没有 CORS 头（浏览器拦下），预检也不给
    let r = c
        .get(s.api("/me"))
        .header("origin", "https://evil.example.com")
        .send()
        .await
        .unwrap();
    assert!(!r.headers().contains_key("access-control-allow-origin"));
    let r = c
        .request(reqwest::Method::OPTIONS, s.api("/me"))
        .header("origin", "https://evil.example.com")
        .header("access-control-request-method", "GET")
        .send()
        .await
        .unwrap();
    assert!(!r.headers().contains_key("access-control-allow-origin"));
    // 写请求：允许的过，不允许的 403（和 WebSocket 一样的检查）
    let (st, _) = json(
        c.post(s.api(&format!("/workspaces/{}/pages", s.ws_id())))
            .header("origin", app)
            .json(&json!({ "title": "跨域建的" })),
    )
    .await;
    assert_eq!(st, 201);
    let (st, _) = json(
        c.post(s.api(&format!("/workspaces/{}/pages", s.ws_id())))
            .header("origin", "https://evil.example.com")
            .json(&json!({ "title": "x" })),
    )
    .await;
    assert_eq!(st, 403);

    // 错误响应也带（别的域名的页面才读得到 404 的内容）
    let r = c
        .get(s.api("/pages/nope"))
        .header("origin", app)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
    assert_eq!(r.headers()["access-control-allow-origin"], app);

    // SSE
    let r = c
        .get(s.api(&format!("/workspaces/{}/events", s.ws_id())))
        .header("origin", app)
        .header("accept", "text/event-stream")
        .header("last-event-id", "0")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["access-control-allow-origin"], app);
    assert!(
        r.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    drop(r);
    s.stop().await;

    // 限流的 429 也带 CORS 头和 Retry-After
    let dir = tempfile::tempdir().unwrap();
    let cfg = ServerConfig {
        allowed_origins: vec![app.into()],
        rate_limit: 1,
        ..ServerConfig::default()
    };
    let s = Server::start_with(dir.path(), AuthConfig::None, fast_hub(), cfg).await;
    let _ = c
        .get(s.api("/me"))
        .header("origin", app)
        .send()
        .await
        .unwrap();
    let r = c
        .get(s.api("/me"))
        .header("origin", app)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 429);
    assert_eq!(r.headers()["access-control-allow-origin"], app);
    assert!(r.headers().contains_key("retry-after"));
    s.stop().await;
}
