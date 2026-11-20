//! 嵌入用的静态资源：`GET /embed/content.css`（只读 HTML 的样式表，`?format=html` 的输出配它显示）。
//! 不用登录；带 ETag，缓存一天（过期后一周内先用旧的、后台再问）；允许任何网站跨源读（Shadow DOM 里 fetch 进来也行）。
use axum::Router;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

pub fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/content.css", get(content_css)).layer(
        tower_http::compression::CompressionLayer::new()
            .no_deflate()
            .no_zstd(),
    )
}

/// 内容的指纹（同一个二进制里不变；换了样式表就变）。
static ETAG: LazyLock<String> = LazyLock::new(|| {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    oad_markdown::CONTENT_CSS.hash(&mut h);
    format!("\"css-{:016x}\"", h.finish())
});

async fn content_css(headers: HeaderMap) -> Response {
    let etag = ETAG.as_str();
    let mut h = HeaderMap::new();
    h.insert(header::ETAG, HeaderValue::from_static(etag));
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400, stale-while-revalidate=604800"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    let hit = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',')
                .any(|t| t.trim() == "*" || t.trim().trim_start_matches("W/") == etag)
        });
    if hit {
        return (StatusCode::NOT_MODIFIED, h).into_response();
    }
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/css; charset=utf-8"),
    );
    (h, oad_markdown::CONTENT_CSS).into_response()
}
