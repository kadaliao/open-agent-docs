//! 宿主网站：自己做登录，后端签 JWT，把文档用 iframe 嵌进页面（跨域也行）。
//!
//! Open Agent Docs 那边：`OAD_AUTH=jwt OAD_JWT_SECRET=<同一个密钥> OAD_ALLOWED_ORIGINS=http://127.0.0.1:3000`。
//!
//!     OAD_BASE_URL=http://127.0.0.1:8848 OAD_JWT_SECRET=… cargo run --example embed_host
//!
//! 打开 http://127.0.0.1:3000 。token 放在 iframe 地址的 `#token=` 后面（不进服务器日志）；
//! 快过期时 iframe 发 `oad:auth-required`，这里从 `/token` 换一个新的回 `oad:token`。
use axum::Router;
use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use oad::types::CreatePage;
use open_agent_docs as oad;
use std::sync::Arc;
use std::time::Duration;

struct App {
    base_url: String,
    secret: String,
    page_id: String,
}

/// 宿主自己的登录：这里假装当前用户是 u-42
fn current_user() -> oad::User {
    oad::User::new("u-42").name("王小明").email("x@example.com")
}

fn token(app: &App) -> String {
    let u = current_user();
    oad::sign_jwt(
        &app.secret,
        &u.id,
        &oad::JwtOptions {
            name: u.name,
            email: u.email,
            ttl: Duration::from_secs(600),
            ..Default::default()
        },
    )
}

async fn index(State(app): State<Arc<App>>) -> Html<String> {
    let src = oad::embed_url(
        &app.base_url,
        &app.page_id,
        &oad::EmbedOptions {
            theme: Some("light".into()),
            lang: Some("zh".into()),
            token: Some(token(&app)),
        },
    );
    let origin = app.base_url.trim_end_matches('/');
    Html(format!(
        r#"<!doctype html>
<meta charset="utf-8">
<title>嵌入 Open Agent Docs</title>
<h1>宿主页面</h1>
<iframe id="doc" src="{src}" style="width:100%;height:80vh;border:1px solid #ddd"></iframe>
<script>
  const frame = document.getElementById('doc')
  addEventListener('message', async (e) => {{
    if (e.origin !== '{origin}' || e.data?.type !== 'oad:auth-required') return
    const token = await (await fetch('/token')).text()
    frame.contentWindow.postMessage({{ type: 'oad:token', token }}, '{origin}')
  }})
</script>"#
    ))
}

async fn refresh(State(app): State<Arc<App>>) -> String {
    token(&app)
}

#[tokio::main]
async fn main() -> oad::Result<()> {
    let base_url = std::env::var("OAD_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8848".into());
    let secret = std::env::var("OAD_JWT_SECRET").expect("设 OAD_JWT_SECRET（和服务器的一样）");
    // 要嵌的页面：给了 OAD_PAGE_ID 就用它，没给就以当前用户的身份建一页
    let page_id = match std::env::var("OAD_PAGE_ID") {
        Ok(id) => id,
        Err(_) => {
            let admin = oad::Client::builder()
                .base_url(&base_url)
                .jwt_secret(&secret)
                .build()?;
            let u = admin.with_user(current_user())?;
            let ws = u.default_workspace().await?;
            u.pages()
                .create_with_markdown(
                    &ws,
                    CreatePage {
                        title: "嵌入的文档".into(),
                        ..Default::default()
                    },
                    "# 你好\n\n在宿主页面里编辑我。",
                )
                .await?
                .id
        }
    };
    let app = Arc::new(App {
        base_url,
        secret,
        page_id,
    });
    let router = Router::new()
        .route("/", get(index))
        .route("/token", get(refresh))
        .with_state(app);
    let l = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("打开 http://{}", l.local_addr().unwrap());
    axum::serve(l, router).await.unwrap();
    Ok(())
}
