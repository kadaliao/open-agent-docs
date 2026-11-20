//! 请求来源检查。
//!
//! - Host：防 DNS rebinding（恶意网站把自己的域名解析到 127.0.0.1，再用浏览器读写本机服务）。
//!   不登录模式下一直检查；别的模式只在配了 `allowed_hosts` / `public_url` 时检查（有登录态的话，换了域名 cookie 也跟不过去）。
//!   默认允许 localhost、127.0.0.1、[::1]。
//! - Origin：改数据的请求（非 GET/HEAD/OPTIONS）和 WebSocket 握手，带了 Origin 的必须是同源、配置里允许的，
//!   或者服务器在本机时的本机页面（开发时 Vite 在另一个端口）。不带 Origin 的（Agent、curl）照常放行。
//! - CORS（[`cors`]）：`/api/v1` 只对允许的 Origin（[`allowed_origins`]，和上面、`/auth/config` 给网页的是同一个列表）回 CORS 头。
use crate::{ApiError, AppState, ServerConfig};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use oad_core::AuthMode;

const LOOPBACK: &[&str] = &["localhost", "127.0.0.1", "[::1]", "::1"];

/// "Example.com:8848" → "example.com"；"[::1]:80" → "[::1]"。
pub fn host_name(authority: &str) -> String {
    let a = authority.trim().to_ascii_lowercase();
    if a.starts_with('[') {
        return a.split_inclusive(']').next().unwrap_or(&a).to_string();
    }
    match a.rsplit_once(':') {
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h.to_string(),
        _ => a,
    }
}

/// "https://Docs.example.com/" → "https://docs.example.com"。
pub fn normalize_origin(o: &str) -> String {
    let o = o.trim().trim_end_matches('/').to_ascii_lowercase();
    // 去掉路径（public_url 可能带）
    match o.split_once("://") {
        Some((scheme, rest)) => format!("{scheme}://{}", rest.split('/').next().unwrap_or(rest)),
        None => o,
    }
}

/// 允许跨源的 Origin：`OAD_ALLOWED_ORIGINS` 加上 `OAD_PUBLIC_URL` 的 origin（规范化、去重）。
/// 写请求和 WebSocket 的 Origin 检查、CORS、`/auth/config` 的 `allowed_origins`（iframe 认不认 `oad:token`）都用它。
pub fn allowed_origins(cfg: &ServerConfig) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for o in cfg
        .allowed_origins
        .iter()
        .chain(cfg.public_url.as_ref())
        .map(|o| normalize_origin(o))
    {
        if !o.is_empty() && o != "*" && !out.contains(&o) {
            out.push(o);
        }
    }
    out
}

/// `/api/v1` 的 CORS：只对 [`allowed_origins`] 里的 Origin 回（别的不回 CORS 头，浏览器就拦了）。
/// 允许带 cookie（同一个站点下的子域名用 cookie 登录也行）；预检缓存一小时；`Vary: Origin` 由 tower-http 加。
pub fn cors(cfg: &ServerConfig) -> tower_http::cors::CorsLayer {
    use axum::http::{HeaderName, Method};
    use tower_http::cors::{AllowOrigin, CorsLayer};
    let list = allowed_origins(cfg);
    let h = HeaderName::from_static;
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |o, _| {
            o.to_str()
                .is_ok_and(|o| list.contains(&normalize_origin(o)))
        }))
        .allow_methods([
            Method::GET,
            Method::HEAD,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::ACCEPT_LANGUAGE,
            header::IF_NONE_MATCH,
            header::IF_MATCH,
            header::RANGE,
            h("last-event-id"),
            h("x-filename"),
        ])
        .expose_headers([
            h("ratelimit-limit"),
            h("ratelimit-remaining"),
            h("ratelimit-reset"),
            header::RETRY_AFTER,
            header::ETAG,
            header::CONTENT_DISPOSITION,
            header::CONTENT_RANGE,
            h("x-request-id"),
        ])
        .allow_credentials(true)
        .max_age(std::time::Duration::from_secs(3600))
}

fn authority_of(origin: &str) -> &str {
    origin.split_once("://").map_or(origin, |(_, a)| a)
}

fn header_str(h: &HeaderMap, name: impl header::AsHeaderName) -> Option<&str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

fn forbidden(msg: &str) -> Response {
    ApiError::new(StatusCode::FORBIDDEN, "forbidden", msg).into_response()
}

pub async fn check(State(s): State<AppState>, req: Request, next: Next) -> Response {
    let cfg = &s.config;
    let headers = req.headers();
    let host = header_str(headers, header::HOST)
        .map(str::to_string)
        .or_else(|| req.uri().authority().map(|a| a.to_string()))
        .unwrap_or_default();
    let host_name = host_name(&host);
    let public = cfg.public_url.as_deref().map(normalize_origin);

    // Host
    let wildcard = cfg.allowed_hosts.iter().any(|h| h == "*");
    let enforce = !wildcard
        && (s.core.auth.mode() == AuthMode::None
            || !cfg.allowed_hosts.is_empty()
            || public.is_some());
    if enforce {
        let ok = LOOPBACK.contains(&host_name.as_str())
            || cfg
                .allowed_hosts
                .iter()
                .any(|h| host_name == self::host_name(h))
            || public
                .as_deref()
                .is_some_and(|p| self::host_name(authority_of(p)) == host_name);
        if !ok {
            return forbidden("Host 不在允许列表里（设 OAD_ALLOWED_HOSTS 或 OAD_PUBLIC_URL）");
        }
    }

    // Origin
    let is_ws =
        header_str(headers, header::UPGRADE).is_some_and(|u| u.eq_ignore_ascii_case("websocket"));
    let writes = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if (writes || is_ws)
        && let Some(origin) = header_str(headers, header::ORIGIN)
    {
        let o = normalize_origin(origin);
        let auth = authority_of(&o);
        // 反向代理改了 Host 时用 X-Forwarded-Host 比；浏览器跨源请求加不了这个头
        let same = auth == host.to_ascii_lowercase()
            || header_str(headers, "x-forwarded-host").is_some_and(|f| {
                auth == f
                    .split(',')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
            });
        let local_dev = LOOPBACK.contains(&host_name.as_str())
            && LOOPBACK.contains(&self::host_name(auth).as_str());
        let listed = allowed_origins(cfg).contains(&o);
        if o == "null" || !(same || local_dev || listed) {
            return forbidden("Origin 不在允许列表里（嵌入时设 OAD_ALLOWED_ORIGINS）");
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        assert_eq!(host_name("Example.com:8848"), "example.com");
        assert_eq!(host_name("[::1]:8848"), "[::1]");
        assert_eq!(host_name("127.0.0.1"), "127.0.0.1");
        assert_eq!(
            normalize_origin("https://Docs.Example.com/app/"),
            "https://docs.example.com"
        );
        assert_eq!(authority_of("http://localhost:5173"), "localhost:5173");
    }
}
