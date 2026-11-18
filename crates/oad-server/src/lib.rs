//! HTTP 层：`router(state)` 返回 axum Router，别的 Rust 服务可以直接挂进去。接口见 docs/phase1-contract.md。
mod api;
mod error;
mod sync;
mod web;

pub use error::ApiError;

use axum::Router;
use oad_core::Core;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Default, Debug)]
pub struct ServerConfig {
    /// 从这个目录读网页（开发用）；不给就用打进二进制的 apps/web/dist。
    pub web_dir: Option<PathBuf>,
    /// 会话 cookie 加 Secure（走 HTTPS 时打开）。
    pub cookie_secure: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub core: Arc<Core>,
    pub config: Arc<ServerConfig>,
}

impl AppState {
    pub fn new(core: Arc<Core>, config: ServerConfig) -> Self {
        AppState {
            core,
            config: Arc::new(config),
        }
    }
}

/// 全部路由：`/api/v1/...` 和网页。
pub fn router(state: AppState) -> Router {
    Router::new()
        .nest("/api/v1", api::routes())
        .fallback(web::serve)
        .with_state(state)
}

/// 只有接口，不带网页（宿主自己出网页时用）。
pub fn api_router(state: AppState) -> Router {
    Router::new()
        .nest("/api/v1", api::routes())
        .with_state(state)
}
