//! 登录方式。三种：内置账号（argon2 + 会话 cookie）、宿主可信请求头、不登录（本机单人）。
//! 只管“这是谁”；能不能进某个工作区由成员表决定（[`crate::Core::role`]）。
use crate::model::*;
use crate::storage::Storage;
use crate::{Error, Result, ids};
use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash};
use async_trait::async_trait;
use http::HeaderMap;
use serde::Serialize;
use std::sync::{Arc, OnceLock};
use subtle::ConstantTimeEq;

pub const SESSION_COOKIE: &str = "oad_session";
pub const SESSION_DAYS: i64 = 30;

pub const H_USER_ID: &str = "x-oad-user-id";
pub const H_USER_NAME: &str = "x-oad-user-name";
pub const H_USER_EMAIL: &str = "x-oad-user-email";
pub const H_SECRET: &str = "x-oad-secret";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Local,
    Header,
    None,
}

impl AuthMode {
    pub fn parse(s: &str) -> Option<AuthMode> {
        match s {
            "local" => Some(AuthMode::Local),
            "header" => Some(AuthMode::Header),
            "none" => Some(AuthMode::None),
            _ => None,
        }
    }
}

/// 认证要看的请求信息。
pub struct AuthRequest<'a> {
    pub headers: &'a HeaderMap,
    /// `?token=`（跨源嵌入的 WebSocket 用）。
    pub query_token: Option<&'a str>,
}

impl AuthRequest<'_> {
    /// 带来的会话令牌，按优先级：`Authorization: Bearer`、`?token=`、cookie。
    /// 明确给的排前面，浏览器里过期的 cookie 不会挡住它们；认证时依次试。
    pub fn session_tokens(&self) -> Vec<&str> {
        let bearer = self
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let mut out: Vec<&str> = [
            bearer,
            self.query_token,
            cookie(self.headers, SESSION_COOKIE),
        ]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
        out.dedup();
        out
    }
}

pub fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    fn mode(&self) -> AuthMode;
    /// 认不出来返回 None（接口回 401）。
    async fn authenticate(&self, req: &AuthRequest<'_>) -> Result<Option<Principal>>;
    /// 用户名密码登录，返回 (会话令牌, 用户)。只有内置账号支持。
    async fn login(&self, _email: &str, _password: &str) -> Result<(String, User)> {
        Err(Error::BadRequest("当前登录方式不支持密码登录".into()))
    }
    async fn logout(&self, _req: &AuthRequest<'_>) -> Result<()> {
        Ok(())
    }
}

/// argon2id 哈希（耗 CPU，放在 blocking 线程上）。
pub async fn hash_password(password: &str) -> Result<String> {
    let pw = password.to_string();
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(pw.as_bytes())
            .map(|h| h.to_string())
            .map_err(|e| Error::Internal(e.to_string()))
    })
    .await
    .map_err(|e| Error::Internal(e.to_string()))?
}

pub async fn verify_password(password: &str, hash: &str) -> bool {
    let (pw, h) = (password.to_string(), hash.to_string());
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&h)
            .map(|parsed| {
                Argon2::default()
                    .verify_password(pw.as_bytes(), &parsed)
                    .is_ok()
            })
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// 用户不存在时也算一遍哈希，不让响应时间泄露邮箱是否注册过。
fn dummy_hash() -> &'static str {
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| {
        Argon2::default()
            .hash_password(b"dummy-password")
            .map(|h| h.to_string())
            .unwrap_or_default()
    })
}

/// 内置账号。
pub struct LocalAuth {
    storage: Arc<dyn Storage>,
}

impl LocalAuth {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        LocalAuth { storage }
    }
}

#[async_trait]
impl AuthProvider for LocalAuth {
    fn mode(&self) -> AuthMode {
        AuthMode::Local
    }

    async fn authenticate(&self, req: &AuthRequest<'_>) -> Result<Option<Principal>> {
        for token in req.session_tokens() {
            let user = self
                .storage
                .session_user(&ids::hash_token(token), ids::now_ms())
                .await?;
            if let Some(u) = user {
                return Ok(Some(Principal::from(&u)));
            }
        }
        Ok(None)
    }

    async fn login(&self, email: &str, password: &str) -> Result<(String, User)> {
        let found = self.storage.user_by_email(email.trim()).await?;
        let ok = match &found {
            Some((_, Some(hash))) => verify_password(password, hash).await,
            _ => {
                let _ = verify_password(password, dummy_hash()).await;
                false
            }
        };
        let user = match (ok, found) {
            (true, Some((u, _))) => u,
            _ => return Err(Error::Unauthorized("邮箱或密码不对".into())),
        };
        let token = ids::new_token();
        let expires = ids::now_ms() + SESSION_DAYS * 24 * 3600 * 1000;
        self.storage
            .create_session(&ids::hash_token(&token), &user.id, expires)
            .await?;
        Ok((token, user))
    }

    async fn logout(&self, req: &AuthRequest<'_>) -> Result<()> {
        for token in req.session_tokens() {
            self.storage.delete_session(&ids::hash_token(token)).await?;
        }
        Ok(())
    }
}

/// 宿主可信请求头：宿主自己做登录，转发时带用户信息和共享密钥。用户第一次出现时自动建，并加进默认工作区。
pub struct HeaderAuth {
    storage: Arc<dyn Storage>,
    secret: String,
    workspace_id: String,
}

impl HeaderAuth {
    pub fn new(storage: Arc<dyn Storage>, secret: String, default_workspace: String) -> Self {
        HeaderAuth {
            storage,
            secret,
            workspace_id: default_workspace,
        }
    }
}

/// 请求头的值：UTF-8，可以百分号编码（中文名字常这样传）。
fn header(h: &HeaderMap, name: &str) -> Option<String> {
    let raw = std::str::from_utf8(h.get(name)?.as_bytes()).ok()?.trim();
    let v = if raw.contains('%') {
        percent_decode(raw).unwrap_or_else(|| raw.to_string())
    } else {
        raw.to_string()
    };
    (!v.is_empty()).then_some(v)
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[async_trait]
impl AuthProvider for HeaderAuth {
    fn mode(&self) -> AuthMode {
        AuthMode::Header
    }

    async fn authenticate(&self, req: &AuthRequest<'_>) -> Result<Option<Principal>> {
        let given = req
            .headers
            .get(H_SECRET)
            .map(|v| v.as_bytes())
            .unwrap_or_default();
        if self.secret.is_empty() || !bool::from(given.ct_eq(self.secret.as_bytes())) {
            return Ok(None);
        }
        let Some(ext) = header(req.headers, H_USER_ID) else {
            return Ok(None);
        };
        let ext = ext.as_str();
        let ext_id = format!("header:{ext}");
        if let Some(u) = self.storage.user_by_external(&ext_id).await? {
            return Ok(Some(Principal::from(&u)));
        }
        let name = header(req.headers, H_USER_NAME).unwrap_or_else(|| ext.to_string());
        let email = header(req.headers, H_USER_EMAIL);
        let new = |email| NewUser {
            name: name.clone(),
            email,
            password_hash: None,
            external_id: Some(ext_id.clone()),
            kind: PrincipalKind::User,
        };
        // 邮箱和内置账号撞了就不存邮箱；两个请求同时建同一个用户，后到的去读先到的
        let user = match self.storage.create_user(new(email)).await {
            Ok(u) => u,
            Err(Error::Conflict(_)) => match self.storage.user_by_external(&ext_id).await? {
                Some(u) => return Ok(Some(Principal::from(&u))),
                None => self.storage.create_user(new(None)).await?,
            },
            Err(e) => return Err(e),
        };
        crate::join_default(&*self.storage, &self.workspace_id, &user.id).await?;
        Ok(Some(Principal::from(&user)))
    }
}

/// 不登录：所有请求都是同一个本机用户。
pub struct NoAuth {
    principal: Principal,
}

impl NoAuth {
    pub fn new(user: &User) -> Self {
        NoAuth {
            principal: Principal::from(user),
        }
    }
}

#[async_trait]
impl AuthProvider for NoAuth {
    fn mode(&self) -> AuthMode {
        AuthMode::None
    }
    async fn authenticate(&self, _req: &AuthRequest<'_>) -> Result<Option<Principal>> {
        Ok(Some(self.principal.clone()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn percent() {
        assert_eq!(
            super::percent_decode("%E5%AE%BF%E4%B8%BB").as_deref(),
            Some("宿主")
        );
        assert_eq!(super::percent_decode("50%"), None);
    }
}
