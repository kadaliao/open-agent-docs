//! 一个请求的样子（生成的代码拼好交给 [`Client`](crate::Client) 发）。重试时要重发，所以正文存成 `Bytes`。
use crate::error::{Error, Result};
use bytes::Bytes;
use reqwest::Method;
use serde::Serialize;

#[derive(Clone, Debug)]
pub(crate) enum Body {
    None,
    Raw {
        content_type: &'static str,
        data: Bytes,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Spec {
    pub method: Method,
    /// `/api/v1` 之后的路径，路径参数已经编码好
    pub path: String,
    pub query: Vec<(&'static str, String)>,
    pub headers: Vec<(&'static str, String)>,
    pub body: Body,
}

impl Spec {
    pub fn new(method: Method, path: String) -> Spec {
        Spec {
            method,
            path,
            query: Vec::new(),
            headers: Vec::new(),
            body: Body::None,
        }
    }

    pub fn query(&mut self, k: &'static str, v: impl Into<String>) {
        self.query.push((k, v.into()));
    }

    pub fn header(&mut self, k: &'static str, v: impl Into<String>) {
        self.headers.push((k, v.into()));
    }

    pub fn json<T: Serialize>(&mut self, body: &T) -> Result<()> {
        let data = serde_json::to_vec(body)
            .map_err(|e| Error::Config(format!("请求体序列化失败：{e}")))?;
        self.body = Body::Raw {
            content_type: "application/json",
            data: data.into(),
        };
        Ok(())
    }

    pub fn raw(&mut self, content_type: &'static str, data: Bytes) {
        self.body = Body::Raw { content_type, data };
    }

    /// 路径 + 查询串
    pub fn url_path(&self) -> String {
        let mut s = self.path.clone();
        for (i, (k, v)) in self.query.iter().enumerate() {
            s.push(if i == 0 { '?' } else { '&' });
            s.push_str(&enc(k));
            s.push('=');
            s.push_str(&enc(v));
        }
        s
    }

    /// 重试是否安全：GET、PUT、DELETE（还有 HEAD、OPTIONS）
    pub fn idempotent(&self) -> bool {
        matches!(
            self.method,
            Method::GET | Method::PUT | Method::DELETE | Method::HEAD | Method::OPTIONS
        )
    }
}

/// 百分号编码，和 JS 的 `encodeURIComponent` 一样（字母数字和 `-_.!~*'()` 不编）。
pub(crate) fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_like_js() {
        assert_eq!(enc("a b/c"), "a%20b%2Fc");
        assert_eq!(enc("王小明"), "%E7%8E%8B%E5%B0%8F%E6%98%8E");
        assert_eq!(enc("x@example.com"), "x%40example.com");
        assert_eq!(enc("-_.!~*'()"), "-_.!~*'()");
    }

    #[test]
    fn query_string() {
        let mut s = Spec::new(Method::GET, "/workspaces/w/search".into());
        s.query("q", "周 报");
        s.query("limit", "5");
        assert_eq!(
            s.url_path(),
            "/workspaces/w/search?q=%E5%91%A8%20%E6%8A%A5&limit=5"
        );
    }
}
