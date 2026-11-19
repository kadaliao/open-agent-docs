//! `OAD_AUTH=jwt`：宿主签 JWT，浏览器 / 程序直接带着来（`Authorization: Bearer`、`?token=`、或 cookie `oad_token`）。
//! 验签用共享密钥（HS256）或 JWKS 地址（RS256 / ES256 / EdDSA，按 kid 缓存 10 分钟，遇到不认识的 kid 再取，最多 30 秒一次）。
//! claims：`sub`（必须）、`name`、`email`、`workspaces`（工作区 id 的数组，或 `[{ id, role }]`；不给就进默认工作区）。
//! 允许的时钟误差 `leeway` 秒（默认 60）；可选核对 `iss`、`aud`。
use crate::auth::{AuthMode, AuthProvider, AuthRequest};
use crate::model::*;
use crate::storage::Storage;
use crate::{Error, Result};
use async_trait::async_trait;
use jsonwebtoken::jwk::{AlgorithmParameters, EllipticCurve, JwkSet, KeyAlgorithm};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const JWT_COOKIE: &str = "oad_token";
const JWKS_TTL: Duration = Duration::from_secs(600);
const JWKS_MIN_REFETCH: Duration = Duration::from_secs(30);

/// JWT 的配置。
#[derive(Clone, Debug, Default)]
pub struct JwtConfig {
    /// HS256 的共享密钥
    pub secret: Option<String>,
    /// JWKS 的地址（https://…/.well-known/jwks.json）
    pub jwks_url: Option<String>,
    pub issuer: Option<String>,
    pub audience: Option<String>,
    /// 允许的时钟误差（秒）
    pub leeway: u64,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    workspaces: Option<Vec<WsClaim>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WsClaim {
    Id(String),
    Full { id: String, role: Option<String> },
}

struct JwksCache {
    set: Option<JwkSet>,
    fetched: Option<Instant>,
    /// 上一次因为不认识的 kid 去取的时候
    forced: Option<Instant>,
}

pub struct JwtAuth {
    storage: Arc<dyn Storage>,
    cfg: JwtConfig,
    default_workspace: String,
    http: reqwest::Client,
    jwks: Mutex<JwksCache>,
}

impl JwtAuth {
    pub fn new(
        storage: Arc<dyn Storage>,
        cfg: JwtConfig,
        default_workspace: String,
    ) -> Result<Self> {
        if cfg.secret.as_deref().is_none_or(str::is_empty)
            && cfg.jwks_url.as_deref().is_none_or(str::is_empty)
        {
            return Err(Error::BadRequest(
                "OAD_AUTH=jwt 要给 OAD_JWT_SECRET 或 OAD_JWT_JWKS_URL".into(),
            ));
        }
        crate::notify::install_tls();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| Error::Internal(e.to_string()))?;
        Ok(JwtAuth {
            storage,
            cfg,
            default_workspace,
            http,
            jwks: Mutex::new(JwksCache {
                set: None,
                fetched: None,
                forced: None,
            }),
        })
    }

    fn validation(&self, alg: Algorithm) -> Validation {
        let mut v = Validation::new(alg);
        v.leeway = self.cfg.leeway;
        v.set_required_spec_claims(&["exp", "sub"]);
        if let Some(i) = &self.cfg.issuer {
            v.set_issuer(&[i]);
        }
        match &self.cfg.audience {
            Some(a) => v.set_audience(&[a]),
            None => v.validate_aud = false,
        }
        v
    }

    /// 取（缓存的）JWKS；force 时不管缓存再取一次（但两次之间至少隔 30 秒）。
    async fn jwks(&self, force: bool) -> Result<Option<JwkSet>> {
        let Some(url) = &self.cfg.jwks_url else {
            return Ok(None);
        };
        {
            let mut c = self.jwks.lock().unwrap();
            let fresh = c.fetched.is_some_and(|t| t.elapsed() < JWKS_TTL);
            let too_soon = c.forced.is_some_and(|t| t.elapsed() < JWKS_MIN_REFETCH);
            if c.set.is_some() && (fresh && !force || force && too_soon) {
                return Ok(c.set.clone());
            }
            if force {
                c.forced = Some(Instant::now());
            }
        }
        let body = self
            .http
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| Error::BadGateway(format!("取 JWKS 失败：{e}")))?
            .bytes()
            .await
            .map_err(|e| Error::BadGateway(format!("取 JWKS 失败：{e}")))?;
        let set: JwkSet = serde_json::from_slice(&body)
            .map_err(|e| Error::BadGateway(format!("JWKS 格式不对：{e}")))?;
        let mut c = self.jwks.lock().unwrap();
        c.set = Some(set.clone());
        c.fetched = Some(Instant::now());
        Ok(Some(set))
    }

    /// 验签、取 claims。签名不对、过期、iss / aud 不对：None。
    async fn verify(&self, token: &str) -> Result<Option<Claims>> {
        let Ok(header) = decode_header(token) else {
            return Ok(None);
        };
        if header.alg == Algorithm::HS256
            || header.alg == Algorithm::HS384
            || header.alg == Algorithm::HS512
        {
            let Some(secret) = self.cfg.secret.as_deref().filter(|s| !s.is_empty()) else {
                return Ok(None);
            };
            let key = DecodingKey::from_secret(secret.as_bytes())
                .map_err(|e| Error::Internal(e.to_string()))?;
            return Ok(decode::<Claims>(token, &key, &self.validation(header.alg))
                .ok()
                .map(|d| d.claims));
        }
        for force in [false, true] {
            let Some(set) = self.jwks(force).await? else {
                return Ok(None);
            };
            let jwk = match &header.kid {
                Some(k) => set.find(k),
                None => set.keys.first(),
            };
            let Some(jwk) = jwk else { continue };
            let alg = match (&jwk.common.key_algorithm, &jwk.algorithm) {
                (Some(KeyAlgorithm::RS256), _) => Algorithm::RS256,
                (Some(KeyAlgorithm::RS384), _) => Algorithm::RS384,
                (Some(KeyAlgorithm::RS512), _) => Algorithm::RS512,
                (Some(KeyAlgorithm::ES256), _) => Algorithm::ES256,
                (Some(KeyAlgorithm::ES384), _) => Algorithm::ES384,
                (Some(KeyAlgorithm::EdDSA), _) => Algorithm::EdDSA,
                (_, AlgorithmParameters::RSA(_)) => Algorithm::RS256,
                (_, AlgorithmParameters::EllipticCurve(p)) if p.curve == EllipticCurve::P384 => {
                    Algorithm::ES384
                }
                (_, AlgorithmParameters::EllipticCurve(_)) => Algorithm::ES256,
                (_, AlgorithmParameters::OctetKeyPair(_)) => Algorithm::EdDSA,
                _ => return Ok(None),
            };
            // 签名的算法要和这把钥匙的一致（防止拿公钥当 HMAC 密钥）
            if alg != header.alg {
                return Ok(None);
            }
            let Ok(key) = DecodingKey::from_jwk(jwk) else {
                return Ok(None);
            };
            return Ok(decode::<Claims>(token, &key, &self.validation(alg))
                .ok()
                .map(|d| d.claims));
        }
        Ok(None)
    }

    /// claims → 用户（第一次见到时建；名字、邮箱跟着变）和成员关系。
    async fn principal(&self, c: Claims) -> Result<Principal> {
        let ext = format!("jwt:{}", c.sub);
        let name = c
            .name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| c.sub.clone());
        let user = match self.storage.user_by_external(&ext).await? {
            Some(u) => {
                if u.name != name {
                    self.storage
                        .update_user(
                            &u.id,
                            PrefsPatch {
                                name: Some(name.clone()),
                                ..Default::default()
                            },
                        )
                        .await?;
                }
                User { name, ..u }
            }
            None => {
                let new = |email| NewUser {
                    name: name.clone(),
                    email,
                    password_hash: None,
                    external_id: Some(ext.clone()),
                    kind: PrincipalKind::User,
                };
                match self.storage.create_user(new(c.email.clone())).await {
                    Ok(u) => u,
                    // 邮箱和别的账号撞了就不存邮箱；两个请求同时建，后到的去读先到的
                    Err(Error::Conflict(_)) => match self.storage.user_by_external(&ext).await? {
                        Some(u) => u,
                        None => self.storage.create_user(new(None)).await?,
                    },
                    Err(e) => return Err(e),
                }
            }
        };
        match &c.workspaces {
            None => {
                crate::join_default(&*self.storage, &self.default_workspace, &user.id).await?;
            }
            Some(list) => {
                for w in list {
                    let (id, role) = match w {
                        WsClaim::Id(id) => (id, None),
                        WsClaim::Full { id, role } => (id, role.as_deref().and_then(Role::parse)),
                    };
                    if self.storage.workspace(id).await?.is_none() {
                        continue;
                    }
                    let cur = self.storage.member_role(id, &user.id).await?;
                    match (cur, role) {
                        // claims 里给了角色：以 claims 为准
                        (c, Some(r)) if c != Some(r) => {
                            self.storage.set_member(id, &user.id, r).await?
                        }
                        (None, None) => self.storage.set_member(id, &user.id, Role::Editor).await?,
                        _ => {}
                    }
                }
            }
        }
        Ok(Principal::from(&user))
    }
}

#[async_trait]
impl AuthProvider for JwtAuth {
    fn mode(&self) -> AuthMode {
        AuthMode::Jwt
    }

    async fn authenticate(&self, req: &AuthRequest<'_>) -> Result<Option<Principal>> {
        let mut tokens = req.session_tokens();
        if let Some(t) = crate::auth::cookie(req.headers, JWT_COOKIE) {
            tokens.push(t);
        }
        for t in tokens {
            // 三段的才是 JWT（会话令牌、Agent token 不是）
            if t.split('.').count() != 3 {
                continue;
            }
            if let Some(c) = self.verify(t).await? {
                return Ok(Some(self.principal(c).await?));
            }
        }
        Ok(None)
    }
}
