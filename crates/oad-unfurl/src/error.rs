use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("unsupported scheme: {0}")]
    UnsupportedScheme(String),
    /// SSRF 防护拦下的地址。
    #[error("blocked address {host}: {reason}")]
    Blocked { host: String, reason: String },
    #[error("dns lookup failed for {host}: {message}")]
    Dns { host: String, message: String },
    #[error("timed out")]
    Timeout,
    #[error("too many redirects (max {0})")]
    TooManyRedirects(usize),
    #[error("bad redirect: {0}")]
    BadRedirect(String),
    #[error("http status {status}")]
    Http { status: u16 },
    #[error("rate limited")]
    RateLimited,
    #[error("not found")]
    NotFound,
    #[error("response body exceeds {0} bytes")]
    BodyTooLarge(usize),
    #[error("request failed: {0}")]
    Request(String),
    #[error("unexpected response: {0}")]
    Parse(String),
    /// 所有回退都失败了；`attempts` 里是每一级的原因。
    #[error("all strategies failed: {}", .0.join("; "))]
    AllFailed(Vec<String>),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            return Error::Timeout;
        }
        // 自定义 DNS 解析器拒绝时，错误会包在 reqwest 的 connect 错误里；把链条拼出来便于排查。
        let mut msg = e.to_string();
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            msg.push_str(": ");
            msg.push_str(&s.to_string());
            src = s.source();
        }
        if msg.contains("blocked address") {
            return Error::Blocked {
                host: e
                    .url()
                    .and_then(|u| u.host_str())
                    .unwrap_or("?")
                    .to_string(),
                reason: msg,
            };
        }
        Error::Request(msg)
    }
}
