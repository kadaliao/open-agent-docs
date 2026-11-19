//! HTTP 层：`router(state)` 返回 axum Router，别的 Rust 服务可以直接挂进去。接口见 docs/phase1-contract.md。
mod api;
mod collab;
mod dbs;
mod error;
mod files;
mod guard;
mod history;
pub mod i18n;
mod limit;
mod members;
mod share;
mod sync;
mod trash;
mod web;

pub use error::ApiError;

use axum::Router;
use oad_core::Core;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// 从这个目录读网页（开发用）；不给就用打进二进制的 apps/web/dist。
    pub web_dir: Option<PathBuf>,
    /// 会话 cookie 加 Secure（走 HTTPS 时打开）。
    pub cookie_secure: bool,
    /// 对外的地址（如 `https://docs.example.com`），它的 Host 和 Origin 都允许。
    pub public_url: Option<String>,
    /// 额外允许的 Host（不含端口）；有 `*` 就不检查 Host。见 guard 模块。
    pub allowed_hosts: Vec<String>,
    /// 额外允许的 Origin（跨源嵌入时），如 `https://app.example.com`。
    pub allowed_origins: Vec<String>,
    /// 同一个 IP + 邮箱在 `login_window` 里最多失败几次。
    pub login_max_failures: u32,
    pub login_window: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            web_dir: None,
            cookie_secure: false,
            public_url: None,
            allowed_hosts: Vec::new(),
            allowed_origins: Vec::new(),
            login_max_failures: 10,
            login_window: Duration::from_secs(15 * 60),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub core: Arc<Core>,
    pub config: Arc<ServerConfig>,
    login_limit: Arc<limit::LoginLimiter>,
}

impl AppState {
    pub fn new(core: Arc<Core>, config: ServerConfig) -> Self {
        AppState {
            core,
            login_limit: Arc::new(limit::LoginLimiter::new(
                config.login_max_failures,
                config.login_window,
            )),
            config: Arc::new(config),
        }
    }
}

/// 全部路由：`/api/v1/...`、`/mcp`（MCP，Agent token）和网页。登录限流按 IP 算，要用
/// `into_make_service_with_connect_info::<SocketAddr>()` 起服务（没有就只按邮箱算）。
pub fn router(state: AppState) -> Router {
    Router::new()
        .nest(
            "/api/v1",
            api::routes().layer(axum::middleware::from_fn_with_state(
                state.clone(),
                i18n::errors,
            )),
        )
        .nest("/s", share::public_routes())
        .nest_service("/mcp", oad_mcp::router(state.core.clone()))
        .fallback(web::serve)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            guard::check,
        ))
        .with_state(state)
}

/// 只有接口，不带网页（宿主自己出网页时用）。
pub fn api_router(state: AppState) -> Router {
    Router::new()
        .nest(
            "/api/v1",
            api::routes().layer(axum::middleware::from_fn_with_state(
                state.clone(),
                i18n::errors,
            )),
        )
        .nest("/s", share::public_routes())
        .nest_service("/mcp", oad_mcp::router(state.core.clone()))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            guard::check,
        ))
        .with_state(state)
}
