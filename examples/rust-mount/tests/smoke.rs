//! 冒烟测试：宿主的路由和 Open Agent Docs 一起服务；宿主的登录（cookie）在 Open Agent Docs 里认得；没登录 401。
use serde_json::{Value, json};
use std::net::SocketAddr;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mounted() {
    let dir = tempfile::tempdir().unwrap();
    let (app, core) = oad_example_rust_mount::app(dir.path()).await.unwrap();
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap()
    });
    let base = format!("http://{addr}");
    let c = reqwest::Client::builder().no_proxy().build().unwrap();
    // 宿主自己的路由
    let r = c.get(format!("{base}/host")).send().await.unwrap();
    assert!(r.text().await.unwrap().contains("宿主自己的页面"));
    // 没登录
    assert_eq!(
        c.get(format!("{base}/api/v1/me"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16(),
        401
    );
    // 宿主登录后，Open Agent Docs 认得这个人
    let r = c
        .get(format!("{base}/login?name=alice"))
        .send()
        .await
        .unwrap();
    let cookie = r.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let me: Value = c
        .get(format!("{base}/api/v1/me"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["user"]["name"], "alice");
    let ws = me["workspaces"][0]["id"].as_str().unwrap();
    let r = c
        .post(format!("{base}/api/v1/workspaces/{ws}/pages"))
        .header("cookie", &cookie)
        .json(&json!({ "title": "宿主里建的页面" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 201);
    // 网页也在
    let r = c.get(format!("{base}/")).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 200);
    core.shutdown().await;
}
