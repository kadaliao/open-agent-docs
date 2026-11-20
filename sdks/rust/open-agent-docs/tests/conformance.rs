//! 统一场景 S1–S15（docs/sdk-contract.md §14），对着真的服务器跑。
//!
//!     OAD_BINARY=/path/to/open-agent-docs cargo test --all-features --test conformance
//!
//! 没设 `OAD_BINARY` 时要服务器的场景直接返回（打印 skip）；S10、S11、S14 不用服务器，照常跑。
//! S1 要 feature `launcher`（`--all-features`），没开时 skip。每个场景自己一个临时数据目录、一个服务器。
mod common;

use futures_util::{StreamExt, TryStreamExt};
use oad::types::{
    Ack, CreateAgent, CreatePage, NewThread, PatchPage, PrincipalKind, PutContent, SaveBody,
};
use open_agent_docs as oad;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const SECRET: &str = "conformance-header-secret";

fn binary() -> Option<PathBuf> {
    std::env::var_os("OAD_BINARY")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

macro_rules! need_binary {
    ($name:expr) => {
        match binary() {
            Some(b) => b,
            None => {
                eprintln!("skip {}：没设 OAD_BINARY", $name);
                return;
            }
        }
    };
}

/// 临时数据目录（drop 时删掉）
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "oad-rs-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 测试自己拉起的服务器（S1 以外的场景用；S1 测的是 oad::start）
struct Srv {
    url: String,
    child: tokio::process::Child,
    _dir: TempDir,
}

impl Srv {
    async fn start(bin: &PathBuf, env: &[(&str, &str)]) -> Srv {
        let dir = TempDir::new("conf");
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut cmd = tokio::process::Command::new(bin);
        cmd.arg("serve")
            .env("OAD_PORT", port.to_string())
            .env("OAD_HOST", "127.0.0.1")
            .env("OAD_DATA_DIR", &dir.0)
            .env("OAD_RATE_LIMIT", "0")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("跑不了 OAD_BINARY");
        let url = format!("http://127.0.0.1:{port}");
        let http = oad::reqwest::Client::builder().no_proxy().build().unwrap();
        for _ in 0..300 {
            if let Ok(r) = http.get(format!("{url}/api/v1/auth/config")).send().await {
                if r.status().is_success() {
                    return Srv {
                        url,
                        child,
                        _dir: dir,
                    };
                }
            }
            if let Ok(Some(st)) = child.try_wait() {
                panic!("服务器退出了：{st}");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("服务器 30 秒内没起来");
    }

    /// header 模式的服务器
    async fn header(bin: &PathBuf) -> Srv {
        Srv::start(
            bin,
            &[("OAD_AUTH", "header"), ("OAD_HEADER_SECRET", SECRET)],
        )
        .await
    }

    fn builder(&self) -> oad::ClientBuilder {
        oad::Client::builder()
            .base_url(&self.url)
            .http_client(oad::reqwest::Client::builder().no_proxy().build().unwrap())
    }

    /// 以用户 u1 的身份
    fn user(&self, id: &str) -> oad::Client {
        self.builder()
            .header_secret(SECRET)
            .build()
            .unwrap()
            .with_user(oad::User::new(id).name("测试用户"))
            .unwrap()
    }
}

impl Drop for Srv {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// S1：start(data_dir, auth=header, header_secret) → auth.config().mode == header；关了以后端口连不上
#[cfg(feature = "launcher")]
#[tokio::test]
async fn s1_start() {
    let _ = need_binary!("S1");
    let dir = TempDir::new("s1");
    let server = oad::start(
        oad::StartOptions::new()
            .data_dir(&dir.0)
            .auth("header")
            .header_secret(SECRET)
            .set("rate_limit", "0"),
    )
    .await
    .unwrap();
    let c = server
        .client_builder()
        .header_secret(SECRET)
        .http_client(oad::reqwest::Client::builder().no_proxy().build().unwrap())
        .build()
        .unwrap();
    let cfg = c.auth().config().await.unwrap();
    assert_eq!(cfg.mode, oad::types::AuthMode::Header);
    let port = server.port;
    server.shutdown().await.unwrap();
    assert!(
        tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_err(),
        "关了以后端口还能连"
    );
}

#[cfg(not(feature = "launcher"))]
#[test]
fn s1_start() {
    eprintln!("skip S1：要 feature launcher（cargo test --all-features --test conformance）");
}

/// S2：with_user("u1", name="王小明") → me.get().user.name；default_workspace 是 /me 的第一个
#[tokio::test]
async fn s2_with_user() {
    let bin = need_binary!("S2");
    let s = Srv::header(&bin).await;
    let admin = s.builder().header_secret(SECRET).build().unwrap();
    let u = admin
        .with_user(oad::User::new("u1").name("王小明"))
        .unwrap();
    let me = u.me().get().await.unwrap();
    assert_eq!(me.user.name, "王小明");
    assert_eq!(u.default_workspace().await.unwrap(), me.workspaces[0].id);
}

/// S3：建页面写 Markdown、读、列表、改标题、按 base 替换正文、删
#[tokio::test]
async fn s3_pages() {
    let bin = need_binary!("S3");
    let s = Srv::header(&bin).await;
    let u = s.user("u3");
    let ws = u.default_workspace().await.unwrap();
    let page = u
        .pages()
        .create_with_markdown(
            &ws,
            CreatePage {
                title: "周报".into(),
                ..Default::default()
            },
            "# 标题\n\n正文",
        )
        .await
        .unwrap();
    let content = u.pages().content().get(&page.id).await.unwrap();
    let md = content.markdown.clone().unwrap_or_default();
    assert!(md.contains("# 标题") && md.contains("正文"), "{md}");
    assert!(
        u.pages()
            .list(&ws)
            .await
            .unwrap()
            .iter()
            .any(|p| p.id == page.id)
    );
    let p = u
        .pages()
        .update(
            &page.id,
            PatchPage {
                title: "周报（改）".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(p.title, "周报（改）");
    u.pages()
        .content()
        .replace(
            &page.id,
            PutContent {
                markdown: "# 新的\n\n第二版".into(),
                base: content.version.clone().into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let md = u
        .pages()
        .content()
        .get(&page.id)
        .await
        .unwrap()
        .markdown
        .unwrap_or_default();
    assert!(md.contains("第二版"), "{md}");
    u.pages().delete(&page.id).await.unwrap();
    let e = u.pages().get(&page.id).await.unwrap_err();
    assert!(e.is_not_found(), "{e:?}");
}

/// S4：错误
#[tokio::test]
async fn s4_errors() {
    let bin = need_binary!("S4");
    let s = Srv::header(&bin).await;
    let u = s.user("u4");
    let e = u.pages().get("nope").await.unwrap_err();
    assert!(e.is_not_found());
    let api = e.api().unwrap();
    assert_eq!((api.status, api.code.as_str()), (404, "not_found"));
    assert!(!api.message.is_empty());
    let bogus = s.builder().token("oad_bogus").build().unwrap();
    let e = bogus.me().get().await.unwrap_err();
    assert!(e.is_unauthorized(), "{e:?}");
}

/// S5：建 Agent，拿它的 token 调 → kind == agent
#[tokio::test]
async fn s5_agent_token() {
    let bin = need_binary!("S5");
    let s = Srv::header(&bin).await;
    let u = s.user("u5");
    let ws = u.default_workspace().await.unwrap();
    let created = u
        .agents()
        .create(
            &ws,
            CreateAgent {
                name: "bot".into(),
                scope: "write".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let bot = s.builder().token(&created.token).build().unwrap();
    assert_eq!(
        bot.me().get().await.unwrap().user.kind,
        PrincipalKind::Agent
    );
}

/// S6：存 5 个版本，limit=2 逐条取：≥ 5 条、id 不重复、确实翻了页
#[tokio::test]
async fn s6_pagination() {
    let bin = need_binary!("S6");
    let s = Srv::header(&bin).await;
    let u = s.user("u6");
    let ws = u.default_workspace().await.unwrap();
    let page = u
        .pages()
        .create(
            &ws,
            CreatePage {
                title: "版本".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    for i in 0..5 {
        u.pages()
            .content()
            .replace(
                &page.id,
                PutContent {
                    markdown: format!("第 {i} 版").into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        u.pages()
            .versions()
            .create(
                &page.id,
                SaveBody {
                    label: format!("v{i}").into(),
                },
            )
            .await
            .unwrap();
    }
    let all: Vec<_> = u
        .pages()
        .versions()
        .list_all(&page.id)
        .limit(2)
        .try_collect()
        .await
        .unwrap();
    assert!(all.len() >= 5, "{}", all.len());
    let ids: HashSet<i64> = all.iter().map(|v| v.id).collect();
    assert_eq!(ids.len(), all.len(), "id 重复了");
    // 一页最多 2 条，≥ 5 条就是 ≥ 3 页；再手动数一遍请求
    let mut pages = 0;
    let mut cursor: Option<String> = None;
    loop {
        let mut req = u.pages().versions().list(&page.id).limit(2);
        if let Some(c) = &cursor {
            req = req.cursor(c);
        }
        let p = req.await.unwrap();
        pages += 1;
        assert!(p.versions.len() <= 2);
        match p.next_cursor {
            Some(c) if !p.versions.is_empty() && Some(&c) != cursor.as_ref() => cursor = Some(c),
            _ => break,
        }
    }
    assert!(pages >= 3, "只翻了 {pages} 页");
}

/// S7：开工作区事件，再建页面 → 10 秒内收到 page.created，data 是 Page
#[tokio::test]
async fn s7_events() {
    let bin = need_binary!("S7");
    let s = Srv::header(&bin).await;
    let u = s.user("u7");
    let ws = u.default_workspace().await.unwrap();
    let mut events = u.workspaces().events(&ws).await.unwrap();
    let page = u
        .pages()
        .create(
            &ws,
            CreatePage {
                title: "事件".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let got = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(ev) = events.next().await {
            if let oad::types::WorkspaceEvent::PageCreated(p) = ev.unwrap().data {
                if p.id == page.id {
                    return p;
                }
            }
        }
        panic!("事件流结束了");
    })
    .await
    .expect("10 秒内没收到 page.created");
    assert_eq!(got.title, "事件");
}

/// S8：用户评论 @bot，Agent 的通知流收到；ack 后 list() 不再有它
#[tokio::test]
async fn s8_agent_notifications() {
    let bin = need_binary!("S8");
    let s = Srv::header(&bin).await;
    let u = s.user("u8");
    let ws = u.default_workspace().await.unwrap();
    let created = u
        .agents()
        .create(
            &ws,
            CreateAgent {
                name: "bot".into(),
                scope: "write".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let bot = s.builder().token(&created.token).build().unwrap();
    let page = u
        .pages()
        .create_with_markdown(
            &ws,
            CreatePage {
                title: "评论".into(),
                ..Default::default()
            },
            "一段话",
        )
        .await
        .unwrap();
    let mut stream = bot.agent().notifications().stream().await.unwrap();
    u.threads()
        .create(
            &page.id,
            NewThread {
                body: "@bot 看看".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let ev = tokio::time::timeout(Duration::from_secs(10), stream.next())
        .await
        .expect("10 秒内没收到通知")
        .expect("流结束了")
        .unwrap();
    let note = ev.data;
    assert!(note.comment.body.contains("看看"), "{}", note.comment.body);
    assert_eq!(ev.id, Some(note.id.to_string()));
    drop(stream);
    let acked = bot
        .agent()
        .notifications()
        .ack(Ack {
            ids: Some(vec![note.id]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(acked.acked, 1);
    let left = bot.agent().notifications().list().await.unwrap();
    assert!(left.notifications.iter().all(|n| n.id != note.id));
}

/// S9：JWT 模式：jwt_secret + with_user 能用；别的密钥签的 → 401
#[tokio::test]
async fn s9_jwt() {
    let bin = need_binary!("S9");
    let secret = "conformance-jwt-secret-0123456789";
    let s = Srv::start(&bin, &[("OAD_AUTH", "jwt"), ("OAD_JWT_SECRET", secret)]).await;
    let u = s
        .builder()
        .jwt_secret(secret)
        .build()
        .unwrap()
        .with_user(oad::User::new("u9").name("小九"))
        .unwrap();
    assert_eq!(u.me().get().await.unwrap().user.name, "小九");
    let wrong = s
        .builder()
        .jwt_secret("another-secret-0123456789")
        .build()
        .unwrap()
        .with_user("u9")
        .unwrap();
    let e = wrong.me().get().await.unwrap_err();
    assert!(e.is_unauthorized(), "{e:?}");
}

// S10、S11 在 tests/vectors.rs（不用服务器）；这里再跑一遍，统一场景一条命令就能看全。
#[test]
fn s10_s11_vectors() {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../conformance/vectors.json"
    );
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    for c in v["webhook"].as_array().unwrap() {
        let headers: std::collections::HashMap<String, String> =
            serde_json::from_value(c["headers"].clone()).unwrap();
        let now = std::time::UNIX_EPOCH + Duration::from_millis(c["now_ms"].as_u64().unwrap());
        let tol = Duration::from_secs(c["tolerance_s"].as_u64().unwrap());
        let r = oad::verify_webhook_signature(
            c["secret"].as_str().unwrap(),
            &headers,
            c["body"].as_str().unwrap(),
            now,
            tol,
        );
        let got = r
            .map(|_| "ok")
            .unwrap_or_else(|e| e.webhook_kind().unwrap().as_str());
        assert_eq!(got, c["expect"].as_str().unwrap(), "{}", c["name"]);
    }
    for c in v["embed_url"].as_array().unwrap() {
        let s = |k: &str| {
            c["options"]
                .get(k)
                .and_then(|x| x.as_str())
                .map(String::from)
        };
        let opts = oad::EmbedOptions {
            theme: s("theme"),
            lang: s("lang"),
            token: s("token"),
        };
        assert_eq!(
            oad::embed_url(
                c["base_url"].as_str().unwrap(),
                c["page_id"].as_str().unwrap(),
                &opts
            ),
            c["expect"].as_str().unwrap()
        );
    }
}

/// S12：分享：新页面 get 是空；enable 的 url 含 /s/；disable 后又是空
#[tokio::test]
async fn s12_share() {
    let bin = need_binary!("S12");
    let s = Srv::header(&bin).await;
    let u = s.user("u12");
    let ws = u.default_workspace().await.unwrap();
    let page = u
        .pages()
        .create(
            &ws,
            CreatePage {
                title: "分享".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(u.pages().share().get(&page.id).await.unwrap(), None);
    let share = u
        .pages()
        .share()
        .enable(&page.id, Default::default())
        .await
        .unwrap();
    assert!(share.url.contains("/s/"), "{}", share.url);
    assert!(u.pages().share().get(&page.id).await.unwrap().is_some());
    u.pages().share().disable(&page.id).await.unwrap();
    assert_eq!(u.pages().share().get(&page.id).await.unwrap(), None);
}

/// S13：content.get(format=html) 的 html 含 <h1、不含 <script
#[tokio::test]
async fn s13_html() {
    let bin = need_binary!("S13");
    let s = Srv::header(&bin).await;
    let u = s.user("u13");
    let ws = u.default_workspace().await.unwrap();
    let page = u
        .pages()
        .create_with_markdown(
            &ws,
            CreatePage {
                title: "HTML".into(),
                ..Default::default()
            },
            "# 标题\n\n<script>alert(1)</script>\n\n正文",
        )
        .await
        .unwrap();
    let html = match u.pages().content().get(&page.id).format("html").await {
        Ok(c) => c.html,
        // 旧服务器不认 format=html
        Err(e) if e.is_bad_request() => None,
        Err(e) => panic!("{e:?}"),
    };
    let Some(html) = html else {
        eprintln!("skip S13：这个服务器还不支持 format=html（W-WEB 在做，响应里没有 html 字段）");
        return;
    };
    assert!(html.contains("<h1"), "{html}");
    assert!(!html.contains("<script"), "{html}");
}

/// S14：重试（本地假服务器）：429 + Retry-After: 1 再 200 → 成功且等了 ≥ 1 秒；max_retries=0 → RateLimit，retry_after == 1
#[tokio::test]
async fn s14_retry() {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    let n = common::Counter::default();
    let app = axum::Router::new().route(
        "/api/v1/me",
        axum::routing::get({
            let n = n.clone();
            move || async move {
                if n.bump() % 2 == 0 {
                    (
                        StatusCode::TOO_MANY_REQUESTS,
                        [("retry-after", "1")],
                        "{\"error\":{\"code\":\"too_many_requests\",\"message\":\"慢点\"}}",
                    )
                        .into_response()
                } else {
                    axum::Json(common::me_json("u")).into_response()
                }
            }
        }),
    );
    let base = common::serve(app).await;
    let t = std::time::Instant::now();
    oad::Client::builder()
        .base_url(&base)
        .build()
        .unwrap()
        .me()
        .get()
        .await
        .unwrap();
    assert!(t.elapsed() >= Duration::from_secs(1));
    let e = oad::Client::builder()
        .base_url(&base)
        .max_retries(0)
        .build()
        .unwrap()
        .me()
        .get()
        .await
        .unwrap_err();
    assert!(e.is_rate_limited());
    assert_eq!(e.api().unwrap().retry_after, Some(Duration::from_secs(1)));
}

/// S15：上传 b"hello"，下载回来一样
#[tokio::test]
async fn s15_files() {
    let bin = need_binary!("S15");
    let s = Srv::header(&bin).await;
    let u = s.user("u15");
    let ws = u.default_workspace().await.unwrap();
    let up = u.files().upload(&ws, "a.txt", &b"hello"[..]).await.unwrap();
    assert_eq!(up.name, "a.txt");
    let bytes = u.files().download(&up.id).await.unwrap();
    assert_eq!(&bytes[..], b"hello");
}
