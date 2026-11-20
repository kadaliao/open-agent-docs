//! 嵌入工具（契约 §10）：宿主签 JWT（HS256）、拼 iframe 的地址。
use crate::request::enc;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// JWT 里 `workspaces` 的一项：工作区 id，或者 `{ id, role }`。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JwtWorkspace {
    /// 只给 id（角色按服务器默认）
    Id(String),
    /// id 和角色（`viewer` / `editor` / `admin` / `owner`）
    WithRole { id: String, role: String },
}

impl JwtWorkspace {
    pub fn with_role(id: impl Into<String>, role: impl Into<String>) -> JwtWorkspace {
        JwtWorkspace::WithRole {
            id: id.into(),
            role: role.into(),
        }
    }
}

impl From<&str> for JwtWorkspace {
    fn from(id: &str) -> JwtWorkspace {
        JwtWorkspace::Id(id.to_string())
    }
}

impl From<String> for JwtWorkspace {
    fn from(id: String) -> JwtWorkspace {
        JwtWorkspace::Id(id)
    }
}

/// [`sign_jwt`] 的可选项。
#[derive(Clone, Debug, PartialEq)]
pub struct JwtOptions {
    /// `name`
    pub name: Option<String>,
    /// `email`
    pub email: Option<String>,
    /// `workspaces`：不给就进默认工作区
    pub workspaces: Option<Vec<JwtWorkspace>>,
    /// 多久过期（默认 3600 秒）
    pub ttl: Duration,
    /// `iss`（服务器设了 `OAD_JWT_ISSUER` 时要对得上）
    pub issuer: Option<String>,
    /// `aud`（服务器设了 `OAD_JWT_AUDIENCE` 时要对得上）
    pub audience: Option<String>,
    /// 当成“现在”的时间（测试用；默认真的现在）
    pub now: Option<SystemTime>,
}

impl Default for JwtOptions {
    fn default() -> Self {
        JwtOptions {
            name: None,
            email: None,
            workspaces: None,
            ttl: Duration::from_secs(3600),
            issuer: None,
            audience: None,
            now: None,
        }
    }
}

#[derive(Serialize)]
struct Claims<'a> {
    sub: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspaces: Option<&'a [JwtWorkspace]>,
    iat: u64,
    exp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    iss: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    aud: Option<&'a str>,
}

/// 签一个 HS256 的 JWT，给 `OAD_AUTH=jwt OAD_JWT_SECRET=<secret>` 的服务器用（iframe 的 `#token=`、`Authorization: Bearer`）。
///
/// claims：`sub`、`name`、`email`、`workspaces`、`iat`、`exp`、`iss`、`aud`（没给的不写）。只做 HS256；
/// 要 RS256 / ES256 的用自己的 JWT 库签同样的 claims。
///
/// ```
/// use open_agent_docs::{JwtOptions, sign_jwt};
/// let token = sign_jwt("jwt-secret", "u-42", &JwtOptions { name: Some("王小明".into()), ..Default::default() });
/// assert_eq!(token.split('.').count(), 3);
/// ```
pub fn sign_jwt(secret: &str, user_id: &str, opts: &JwtOptions) -> String {
    let now = opts
        .now
        .unwrap_or_else(SystemTime::now)
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let claims = Claims {
        sub: user_id,
        name: opts.name.as_deref(),
        email: opts.email.as_deref(),
        workspaces: opts.workspaces.as_deref(),
        iat: now,
        exp: now + opts.ttl.as_secs(),
        iss: opts.issuer.as_deref(),
        aud: opts.audience.as_deref(),
    };
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).expect("claims 能序列化"));
    let signing_input = format!("{header}.{payload}");
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC 接受任意长度的密钥");
    mac.update(signing_input.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{signing_input}.{sig}")
}

/// [`embed_url`] 的可选项。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EmbedOptions {
    /// `light` / `dark`
    pub theme: Option<String>,
    /// `zh` / `en`
    pub lang: Option<String>,
    /// 登录用的 token（一般是 [`sign_jwt`] 签的）：放在 `#` 后面，不进服务器日志、不进 Referer
    pub token: Option<String>,
}

/// iframe 的地址：`<base>/p/<page_id>?embed=1[&theme=][&lang=][#token=<token>]`。
///
/// ```
/// use open_agent_docs::{EmbedOptions, embed_url};
/// let url = embed_url("https://docs.example.com/", "p1", &EmbedOptions { theme: Some("dark".into()), ..Default::default() });
/// assert_eq!(url, "https://docs.example.com/p/p1?embed=1&theme=dark");
/// ```
pub fn embed_url(base_url: &str, page_id: &str, opts: &EmbedOptions) -> String {
    let base = base_url.trim().trim_end_matches('/');
    let mut url = format!("{base}/p/{}?embed=1", enc(page_id));
    if let Some(t) = &opts.theme {
        url.push_str("&theme=");
        url.push_str(&enc(t));
    }
    if let Some(l) = &opts.lang {
        url.push_str("&lang=");
        url.push_str(&enc(l));
    }
    if let Some(t) = &opts.token {
        url.push_str("#token=");
        url.push_str(&enc(t));
    }
    url
}
