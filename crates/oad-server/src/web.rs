//! 网页：默认用打进二进制的 apps/web/dist；配了 web_dir 就从磁盘读。找不到的路径回 index.html（前端路由）。
use crate::{ApiError, AppState};
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "../../apps/web/dist"]
#[allow_missing = true]
struct Dist;

const PLACEHOLDER: &str = "<!doctype html><meta charset=utf-8><title>Open Agent Docs</title>\
<p>Open Agent Docs 服务器在运行，网页还没构建（apps/web/dist）。接口在 <code>/api/v1</code>。</p>";

async fn read(s: &AppState, path: &str) -> Option<Cow<'static, [u8]>> {
    match &s.config.web_dir {
        Some(dir) => {
            // 不许跳出目录
            if path.split('/').any(|seg| seg == ".." || seg.contains('\\')) {
                return None;
            }
            tokio::fs::read(dir.join(path)).await.ok().map(Cow::Owned)
        }
        None => Dist::get(path).map(|f| f.data),
    }
}

fn respond(path: &str, data: Cow<'static, [u8]>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    // 带哈希的打包产物可以长缓存，index.html 每次都要问
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (header::CONTENT_TYPE, mime.as_ref().to_string()),
            (header::CACHE_CONTROL, cache.to_string()),
        ],
        data.into_owned(),
    )
        .into_response()
}

pub async fn serve(State(s): State<AppState>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path == "api" || path.starts_with("api/") {
        return ApiError::not_found("没有这个接口").into_response();
    }
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(data) = read(&s, path).await {
        return respond(path, data);
    }
    // 像文件的路径（带扩展名）找不到就是 404；别的交给前端路由
    let last = path.rsplit('/').next().unwrap_or("");
    if last.contains('.') && path != "index.html" {
        return StatusCode::NOT_FOUND.into_response();
    }
    match read(&s, "index.html").await {
        Some(data) => respond("index.html", data),
        None => Html(PLACEHOLDER).into_response(),
    }
}
