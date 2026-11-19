//! 多租户：按 Host 或请求头认出租户，每个租户一套独立的服务（自己的 Core：数据目录 / 数据库 schema、文件、索引），
//! 第一次用到时由宿主给的工厂建出来，之后复用。默认单租户，不用这个模块。
//!
//! 请求头方式只能放在可信的反向代理后面（代理负责设好、并去掉客户端自己带的同名头）。
use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use futures_util::future::BoxFuture;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

/// 从请求认出租户（小写字母、数字、`-`，最长 63）；认不出返回 None（404）。
pub trait TenantResolver: Send + Sync + 'static {
    fn tenant(&self, parts: &Parts) -> Option<String>;
}

/// 租户 id 合不合规矩（也是目录名 / schema 名，不能拿来跳目录）。
pub fn valid_tenant(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= 63
        && !t.starts_with('-')
        && t.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// 按子域名：`<租户>.<base>`；`base` 本身是 `default`（不给就 404）。
pub struct ByHost {
    pub base: String,
    pub default: Option<String>,
}

impl TenantResolver for ByHost {
    fn tenant(&self, parts: &Parts) -> Option<String> {
        let host = parts
            .headers
            .get(axum::http::header::HOST)
            .and_then(|v| v.to_str().ok())
            .map(crate::guard::host_name)?;
        let base = self.base.trim().to_ascii_lowercase();
        if host == base {
            return self.default.clone();
        }
        let t = host.strip_suffix(&format!(".{base}"))?;
        valid_tenant(t).then(|| t.to_string())
    }
}

/// 按请求头（默认 `X-OAD-Tenant`），给了 `secret` 时还要 `X-OAD-Tenant-Secret` 对上。
pub struct ByHeader {
    pub header: String,
    pub secret: Option<String>,
    pub default: Option<String>,
}

impl TenantResolver for ByHeader {
    fn tenant(&self, parts: &Parts) -> Option<String> {
        if let Some(s) = &self.secret {
            use subtle::ConstantTimeEq;
            let given = parts
                .headers
                .get("x-oad-tenant-secret")
                .map(|v| v.as_bytes())
                .unwrap_or_default();
            if !bool::from(given.ct_eq(s.as_bytes())) {
                return None;
            }
        }
        match parts
            .headers
            .get(self.header.as_str())
            .and_then(|v| v.to_str().ok())
        {
            Some(t) => {
                let t = t.trim().to_ascii_lowercase();
                valid_tenant(&t).then_some(t)
            }
            None => self.default.clone(),
        }
    }
}

/// 给租户建服务：返回这个租户的完整路由（`oad_server::router(AppState::new(core, cfg))`）。
pub type TenantFactory =
    Arc<dyn Fn(String) -> BoxFuture<'static, Result<Router, String>> + Send + Sync>;

struct Tenants {
    resolver: Box<dyn TenantResolver>,
    factory: TenantFactory,
    routers: tokio::sync::Mutex<()>,
    built: Mutex<HashMap<String, Router>>,
}

impl Tenants {
    async fn router_for(&self, t: &str) -> Result<Router, String> {
        if let Some(r) = self.built.lock().unwrap().get(t) {
            return Ok(r.clone());
        }
        // 同一时间只建一个，免得同一个租户被建两次
        let _g = self.routers.lock().await;
        if let Some(r) = self.built.lock().unwrap().get(t) {
            return Ok(r.clone());
        }
        let r = (self.factory)(t.to_string()).await?;
        self.built.lock().unwrap().insert(t.to_string(), r.clone());
        Ok(r)
    }
}

/// 多租户的入口：认出租户、转给它的服务。
pub fn multi_tenant(resolver: impl TenantResolver, factory: TenantFactory) -> Router {
    multi_tenant_boxed(Box::new(resolver), factory)
}

/// 同 [`multi_tenant`]，解析方式运行时才定（命令行按配置挑）。
pub fn multi_tenant_boxed(resolver: Box<dyn TenantResolver>, factory: TenantFactory) -> Router {
    let t = Arc::new(Tenants {
        resolver,
        factory,
        routers: tokio::sync::Mutex::new(()),
        built: Mutex::new(HashMap::new()),
    });
    Router::new().fallback_service(tower::service_fn(move |req: Request<Body>| {
        let t = t.clone();
        async move {
            let (parts, body) = req.into_parts();
            let Some(name) = t.resolver.tenant(&parts) else {
                return Ok::<_, Infallible>(
                    crate::ApiError::new(StatusCode::NOT_FOUND, "not_found", "没有这个租户")
                        .into_response(),
                );
            };
            let res: Response = match t.router_for(&name).await {
                Ok(r) => match r.oneshot(Request::from_parts(parts, body)).await {
                    Ok(res) => res,
                    Err(e) => match e {},
                },
                Err(e) => {
                    tracing::error!(tenant = %name, "建租户失败：{e}");
                    crate::ApiError::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "internal",
                        "服务器内部错误",
                    )
                    .into_response()
                }
            };
            Ok(res)
        }
    }))
}
