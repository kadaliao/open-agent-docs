//! Agent webhook 验签（契约 §9）。
//!
//! 服务器 POST 到 Agent 设的地址，带 `X-OAD-Timestamp`（毫秒）、`X-OAD-Signature: sha256=<hex>`、`X-OAD-Delivery`。
//! 签名 = HMAC-SHA256(webhook 密钥, `"<X-OAD-Timestamp>.<请求体原文>"`) 的小写十六进制。
use crate::error::{Error, Result, WebhookErrorKind};
use crate::types::AgentNotification;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 默认允许的时间差：前后 5 分钟
pub const DEFAULT_TOLERANCE: Duration = Duration::from_secs(300);

/// 能按名字（大小写不论）取请求头的东西：`http::HeaderMap`（axum、reqwest 的）、`HashMap`、`BTreeMap`、`[(k, v)]`。
pub trait WebhookHeaders {
    /// 取一个头；`name` 是小写的
    fn header(&self, name: &str) -> Option<&str>;
}

impl WebhookHeaders for reqwest::header::HeaderMap {
    fn header(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(|v| v.to_str().ok())
    }
}

fn find<'a, K: AsRef<str> + 'a, V: AsRef<str> + 'a>(
    it: impl IntoIterator<Item = (&'a K, &'a V)>,
    name: &str,
) -> Option<&'a str> {
    it.into_iter()
        .find(|(k, _)| k.as_ref().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_ref())
}

impl<K: AsRef<str>, V: AsRef<str>, S> WebhookHeaders for HashMap<K, V, S> {
    fn header(&self, name: &str) -> Option<&str> {
        find(self.iter(), name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>> WebhookHeaders for BTreeMap<K, V> {
    fn header(&self, name: &str) -> Option<&str> {
        find(self.iter(), name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>> WebhookHeaders for [(K, V)] {
    fn header(&self, name: &str) -> Option<&str> {
        find(self.iter().map(|(k, v)| (k, v)), name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>> WebhookHeaders for Vec<(K, V)> {
    fn header(&self, name: &str) -> Option<&str> {
        self.as_slice().header(name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>, const N: usize> WebhookHeaders for [(K, V); N] {
    fn header(&self, name: &str) -> Option<&str> {
        self.as_slice().header(name)
    }
}

impl<T: WebhookHeaders + ?Sized> WebhookHeaders for &T {
    fn header(&self, name: &str) -> Option<&str> {
        (**self).header(name)
    }
}

/// 验签并解析成 [`AgentNotification`]；时间戳和现在差 5 分钟以内才算数。
///
/// 失败是 [`Error::Webhook`]（[`WebhookErrorKind`]：`Signature` / `Stale` / `Malformed`），
/// 签名对但正文不是通知是 [`Error::Decode`]。`body` 要用收到的原始字节，别先解析再序列化。
///
/// ```no_run
/// # use open_agent_docs as oad;
/// # fn handle(headers: &oad::reqwest::header::HeaderMap, body: &[u8]) -> oad::Result<()> {
/// let secret = std::env::var("OAD_WEBHOOK_SECRET").unwrap();
/// let note = oad::verify_webhook(&secret, headers, body)?;
/// println!("{} 在《{}》里叫了 {}：{}", note.comment.body, note.page.title, note.agent.name, note.prompt);
/// # Ok(())
/// # }
/// ```
pub fn verify_webhook<H: WebhookHeaders + ?Sized>(
    secret: &str,
    headers: &H,
    body: impl AsRef<[u8]>,
) -> Result<AgentNotification> {
    verify_webhook_at(secret, headers, body, SystemTime::now(), DEFAULT_TOLERANCE)
}

/// 同 [`verify_webhook`]，但“现在”和允许的时间差自己给（测试用）。
pub fn verify_webhook_at<H: WebhookHeaders + ?Sized>(
    secret: &str,
    headers: &H,
    body: impl AsRef<[u8]>,
    now: SystemTime,
    tolerance: Duration,
) -> Result<AgentNotification> {
    let body = body.as_ref();
    verify_webhook_signature(secret, headers, body, now, tolerance)?;
    serde_json::from_slice(body)
        .map_err(|e| Error::Decode(format!("webhook 正文不是 AgentNotification：{e}")))
}

/// 只验签，不解析正文（自己解析、或者正文不是通知时用）。
pub fn verify_webhook_signature<H: WebhookHeaders + ?Sized>(
    secret: &str,
    headers: &H,
    body: impl AsRef<[u8]>,
    now: SystemTime,
    tolerance: Duration,
) -> Result<()> {
    let bad = |k| Err(Error::Webhook(k));
    let (Some(ts), Some(sig)) = (
        headers.header("x-oad-timestamp"),
        headers.header("x-oad-signature"),
    ) else {
        return bad(WebhookErrorKind::Malformed);
    };
    let Ok(ts_ms) = ts.trim().parse::<i64>() else {
        return bad(WebhookErrorKind::Malformed);
    };
    let Some(want) = sig.trim().strip_prefix("sha256=").and_then(hex) else {
        return bad(WebhookErrorKind::Malformed);
    };
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC 接受任意长度的密钥");
    mac.update(ts.as_bytes());
    mac.update(b".");
    mac.update(body.as_ref());
    // verify_slice 是常量时间比较
    if mac.verify_slice(&want).is_err() {
        return bad(WebhookErrorKind::Signature);
    }
    let now_ms = match now.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    };
    if now_ms.abs_diff(ts_ms) > tolerance.as_millis() as u64 {
        return bad(WebhookErrorKind::Stale);
    }
    Ok(())
}

/// 64 位十六进制 → 32 字节
fn hex(s: &str) -> Option<Vec<u8>> {
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}
