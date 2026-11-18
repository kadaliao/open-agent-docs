//! 带 SSRF 防护的抓取。
//!
//! - 只允许 http / https；每一跳重定向都手动跟、重新检查（最多 5 跳）。
//! - 直连时用自定义 DNS 解析器：解析出的地址先过 `ssrf::classify_ip`，内网地址直接丢掉，
//!   reqwest 只会连我们放行的地址，所以 DNS rebinding（检查时是公网、连接时变内网）绕不过去。
//!   网址里直接写 IP 时 reqwest 不走解析器，由 `ssrf::check_url` 拦。
//! - 不读系统代理环境变量（否则 HTTPS_PROXY 会让上面的检查失效）；只认 `OAD_FETCH_PROXY`。
//!   走代理时 DNS 由代理做，我们只能先自己解析一遍、有内网地址就拒绝——代理那边解析结果不同的话拦不住，见 README。
//! - 整个抓取（含重定向）8 秒超时；正文边读边数，超过上限就停。

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{
    ACCEPT, ACCEPT_LANGUAGE, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, LOCATION,
};
use reqwest::{Method, StatusCode};
use tokio::time::{Instant, timeout_at};
use url::Url;

use crate::error::{Error, Result};
use crate::ssrf;

pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; OpenAgentDocs-Unfurl/0.1; link preview bot)";

/// 允许的网段（例外）。例如开发机上 Clash 的 fake-ip 段 `198.18.0.0/15`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    pub addr: IpAddr,
    pub prefix: u8,
}

impl Cidr {
    pub fn parse(s: &str) -> Option<Self> {
        let (a, p) = s.trim().split_once('/')?;
        let addr: IpAddr = a.parse().ok()?;
        let prefix: u8 = p.parse().ok()?;
        let max = if addr.is_ipv4() { 32 } else { 128 };
        (prefix <= max).then_some(Self { addr, prefix })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip) {
            (IpAddr::V4(n), IpAddr::V4(i)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - self.prefix as u32)
                };
                u32::from(n) & mask == u32::from(i) & mask
            }
            (IpAddr::V6(n), IpAddr::V6(i)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - self.prefix as u32)
                };
                u128::from(n) & mask == u128::from(i) & mask
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FetchConfig {
    /// 一次抓取（含所有重定向）的总时长。
    pub timeout: Duration,
    pub max_body: usize,
    pub max_redirects: usize,
    pub user_agent: String,
    pub accept_language: String,
    /// `OAD_FETCH_PROXY`，例如 `http://127.0.0.1:7890` 或 `socks5h://…`（reqwest 需开 socks 特性，本实验没开）。
    pub proxy: Option<String>,
    /// `OAD_FETCH_ALLOW_CIDRS`，逗号分隔。默认空。
    pub allow_cidrs: Vec<Cidr>,
    /// 每个域名同时最多几个请求、每秒最多几个。
    pub host_concurrency: usize,
    pub host_per_second: usize,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(8),
            max_body: 2 * 1024 * 1024,
            max_redirects: 5,
            user_agent: DEFAULT_USER_AGENT.into(),
            accept_language: "en-US,en;q=0.9,zh-CN;q=0.8,zh;q=0.7".into(),
            proxy: None,
            allow_cidrs: Vec::new(),
            host_concurrency: 2,
            host_per_second: 5,
        }
    }
}

impl FetchConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self {
            proxy: std::env::var("OAD_FETCH_PROXY")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            ..Self::default()
        };
        if let Ok(s) = std::env::var("OAD_FETCH_ALLOW_CIDRS") {
            cfg.allow_cidrs = s.split(',').filter_map(Cidr::parse).collect();
        }
        cfg
    }
}

#[derive(Debug)]
struct Guard {
    allow: Vec<Cidr>,
}

impl Guard {
    fn check_ip(&self, ip: IpAddr) -> std::result::Result<(), String> {
        if self.allow.iter().any(|c| c.contains(ip)) {
            return Ok(());
        }
        match ssrf::classify_ip(ip) {
            None => Ok(()),
            Some(why) => Err(format!("{ip}: {why}")),
        }
    }

    fn check_url(&self, url: &Url) -> std::result::Result<(), String> {
        // 例外网段只对解析出来的地址生效；网址里直接写的 IP 也走同一套例外。
        if let Some(url::Host::Ipv4(v4)) = url.host()
            && self.allow.iter().any(|c| c.contains(IpAddr::V4(v4)))
        {
            return Ok(());
        }
        ssrf::check_url(url)
    }
}

/// 只返回通过检查的地址；一个都不剩就报错。
struct GuardedResolver {
    guard: Arc<Guard>,
}

#[derive(Debug, thiserror::Error)]
#[error("blocked address {0}")]
struct BlockedByResolver(String);

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        let guard = self.guard.clone();
        Box::pin(async move {
            if let Err(why) = ssrf::check_domain(&host) {
                return Err(
                    Box::new(BlockedByResolver(why)) as Box<dyn std::error::Error + Send + Sync>
                );
            }
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let mut rejected = Vec::new();
            let ok: Vec<SocketAddr> = addrs
                .into_iter()
                .filter(|a| match guard.check_ip(a.ip()) {
                    Ok(()) => true,
                    Err(why) => {
                        rejected.push(why);
                        false
                    }
                })
                .collect();
            if ok.is_empty() {
                let why = if rejected.is_empty() {
                    format!("{host}: no addresses")
                } else {
                    format!("{host} -> {}", rejected.join(", "))
                };
                return Err(
                    Box::new(BlockedByResolver(why)) as Box<dyn std::error::Error + Send + Sync>
                );
            }
            Ok(Box::new(ok.into_iter()) as Addrs)
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct RequestOpts {
    pub method: Option<Method>,
    pub accept: Option<&'static str>,
    pub headers: Vec<(HeaderName, String)>,
    /// 覆盖默认的正文上限。
    pub max_body: Option<usize>,
    /// 超过上限时报错（JSON 接口）还是截断（HTML 只需要 <head>）。
    pub truncate_ok: bool,
    /// 响应不是网页（图片、视频、PDF……）时不读正文，只要响应头。
    pub skip_non_html_body: bool,
}

impl RequestOpts {
    pub fn html() -> Self {
        Self {
            accept: Some("text/html,application/xhtml+xml;q=0.9,*/*;q=0.8"),
            truncate_ok: true,
            skip_non_html_body: true,
            ..Default::default()
        }
    }
    pub fn json() -> Self {
        Self {
            accept: Some("application/json"),
            ..Default::default()
        }
    }
    pub fn head() -> Self {
        Self {
            method: Some(Method::HEAD),
            ..Default::default()
        }
    }
    pub fn header(mut self, name: HeaderName, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }
}

#[derive(Debug)]
pub struct Fetched {
    /// 跟完重定向后的地址。
    pub url: Url,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
    pub truncated: bool,
    pub redirects: usize,
}

impl Fetched {
    pub fn content_type(&self) -> Option<&str> {
        self.headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok())
    }

    /// 非 2xx 变成错误。
    pub fn ok(self) -> Result<Self> {
        let s = self.status;
        if s.is_success() {
            return Ok(self);
        }
        if s == StatusCode::NOT_FOUND || s == StatusCode::GONE {
            return Err(Error::NotFound);
        }
        let remaining_zero = self
            .headers
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.trim() == "0");
        if s == StatusCode::TOO_MANY_REQUESTS || (s == StatusCode::FORBIDDEN && remaining_zero) {
            return Err(Error::RateLimited);
        }
        Err(Error::Http { status: s.as_u16() })
    }

    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.body).map_err(|e| Error::Parse(format!("json: {e}")))
    }
}

#[derive(Clone)]
pub struct Fetcher {
    client: reqwest::Client,
    guard: Arc<Guard>,
    cfg: Arc<FetchConfig>,
    hosts: Arc<HostLimits>,
}

/// 按网站限速：每个域名同时最多 `concurrent` 个请求、每秒最多 `per_second` 个。
pub struct HostLimits {
    concurrent: usize,
    per_second: usize,
    slots: std::sync::Mutex<std::collections::HashMap<String, Arc<HostSlot>>>,
}

struct HostSlot {
    sem: Arc<tokio::sync::Semaphore>,
    recent: tokio::sync::Mutex<std::collections::VecDeque<Instant>>,
}

impl HostLimits {
    pub fn new(concurrent: usize, per_second: usize) -> Self {
        HostLimits {
            concurrent: concurrent.max(1),
            per_second: per_second.max(1),
            slots: Default::default(),
        }
    }

    /// 排队等到能对 host 发一个请求；返回的许可在请求结束前别丢。
    pub async fn acquire(&self, host: &str) -> tokio::sync::OwnedSemaphorePermit {
        let slot = {
            let mut m = self.slots.lock().unwrap_or_else(|e| e.into_inner());
            if m.len() > 4096 {
                m.retain(|_, s| s.sem.available_permits() < self.concurrent);
            }
            m.entry(host.to_string())
                .or_insert_with(|| {
                    Arc::new(HostSlot {
                        sem: Arc::new(tokio::sync::Semaphore::new(self.concurrent)),
                        recent: Default::default(),
                    })
                })
                .clone()
        };
        let permit = slot
            .sem
            .clone()
            .acquire_owned()
            .await
            .expect("信号量不会关");
        loop {
            let mut r = slot.recent.lock().await;
            let now = Instant::now();
            while r
                .front()
                .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(1))
            {
                r.pop_front();
            }
            if r.len() < self.per_second {
                r.push_back(now);
                return permit;
            }
            let wait = Duration::from_secs(1) - now.duration_since(*r.front().unwrap());
            drop(r);
            tokio::time::sleep(wait).await;
        }
    }
}

impl Fetcher {
    pub fn new(cfg: FetchConfig) -> Result<Self> {
        // TLS 用 ring；进程里已经装了别的就用已有的
        let _ = rustls::crypto::ring::default_provider().install_default();
        let guard = Arc::new(Guard {
            allow: cfg.allow_cidrs.clone(),
        });
        let mut b = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(cfg.timeout)
            .connect_timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(30))
            .user_agent(cfg.user_agent.clone());
        match &cfg.proxy {
            Some(p) => {
                // 代理地址是管理员配的，可以是 127.0.0.1；它自己不过 SSRF 检查。
                let proxy = reqwest::Proxy::all(p)
                    .map_err(|e| Error::InvalidUrl(format!("OAD_FETCH_PROXY: {e}")))?;
                b = b.proxy(proxy);
            }
            None => {
                b = b.no_proxy().dns_resolver(Arc::new(GuardedResolver {
                    guard: guard.clone(),
                }));
            }
        }
        let client = b.build().map_err(|e| Error::Request(e.to_string()))?;
        let hosts = Arc::new(HostLimits::new(cfg.host_concurrency, cfg.host_per_second));
        Ok(Self {
            client,
            guard,
            cfg: Arc::new(cfg),
            hosts,
        })
    }

    pub fn config(&self) -> &FetchConfig {
        &self.cfg
    }

    pub fn proxied(&self) -> bool {
        self.cfg.proxy.is_some()
    }

    /// 连接前的检查。直连时域名的地址由解析器管；走代理时只能先自己解析一遍（尽力而为）。
    async fn precheck(&self, url: &Url) -> Result<()> {
        let host = url.host_str().unwrap_or_default().to_string();
        self.guard.check_url(url).map_err(|reason| Error::Blocked {
            host: host.clone(),
            reason,
        })?;
        if self.proxied()
            && let Some(url::Host::Domain(d)) = url.host()
        {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((d, 0))
                .await
                .map_err(|e| Error::Dns {
                    host: host.clone(),
                    message: e.to_string(),
                })?
                .collect();
            if addrs.is_empty() {
                return Err(Error::Dns {
                    host,
                    message: "no addresses".into(),
                });
            }
            // 走代理时没法指定代理连哪个地址，所以只要有一个是内网就拒绝。
            for a in addrs {
                self.guard
                    .check_ip(a.ip())
                    .map_err(|reason| Error::Blocked {
                        host: host.clone(),
                        reason,
                    })?;
            }
        }
        Ok(())
    }

    pub async fn get(&self, url: &Url) -> Result<Fetched> {
        self.request(url, RequestOpts::html()).await
    }

    pub async fn request(&self, url: &Url, opts: RequestOpts) -> Result<Fetched> {
        let deadline = Instant::now() + self.cfg.timeout;
        timeout_at(deadline, self.request_inner(url, &opts))
            .await
            .map_err(|_| Error::Timeout)?
    }

    async fn request_inner(&self, url: &Url, opts: &RequestOpts) -> Result<Fetched> {
        let max_body = opts.max_body.unwrap_or(self.cfg.max_body);
        let method = opts.method.clone().unwrap_or(Method::GET);
        let mut current = url.clone();
        for hop in 0..=self.cfg.max_redirects {
            self.precheck(&current).await?;
            let _slot = self.hosts.acquire(&crate::key::host_of(&current)).await;
            let mut req = self
                .client
                .request(method.clone(), current.clone())
                .header(ACCEPT_LANGUAGE, &self.cfg.accept_language);
            if let Some(a) = opts.accept {
                req = req.header(ACCEPT, a);
            }
            for (k, v) in &opts.headers {
                if let Ok(v) = HeaderValue::from_str(v) {
                    req = req.header(k.clone(), v);
                }
            }
            let mut resp = req.send().await?;
            let status = resp.status();
            if status.is_redirection()
                && let Some(loc) = resp.headers().get(LOCATION)
            {
                let loc = loc
                    .to_str()
                    .map_err(|_| Error::BadRedirect("non-ascii Location".into()))?;
                let next = current
                    .join(loc)
                    .map_err(|e| Error::BadRedirect(format!("{loc}: {e}")))?;
                if hop == self.cfg.max_redirects {
                    return Err(Error::TooManyRedirects(self.cfg.max_redirects));
                }
                current = next;
                continue;
            }
            let headers = resp.headers().clone();
            // Content-Length 已经超了，就别读了（HTML 例外：只要开头）。
            if !opts.truncate_ok
                && let Some(len) = resp.content_length()
                && len as usize > max_body
            {
                return Err(Error::BodyTooLarge(max_body));
            }
            let mut body = Vec::new();
            let mut truncated = false;
            let ct = headers
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_ascii_lowercase();
            let non_html = !ct.is_empty()
                && !(ct.starts_with("text/html")
                    || ct.starts_with("application/xhtml")
                    || ct.starts_with("text/plain"));
            if method != Method::HEAD && !(opts.skip_non_html_body && non_html) {
                while let Some(chunk) = resp.chunk().await? {
                    let room = max_body - body.len();
                    if chunk.len() > room {
                        body.extend_from_slice(&chunk[..room]);
                        truncated = true;
                        break;
                    }
                    body.extend_from_slice(&chunk);
                }
                if truncated && !opts.truncate_ok {
                    return Err(Error::BodyTooLarge(max_body));
                }
            }
            return Ok(Fetched {
                url: current,
                status,
                headers,
                body,
                truncated,
                redirects: hop,
            });
        }
        Err(Error::TooManyRedirects(self.cfg.max_redirects))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr() {
        let c = Cidr::parse("198.18.0.0/15").unwrap();
        assert!(c.contains("198.19.1.1".parse().unwrap()));
        assert!(!c.contains("198.20.0.1".parse().unwrap()));
        let c6 = Cidr::parse("fd00::/8").unwrap();
        assert!(c6.contains("fd12::1".parse().unwrap()));
        assert!(Cidr::parse("1.2.3.4/33").is_none());
    }

    #[tokio::test]
    async fn blocks_literals_without_network() {
        let f = Fetcher::new(FetchConfig::default()).unwrap();
        for s in [
            "http://127.0.0.1:1/",
            "http://[::ffff:127.0.0.1]/",
            "http://0x7f000001/",
            "http://2130706433/",
            "http://169.254.169.254/",
        ] {
            let r = f.get(&Url::parse(s).unwrap()).await;
            assert!(matches!(r, Err(Error::Blocked { .. })), "{s}: {r:?}");
        }
    }

    #[tokio::test]
    async fn resolver_blocks_localhost_names() {
        // localhost 被名字检查拦；这里验证即使走到解析器也会拒绝。
        let r = GuardedResolver {
            guard: Arc::new(Guard { allow: vec![] }),
        };
        let name: Name = "localhost".parse().unwrap();
        assert!(r.resolve(name).await.is_err());
    }

    #[tokio::test]
    async fn allow_cidr_exception() {
        let g = Guard {
            allow: vec![Cidr::parse("198.18.0.0/15").unwrap()],
        };
        assert!(g.check_ip("198.18.0.5".parse().unwrap()).is_ok());
        assert!(g.check_ip("127.0.0.1".parse().unwrap()).is_err());
    }
}
