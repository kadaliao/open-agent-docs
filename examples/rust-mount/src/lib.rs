//! 把 Open Agent Docs 挂进自己的 axum 服务：自己的路由（这里是 `/` 和 `/login`）+ `oad_server::router`，
//! 登录用自己的 [`AuthProvider`]：宿主已经知道用户是谁（这里用一个演示用的 cookie `demo_user`，真实服务换成自己的会话）。
use async_trait::async_trait;
use axum::Router;
use axum::extract::Query;
use axum::http::header;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use oad_core::auth::{AuthMode, AuthProvider, AuthRequest};
use oad_core::storage::sqlite::Sqlite;
use oad_core::{
    AuthConfig, Core, Error, HubConfig, MediaConfig, NewUser, NotifyConfig, Principal,
    PrincipalKind, Storage,
};
use oad_server::{AppState, ServerConfig};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// 宿主的登录：cookie `demo_user=<名字>` 就是这个人（演示用；真实服务在这里验自己的会话）。
pub struct DemoAuth {
    storage: Arc<dyn Storage>,
    workspace_id: std::sync::OnceLock<String>,
}

#[async_trait]
impl AuthProvider for DemoAuth {
    fn mode(&self) -> AuthMode {
        AuthMode::Header
    }

    async fn authenticate(&self, req: &AuthRequest<'_>) -> oad_core::Result<Option<Principal>> {
        let Some(name) = oad_core::auth::cookie(req.headers, "demo_user").filter(|n| !n.is_empty())
        else {
            return Ok(None);
        };
        // 宿主的用户 → Open Agent Docs 的用户：按外部 id 找，没有就建，并加进默认工作区
        let ext = format!("demo:{name}");
        let user = match self.storage.user_by_external(&ext).await? {
            Some(u) => u,
            None => {
                let u = self
                    .storage
                    .create_user(NewUser {
                        name: name.to_string(),
                        email: None,
                        password_hash: None,
                        external_id: Some(ext),
                        kind: PrincipalKind::User,
                    })
                    .await?;
                let ws = self
                    .workspace_id
                    .get()
                    .ok_or_else(|| Error::Internal("还没初始化".into()))?;
                oad_core::join_default(&*self.storage, ws, &u.id).await?;
                u
            }
        };
        Ok(Some(Principal::from(&user)))
    }
}

async fn home() -> Html<&'static str> {
    Html(
        "<p>宿主自己的页面。<a href=\"/login?name=alice\">以 alice 登录</a>，再打开 <a href=\"/api/v1/me\">/api/v1/me</a> 或 <a href=\"/\">文档</a>。</p>",
    )
}

/// 宿主自己的登录（演示）：设个 cookie。
async fn login(Query(q): Query<HashMap<String, String>>) -> impl IntoResponse {
    let name = q.get("name").cloned().unwrap_or_else(|| "alice".into());
    let name: String = name.chars().filter(|c| c.is_alphanumeric()).collect();
    (
        [(
            header::SET_COOKIE,
            format!("demo_user={name}; Path=/; HttpOnly; SameSite=Lax"),
        )],
        format!("你好，{name}"),
    )
}

/// 建好整个服务：宿主的路由在 `/host`、`/login`，Open Agent Docs 的接口和网页在其余路径。
pub async fn app(data_dir: &Path) -> Result<(Router, Arc<Core>), String> {
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let storage: Arc<dyn Storage> =
        Arc::new(Sqlite::open(data_dir.join("oad.db")).map_err(|e| e.to_string())?);
    let auth = Arc::new(DemoAuth {
        storage: storage.clone(),
        workspace_id: Default::default(),
    });
    let media = MediaConfig {
        search_dir: Some(data_dir.join("search")),
        ..Default::default()
    };
    let core = Core::open_full(
        storage,
        AuthConfig::Custom(auth.clone()),
        HubConfig::default(),
        NotifyConfig::default(),
        media,
    )
    .await
    .map_err(|e| e.to_string())?;
    let _ = auth.workspace_id.set(core.default_workspace.id.clone());
    let oad = oad_server::router(AppState::new(core.clone(), ServerConfig::default()));
    let host = Router::new()
        .route("/host", get(home))
        .route("/login", get(login));
    Ok((host.merge(oad), core))
}
