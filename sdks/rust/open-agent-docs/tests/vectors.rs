//! 各语言共用的固定用例：sdks/conformance/vectors.json（S10 webhook、S11 embed_url 和 jwt）。
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use open_agent_docs as oad;
use serde_json::{Value, json};
use sha2::Sha256;
use std::collections::HashMap;
use std::time::{Duration, UNIX_EPOCH};

fn vectors() -> Value {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../conformance/vectors.json"
    );
    serde_json::from_str(&std::fs::read_to_string(p).expect("读 sdks/conformance/vectors.json"))
        .unwrap()
}

/// S10
#[test]
fn s10_webhook_vectors() {
    let v = vectors();
    let cases = v["webhook"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let headers: HashMap<String, String> =
            serde_json::from_value(c["headers"].clone()).unwrap();
        let now = UNIX_EPOCH + Duration::from_millis(c["now_ms"].as_u64().unwrap());
        let tol = Duration::from_secs(c["tolerance_s"].as_u64().unwrap());
        let r = oad::verify_webhook_signature(
            c["secret"].as_str().unwrap(),
            &headers,
            c["body"].as_str().unwrap(),
            now,
            tol,
        );
        let got = match r {
            Ok(()) => "ok",
            Err(e) => e.webhook_kind().expect("应该是 webhook 错误").as_str(),
        };
        assert_eq!(got, c["expect"].as_str().unwrap(), "{name}");
        // 换成 http::HeaderMap、[(k, v)] 也一样
        let mut hm = oad::reqwest::header::HeaderMap::new();
        for (k, val) in &headers {
            hm.insert(
                oad::reqwest::header::HeaderName::try_from(k.as_str()).unwrap(),
                val.parse().unwrap(),
            );
        }
        let r2 = oad::verify_webhook_signature(
            c["secret"].as_str().unwrap(),
            &hm,
            c["body"].as_str().unwrap().as_bytes(),
            now,
            tol,
        );
        assert_eq!(r2.is_ok(), got == "ok", "{name}（HeaderMap）");
        let pairs: Vec<(String, String)> = headers.clone().into_iter().collect();
        let r3 = oad::verify_webhook_signature(
            c["secret"].as_str().unwrap(),
            &pairs,
            c["body"].as_str().unwrap(),
            now,
            tol,
        );
        assert_eq!(r3.is_ok(), got == "ok", "{name}（pairs）");
    }
}

/// verify_webhook_at：签名对了再解析成 AgentNotification
#[test]
fn webhook_parses_notification() {
    let body = json!({
        "id": 42, "created_at": 1700000000000i64, "type": "comment",
        "agent": { "id": "a1", "name": "Writer" },
        "page": { "id": "p1", "title": "季度计划", "url": "https://docs.example.com/p/p1" },
        "thread_id": "t1",
        "comment": { "id": "c1", "author": { "kind": "user", "id": "u1", "name": "小王" }, "body": "@Writer 写清楚一点" },
        "prompt": "小王在《季度计划》里叫你"
    })
    .to_string();
    let secret = "whsec_x";
    let ts = "1700000000000";
    let signed = |body: &str| {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{ts}.{body}").as_bytes());
        let sig: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        [
            ("X-OAD-Timestamp", ts.to_string()),
            ("X-OAD-Signature", format!("sha256={sig}")),
        ]
    };
    let now = UNIX_EPOCH + Duration::from_millis(1700000001000);
    let note =
        oad::verify_webhook_at(secret, &signed(&body), &body, now, oad::DEFAULT_TOLERANCE).unwrap();
    assert_eq!(note.id, 42);
    assert_eq!(note.comment.body, "@Writer 写清楚一点");
    // 签名不对
    let e = oad::verify_webhook_at(secret, &signed(&body), "{}", now, oad::DEFAULT_TOLERANCE)
        .unwrap_err();
    assert_eq!(e.webhook_kind(), Some(oad::WebhookErrorKind::Signature));
    // 签名对、正文不是通知：Decode
    let e = oad::verify_webhook_at(
        secret,
        &signed("\"x\""),
        "\"x\"",
        now,
        oad::DEFAULT_TOLERANCE,
    )
    .unwrap_err();
    assert!(matches!(e, oad::Error::Decode(_)), "{e:?}");
}

/// S11：embed_url
#[test]
fn s11_embed_url_vectors() {
    let v = vectors();
    for c in v["embed_url"].as_array().unwrap() {
        let o = &c["options"];
        let s = |k: &str| o.get(k).and_then(Value::as_str).map(String::from);
        let opts = oad::EmbedOptions {
            theme: s("theme"),
            lang: s("lang"),
            token: s("token"),
        };
        let got = oad::embed_url(
            c["base_url"].as_str().unwrap(),
            c["page_id"].as_str().unwrap(),
            &opts,
        );
        assert_eq!(got, c["expect"].as_str().unwrap());
    }
}

/// S11：sign_jwt 的 header、claims、签名
#[test]
fn s11_jwt_vector() {
    let v = vectors();
    let j = &v["jwt"];
    let input = &j["input"];
    let secret = j["secret"].as_str().unwrap();
    let workspaces: Vec<oad::JwtWorkspace> =
        serde_json::from_value(input["workspaces"].clone()).unwrap();
    let opts = oad::JwtOptions {
        name: input["name"].as_str().map(String::from),
        email: input["email"].as_str().map(String::from),
        workspaces: Some(workspaces),
        ttl: Duration::from_secs(input["ttl_s"].as_u64().unwrap()),
        issuer: input["issuer"].as_str().map(String::from),
        audience: input["audience"].as_str().map(String::from),
        now: Some(UNIX_EPOCH + Duration::from_secs(j["now_s"].as_u64().unwrap())),
    };
    let tok = oad::sign_jwt(secret, input["user_id"].as_str().unwrap(), &opts);
    let parts: Vec<&str> = tok.split('.').collect();
    assert_eq!(parts.len(), 3);
    let dec =
        |s: &str| -> Value { serde_json::from_slice(&URL_SAFE_NO_PAD.decode(s).unwrap()).unwrap() };
    assert_eq!(dec(parts[0]), j["expect_header"]);
    assert_eq!(dec(parts[1]), j["expect_claims"]);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{}.{}", parts[0], parts[1]).as_bytes());
    mac.verify_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap())
        .expect("签名对得上");

    // 没给的 claim 不写
    let tok = oad::sign_jwt(
        secret,
        "u1",
        &oad::JwtOptions {
            now: opts.now,
            ..Default::default()
        },
    );
    let claims = dec(tok.split('.').nth(1).unwrap());
    assert_eq!(
        claims,
        json!({ "sub": "u1", "iat": 1700000000u64, "exp": 1700003600u64 })
    );
}
