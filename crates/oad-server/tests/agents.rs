//! Agent：token、scope、管理接口（含 admin Agent 管别的）、作者对应。
mod common;
use common::*;
use oad_core::AuthConfig;
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn agent_auth_scopes_and_management() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let agents_url = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (reader, reader_tok, _) = make_agent(c.post(&agents_url), "读者", "read", None).await;
    let (writer, writer_tok, _) = make_agent(c.post(&agents_url), "写手", "write", None).await;
    let (admin, admin_tok, secret) = make_agent(c.post(&agents_url), "管家", "admin", None).await;
    assert!(writer_tok.starts_with("oad_") && secret.starts_with("whsec_"));
    // 名字重复、名字里有空白
    let (st, _) = json(
        c.post(&agents_url)
            .json(&json!({ "name": "写手", "scope": "write" })),
    )
    .await;
    assert_eq!(st, 409);
    let (st, _) = json(
        c.post(&agents_url)
            .json(&json!({ "name": "有 空格", "scope": "write" })),
    )
    .await;
    assert_eq!(st, 400);
    // 默认不让 webhook 投内网
    let (st, e) =
        json(c.post(&agents_url).json(
            &json!({ "name": "钩子", "scope": "write", "webhook": "http://127.0.0.1:9/hook" }),
        ))
        .await;
    assert_eq!(st, 400, "{e}");

    // 列表里没有 token 和 secret
    let (_, list) = json(c.get(&agents_url)).await;
    let text = list.to_string();
    assert!(
        !text.contains(&writer_tok) && !text.contains(&secret) && !text.contains("webhook_secret")
    );
    assert_eq!(list["agents"].as_array().unwrap().len(), 3);
    assert_eq!(list["agents"][0]["pending"], 0);

    // 写手：能写；/me 显示是 Agent
    let (st, me) = json(c.get(s.api("/me")).bearer_auth(&writer_tok)).await;
    assert_eq!(st, 200);
    assert_eq!(me["user"]["kind"], "agent");
    assert_eq!(me["user"]["id"], writer.as_str());
    let pages = s.api(&format!("/workspaces/{}/pages", s.ws_id()));
    let (st, p) = json(
        c.post(&pages)
            .bearer_auth(&writer_tok)
            .json(&json!({ "title": "Agent 建的" })),
    )
    .await;
    assert_eq!(st, 201);
    assert_eq!(p["created_by"], writer.as_str());
    let page = p["id"].as_str().unwrap().to_string();
    let r = c
        .put(s.api(&format!("/pages/{page}/content")))
        .bearer_auth(&writer_tok)
        .json(&json!({ "markdown": "一段\n" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    // 读者：能读不能写
    let (st, _) = json(
        c.get(s.api(&format!("/pages/{page}/content")))
            .bearer_auth(&reader_tok),
    )
    .await;
    assert_eq!(st, 200);
    let (st, _) = json(c.post(&pages).bearer_auth(&reader_tok).json(&json!({}))).await;
    assert_eq!(st, 403);
    // 错的 token：401，不登录模式下也不会变成本机用户
    let (st, _) = json(c.get(s.api("/me")).bearer_auth("oad_nope")).await;
    assert_eq!(st, 401);
    // 写手不能管 Agent
    let (st, _) = json(
        c.post(&agents_url)
            .bearer_auth(&writer_tok)
            .json(&json!({ "name": "x", "scope": "read" })),
    )
    .await;
    assert_eq!(st, 403);

    // admin：替别的 Bot 建 Agent、改、重发 token、撤销；不能管自己
    let (bot, bot_tok, _) = make_agent(
        c.post(&agents_url).bearer_auth(&admin_tok),
        "小助手",
        "write",
        None,
    )
    .await;
    let (st, _) = json(c.get(s.api("/me")).bearer_auth(&bot_tok)).await;
    assert_eq!(st, 200);
    let (st, a) = json(
        c.patch(s.api(&format!("/agents/{writer}")))
            .bearer_auth(&admin_tok)
            .json(&json!({ "about": "改过的说明", "scope": "read" })),
    )
    .await;
    assert_eq!(st, 200, "{a}");
    assert_eq!(a["about"], "改过的说明");
    let (st, _) = json(c.post(&pages).bearer_auth(&writer_tok).json(&json!({}))).await;
    assert_eq!(st, 403, "改成 read 后不能写");
    let (st, t) = json(
        c.post(s.api(&format!("/agents/{writer}/token")))
            .bearer_auth(&admin_tok),
    )
    .await;
    assert_eq!(st, 200);
    let new_tok = t["token"].as_str().unwrap();
    assert_eq!(
        json(c.get(s.api("/me")).bearer_auth(&writer_tok)).await.0,
        401,
        "旧 token 立即失效"
    );
    assert_eq!(json(c.get(s.api("/me")).bearer_auth(new_tok)).await.0, 200);
    let (st, w) = json(
        c.post(s.api(&format!("/agents/{writer}/webhook-secret")))
            .bearer_auth(&admin_tok),
    )
    .await;
    assert_eq!(st, 200);
    assert!(w["webhook_secret"].as_str().unwrap().starts_with("whsec_"));
    let (st, r) = json(
        c.delete(s.api(&format!("/agents/{reader}")))
            .bearer_auth(&admin_tok),
    )
    .await;
    assert_eq!(st, 200);
    assert!(r["revoked_at"].is_i64());
    assert_eq!(
        json(c.get(s.api("/me")).bearer_auth(&reader_tok)).await.0,
        401
    );
    let (st, _) = json(
        c.patch(s.api(&format!("/agents/{admin}")))
            .bearer_auth(&admin_tok)
            .json(&json!({ "about": "自己改自己" })),
    )
    .await;
    assert_eq!(st, 403);
    // 撤销的名字留着；最后使用时间记上了
    let (_, list) = json(c.get(&agents_url)).await;
    let find = |id: &str| {
        list["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(find(&reader)["name"], "读者");
    assert!(find(&reader)["revoked_at"].is_i64());
    assert!(find(&bot)["last_used_at"].is_i64());
    // 人改 Agent 也行（owner）；清掉 webhook
    let (st, _) = json(
        c.patch(s.api(&format!("/agents/{bot}")))
            .json(&json!({ "name": "新名字", "webhook": null })),
    )
    .await;
    assert_eq!(st, 200);

    // 成员列表：人和 Agent 分开，只读的人也能看
    let (st, m) = json(
        c.get(s.api(&format!("/workspaces/{}/members", s.ws_id())))
            .bearer_auth(new_tok),
    )
    .await;
    assert_eq!(st, 200, "{m}");
    assert_eq!(m["members"].as_array().unwrap().len(), 1);
    assert_eq!(m["members"][0]["role"], "owner");
    assert!(
        m["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == "新名字")
    );
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authors_are_per_principal() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let agents_url = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (agent, tok, _) = make_agent(c.post(&agents_url), "写手", "write", None).await;
    let (agent2, tok2, _) = make_agent(c.post(&agents_url), "二号", "write", None).await;
    let (_, me) = json(c.get(s.api("/me"))).await;
    let me = me["user"]["id"].as_str().unwrap().to_string();
    // 人先经 HTTP 写（PUT content），之后 Agent 经 HTTP 插一段：Agent 的字不能算到人头上
    let page = page_with(&s, &c, "作者", "人写的第一段\n").await;
    let (st, r) = json(
        c.post(s.api(&format!("/pages/{page}/blocks")))
            .bearer_auth(&tok)
            .json(&json!({ "markdown": "Agent 写的第二段，写得比较长一些" })),
    )
    .await;
    assert_eq!(st, 200);
    let agent_block = r["ids"][0].as_str().unwrap().to_string();
    // 另一个 Agent 也写一段
    let (_, r2) = json(
        c.post(s.api(&format!("/pages/{page}/blocks")))
            .bearer_auth(&tok2)
            .json(&json!({ "markdown": "二号写的" })),
    )
    .await;
    let block2 = r2["ids"][0].as_str().unwrap().to_string();
    // 人在网页上打字
    let doc = s.server_doc(&page).await;
    let human_block = block_id(&doc, 0);
    let mut y = YClient::new(4242);
    y.connect(&s.ws(&page), &[]).await;
    eventually("同步上", async || y.synced()).await;
    y.type_at(&human_block, 0, "网页上打的");
    eventually("收到", async || {
        s.server_doc(&page)
            .await
            .plain_text()
            .contains("网页上打的")
    })
    .await;

    let wait = async || {
        let (_, a) = json(c.get(s.api(&format!("/pages/{page}/authors")))).await;
        a["clients"].as_object().unwrap().clone()
    };
    eventually("作者都记下了", async || {
        let a = wait().await;
        a.get("4242").is_some() && a.values().filter(|v| v["kind"] == "agent").count() == 2
    })
    .await;
    let a = wait().await;
    assert_eq!(a["4242"]["kind"], "user");
    assert_eq!(a["4242"]["id"], me.as_str());
    let ids_of = |who: &str| a.iter().filter(|(_, v)| v["id"] == who).count();
    assert!(
        ids_of(&agent) >= 1 && ids_of(&agent2) >= 1 && ids_of(&me) >= 2,
        "{a:?}"
    );
    // 每个 client id 只属于一个人
    let mut owners = std::collections::HashMap::new();
    for (cid, v) in &a {
        assert!(owners.insert(cid.clone(), v["id"].clone()).is_none());
    }
    // 一块的作者 = 现存文字最多的那个人
    use oad_core::PrincipalKind;
    assert_eq!(
        s.core.block_author(&page, &agent_block).await.unwrap(),
        Some((PrincipalKind::Agent, agent.clone()))
    );
    assert_eq!(
        s.core.block_author(&page, &block2).await.unwrap(),
        Some((PrincipalKind::Agent, agent2.clone()))
    );
    assert_eq!(
        s.core.block_author(&page, &human_block).await.unwrap(),
        Some((PrincipalKind::User, me.clone()))
    );
    // Agent 改了人写的那段的大半：作者变成 Agent
    let (st, _) = json(
        c.put(s.api(&format!("/pages/{page}/blocks/{human_block}")))
            .bearer_auth(&tok)
            .json(&json!({ "markdown": "Agent 把这一段几乎全部重写成了一段很长很长的新文字" })),
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(
        s.core.block_author(&page, &human_block).await.unwrap(),
        Some((PrincipalKind::Agent, agent.clone()))
    );
    // 重启后作者表还在、同一个 Agent 再写会换 client id 但仍然记在它名下
    s.stop().await;
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let (st, r) = json(
        c.post(s.api(&format!("/pages/{page}/blocks")))
            .bearer_auth(&tok)
            .json(&json!({ "markdown": "重启后又写的" })),
    )
    .await;
    assert_eq!(st, 200);
    let b = r["ids"][0].as_str().unwrap().to_string();
    assert_eq!(
        s.core.block_author(&page, &b).await.unwrap(),
        Some((PrincipalKind::Agent, agent.clone()))
    );
    s.stop().await;
}
