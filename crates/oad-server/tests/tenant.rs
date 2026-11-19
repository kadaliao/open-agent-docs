//! 多租户：按子域名 / 请求头认出租户，各自独立的数据（页面互相看不到），第一次用到时才建，之后复用。
mod common;
use common::*;
use oad_core::storage::sqlite::Sqlite;
use oad_core::{AuthConfig, Core, MediaConfig, NotifyConfig};
use oad_server::tenant::{ByHeader, ByHost, TenantFactory, multi_tenant, valid_tenant};
use oad_server::{AppState, ServerConfig};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn factory(root: std::path::PathBuf, built: Arc<AtomicUsize>) -> TenantFactory {
    Arc::new(move |t: String| {
        let (root, built) = (root.clone(), built.clone());
        Box::pin(async move {
            built.fetch_add(1, Ordering::SeqCst);
            let dir = root.join(&t);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let storage = Arc::new(Sqlite::open(dir.join("oad.db")).map_err(|e| e.to_string())?);
            let media = MediaConfig {
                search_dir: Some(dir.join("search")),
                ..Default::default()
            };
            let core = Core::open_full(
                storage,
                AuthConfig::None,
                fast_hub(),
                NotifyConfig::default(),
                media,
            )
            .await
            .map_err(|e| e.to_string())?;
            let cfg = ServerConfig {
                allowed_hosts: vec!["*".into()],
                ..Default::default()
            };
            Ok(oad_server::api_router(AppState::new(core, cfg)))
        })
    })
}

async fn serve(app: axum::Router) -> SocketAddr {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap();
    });
    addr
}

async fn req(
    addr: SocketAddr,
    m: reqwest::Method,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> (u16, Value) {
    let mut r = http().request(m, format!("http://{addr}/api/v1{path}"));
    for (k, v) in headers {
        r = r.header(*k, *v);
    }
    if let Some(b) = body {
        r = r.json(&b);
    }
    json(r).await
}

async fn ws_of(addr: SocketAddr, h: &[(&str, &str)]) -> String {
    let (st, me) = req(addr, reqwest::Method::GET, "/me", h, None).await;
    assert_eq!(st, 200, "{me}");
    me["workspaces"][0]["id"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn by_host() {
    let dir = tempfile::tempdir().unwrap();
    let built = Arc::new(AtomicUsize::new(0));
    let app = multi_tenant(
        ByHost {
            base: "docs.test".into(),
            default: Some("main".into()),
        },
        factory(dir.path().to_path_buf(), built.clone()),
    );
    let addr = serve(app).await;
    let acme = [("host", "acme.docs.test")];
    let beta = [("host", "beta.docs.test")];
    let ws = ws_of(addr, &acme).await;
    let (st, _) = req(
        addr,
        reqwest::Method::POST,
        &format!("/workspaces/{ws}/pages"),
        &acme,
        Some(json!({ "title": "acme 的页面" })),
    )
    .await;
    assert_eq!(st, 201);
    let ws_b = ws_of(addr, &beta).await;
    let (_, pages) = req(
        addr,
        reqwest::Method::GET,
        &format!("/workspaces/{ws_b}/pages"),
        &beta,
        None,
    )
    .await;
    assert!(
        pages["pages"].as_array().unwrap().is_empty(),
        "别的租户看不到"
    );
    let (_, pages) = req(
        addr,
        reqwest::Method::GET,
        &format!("/workspaces/{ws}/pages"),
        &acme,
        None,
    )
    .await;
    assert_eq!(pages["pages"][0]["title"], "acme 的页面");
    assert!(dir.path().join("acme/oad.db").exists() && dir.path().join("beta/oad.db").exists());
    // 根域名是默认租户；认不出的 404
    ws_of(addr, &[("host", "docs.test")]).await;
    let (st, _) = req(
        addr,
        reqwest::Method::GET,
        "/me",
        &[("host", "evil.example.com")],
        None,
    )
    .await;
    assert_eq!(st, 404);
    let (st, _) = req(
        addr,
        reqwest::Method::GET,
        "/me",
        &[("host", "a.b.docs.test")],
        None,
    )
    .await;
    assert_eq!(st, 404, "a.b 不是合规的租户名");
    assert_eq!(built.load(Ordering::SeqCst), 3, "每个租户只建一次");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn by_header() {
    let dir = tempfile::tempdir().unwrap();
    let built = Arc::new(AtomicUsize::new(0));
    let app = multi_tenant(
        ByHeader {
            header: "x-oad-tenant".into(),
            secret: Some("proxy-secret".into()),
            default: None,
        },
        factory(dir.path().to_path_buf(), built.clone()),
    );
    let addr = serve(app).await;
    let (st, _) = req(
        addr,
        reqwest::Method::GET,
        "/me",
        &[("x-oad-tenant", "t1")],
        None,
    )
    .await;
    assert_eq!(st, 404, "没带代理的密钥");
    let (st, _) = req(
        addr,
        reqwest::Method::GET,
        "/me",
        &[("x-oad-tenant", "t1"), ("x-oad-tenant-secret", "wrong")],
        None,
    )
    .await;
    assert_eq!(st, 404);
    let t1 = [
        ("x-oad-tenant", "t1"),
        ("x-oad-tenant-secret", "proxy-secret"),
    ];
    ws_of(addr, &t1).await;
    let (st, _) = req(
        addr,
        reqwest::Method::GET,
        "/me",
        &[
            ("x-oad-tenant", "../etc"),
            ("x-oad-tenant-secret", "proxy-secret"),
        ],
        None,
    )
    .await;
    assert_eq!(st, 404);
    assert_eq!(built.load(Ordering::SeqCst), 1);
    assert!(
        valid_tenant("t-1") && !valid_tenant("../x") && !valid_tenant("") && !valid_tenant("A")
    );
}
