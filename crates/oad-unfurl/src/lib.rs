//! 第 0 阶段实验：服务器端把链接解析成卡片数据。将来的 `oad-unfurl`。
//!
//! ```no_run
//! # async fn demo() -> oad_unfurl::Result<()> {
//! let card = oad_unfurl::unfurl("https://youtu.be/dQw4w9WgXcQ?t=42").await?;
//! println!("{}", serde_json::to_string_pretty(&card).unwrap());
//! # Ok(()) }
//! ```

pub mod error;
pub mod fetch;
pub mod html;
pub mod jsnum;
pub mod key;
pub mod model;
pub mod providers;
pub mod ssrf;

use std::time::Duration;

pub use error::{Error, Result};
pub use fetch::{FetchConfig, Fetcher, RequestOpts};
pub use key::{cache_key, failure_backoff, ttl};
pub use model::*;
use providers::{Log, bilibili, generic, github, wechat, x, youtube};
use url::Url;

/// 一次解析（含所有回退）的总时限。单次抓取另有 8 秒上限。
pub const OVERALL_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct Unfurler {
    fetcher: Fetcher,
}

impl Unfurler {
    pub fn new(cfg: FetchConfig) -> Result<Self> {
        Ok(Self {
            fetcher: Fetcher::new(cfg)?,
        })
    }

    pub fn from_env() -> Result<Self> {
        Self::new(FetchConfig::from_env())
    }

    pub fn fetcher(&self) -> &Fetcher {
        &self.fetcher
    }

    pub async fn unfurl(&self, input: &str) -> Result<Unfurl> {
        tokio::time::timeout(OVERALL_TIMEOUT, self.unfurl_inner(input))
            .await
            .map_err(|_| Error::Timeout)?
    }

    async fn unfurl_inner(&self, input: &str) -> Result<Unfurl> {
        let mut url = normalize(input)?;
        let f = &self.fetcher;
        let mut log = Log::default();

        // 短链先展开（目前只有 b23.tv），展开后再按网站分派
        if bilibili::is_short_link(&url) {
            match log
                .step("bilibili.short_link", bilibili::resolve_short(f, &url))
                .await
            {
                Some(u) => url = u,
                None => return Err(log.all_failed()),
            }
        }

        let result = if let Some(r) = youtube::parse(&url) {
            youtube::unfurl(f, &url, r, &mut log).await
        } else if let Some(r) = x::parse(&url) {
            x::unfurl(f, &url, r, &mut log).await
        } else if let Some(r) = github::parse(&url) {
            github::unfurl(f, &url, r, &mut log).await
        } else if let Some(r) = bilibili::parse(&url) {
            bilibili::unfurl(f, &url, r, &mut log).await
        } else if wechat::matches(&url) {
            wechat::unfurl(f, &url, &mut log).await
        } else {
            generic::unfurl(f, &url, &mut log).await
        };
        let mut u = result.map_err(|e| match e {
            Error::AllFailed(_) | Error::NotFound | Error::Blocked { .. } => e,
            other if log.attempts.is_empty() => other,
            other => {
                let mut v: Vec<String> = log
                    .attempts
                    .iter()
                    .filter(|a| !a.ok)
                    .map(|a| format!("{}: {}", a.step, a.error.as_deref().unwrap_or("?")))
                    .collect();
                v.push(other.to_string());
                Error::AllFailed(v)
            }
        })?;
        u.url = input.trim().to_string();
        u.attempts = log.attempts;
        Ok(u)
    }
}

/// 补协议、解析、做最基本的检查。
pub fn normalize(input: &str) -> Result<Url> {
    let s = input.trim();
    if s.is_empty() {
        return Err(Error::InvalidUrl("empty".into()));
    }
    let with_scheme = if s.contains("://") {
        s.to_string()
    } else {
        format!("https://{s}")
    };
    let url = Url::parse(&with_scheme).map_err(|e| Error::InvalidUrl(format!("{s}: {e}")))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        other => Err(Error::UnsupportedScheme(other.to_string())),
    }
}

/// 用环境变量里的配置解析一个网址。每次新建 HTTP 客户端，服务器里请复用 `Unfurler`。
pub async fn unfurl(url: &str) -> Result<Unfurl> {
    Unfurler::from_env()?.unfurl(url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_input() {
        assert_eq!(
            normalize("example.com/a").unwrap().as_str(),
            "https://example.com/a"
        );
        assert!(matches!(
            normalize("ftp://example.com"),
            Err(Error::UnsupportedScheme(_))
        ));
        assert!(matches!(
            normalize("javascript:alert(1)"),
            Err(Error::InvalidUrl(_) | Error::UnsupportedScheme(_))
        ));
        assert!(normalize("").is_err());
    }

    #[tokio::test]
    async fn blocked_targets_fail_fast() {
        let u = Unfurler::new(FetchConfig::default()).unwrap();
        for s in [
            "http://127.0.0.1:8080/",
            "http://localhost/",
            "http://[::ffff:127.0.0.1]/",
            "http://0x7f000001/",
            "http://10.1.2.3/admin",
        ] {
            let e = u.unfurl(s).await.unwrap_err();
            assert!(e.to_string().contains("blocked"), "{s}: {e}");
        }
    }
}
