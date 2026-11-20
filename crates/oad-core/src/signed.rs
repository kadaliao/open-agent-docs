//! 文件的签名地址：`/api/v1/files/<id>[/<文件名>]?exp=<unix 秒>&sig=<签名>`、`/api/v1/media/<hash>?exp=…&sig=…`。
//!
//! `<img>`、链接带不了 `Authorization`，跨域 iframe、Web Component、宿主自己渲染的 HTML 里图片就打不开；带上签名就不用登录。
//! 签名 = base64url（不补 `=`）的 HMAC-SHA256(密钥, `"<文件 id 或媒体 hash>|<exp>"`)。不含 `w`（缩略图宽度）和文件名，
//! 客户端可以自己加 `w=`。密钥第一次启动时随机生成，存进存储的 settings 表（多租户每个租户一把，各存各的）。
//! 拿到签名地址的人在有效期内不登录也能取，所以只给调用者看得到的文件签。
use crate::Result;
use crate::storage::Storage;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// 存储里放密钥的键。
pub const SETTING_KEY: &str = "url_signing_key";

/// 签的是什么：上传的文件（按 id）还是转存的图片（按 hash）。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Subject {
    File(String),
    Media(String),
}

impl Subject {
    /// 签名里用的那一段：文件 id 或媒体 hash。
    pub fn key(&self) -> &str {
        match self {
            Subject::File(id) | Subject::Media(id) => id,
        }
    }
}

/// 地址（根相对的 `/api/v1/…`，或者带域名的）→ 签的是什么。不是本站的文件、图片地址返回 None。
pub fn subject_of(url: &str) -> Option<Subject> {
    if let Some(id) = crate::media::own_file_id(url) {
        return Some(Subject::File(id));
    }
    let path = if url.starts_with('/') {
        url.split(['?', '#']).next()?.to_string()
    } else {
        let u = url::Url::parse(url).ok()?;
        if !matches!(u.scheme(), "http" | "https") {
            return None;
        }
        u.path().to_string()
    };
    let hash = path.strip_prefix("/api/v1/media/")?;
    (!hash.is_empty() && hash.len() <= 128 && hash.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then(|| Subject::Media(hash.to_string()))
}

/// 验签的结果（不对的都回 403）。
#[derive(Debug, PartialEq, Eq)]
pub enum Invalid {
    /// 过期了
    Expired,
    /// 签名不对、格式不对
    Bad,
}

pub struct UrlSigner {
    key: Vec<u8>,
}

impl UrlSigner {
    /// 从存储里取密钥；还没有就随机生成一把存进去。
    pub async fn load(storage: &dyn Storage) -> Result<UrlSigner> {
        let key = storage
            .setting_or_init(SETTING_KEY, &crate::ids::new_token())
            .await?;
        Ok(UrlSigner {
            key: key.into_bytes(),
        })
    }

    /// 测试用：给定密钥。
    pub fn with_key(key: &[u8]) -> UrlSigner {
        UrlSigner { key: key.to_vec() }
    }

    pub fn sign(&self, subject: &str, exp: i64) -> String {
        let mut m =
            <Hmac<Sha256> as KeyInit>::new_from_slice(&self.key).expect("HMAC 接受任意长度的密钥");
        m.update(format!("{subject}|{exp}").as_bytes());
        URL_SAFE_NO_PAD.encode(m.finalize().into_bytes())
    }

    /// 加在地址后面的查询参数：`exp=…&sig=…`。
    pub fn query(&self, subject: &str, exp: i64) -> String {
        format!("exp={exp}&sig={}", self.sign(subject, exp))
    }

    /// 验签。`now` 是 unix 秒。
    pub fn verify(
        &self,
        subject: &str,
        exp: &str,
        sig: &str,
        now: i64,
    ) -> std::result::Result<(), Invalid> {
        let exp_n: i64 = exp.parse().map_err(|_| Invalid::Bad)?;
        // exp 要原样是那个数（`+5`、`05` 之类不认，免得同一个签名有好几种写法）
        if exp_n.to_string() != exp {
            return Err(Invalid::Bad);
        }
        let want = self.sign(subject, exp_n);
        if !bool::from(want.as_bytes().ct_eq(sig.as_bytes())) {
            return Err(Invalid::Bad);
        }
        if exp_n <= now {
            return Err(Invalid::Expired);
        }
        Ok(())
    }
}

/// 有效期 `ttl` 秒的过期时间（unix 秒）。往后凑整到一个刻度（有效期的 1/24，最多一小时），
/// 同一段时间里签出来的地址一样，浏览器缓存、宿主缓存的 HTML 才用得上。实际有效期在 `[ttl, ttl + 刻度)` 之间。
pub fn expiry(ttl: u64, now: i64) -> i64 {
    let ttl = ttl.max(1) as i64;
    let step = (ttl / 24).clamp(1, 3600);
    let at = now + ttl;
    (at + step - 1) / step * step
}

/// 现在（unix 秒）。
pub fn now_secs() -> i64 {
    crate::ids::now_ms() / 1000
}

/// 在地址上加 `exp`、`sig`（先去掉原来的 `exp`、`sig`；`w` 之类别的参数和 `#…` 留着）。
pub fn append_query(url: &str, query: &str) -> String {
    let (main, frag) = match url.split_once('#') {
        Some((m, f)) => (m, Some(f)),
        None => (url, None),
    };
    let (path, q) = main.split_once('?').unwrap_or((main, ""));
    let mut parts: Vec<&str> = q
        .split('&')
        .filter(|kv| {
            let k = kv.split('=').next().unwrap_or("");
            !kv.is_empty() && k != "exp" && k != "sig"
        })
        .collect();
    parts.push(query);
    let mut out = format!("{path}?{}", parts.join("&"));
    if let Some(f) = frag {
        out.push('#');
        out.push_str(f);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let s = UrlSigner::with_key(b"k");
        let sig = s.sign("abcdefghijkl", 2_000_000_000);
        assert_eq!(s.verify("abcdefghijkl", "2000000000", &sig, 1), Ok(()));
        assert_eq!(
            s.verify("abcdefghijkl", "2000000000", &sig, 2_000_000_000),
            Err(Invalid::Expired)
        );
        assert_eq!(
            s.verify("abcdefghijkm", "2000000000", &sig, 1),
            Err(Invalid::Bad)
        );
        assert_eq!(
            s.verify("abcdefghijkl", "2000000001", &sig, 1),
            Err(Invalid::Bad)
        );
        assert_eq!(
            s.verify("abcdefghijkl", "02000000000", &sig, 1),
            Err(Invalid::Bad)
        );
        assert_eq!(s.verify("abcdefghijkl", "x", &sig, 1), Err(Invalid::Bad));
        // 别的密钥签的不认
        let other = UrlSigner::with_key(b"k2").sign("abcdefghijkl", 2_000_000_000);
        assert_eq!(
            s.verify("abcdefghijkl", "2000000000", &other, 1),
            Err(Invalid::Bad)
        );
        assert!(!sig.contains('=') && !sig.contains('+') && !sig.contains('/'));
    }

    #[test]
    fn expiry_rounds_up() {
        let e = expiry(86400, 1_000);
        assert!((1_000 + 86400..1_000 + 86400 + 3600).contains(&e) && e % 3600 == 0);
        assert_eq!(expiry(86400, 0), 86400);
        assert_eq!(expiry(60, 7), 68); // 刻度 2 秒
        assert_eq!(expiry(10, 7), 17); // 刻度 1 秒
    }

    #[test]
    fn subjects_and_query() {
        assert_eq!(
            subject_of("/api/v1/files/abcdefghijkl/a.png?w=320"),
            Some(Subject::File("abcdefghijkl".into()))
        );
        assert_eq!(
            subject_of("https://docs.example.com/api/v1/media/0a1b2c"),
            Some(Subject::Media("0a1b2c".into()))
        );
        assert_eq!(subject_of("/api/v1/media/../x"), None);
        assert_eq!(subject_of("/p/abcdefghijkl"), None);
        assert_eq!(
            append_query("/api/v1/files/a/b.png?w=320&exp=1&sig=x#f", "exp=2&sig=y"),
            "/api/v1/files/a/b.png?w=320&exp=2&sig=y#f"
        );
        assert_eq!(append_query("/x", "exp=2&sig=y"), "/x?exp=2&sig=y");
    }
}
