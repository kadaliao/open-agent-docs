//! 成员和邀请：邀请链接（看、过期、撤销、凭它注册、已登录接受）、改角色和移出的规矩、工作区改名、个人资料、改密码、注册方式。
mod common;
use common::*;
use oad_core::AuthConfig;
use oad_core::members::SignupMode;
use serde_json::{Value, json};

/// 登录，返回会话令牌（Set-Cookie 里的）。
async fn login(s: &Server, email: &str, pw: &str) -> String {
    let r = http()
        .post(s.api("/auth/login"))
        .json(&json!({ "email": email, "password": pw }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 200);
    cookie_token(&r)
}

fn cookie_token(r: &reqwest::Response) -> String {
    let v = r.headers()["set-cookie"].to_str().unwrap();
    v.split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_string()
}

async fn call(
    s: &Server,
    tok: &str,
    m: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let mut r = http().request(m, s.api(path)).bearer_auth(tok);
    if let Some(b) = body {
        r = r.json(&b);
    }
    json(r).await
}

use reqwest::Method as M;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invites_roles_and_profile() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::Local, fast_hub()).await;
    s.core
        .ensure_admin("owner@example.com", "owner-password")
        .await
        .unwrap();
    let owner = login(&s, "owner@example.com", "owner-password").await;
    let ws = s.ws_id();
    let (_, cfg) = json(http().get(s.api("/auth/config"))).await;
    assert_eq!(cfg["signup"], "invite");
    assert!(cfg["timezone"].is_string());

    // 建邀请；不用登录就能看
    let (st, inv) = call(
        &s,
        &owner,
        M::POST,
        &format!("/workspaces/{ws}/invites"),
        Some(json!({ "role": "editor", "expires_in": 3600 })),
    )
    .await;
    assert_eq!(st, 201, "{inv}");
    let token = inv["token"].as_str().unwrap().to_string();
    assert!(
        inv["url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/invite/{token}"))
    );
    let (st, info) = json(http().get(s.api(&format!("/invites/{token}")))).await;
    assert_eq!(st, 200, "{info}");
    assert_eq!(
        (info["role"].as_str(), info["signup"].as_bool()),
        (Some("editor"), Some(true))
    );
    assert_eq!(info["workspace"]["id"], ws.as_str());
    assert!(info["invited_by"]["name"].is_string());
    let (st, _) = json(http().get(s.api("/invites/nope"))).await;
    assert_eq!(st, 404);

    // 注册：invite 模式要邀请；凭邀请注册就登录了、加进去了
    let (st, _) = json(
        http()
            .post(s.api("/auth/signup"))
            .json(&json!({ "email": "a@example.com", "password": "password-a" })),
    )
    .await;
    assert_eq!(st, 403);
    let r = http()
        .post(s.api("/auth/signup"))
        .json(&json!({ "email": "a@example.com", "password": "password-a", "name": "阿甲", "invite": token }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 201);
    let a = cookie_token(&r);
    let me: Value = r.json().await.unwrap();
    assert_eq!(me["user"]["name"], "阿甲");
    assert_eq!(me["workspaces"][0]["role"], "editor");
    let (_, list) = call(
        &s,
        &owner,
        M::GET,
        &format!("/workspaces/{ws}/invites"),
        None,
    )
    .await;
    assert_eq!(list["invites"][0]["uses"], 1);
    let (st, _) = json(
        http()
            .post(s.api("/auth/signup"))
            .json(&json!({ "email": "a@example.com", "password": "password-a", "invite": token })),
    )
    .await;
    assert_eq!(st, 409, "邮箱注册过");

    // 编辑不能邀请、不能改角色
    let (st, _) = call(
        &s,
        &a,
        M::POST,
        &format!("/workspaces/{ws}/invites"),
        Some(json!({ "role": "viewer" })),
    )
    .await;
    assert_eq!(st, 403);
    let a_id = me["user"]["id"].as_str().unwrap().to_string();
    let owner_id = {
        let (_, m) = call(&s, &owner, M::GET, "/me", None).await;
        m["user"]["id"].as_str().unwrap().to_string()
    };
    let (st, _) = call(
        &s,
        &a,
        M::PATCH,
        &format!("/workspaces/{ws}/members/{owner_id}"),
        Some(json!({ "role": "viewer" })),
    )
    .await;
    assert_eq!(st, 403);
    // 升成 admin：能邀请，但不能邀请 / 改所有者
    let (st, _) = call(
        &s,
        &owner,
        M::PATCH,
        &format!("/workspaces/{ws}/members/{a_id}"),
        Some(json!({ "role": "admin" })),
    )
    .await;
    assert_eq!(st, 200);
    let (st, _) = call(
        &s,
        &a,
        M::POST,
        &format!("/workspaces/{ws}/invites"),
        Some(json!({ "role": "owner" })),
    )
    .await;
    assert_eq!(st, 403);
    let (st, vi) = call(
        &s,
        &a,
        M::POST,
        &format!("/workspaces/{ws}/invites"),
        Some(json!({ "role": "viewer" })),
    )
    .await;
    assert_eq!(st, 201);
    let (st, _) = call(
        &s,
        &a,
        M::DELETE,
        &format!("/workspaces/{ws}/members/{owner_id}"),
        None,
    )
    .await;
    assert_eq!(st, 403);
    let (st, _) = call(
        &s,
        &owner,
        M::DELETE,
        &format!("/workspaces/{ws}/members/{owner_id}"),
        None,
    )
    .await;
    assert_eq!(st, 400, "最后一个所有者不能走");
    let (st, _) = call(
        &s,
        &owner,
        M::PATCH,
        &format!("/workspaces/{ws}/members/{owner_id}"),
        Some(json!({ "role": "editor" })),
    )
    .await;
    assert_eq!(st, 400);

    // 已登录接受邀请：角色只升不降
    let (st, me2) = call(
        &s,
        &a,
        M::POST,
        &format!("/invites/{}/accept", vi["token"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(st, 200, "{me2}");
    assert_eq!(me2["workspaces"][0]["role"], "admin");

    // 撤销、过期：410
    let vt = vi["token"].as_str().unwrap().to_string();
    let r = http()
        .delete(s.api(&format!("/workspaces/{ws}/invites/{vt}")))
        .bearer_auth(&a)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    let (st, _) = json(http().get(s.api(&format!("/invites/{vt}")))).await;
    assert_eq!(st, 410);
    let (_, short) = call(
        &s,
        &owner,
        M::POST,
        &format!("/workspaces/{ws}/invites"),
        Some(json!({ "role": "viewer", "expires_in": 1 })),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let (st, _) =
        json(http().get(s.api(&format!("/invites/{}", short["token"].as_str().unwrap())))).await;
    assert_eq!(st, 410);

    // 工作区改名、图标
    let (st, w) = call(
        &s,
        &a,
        M::PATCH,
        &format!("/workspaces/{ws}"),
        Some(json!({ "name": "新名字", "icon": "🚀" })),
    )
    .await;
    assert_eq!(st, 200, "{w}");
    let (_, m) = call(&s, &owner, M::GET, "/me", None).await;
    assert_eq!(
        (
            m["workspaces"][0]["name"].as_str(),
            m["workspaces"][0]["icon"].as_str()
        ),
        (Some("新名字"), Some("🚀"))
    );

    // 个人资料
    let (st, m) = call(
        &s,
        &a,
        M::PATCH,
        "/me",
        Some(json!({ "name": "甲", "locale": "en", "avatar": "/api/v1/files/abc/a.png" })),
    )
    .await;
    assert_eq!(st, 200, "{m}");
    assert_eq!(
        (m["user"]["name"].as_str(), m["user"]["locale"].as_str()),
        (Some("甲"), Some("en"))
    );
    assert_eq!(m["user"]["avatar"], "/api/v1/files/abc/a.png");
    let (st, _) = call(&s, &a, M::PATCH, "/me", Some(json!({ "locale": "fr" }))).await;
    assert_eq!(st, 400);
    let (_, m) = call(
        &s,
        &a,
        M::PATCH,
        "/me",
        Some(json!({ "avatar": null, "locale": null })),
    )
    .await;
    assert!(m["user"]["avatar"].is_null() && m["user"]["locale"].is_null());

    // 改密码：别处的登录退出，这一个留着
    let a2 = login(&s, "a@example.com", "password-a").await;
    let r = http()
        .post(s.api("/me/password"))
        .bearer_auth(&a)
        .json(&json!({ "current": "wrong-one", "new": "password-b" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 403);
    let r = http()
        .post(s.api("/me/password"))
        .bearer_auth(&a)
        .json(&json!({ "current": "password-a", "new": "password-b" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    assert_eq!(call(&s, &a, M::GET, "/me", None).await.0, 200);
    assert_eq!(call(&s, &a2, M::GET, "/me", None).await.0, 401);
    login(&s, "a@example.com", "password-b").await;

    // 移出：之后没权限
    let (st, _) = call(
        &s,
        &owner,
        M::DELETE,
        &format!("/workspaces/{ws}/members/{a_id}"),
        None,
    )
    .await;
    assert_eq!(st, 204);
    let (st, _) = call(&s, &a, M::GET, &format!("/workspaces/{ws}/pages"), None).await;
    assert_eq!(st, 403);
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signup_modes() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::Local, fast_hub()).await;
    s.core
        .ensure_admin("owner@example.com", "owner-password")
        .await
        .unwrap();
    s.core.set_signup_mode(SignupMode::Open);
    let r = http()
        .post(s.api("/auth/signup"))
        .json(&json!({ "email": "b@example.com", "password": "password-b" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 201);
    let me: Value = r.json().await.unwrap();
    assert_eq!(
        me["workspaces"][0]["id"],
        s.ws_id().as_str(),
        "没邀请：进默认工作区"
    );
    assert_eq!(me["user"]["name"], "b");
    let (st, _) = json(
        http()
            .post(s.api("/auth/signup"))
            .json(&json!({ "email": "bad", "password": "password-b" })),
    )
    .await;
    assert_eq!(st, 400);
    let (st, _) = json(
        http()
            .post(s.api("/auth/signup"))
            .json(&json!({ "email": "c@example.com", "password": "short" })),
    )
    .await;
    assert_eq!(st, 400);
    s.core.set_signup_mode(SignupMode::Off);
    let (st, _) = json(
        http()
            .post(s.api("/auth/signup"))
            .json(&json!({ "email": "c@example.com", "password": "password-c" })),
    )
    .await;
    assert_eq!(st, 403);
    s.stop().await;
    // 不是本地登录：不能注册、不能改密码
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let (st, _) = json(
        http()
            .post(s.api("/auth/signup"))
            .json(&json!({ "email": "c@example.com", "password": "password-c" })),
    )
    .await;
    assert_eq!(st, 400);
    let (st, _) = json(
        http()
            .post(s.api("/me/password"))
            .json(&json!({ "current": "x", "new": "password-c" })),
    )
    .await;
    assert_eq!(st, 400);
    s.stop().await;
}
