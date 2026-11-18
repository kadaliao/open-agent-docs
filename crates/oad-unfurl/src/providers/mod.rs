//! 各网站的专用解析器。每个解析器自己决定回退顺序，最后一级一般是通用网页解析。

pub mod bilibili;
pub mod generic;
pub mod github;
pub mod wechat;
pub mod x;
pub mod youtube;

use std::future::Future;
use std::time::Instant;

use crate::error::{Error, Result};
use crate::model::Attempt;

/// 记录每一级尝试。
#[derive(Debug, Default)]
pub struct Log {
    pub attempts: Vec<Attempt>,
}

impl Log {
    pub async fn step<T>(&mut self, name: &str, fut: impl Future<Output = Result<T>>) -> Option<T> {
        if skipped(name) {
            self.record::<T>(
                name,
                0,
                &Err(Error::Request("skipped (OAD_UNFURL_SKIP)".into())),
            );
            return None;
        }
        let t = Instant::now();
        let r = fut.await;
        self.record(name, t.elapsed().as_millis() as u64, &r);
        r.ok()
    }

    pub fn record<T>(&mut self, name: &str, ms: u64, r: &Result<T>) {
        self.attempts.push(Attempt {
            step: name.to_string(),
            ok: r.is_ok(),
            ms,
            error: r.as_ref().err().map(|e| e.to_string()),
        });
    }

    pub fn all_failed(&self) -> Error {
        Error::AllFailed(
            self.attempts
                .iter()
                .filter(|a| !a.ok)
                .map(|a| format!("{}: {}", a.step, a.error.as_deref().unwrap_or("?")))
                .collect(),
        )
    }
}

/// 测回退用：`OAD_UNFURL_SKIP=x.syndication,x.oembed` 跳过这些步骤。
pub fn skipped(step: &str) -> bool {
    std::env::var("OAD_UNFURL_SKIP").is_ok_and(|v| v.split(',').any(|s| s.trim() == step))
}

/// 计时一个 future，不记录。并发跑几个请求时用。
pub async fn timed<T>(step: &str, fut: impl Future<Output = Result<T>>) -> (Result<T>, u64) {
    if skipped(step) {
        return (Err(Error::Request("skipped (OAD_UNFURL_SKIP)".into())), 0);
    }
    let t = Instant::now();
    let r = fut.await;
    (r, t.elapsed().as_millis() as u64)
}

/// Unix 秒转 RFC 3339（UTC）。不想为这一个函数引 chrono。
pub fn unix_to_rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant 的 civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// 宽松地从 JSON 里取字符串 / 数字。
pub(crate) fn js<'a>(v: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut cur = v;
    for p in path {
        cur = match cur {
            serde_json::Value::Array(a) => a.get(p.parse::<usize>().ok()?)?,
            _ => cur.get(p)?,
        };
    }
    (!cur.is_null()).then_some(cur)
}

pub(crate) fn jstr(v: &serde_json::Value, path: &[&str]) -> Option<String> {
    js(v, path).and_then(|x| match x {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

pub(crate) fn ju64(v: &serde_json::Value, path: &[&str]) -> Option<u64> {
    js(v, path).and_then(|x| {
        x.as_u64()
            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339() {
        assert_eq!(unix_to_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(unix_to_rfc3339(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(unix_to_rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    }
}
