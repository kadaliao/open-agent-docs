//! 错误：服务器回的（[`ApiError`]）、连不上、超时、配置不对、webhook 验签失败。
use std::fmt;
use std::time::Duration;

/// `Result<T, oad::Error>`
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// 这个 crate 的所有错误。
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// 服务器回了错误（4xx、5xx）
    #[error(transparent)]
    Api(ApiError),
    /// 连不上、连接断了
    #[error("连不上服务器：{0}")]
    Connection(#[source] reqwest::Error),
    /// 超时（`timeout`，默认 60 秒）
    #[error("请求超时：{0}")]
    Timeout(String),
    /// 响应不是想要的样子（JSON 解析不了）
    #[error("看不懂服务器的响应：{0}")]
    Decode(String),
    /// 配置错误：地址不对、`with_user` 没配密钥……
    #[error("配置错误：{0}")]
    Config(String),
    /// webhook 验签失败
    #[error("webhook 验签失败：{0}")]
    Webhook(WebhookErrorKind),
    /// 本地启动服务器失败（feature `launcher`）
    #[error("启动服务器失败：{0}")]
    Launch(String),
}

/// 服务器回的错误：`{ "error": { "code", "message" } }`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    /// HTTP 状态码
    pub status: u16,
    /// 错误码：`not_found`、`bad_request`、`unauthorized`、`forbidden`、`conflict`、`gone`、`too_large`、
    /// `too_many_requests`、`bad_gateway`、`internal`；响应不是 JSON 时是 `http_<状态码>`
    pub code: String,
    /// 给人看的错误信息（语言看 `locale`）
    pub message: String,
    /// 响应头 `X-Request-Id`（有的话）
    pub request_id: Option<String>,
    /// 响应头 `Retry-After`（429 时服务器会给）
    pub retry_after: Option<Duration>,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}：{}", self.status, self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

/// webhook 验签失败的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WebhookErrorKind {
    /// 签名对不上（密钥不对、正文被改过）
    Signature,
    /// 时间戳离现在太远（防重放）
    Stale,
    /// 缺头、时间戳不是整数、签名格式不对
    Malformed,
}

impl WebhookErrorKind {
    /// `signature` / `stale` / `malformed`
    pub fn as_str(self) -> &'static str {
        match self {
            WebhookErrorKind::Signature => "signature",
            WebhookErrorKind::Stale => "stale",
            WebhookErrorKind::Malformed => "malformed",
        }
    }
}

impl fmt::Display for WebhookErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WebhookErrorKind::Signature => "签名不对（signature）",
            WebhookErrorKind::Stale => "时间戳太旧或太新（stale）",
            WebhookErrorKind::Malformed => "缺头或格式不对（malformed）",
        })
    }
}

macro_rules! status_checks {
    ($($(#[$m:meta])* $name:ident => $pat:pat),* $(,)?) => {
        impl ApiError {
            $($(#[$m])* pub fn $name(&self) -> bool { matches!(self.status, $pat) })*
        }
        impl Error {
            $($(#[$m])* pub fn $name(&self) -> bool { self.api().is_some_and(ApiError::$name) })*
        }
    };
}

status_checks! {
    /// 400
    is_bad_request => 400,
    /// 401：没登录、token 不对
    is_unauthorized => 401,
    /// 403：没权限
    is_forbidden => 403,
    /// 404
    is_not_found => 404,
    /// 409：冲突（比如 `base` 版本过时了）
    is_conflict => 409,
    /// 410：过期了（邀请之类）
    is_gone => 410,
    /// 413：太大
    is_too_large => 413,
    /// 429：太频繁，看 `retry_after`
    is_rate_limited => 429,
    /// 502：服务器去外部网站取东西失败
    is_bad_gateway => 502,
    /// 5xx
    is_server_error => 500..=599,
}

impl Error {
    /// 服务器回的错误（不是的话 `None`）
    pub fn api(&self) -> Option<&ApiError> {
        match self {
            Error::Api(e) => Some(e),
            _ => None,
        }
    }

    /// HTTP 状态码（服务器回了错误时）
    pub fn status(&self) -> Option<u16> {
        self.api().map(|e| e.status)
    }

    /// 超时了
    pub fn is_timeout(&self) -> bool {
        matches!(self, Error::Timeout(_))
    }

    /// 连不上服务器
    pub fn is_connection(&self) -> bool {
        matches!(self, Error::Connection(_))
    }

    /// webhook 验签失败的原因
    pub fn webhook_kind(&self) -> Option<WebhookErrorKind> {
        match self {
            Error::Webhook(k) => Some(*k),
            _ => None,
        }
    }

    pub(crate) fn from_reqwest(e: reqwest::Error) -> Error {
        if e.is_timeout() {
            Error::Timeout(e.to_string())
        } else if e.is_builder() {
            Error::Config(e.to_string())
        } else if e.is_decode() {
            Error::Decode(e.to_string())
        } else {
            Error::Connection(e)
        }
    }
}

impl From<ApiError> for Error {
    fn from(e: ApiError) -> Error {
        Error::Api(e)
    }
}
