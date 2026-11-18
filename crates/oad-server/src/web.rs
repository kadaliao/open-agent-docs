//! 网页：默认用打进二进制的 apps/web/dist（build.rs 预先压好 brotli、gzip，按 Accept-Encoding 给）；
//! 配了 web_dir 就从磁盘读原文。找不到的路径回 index.html（前端路由）。
//! 带哈希的打包产物（assets/）长缓存，别的（index.html）每次都要问；都带 ETag。
use crate::{ApiError, AppState};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use std::io::Read;

struct Asset {
    path: &'static str,
    mime: &'static str,
    etag: &'static str,
    #[allow(dead_code)]
    len: usize,
    /// 不压缩的文件放原文；压缩了的只放 br 和 gz。
    raw: Option<&'static [u8]>,
    br: Option<&'static [u8]>,
    gz: Option<&'static [u8]>,
}

/// 按 path 排好序的表（build.rs 生成）。
static ASSETS: &[Asset] = include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

fn find(path: &str) -> Option<&'static Asset> {
    ASSETS
        .binary_search_by(|a| a.path.cmp(path))
        .ok()
        .map(|i| &ASSETS[i])
}

const PLACEHOLDER: &str = "<!doctype html><meta charset=utf-8><title>Open Agent Docs</title>\
<p>Open Agent Docs 服务器在运行，网页还没构建（apps/web/dist）。接口在 <code>/api/v1</code>。</p>";

fn cache_control(path: &str) -> &'static str {
    if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

/// 客户端接受哪种压缩（q=0 的不算）。
fn accepts(headers: &HeaderMap, enc: &str) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .any(|item| {
            let mut parts = item.split(';');
            let name = parts.next().unwrap_or("").trim();
            let q0 = parts.any(|p| matches!(p.trim(), "q=0" | "q=0.0" | "q=0.00" | "q=0.000"));
            (name.eq_ignore_ascii_case(enc) || name == "*") && !q0
        })
}

fn embedded(a: &'static Asset, headers: &HeaderMap) -> Response {
    let etag = format!("\"{}\"", a.etag);
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(a.mime));
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control(a.path)),
    );
    h.insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
    h.insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',')
                .any(|t| t.trim().trim_start_matches("W/") == etag)
        })
    {
        return (StatusCode::NOT_MODIFIED, h).into_response();
    }
    if let Some(raw) = a.raw {
        return (h, raw).into_response();
    }
    let (br, gz) = (a.br.unwrap(), a.gz.unwrap());
    if accepts(headers, "br") {
        h.insert(header::CONTENT_ENCODING, HeaderValue::from_static("br"));
        return (h, br).into_response();
    }
    if accepts(headers, "gzip") {
        h.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        return (h, gz).into_response();
    }
    // 不支持压缩的客户端：现解
    let mut out = Vec::new();
    match flate2::read::GzDecoder::new(gz).read_to_end(&mut out) {
        Ok(_) => (h, out).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn from_disk(s: &AppState, path: &str) -> Option<Response> {
    let dir = s.config.web_dir.as_ref()?;
    // 不许跳出目录
    if path.split('/').any(|seg| seg == ".." || seg.contains('\\')) {
        return None;
    }
    let data = tokio::fs::read(dir.join(path)).await.ok()?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Some(
        (
            [
                (header::CONTENT_TYPE, mime.as_ref().to_string()),
                (header::CACHE_CONTROL, cache_control(path).to_string()),
            ],
            data,
        )
            .into_response(),
    )
}

async fn lookup(s: &AppState, path: &str, headers: &HeaderMap) -> Option<Response> {
    if s.config.web_dir.is_some() {
        return from_disk(s, path).await;
    }
    find(path).map(|a| embedded(a, headers))
}

pub async fn serve(State(s): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path == "api" || path.starts_with("api/") {
        return ApiError::not_found("没有这个接口").into_response();
    }
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(r) = lookup(&s, path, &headers).await {
        return r;
    }
    // 像文件的路径（带扩展名）找不到就是 404；别的交给前端路由
    let last = path.rsplit('/').next().unwrap_or("");
    if last.contains('.') && path != "index.html" {
        return StatusCode::NOT_FOUND.into_response();
    }
    match lookup(&s, "index.html", &headers).await {
        Some(r) => r,
        None => Html(PLACEHOLDER).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_encodings() {
        assert!(ASSETS.windows(2).all(|w| w[0].path < w[1].path));
        for a in ASSETS {
            assert!(
                a.raw.is_some() != (a.br.is_some() && a.gz.is_some()),
                "{}",
                a.path
            );
        }
        let mut h = HeaderMap::new();
        h.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_static("gzip, deflate, br;q=0"),
        );
        assert!(accepts(&h, "gzip") && !accepts(&h, "br"));
        h.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("br"));
        assert!(accepts(&h, "br") && !accepts(&h, "gzip"));
    }
}
