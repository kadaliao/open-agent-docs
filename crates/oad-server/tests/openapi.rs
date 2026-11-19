//! OpenAPI：`docs/openapi.json` 要和代码生成的一样（`OAD_WRITE_OPENAPI=1 cargo test -p oad-server --test openapi` 重写）；
//! 每个注册了的路由都要写进 OpenAPI；`GET /api/v1/openapi.json` 能取到。
mod common;
use common::*;
use oad_core::AuthConfig;
use serde_json::Value;
use std::collections::BTreeSet;

fn docs_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/openapi.json")
}

#[test]
fn docs_openapi_is_fresh() {
    let spec = serde_json::to_string_pretty(&oad_server::openapi::spec()).unwrap() + "\n";
    let path = docs_path();
    if std::env::var("OAD_WRITE_OPENAPI").is_ok_and(|v| v == "1") {
        std::fs::write(&path, &spec).unwrap();
        return;
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        on_disk == spec,
        "docs/openapi.json 过期了：跑 OAD_WRITE_OPENAPI=1 cargo test -p oad-server --test openapi 重写"
    );
}

/// 源代码里 `.route("/path", get(x).post(y))` 注册的 (方法, 路径)。
fn routes_in_source() -> BTreeSet<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = BTreeSet::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        // 分享页面（/s）不在 /api/v1 里
        if name == "share.rs" || !name.ends_with(".rs") {
            continue;
        }
        let src = std::fs::read_to_string(&p).unwrap();
        let mut rest = src.as_str();
        while let Some(i) = rest.find(".route(") {
            rest = &rest[i + 7..];
            let end = rest.find(")\n").unwrap_or(rest.len());
            let call = &rest[..end];
            let Some(path) = call.split('"').nth(1) else {
                continue;
            };
            for m in ["get", "post", "put", "patch", "delete"] {
                for pre in [format!("{m}("), format!(".{m}(")] {
                    let hit = call.match_indices(&pre).any(|(j, _)| {
                        j == 0
                            || !call.as_bytes()[j - 1].is_ascii_alphanumeric()
                                && call.as_bytes()[j - 1] != b'_'
                    });
                    if hit {
                        out.insert((m.to_string(), path.to_string()));
                    }
                }
            }
        }
    }
    out
}

#[test]
fn every_route_is_documented() {
    let spec = oad_server::openapi::spec();
    let mut documented = BTreeSet::new();
    for (path, ops) in spec["paths"].as_object().unwrap() {
        for m in ops.as_object().unwrap().keys() {
            documented.insert((m.clone(), path.clone()));
        }
    }
    let mut share = BTreeSet::new();
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/share.rs"),
    )
    .unwrap();
    if src.contains("\"/pages/{id}/share\"") {
        for m in ["get", "post", "delete"] {
            share.insert((m.to_string(), "/pages/{id}/share".to_string()));
        }
    }
    let mut missing: Vec<_> = routes_in_source()
        .union(&share)
        .filter(|r| r.1 != "/openapi.json" && !documented.contains(*r))
        .cloned()
        .collect();
    missing.sort();
    assert!(missing.is_empty(), "这些接口没写进 OpenAPI：{missing:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn served() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::Local, fast_hub()).await;
    let (st, v): (u16, Value) = json(http().get(s.api("/openapi.json"))).await;
    assert_eq!(st, 200);
    assert!(v["openapi"].as_str().unwrap().starts_with("3.1"));
    assert!(v["paths"]["/pages/{id}"]["get"].is_object());
    assert!(v["webhooks"]["db.changed"].is_object());
    assert!(v["components"]["schemas"]["ErrorBody"].is_object());
    s.stop().await;
}

/// 限流：每个响应带 RateLimit-* 头，超了 429 + Retry-After（错误码 too_many_requests）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rate_limit_headers() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = oad_server::ServerConfig {
        rate_limit: 3,
        ..Default::default()
    };
    let s = Server::start_with(dir.path(), AuthConfig::None, fast_hub(), cfg).await;
    let c = http();
    for left in [2, 1, 0] {
        let r = c.get(s.api("/me")).send().await.unwrap();
        assert_eq!(r.status().as_u16(), 200);
        assert_eq!(r.headers()["ratelimit-limit"], "3");
        assert_eq!(
            r.headers()["ratelimit-remaining"],
            left.to_string().as_str()
        );
        assert!(r.headers().contains_key("ratelimit-reset"));
    }
    let r = c.get(s.api("/me")).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 429);
    assert!(r.headers().contains_key("retry-after"));
    let v: Value = r.json().await.unwrap();
    assert_eq!(v["error"]["code"], "too_many_requests");
    // 别的调用者（不同的 token）不受影响
    let r = c
        .get(s.api("/me"))
        .bearer_auth("someone-else")
        .send()
        .await
        .unwrap();
    assert_ne!(r.status().as_u16(), 429);
    s.stop().await;
}
