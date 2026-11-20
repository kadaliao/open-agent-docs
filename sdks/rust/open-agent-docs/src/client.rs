//! 客户端：配置、发请求（重试、错误）、替用户调（`with_user`）。
use crate::embed::{JwtOptions, JwtWorkspace, sign_jwt};
use crate::error::{ApiError, Error, Result};
use crate::events::{EventData, EventStream, SpecFn};
use crate::request::{Body, Spec, enc};
use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::de::DeserializeOwned;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::OnceCell;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8848";
const USER_AGENT: &str = concat!("open-agent-docs-rust/", env!("CARGO_PKG_VERSION"));

/// Open Agent Docs 的客户端。克隆很便宜（共享连接池和配置），可以随便传给别的任务。
///
/// ```no_run
/// # use open_agent_docs as oad;
/// # fn run() -> oad::Result<()> {
/// let client = oad::Client::builder()
///     .base_url("https://docs.example.com")
///     .token("oad_…")
///     .build()?;
/// // 或者读环境变量 OAD_BASE_URL、OAD_TOKEN
/// let client = oad::Client::from_env()?;
/// # let _ = client;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
    user: Option<Arc<Acting>>,
    default_ws: Arc<OnceCell<String>>,
}

struct Inner {
    http: reqwest::Client,
    base_url: String,
    token: Option<String>,
    header_secret: Option<String>,
    jwt_secret: Option<String>,
    jwt_issuer: Option<String>,
    jwt_audience: Option<String>,
    jwt_ttl: Duration,
    timeout: Duration,
    max_retries: u32,
    locale: Option<String>,
    headers: HeaderMap,
    reconnect_delay: Duration,
}

/// `with_user` 的身份
struct Acting {
    user: User,
    jwt: bool,
    /// 签好的 JWT 和它的过期时间（秒）
    cached: Mutex<Option<(String, u64)>>,
}

/// [`Client::with_user`] 代表的用户。
///
/// ```
/// # use open_agent_docs as oad;
/// let u = oad::User::new("u-42").name("王小明").email("x@example.com");
/// # let _ = u;
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct User {
    /// 宿主里的用户 id（JWT 的 `sub`、`X-OAD-User-Id`）
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    /// 能进的工作区（只有 JWT 模式支持）；不给就进默认工作区
    pub workspaces: Option<Vec<JwtWorkspace>>,
}

impl User {
    pub fn new(id: impl Into<String>) -> User {
        User {
            id: id.into(),
            ..Default::default()
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> User {
        self.name = Some(name.into());
        self
    }

    pub fn email(mut self, email: impl Into<String>) -> User {
        self.email = Some(email.into());
        self
    }

    /// 工作区 id，或者 [`JwtWorkspace::with_role`]
    pub fn workspaces<W: Into<JwtWorkspace>>(mut self, ws: impl IntoIterator<Item = W>) -> User {
        self.workspaces = Some(ws.into_iter().map(Into::into).collect());
        self
    }
}

impl From<&str> for User {
    fn from(id: &str) -> User {
        User::new(id)
    }
}

impl From<String> for User {
    fn from(id: String) -> User {
        User::new(id)
    }
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 不打印密钥
        f.debug_struct("Client")
            .field("base_url", &self.inner.base_url)
            .field("token", &self.inner.token.as_ref().map(|_| "…"))
            .field("user", &self.user.as_ref().map(|u| &u.user.id))
            .finish_non_exhaustive()
    }
}

/// [`Client`] 的配置。没设的项按契约的默认值：地址读 `OAD_BASE_URL`（没有就 `http://127.0.0.1:8848`），
/// token 读 `OAD_TOKEN`，超时 60 秒，最多重试 2 次。
#[derive(Clone)]
#[must_use]
pub struct ClientBuilder {
    base_url: Option<String>,
    token: Option<String>,
    header_secret: Option<String>,
    jwt_secret: Option<String>,
    jwt_issuer: Option<String>,
    jwt_audience: Option<String>,
    jwt_ttl: Duration,
    timeout: Duration,
    max_retries: u32,
    locale: Option<String>,
    headers: Vec<(String, String)>,
    http: Option<reqwest::Client>,
    reconnect_delay: Duration,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        ClientBuilder {
            base_url: None,
            token: None,
            header_secret: None,
            jwt_secret: None,
            jwt_issuer: None,
            jwt_audience: None,
            jwt_ttl: Duration::from_secs(3600),
            timeout: Duration::from_secs(60),
            max_retries: 2,
            locale: None,
            headers: Vec::new(),
            http: None,
            reconnect_delay: Duration::from_secs(1),
        }
    }
}

impl fmt::Debug for ClientBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientBuilder")
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .field("max_retries", &self.max_retries)
            .finish_non_exhaustive()
    }
}

impl ClientBuilder {
    /// 服务器地址，比如 `https://docs.example.com`（`/api/v1` 由 SDK 拼，末尾的 `/` 去掉）
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Agent 的 token（`oad_…`）、会话令牌或 JWT，带成 `Authorization: Bearer`
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// `OAD_AUTH=header` 的服务器的 `OAD_HEADER_SECRET`：配合 [`Client::with_user`] 用
    pub fn header_secret(mut self, secret: impl Into<String>) -> Self {
        self.header_secret = Some(secret.into());
        self
    }

    /// `OAD_AUTH=jwt` 的服务器的 `OAD_JWT_SECRET`（HS256）：配合 [`Client::with_user`] 用
    pub fn jwt_secret(mut self, secret: impl Into<String>) -> Self {
        self.jwt_secret = Some(secret.into());
        self
    }

    /// `with_user` 签 JWT 时写进 `iss`
    pub fn jwt_issuer(mut self, iss: impl Into<String>) -> Self {
        self.jwt_issuer = Some(iss.into());
        self
    }

    /// `with_user` 签 JWT 时写进 `aud`
    pub fn jwt_audience(mut self, aud: impl Into<String>) -> Self {
        self.jwt_audience = Some(aud.into());
        self
    }

    /// `with_user` 签的 JWT 多久过期（默认 3600 秒）
    pub fn jwt_ttl(mut self, ttl: Duration) -> Self {
        self.jwt_ttl = ttl;
        self
    }

    /// 单个请求的超时（默认 60 秒）。事件流没有总超时，只用它当连接超时
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 最多重试几次（默认 2）：429；502、503、504；连不上。见契约 §5
    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    /// 服务器错误信息的语言：`zh` / `en`（`Accept-Language`）
    pub fn locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    /// 每个请求多带一个头
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// 自己的 `reqwest::Client`（代理、证书……）。连接超时这时要自己在上面设
    pub fn http_client(mut self, http: reqwest::Client) -> Self {
        self.http = Some(http);
        self
    }

    /// 事件流断了以后第一次重连等多久（默认 1 秒，之后翻倍，最多 30 秒）
    pub fn reconnect_delay(mut self, d: Duration) -> Self {
        self.reconnect_delay = d;
        self
    }

    pub fn build(self) -> Result<Client> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let base = self
            .base_url
            .or_else(|| env("OAD_BASE_URL"))
            .unwrap_or_else(|| DEFAULT_BASE_URL.into());
        let base = base.trim().trim_end_matches('/').to_string();
        let base = base.strip_suffix("/api/v1").unwrap_or(&base).to_string();
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(Error::Config(format!(
                "base_url 要以 http:// 或 https:// 开头：{base}"
            )));
        }
        let mut headers = HeaderMap::new();
        for (k, v) in &self.headers {
            let name = HeaderName::try_from(k.as_str())
                .map_err(|e| Error::Config(format!("请求头 {k}：{e}")))?;
            let value = HeaderValue::try_from(v.as_str())
                .map_err(|e| Error::Config(format!("请求头 {k} 的值：{e}")))?;
            headers.append(name, value);
        }
        let http = match self.http {
            Some(h) => h,
            None => reqwest::Client::builder()
                .connect_timeout(self.timeout)
                .build()
                .map_err(|e| Error::Config(format!("建 HTTP 客户端失败：{e}")))?,
        };
        Ok(Client {
            inner: Arc::new(Inner {
                http,
                base_url: base,
                token: self.token.or_else(|| env("OAD_TOKEN")),
                header_secret: self.header_secret,
                jwt_secret: self.jwt_secret,
                jwt_issuer: self.jwt_issuer,
                jwt_audience: self.jwt_audience,
                jwt_ttl: self.jwt_ttl,
                timeout: self.timeout,
                max_retries: self.max_retries,
                locale: self.locale,
                headers,
                reconnect_delay: self.reconnect_delay,
            }),
            user: None,
            default_ws: Arc::new(OnceCell::new()),
        })
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// 0.5 秒 × 2^n，最多 8 秒，再加 0–25% 的随机
fn backoff(attempt: u32) -> Duration {
    let base = Duration::from_millis(500).saturating_mul(1u32 << attempt.min(5));
    let base = base.min(Duration::from_secs(8));
    base + base.mul_f64(jitter() * 0.25)
}

/// [0, 1) 的随机数（不为这点事加 rand 依赖：标准库的 RandomState 每次的种子都不一样）
fn jitter() -> f64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    (h.finish() >> 11) as f64 / (1u64 << 53) as f64
}

fn retry_after(h: &HeaderMap) -> Option<Duration> {
    let v = h.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?.trim();
    v.parse::<f64>()
        .ok()
        .filter(|s| s.is_finite() && *s >= 0.0)
        .map(Duration::from_secs_f64)
}

/// 错误响应 → ApiError
async fn api_error(r: reqwest::Response) -> ApiError {
    let status = r.status();
    let request_id = r
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let retry_after = retry_after(r.headers());
    let body = r.bytes().await.unwrap_or_default();
    #[derive(serde::Deserialize)]
    struct Body {
        error: Info,
    }
    #[derive(serde::Deserialize)]
    struct Info {
        code: String,
        #[serde(default)]
        message: String,
    }
    let (code, message) = match serde_json::from_slice::<Body>(&body) {
        Ok(b) => (b.error.code, b.error.message),
        Err(_) => {
            let text = String::from_utf8_lossy(&body);
            let text = text.trim();
            let message = if text.is_empty() {
                status.canonical_reason().unwrap_or("").to_string()
            } else {
                text.chars().take(500).collect()
            };
            (format!("http_{}", status.as_u16()), message)
        }
    };
    ApiError {
        status: status.as_u16(),
        code,
        message,
        request_id,
        retry_after,
    }
}

impl Client {
    /// 配置客户端
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// 按环境变量建：`OAD_BASE_URL`（默认 `http://127.0.0.1:8848`）、`OAD_TOKEN`
    pub fn from_env() -> Result<Client> {
        ClientBuilder::default().build()
    }

    /// 服务器地址（不带 `/api/v1`）
    pub fn base_url(&self) -> &str {
        &self.inner.base_url
    }

    /// `with_user` 给的用户（没有就是 `None`）
    pub fn user(&self) -> Option<&User> {
        self.user.as_ref().map(|u| &u.user)
    }

    pub(crate) fn reconnect_delay(&self) -> Duration {
        self.inner.reconnect_delay
    }

    /// 以某个用户的身份调（宿主后端用）：返回一个新客户端，共享连接池和配置，原来的不变。
    ///
    /// - 配了 [`header_secret`](ClientBuilder::header_secret)：带 `X-OAD-Secret`、`X-OAD-User-Id`、`X-OAD-User-Name`、
    ///   `X-OAD-User-Email`（百分号编码）。这个模式不支持 `workspaces`。
    /// - 配了 [`jwt_secret`](ClientBuilder::jwt_secret)：签一个 HS256 的 JWT 带成 `Authorization: Bearer`，
    ///   缓存到过期前 60 秒再换。
    /// - 两个都配了，或者都没配：[`Error::Config`]。
    pub fn with_user(&self, user: impl Into<User>) -> Result<Client> {
        let user = user.into();
        let jwt = match (&self.inner.header_secret, &self.inner.jwt_secret) {
            (Some(_), None) => false,
            (None, Some(_)) => true,
            (Some(_), Some(_)) => {
                return Err(Error::Config(
                    "header_secret 和 jwt_secret 只能配一个".into(),
                ));
            }
            (None, None) => {
                return Err(Error::Config(
                    "with_user 要先配 header_secret 或 jwt_secret".into(),
                ));
            }
        };
        if user.id.is_empty() {
            return Err(Error::Config("with_user 的用户 id 不能是空的".into()));
        }
        if !jwt && user.workspaces.is_some() {
            return Err(Error::Config(
                "header 模式不支持 workspaces（用 JWT 模式）".into(),
            ));
        }
        Ok(Client {
            inner: self.inner.clone(),
            user: Some(Arc::new(Acting {
                user,
                jwt,
                cached: Mutex::new(None),
            })),
            default_ws: Arc::new(OnceCell::new()),
        })
    }

    /// 当前身份能进的第一个工作区（`/me` 里的第一个）。每个客户端只查一次。
    pub async fn default_workspace(&self) -> Result<String> {
        self.default_ws
            .get_or_try_init(|| async {
                let me = self.me().get().await?;
                me.workspaces
                    .first()
                    .map(|w| w.id.clone())
                    .ok_or_else(|| Error::Config("这个身份没有任何工作区".into()))
            })
            .await
            .cloned()
    }

    /// 发一个任意的 JSON 请求（路径不在 SDK 里时用）：`path` 是 `/api/v1` 之后的部分，比如 `/me`。
    pub async fn request_json(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let mut spec = Spec::new(method, path.to_string());
        if let Some(b) = body {
            spec.json(b)?;
        }
        let r = self.execute(&spec, false).await?;
        if r.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(serde_json::Value::Null);
        }
        let bytes = r.bytes().await.map_err(Error::from_reqwest)?;
        if bytes.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|e| Error::Decode(e.to_string()))
    }

    fn jwt(&self, a: &Acting) -> Result<String> {
        let now = now_secs();
        let mut cached = a.cached.lock().unwrap_or_else(|e| e.into_inner());
        // 离过期还有 60 秒以上就接着用（rust-version 1.85 还没有 let 链）
        if let Some((tok, _)) = cached.as_ref().filter(|(_, exp)| now + 60 < *exp) {
            return Ok(tok.clone());
        }
        let secret = self
            .inner
            .jwt_secret
            .as_deref()
            .ok_or_else(|| Error::Config("没配 jwt_secret".into()))?;
        let opts = JwtOptions {
            name: a.user.name.clone(),
            email: a.user.email.clone(),
            workspaces: a.user.workspaces.clone(),
            ttl: self.inner.jwt_ttl,
            issuer: self.inner.jwt_issuer.clone(),
            audience: self.inner.jwt_audience.clone(),
            now: Some(UNIX_EPOCH + Duration::from_secs(now)),
        };
        let tok = sign_jwt(secret, &a.user.id, &opts);
        *cached = Some((tok.clone(), now + self.inner.jwt_ttl.as_secs()));
        Ok(tok)
    }

    fn request(&self, spec: &Spec, stream: bool) -> Result<reqwest::RequestBuilder> {
        let url = format!("{}/api/v1{}", self.inner.base_url, spec.url_path());
        let mut rb = self
            .inner
            .http
            .request(spec.method.clone(), url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .headers(self.inner.headers.clone());
        if let Some(l) = &self.inner.locale {
            let l = match l.as_str() {
                "zh" => "zh-CN",
                other => other,
            };
            rb = rb.header(reqwest::header::ACCEPT_LANGUAGE, l);
        }
        match &self.user {
            Some(a) if a.jwt => rb = rb.bearer_auth(self.jwt(a)?),
            Some(a) => {
                let secret = self.inner.header_secret.as_deref().unwrap_or_default();
                rb = rb
                    .header("x-oad-secret", secret)
                    .header("x-oad-user-id", enc(&a.user.id));
                if let Some(n) = &a.user.name {
                    rb = rb.header("x-oad-user-name", enc(n));
                }
                if let Some(e) = &a.user.email {
                    rb = rb.header("x-oad-user-email", enc(e));
                }
            }
            None => {
                if let Some(t) = &self.inner.token {
                    rb = rb.bearer_auth(t);
                }
            }
        }
        if let Body::Raw { content_type, data } = &spec.body {
            rb = rb
                .header(reqwest::header::CONTENT_TYPE, *content_type)
                .body(data.clone());
        }
        for (k, v) in &spec.headers {
            rb = rb.header(*k, v.as_str());
        }
        if stream {
            rb = rb.header(reqwest::header::ACCEPT, "text/event-stream");
        } else {
            rb = rb.timeout(self.inner.timeout);
        }
        Ok(rb)
    }

    /// 发出去，按契约 §5 重试；成功（2xx）返回响应，否则返回错误。
    pub(crate) async fn execute(&self, spec: &Spec, stream: bool) -> Result<reqwest::Response> {
        let mut attempt = 0u32;
        loop {
            let rb = self.request(spec, stream)?;
            let res = if stream {
                // 事件流：只给“等到响应头”设超时
                match tokio::time::timeout(self.inner.timeout, rb.send()).await {
                    Ok(r) => r,
                    Err(_) => {
                        return Err(Error::Timeout(format!(
                            "{} 秒没连上事件流",
                            self.inner.timeout.as_secs()
                        )));
                    }
                }
            } else {
                rb.send().await
            };
            let can_retry = attempt < self.inner.max_retries;
            match res {
                Ok(r) if r.status().is_success() => return Ok(r),
                Ok(r) => {
                    let s = r.status().as_u16();
                    let retryable = s == 429 || (matches!(s, 502..=504) && spec.idempotent());
                    if retryable && can_retry {
                        let wait = retry_after(r.headers())
                            .map(|d| d.min(Duration::from_secs(60)))
                            .unwrap_or_else(|| backoff(attempt));
                        drop(r);
                        tokio::time::sleep(wait).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(Error::Api(api_error(r).await));
                }
                Err(e) => {
                    // 连不上（请求还没发出去）都能重试；发出去以后断了，只有 GET、PUT、DELETE 重试
                    let retryable = !e.is_timeout()
                        && (e.is_connect()
                            || (spec.idempotent() && (e.is_request() || e.is_body())));
                    if retryable && can_retry {
                        tokio::time::sleep(backoff(attempt)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(Error::from_reqwest(e));
                }
            }
        }
    }

    pub(crate) async fn send_bytes(&self, spec: Spec) -> Result<Bytes> {
        let r = self.execute(&spec, false).await?;
        r.bytes().await.map_err(Error::from_reqwest)
    }

    pub(crate) async fn send_empty(&self, spec: Spec) -> Result<()> {
        self.send_bytes(spec).await.map(|_| ())
    }

    pub(crate) async fn send_json<T: DeserializeOwned>(&self, spec: Spec) -> Result<T> {
        let bytes = self.send_bytes(spec).await?;
        serde_json::from_slice(&bytes).map_err(|e| {
            let head: String = String::from_utf8_lossy(&bytes).chars().take(200).collect();
            Error::Decode(format!("{e}（响应开头：{head}）"))
        })
    }

    pub(crate) async fn send_optional<T: DeserializeOwned>(&self, spec: Spec) -> Result<Option<T>> {
        match self.send_json(spec).await {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.is_not_found() => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub(crate) async fn open_events<T: EventData>(&self, make: SpecFn) -> Result<EventStream<T>> {
        let spec = make(None)?;
        let r = self.execute(&spec, true).await?;
        Ok(EventStream::new(self.clone(), make, r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        let b0 = backoff(0);
        assert!(
            b0 >= Duration::from_millis(500) && b0 <= Duration::from_millis(625),
            "{b0:?}"
        );
        let b2 = backoff(2);
        assert!(b2 >= Duration::from_secs(2) && b2 <= Duration::from_millis(2500));
        let b9 = backoff(9);
        assert!(b9 >= Duration::from_secs(8) && b9 <= Duration::from_secs(10));
    }

    #[test]
    fn base_url_trimmed() {
        let c = Client::builder()
            .base_url("http://h:1/api/v1/")
            .build()
            .unwrap();
        assert_eq!(c.base_url(), "http://h:1");
        assert!(Client::builder().base_url("h:1").build().is_err());
    }

    #[test]
    fn with_user_needs_exactly_one_secret() {
        let c = Client::builder().base_url("http://h").build().unwrap();
        assert!(matches!(c.with_user("u"), Err(Error::Config(_))));
        let c = Client::builder()
            .base_url("http://h")
            .header_secret("s")
            .jwt_secret("j")
            .build()
            .unwrap();
        assert!(matches!(c.with_user("u"), Err(Error::Config(_))));
        let c = Client::builder()
            .base_url("http://h")
            .header_secret("s")
            .build()
            .unwrap();
        assert!(c.with_user("u").is_ok());
        assert!(matches!(
            c.with_user(User::new("u").workspaces(["w1"])),
            Err(Error::Config(_))
        ));
    }

    #[test]
    fn debug_hides_secrets() {
        let c = Client::builder()
            .base_url("http://h")
            .token("oad_secret_value")
            .build()
            .unwrap();
        assert!(!format!("{c:?}").contains("oad_secret_value"));
    }
}
