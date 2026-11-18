//! 请求来源检查。
//!
//! - Host：防 DNS rebinding（恶意网站把自己的域名解析到 127.0.0.1，再用浏览器读写本机服务）。
//!   不登录模式下一直检查；别的模式只在配了 `allowed_hosts` / `public_url` 时检查（有登录态的话，换了域名 cookie 也跟不过去）。
//!   默认允许 localhost、127.0.0.1、[::1]。
//! - Origin：改数据的请求（非 GET/HEAD/OPTIONS）和 WebSocket 握手，带了 Origin 的必须是同源、配置里允许的，
//!   或者服务器在本机时的本机页面（开发时 Vite 在另一个端口）。不带 Origin 的（Agent、curl）照常放行。
use crate::{ApiError, AppState};
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
        let listed = cfg.allowed_origins.iter().any(|a| normalize_origin(a) == o)
            || public.as_deref() == Some(o.as_str());
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
