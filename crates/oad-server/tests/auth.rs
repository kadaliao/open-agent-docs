//! 三种登录方式；只读成员不能写。
mod common;
use common::*;
use oad_core::{AuthConfig, Role};
use serde_json::json;
use std::time::Duration;

fn session_cookie(res: &reqwest::Response) -> String {
    let v = res
        .headers()
        .get("set-cookie")
        .expect("没有 set-cookie")
        .to_str()
        .unwrap();
    assert!(v.contains("HttpOnly") && v.contains("SameSite=Lax"), "{v}");
    v.split(';').next().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_login_logout() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::Local, fast_hub()).await;
    assert!(
        s.core
            .ensure_admin("admin@example.com", "correct horse")
            .await
            .unwrap()
    );
    assert!(
        !s.core
            .ensure_admin("x@example.com", "whatever123")
            .await
            .unwrap()
    );
    let c = http();
    let (_, cfg) = json(c.get(s.api("/auth/config"))).await;
    assert_eq!(cfg["mode"], "local");
    let (st, e) = json(c.get(s.api("/me"))).await;
    assert_eq!(st, 401);
    assert_eq!(e["error"]["code"], "unauthorized");

    let (st, _) = json(
        c.post(s.api("/auth/login"))
            .json(&json!({ "email": "admin@example.com", "password": "wrong password" })),
    )
    .await;
    assert_eq!(st, 401);
    let (st, _) = json(
        c.post(s.api("/auth/login"))
            .json(&json!({ "email": "nobody@example.com", "password": "correct horse" })),
    )
    .await;
    assert_eq!(st, 401);

    let res = c
        .post(s.api("/auth/login"))
        .json(&json!({ "email": "ADMIN@example.com", "password": "correct horse" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    let cookie = session_cookie(&res);
    let me: serde_json::Value = res.json().await.unwrap();
    assert_eq!(me["user"]["email"], "admin@example.com");
    assert_eq!(me["workspaces"][0]["role"], "owner");

    let (st, me2) = json(c.get(s.api("/me")).header("cookie", &cookie)).await;
    assert_eq!(st, 200);
    assert_eq!(me2["user"]["id"], me["user"]["id"]);
    // 令牌也能放在 Authorization 里
    let token = cookie.split_once('=').unwrap().1;
    let (st, _) = json(c.get(s.api("/me")).bearer_auth(token)).await;
    assert_eq!(st, 200);

    let res = c
        .post(s.api("/auth/logout"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 204);
    assert!(
        res.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let (st, _) = json(c.get(s.api("/me")).header("cookie", &cookie)).await;
    assert_eq!(st, 401);
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn header_mode() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(
        dir.path(),
        AuthConfig::Header {
            secret: "s3cret-value".into(),
        },
        fast_hub(),
    )
    .await;
    let c = http();
    let (_, cfg) = json(c.get(s.api("/auth/config"))).await;
    assert_eq!(cfg["mode"], "header");
    let as_user = |r: reqwest::RequestBuilder, secret: &str, id: &str| {
        r.header("X-OAD-Secret", secret)
            .header("X-OAD-User-Id", id)
            .header("X-OAD-User-Name", "%E5%AE%BF%E4%B8%BB%E7%94%A8%E6%88%B7")
            .header("X-OAD-User-Email", format!("{id}@host.example"))
    };
    let (st, _) = json(as_user(c.get(s.api("/me")), "wrong", "u1")).await;
    assert_eq!(st, 401);
    let (st, _) = json(c.get(s.api("/me")).header("X-OAD-User-Id", "u1")).await;
    assert_eq!(st, 401);
    let (st, me) = json(as_user(c.get(s.api("/me")), "s3cret-value", "u1")).await;
    assert_eq!(st, 200, "{me}");
    assert_eq!(me["user"]["name"], "宿主用户");
    assert_eq!(me["workspaces"][0]["role"], "owner");
    // 同一个人再来是同一个用户；第二个人自动建、是编辑者
    let (_, again) = json(as_user(c.get(s.api("/me")), "s3cret-value", "u1")).await;
    assert_eq!(again["user"]["id"], me["user"]["id"]);
    let (_, other) = json(as_user(c.get(s.api("/me")), "s3cret-value", "u2")).await;
    assert_ne!(other["user"]["id"], me["user"]["id"]);
    assert_eq!(other["workspaces"][0]["role"], "editor");
    // 不能用密码登录
    let (st, _) = json(
        c.post(s.api("/auth/login"))
            .json(&json!({ "email": "u1@host.example", "password": "x" })),
    )
    .await;
    assert_eq!(st, 400);
    // WebSocket 也认请求头
    let (st, p) = json(
        as_user(
            c.post(s.api(&format!("/workspaces/{}/pages", s.ws_id()))),
            "s3cret-value",
            "u2",
        )
        .json(&json!({ "title": "t" })),
    )
    .await;
    assert_eq!(st, 201);
    let page = p["id"].as_str().unwrap();
    let mut a = YClient::new(7001);
    a.connect(
        &s.ws(page),
        &[("x-oad-secret", "s3cret-value"), ("x-oad-user-id", "u2")],
    )
    .await;
    eventually("同步上", async || a.synced()).await;
    let mut bad = YClient::new(7002);
    bad.connect(
        &s.ws(page),
        &[("x-oad-secret", "nope"), ("x-oad-user-id", "u2")],
    )
    .await;
    eventually("被拒绝", async || {
        bad.state.lock().unwrap().denied.is_some()
    })
    .await;
    assert!(!bad.synced());
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn none_mode() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let (_, cfg) = json(c.get(s.api("/auth/config"))).await;
    assert_eq!(cfg["mode"], "none");
    let (st, me) = json(c.get(s.api("/me"))).await;
    assert_eq!(st, 200);
    assert_eq!(me["workspaces"][0]["role"], "owner");
    s.stop().await;
    // 重启后还是同一个本机用户
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let (_, me2) = json(c.get(s.api("/me"))).await;
    assert_eq!(me2["user"]["id"], me["user"]["id"]);
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn viewer_cannot_write() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::Local, fast_hub()).await;
    s.core
        .ensure_admin("owner@example.com", "owner-password")
        .await
        .unwrap();
    s.core
        .add_user(
            "viewer@example.com",
            "只读",
            "viewer-password",
            Some(Role::Viewer),
        )
        .await
        .unwrap();
    let c = http();
    let login = async |email: &str, pw: &str| {
        let res = c
            .post(s.api("/auth/login"))
            .json(&json!({ "email": email, "password": pw }))
            .send()
            .await
            .unwrap();
        session_cookie(&res)
    };
    let owner = login("owner@example.com", "owner-password").await;
    let viewer = login("viewer@example.com", "viewer-password").await;

    let (st, p) = json(
        c.post(s.api(&format!("/workspaces/{}/pages", s.ws_id())))
            .header("cookie", &owner)
            .json(&json!({ "title": "只读测试" })),
    )
    .await;
    assert_eq!(st, 201);
    let page = p["id"].as_str().unwrap().to_string();
    let r = c
        .put(s.api(&format!("/pages/{page}/content")))
        .header("cookie", &owner)
        .json(&json!({ "markdown": "原来的\n" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    let para = block_id(&s.server_doc(&page).await, 0);

    // 能读
    let (st, _) = json(
        c.get(s.api(&format!("/pages/{page}/content")))
            .header("cookie", &viewer),
    )
    .await;
    assert_eq!(st, 200);
    // HTTP 写都是 403
    let (st, e) = json(
        c.put(s.api(&format!("/pages/{page}/blocks/{para}")))
            .header("cookie", &viewer)
            .json(&json!({ "markdown": "改" })),
    )
    .await;
    assert_eq!(st, 403);
    assert_eq!(e["error"]["code"], "forbidden");
    let (st, _) = json(
        c.post(s.api(&format!("/workspaces/{}/pages", s.ws_id())))
            .header("cookie", &viewer)
            .json(&json!({})),
    )
    .await;
    assert_eq!(st, 403);
    let (st, _) = json(
        c.patch(s.api(&format!("/pages/{page}")))
            .header("cookie", &viewer)
            .json(&json!({ "title": "x" })),
    )
    .await;
    assert_eq!(st, 403);

    // WebSocket：能连、能收，发的增量被丢掉；连接不断
    let mut v = YClient::new(8001);
    v.connect(&s.ws(&page), &[("cookie", &viewer)]).await;
    eventually("同步上", async || v.synced()).await;
    v.type_at(&para, 0, "偷改");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(s.server_doc(&page).await.plain_text(), "原来的");
    // 还连着：别人改的照样收到
    let r = c
        .put(s.api(&format!("/pages/{page}/blocks/{para}")))
        .header("cookie", &owner)
        .json(&json!({ "markdown": "所有者改的" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 200);
    eventually("只读连接收到", async || {
        v.node().plain_text().contains("所有者改的")
    })
    .await;

    // 没登录：permission-denied
    let mut anon = YClient::new(8002);
    anon.connect(&s.ws(&page), &[]).await;
    eventually("被拒绝", async || {
        anon.state.lock().unwrap().denied.is_some()
    })
    .await;
    // 用 ?token= 连
    let token = viewer.split_once('=').unwrap().1;
    let mut t = YClient::new(8003);
    t.connect(&format!("{}?token={token}", s.ws(&page)), &[])
        .await;
    eventually("token 连上", async || t.synced()).await;
    s.stop().await;
}
