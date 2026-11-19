//! OAD_AUTH=jwt：HS256 共享密钥、JWKS（本机起一个 JWKS 服务，EdDSA，按 kid 缓存、换钥匙后再取）、时钟误差、
//! iss / aud、claims 里的工作区和角色、token 放在 Bearer / ?token= / cookie。
mod common;
use axum::Router;
use axum::routing::get;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::*;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use oad_core::AuthConfig;
use oad_core::jwt::JwtConfig;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn now() -> i64 {
    oad_core::ids::now_ms() / 1000
}

fn hs(secret: &str, claims: Value) -> String {
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()).unwrap(),
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hs256() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = JwtConfig {
        secret: Some("共享密钥-0123456789".into()),
        issuer: Some("https://host.example.com".into()),
        leeway: 60,
        ..Default::default()
    };
    let s = Server::start(dir.path(), AuthConfig::Jwt(cfg), fast_hub()).await;
    let ws = s.ws_id();
    let c = http();
    let (_, conf) = json(c.get(s.api("/auth/config"))).await;
    assert_eq!(conf["mode"], "jwt");
    let claims = |extra: Value| {
        let mut v = json!({ "sub": "u-1", "name": "宿主用户", "email": "u1@host.example.com", "iss": "https://host.example.com", "exp": now() + 600 });
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    let key = "共享密钥-0123456789";
    // 第一次来：建用户、按 claims 进工作区（给了角色就用它）
    let t = hs(
        key,
        claims(json!({ "workspaces": [{ "id": ws, "role": "viewer" }] })),
    );
    let (st, me) = json(c.get(s.api("/me")).bearer_auth(&t)).await;
    assert_eq!(st, 200, "{me}");
    assert_eq!(me["user"]["name"], "宿主用户");
    assert_eq!(me["workspaces"][0]["role"], "viewer");
    let (st, _) = json(
        c.post(s.api(&format!("/workspaces/{ws}/pages")))
            .bearer_auth(&t)
            .json(&json!({})),
    )
    .await;
    assert_eq!(st, 403, "只读");
    // 角色改了、名字改了：跟着 claims 变
    let t2 = hs(
        key,
        claims(json!({ "name": "改了名", "workspaces": [{ "id": ws, "role": "editor" }] })),
    );
    let (_, me) = json(c.get(s.api("/me")).bearer_auth(&t2)).await;
    assert_eq!(
        (
            me["user"]["name"].as_str(),
            me["workspaces"][0]["role"].as_str()
        ),
        (Some("改了名"), Some("editor"))
    );
    // ?token=、cookie 也行
    let (st, _) = json(c.get(format!("{}?token={t2}", s.api("/me")))).await;
    assert_eq!(st, 200);
    let (st, _) = json(
        c.get(s.api("/me"))
            .header("cookie", format!("oad_token={t2}")),
    )
    .await;
    assert_eq!(st, 200);
    // 过期了但在误差里：行；超出：不行
    let (st, _) = json(
        c.get(s.api("/me"))
            .bearer_auth(hs(key, claims(json!({ "exp": now() - 30 })))),
    )
    .await;
    assert_eq!(st, 200);
    let (st, _) = json(
        c.get(s.api("/me"))
            .bearer_auth(hs(key, claims(json!({ "exp": now() - 600 })))),
    )
    .await;
    assert_eq!(st, 401);
    // 签名不对、iss 不对、没有 exp、密码登录：都不行
    let (st, _) = json(
        c.get(s.api("/me"))
            .bearer_auth(hs("别的密钥-0123456789", claims(json!({})))),
    )
    .await;
    assert_eq!(st, 401);
    let (st, _) = json(c.get(s.api("/me")).bearer_auth(hs(
        key,
        claims(json!({ "iss": "https://evil.example.com" })),
    )))
    .await;
    assert_eq!(st, 401);
    let (st, _) = json(
        c.get(s.api("/me"))
            .bearer_auth(hs(key, json!({ "sub": "u-2" }))),
    )
    .await;
    assert_eq!(st, 401);
    let (st, _) = json(
        c.post(s.api("/auth/login"))
            .json(&json!({ "email": "u1@host.example.com", "password": "whatever-123" })),
    )
    .await;
    assert_eq!(st, 400);
    // 没给 workspaces：进默认工作区
    let (_, me) = json(
        c.get(s.api("/me"))
            .bearer_auth(hs(key, claims(json!({ "sub": "u-3" })))),
    )
    .await;
    assert_eq!(me["workspaces"][0]["id"], ws.as_str());
    s.stop().await;
}

/// Ed25519 私钥（32 字节种子）的 PKCS#8 DER。
fn ed_der(seed: [u8; 32]) -> Vec<u8> {
    let mut der = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    der.extend_from_slice(&seed);
    der
}

fn ed_jwk(seed: [u8; 32], kid: &str) -> Value {
    let pk = ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key();
    json!({ "kty": "OKP", "crv": "Ed25519", "kid": kid, "use": "sig", "alg": "EdDSA", "x": URL_SAFE_NO_PAD.encode(pk.as_bytes()) })
}

fn ed_token(seed: [u8; 32], kid: &str, claims: Value) -> String {
    let mut h = Header::new(Algorithm::EdDSA);
    h.kid = Some(kid.into());
    encode(&h, &claims, &EncodingKey::from_ed_der(&ed_der(seed))).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jwks() {
    // 本机的 JWKS 服务：数得出被取了几次，能换钥匙
    let keys = Arc::new(Mutex::new(json!({ "keys": [ed_jwk([1; 32], "k1")] })));
    let fetched = Arc::new(AtomicUsize::new(0));
    let (k2, f2) = (keys.clone(), fetched.clone());
    let app = Router::new().route(
        "/.well-known/jwks.json",
        get(move || {
            let (k, f) = (k2.clone(), f2.clone());
            async move {
                f.fetch_add(1, Ordering::SeqCst);
                axum::Json(k.lock().unwrap().clone())
            }
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });

    let dir = tempfile::tempdir().unwrap();
    let cfg = JwtConfig {
        jwks_url: Some(format!("http://{addr}/.well-known/jwks.json")),
        audience: Some("open-agent-docs".into()),
        leeway: 5,
        ..Default::default()
    };
    let s = Server::start(dir.path(), AuthConfig::Jwt(cfg), fast_hub()).await;
    let c = http();
    let claims = json!({ "sub": "jwks-user", "aud": "open-agent-docs", "exp": now() + 300 });
    let t = ed_token([1; 32], "k1", claims.clone());
    for _ in 0..3 {
        let (st, me) = json(c.get(s.api("/me")).bearer_auth(&t)).await;
        assert_eq!(st, 200, "{me}");
    }
    assert_eq!(fetched.load(Ordering::SeqCst), 1, "按 kid 缓存");
    // 宿主换了钥匙：新 kid 不认识就再取一次
    *keys.lock().unwrap() = json!({ "keys": [ed_jwk([1; 32], "k1"), ed_jwk([2; 32], "k2")] });
    let (st, _) = json(
        c.get(s.api("/me"))
            .bearer_auth(ed_token([2; 32], "k2", claims.clone())),
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(fetched.load(Ordering::SeqCst), 2);
    // 用别的钥匙签、冒用 kid：不行；aud 不对：不行；拿公钥当 HS256 的密钥：不行
    let (st, _) = json(
        c.get(s.api("/me"))
            .bearer_auth(ed_token([3; 32], "k1", claims.clone())),
    )
    .await;
    assert_eq!(st, 401);
    let mut bad = claims.clone();
    bad["aud"] = json!("别的服务");
    let (st, _) = json(
        c.get(s.api("/me"))
            .bearer_auth(ed_token([1; 32], "k1", bad)),
    )
    .await;
    assert_eq!(st, 401);
    let x = ed_jwk([1; 32], "k1")["x"].as_str().unwrap().to_string();
    let mut h = Header::new(Algorithm::HS256);
    h.kid = Some("k1".into());
    let forged = encode(
        &h,
        &claims,
        &EncodingKey::from_secret(x.as_bytes()).unwrap(),
    )
    .unwrap();
    let (st, _) = json(c.get(s.api("/me")).bearer_auth(forged)).await;
    assert_eq!(st, 401);
    // 不认识的 kid 不会每次都去取（30 秒内最多一次）
    let before = fetched.load(Ordering::SeqCst);
    for _ in 0..3 {
        let (st, _) = json(c.get(s.api("/me")).bearer_auth(ed_token(
            [4; 32],
            "k9",
            claims.clone(),
        )))
        .await;
        assert_eq!(st, 401);
    }
    assert!(fetched.load(Ordering::SeqCst) <= before + 1);
    s.stop().await;
}
