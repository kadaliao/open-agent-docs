//! MCP：用 rmcp 的客户端经 Streamable HTTP 调每一个工具。
mod common;
use common::*;
use oad_core::AuthConfig;
use serde_json::json;

const TOOLS: &[&str] = &[
    "search_pages",
    "list_pages",
    "read_page",
    "create_page",
    "edit_page",
    "rewrite_page",
    "update_page",
    "delete_page",
    "list_comments",
    "comment",
    "my_notifications",
    "upload_file",
    "db_create",
    "db_schema",
    "db_query",
    "db_add",
    "db_update",
];

/// 读出来的正文里 ⟦⟧ 中的块编号，按顺序。
fn blocks(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.strip_prefix('⟦').and_then(|l| l.strip_suffix('⟧')))
        .map(str::to_string)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_tool() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let agents = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (agent, token, _) = make_agent(c.post(&agents), "写作助手", "write", None).await;
    let (_, read_tok, _) = make_agent(c.post(&agents), "只读助手", "read", None).await;

    // 没有 token / 人的身份都连不上
    assert!(mcp_client(&s, "oad_wrong").await.is_err());
    let r = c.post(s.mcp()).json(&json!({})).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 401);
    assert!(r.headers().contains_key("www-authenticate"));

    let m = mcp_client(&s, &token).await.unwrap();
    let tools = m.list_all_tools().await.unwrap();
    let mut names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    let mut want: Vec<String> = TOOLS.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(names, want);
    assert!(tools.iter().all(|t| {
        t.description
            .as_deref()
            .is_some_and(|d| d.chars().count() > 15)
    }));
    let info = m.peer_info().unwrap();
    assert!(info.instructions.as_deref().unwrap().contains("⟦块编号⟧"));

    // create_page
    let (t, err) = call(
        &m,
        "create_page",
        json!({ "title": "周报", "markdown": "# 本周\n\n完成了登录功能。\n\n- 一\n- 二\n" }),
    )
    .await;
    assert!(!err, "{t}");
    let page = field(&t, "page_id");
    assert!(t.contains(&format!("/p/{page}")));
    let (_, p) = json(c.get(s.api(&format!("/pages/{page}")))).await;
    assert_eq!(p["created_by"], agent.as_str());
    let (t, _) = call(
        &m,
        "create_page",
        json!({ "title": "子页", "parent_id": page }),
    )
    .await;
    let child = field(&t, "page_id");

    // list_pages：缩进表示层级
    let (t, _) = call(&m, "list_pages", json!({})).await;
    assert!(
        t.contains(&format!("- 周报 （page_id: {page}"))
            && t.contains(&format!("  - 子页 （page_id: {child}")),
        "{t}"
    );
    let (t, _) = call(&m, "list_pages", json!({ "parent_id": page })).await;
    assert!(t.contains("子页") && !t.contains("- 周报"), "{t}");

    // search_pages：标题和正文
    let (t, _) = call(&m, "search_pages", json!({ "query": "登录功能" })).await;
    assert!(t.contains(&page) && t.contains("登录功能"), "{t}");
    let (t, _) = call(&m, "search_pages", json!({ "query": "不存在的词" })).await;
    assert!(t.contains("没有找到"));

    // read_page：每块前 ⟦id⟧、base
    let (t, _) = call(&m, "read_page", json!({ "page_id": page })).await;
    let ids = blocks(&t);
    assert_eq!(ids.len(), 3, "{t}");
    assert!(t.contains("# 周报") && t.contains("完成了登录功能。") && t.contains("- 一"));
    let base = field(&t, "base");
    // 分段读
    let (t2, _) = call(&m, "read_page", json!({ "page_id": page, "max_blocks": 1 })).await;
    assert_eq!(blocks(&t2), [ids[0].clone()]);
    assert!(t2.contains(&format!("from_block: \"{}\"", ids[1])));
    let (t3, _) = call(
        &m,
        "read_page",
        json!({ "page_id": page, "from_block": format!("⟦{}⟧", ids[1]) }),
    )
    .await;
    assert_eq!(blocks(&t3), ids[1..]);

    // 人在网页上接着打字（Agent 读完之后）
    let mut human = YClient::new(777);
    human.connect(&s.ws(&page), &[]).await;
    eventually("同步上", async || human.synced()).await;
    human.type_at(&ids[1], 5, "【人刚打的】");
    eventually("服务器收到", async || {
        s.server_doc(&page)
            .await
            .plain_text()
            .contains("【人刚打的】")
    })
    .await;

    // edit_page：几步一起；带 base，人打的字留着；网页看到 Agent 在写
    let (t, err) = call(
        &m,
        "edit_page",
        json!({ "page_id": page, "base": base, "ops": [
            { "op": "replace", "block": ids[1], "markdown": "完成了登录和注册功能。" },
            { "op": "insert_after", "block": ids[1], "markdown": "插在后面的一段" },
            { "op": "insert_before", "block": format!("⟦{}⟧", ids[0]), "markdown": "最前面的说明" },
            { "op": "append", "markdown": "最后一段" },
            { "op": "delete", "block": ids[2] },
        ]}),
    )
    .await;
    assert!(!err, "{t}");
    assert!(
        t.contains("已改好（5 步）") && t.contains("新的 base"),
        "{t}"
    );
    let doc = s.server_doc(&page).await;
    let texts: Vec<String> = doc.content.iter().map(|n| n.plain_text()).collect();
    assert_eq!(texts.len(), 5, "{texts:?}");
    assert_eq!(texts[0], "最前面的说明");
    assert!(
        texts[2].contains("【人刚打的】") && texts[2].contains("注册"),
        "{texts:?}"
    );
    assert_eq!(texts[3], "插在后面的一段");
    assert_eq!(texts[4], "最后一段");
    eventually("网页收到", async || human.node() == doc).await;
    let aw = human.state.lock().unwrap().awareness.clone();
    assert!(
        aw.iter()
            .any(|j| j.contains("\"kind\":\"agent\"") && j.contains("写作助手")),
        "{aw:?}"
    );
    // 一步出错：都不改
    let before = s.server_doc(&page).await;
    let (t, err) = call(
        &m,
        "edit_page",
        json!({ "page_id": page, "ops": [
            { "op": "append", "markdown": "不该出现" },
            { "op": "delete", "block": "nosuchblk" },
        ]}),
    )
    .await;
    assert!(err && t.contains("第 2 步"), "{t}");
    assert_eq!(s.server_doc(&page).await, before);
    let (t, err) = call(
        &m,
        "edit_page",
        json!({ "page_id": page, "base": "坏的", "ops": [{ "op": "append", "markdown": "x" }] }),
    )
    .await;
    assert!(err && t.contains("base"), "{t}");

    // rewrite_page：没变的块沿用编号；顺便改标题
    let (t, _) = call(&m, "read_page", json!({ "page_id": page })).await;
    let ids = blocks(&t);
    let base = field(&t, "base");
    let (t, err) = call(
        &m,
        "rewrite_page",
        json!({ "page_id": page, "base": base, "title": "周报（改）", "markdown": "最前面的说明\n\n全新的第二段\n" }),
    )
    .await;
    assert!(!err && t.contains("共 2 块"), "{t}");
    let (t, _) = call(&m, "read_page", json!({ "page_id": page })).await;
    assert!(t.starts_with("# 周报（改）"));
    assert_eq!(blocks(&t)[0], ids[0], "没变的块沿用编号");

    // update_page：图标、移动
    let (t, err) = call(
        &m,
        "update_page",
        json!({ "page_id": child, "icon": "📌", "parent_id": "root", "after": "first" }),
    )
    .await;
    assert!(!err && t.contains("📌") && t.contains("最外层"), "{t}");
    let (_, p) = json(c.get(s.api(&format!("/pages/{child}")))).await;
    assert!(p["parent_id"].is_null());
    assert_eq!(p["icon"], "📌");

    // comment / list_comments
    let block = blocks(&t_read(&m, &page).await)[1].clone();
    let (t, err) = call(
        &m,
        "comment",
        json!({ "page_id": page, "block": block, "body": "这段要不要再展开？" }),
    )
    .await;
    assert!(!err, "{t}");
    let thread = field(&t, "thread_id");
    let (t, _) = call(
        &m,
        "comment",
        json!({ "thread_id": thread, "body": "我先补一句" }),
    )
    .await;
    assert!(t.contains("已回复"));
    let (t, _) = call(&m, "list_comments", json!({ "page_id": page })).await;
    assert!(
        t.contains(&format!("thread_id: {thread}"))
            && t.contains(&format!("⟦{block}⟧"))
            && t.contains("我先补一句")
            && t.contains("写作助手（Agent）"),
        "{t}"
    );
    let (t, _) = call(&m, "read_page", json!({ "page_id": page })).await;
    assert!(t.contains("没解决的评论（1 串"), "{t}");
    let (t, _) = call(
        &m,
        "comment",
        json!({ "thread_id": thread, "resolve": true }),
    )
    .await;
    assert!(t.contains("已解决"));
    let (t, _) = call(&m, "list_comments", json!({ "page_id": page })).await;
    assert!(t.contains("没有没解决的评论"), "{t}");
    let (t, _) = call(
        &m,
        "list_comments",
        json!({ "page_id": page, "include_resolved": true }),
    )
    .await;
    assert!(t.contains("已解决"));
    let (t, err) = call(&m, "comment", json!({ "body": "没说给谁" })).await;
    assert!(err, "{t}");

    // my_notifications：人在评论里 @ 它
    let (st, _) = json(
        c.post(s.api(&format!("/pages/{page}/comments")))
            .json(&json!({ "body": "@写作助手 帮忙润色一下" })),
    )
    .await;
    assert_eq!(st, 201);
    let (t, _) = call(&m, "my_notifications", json!({})).await;
    assert!(
        t.contains("【通知 ") && t.contains("帮忙润色一下") && t.contains("comment(thread_id"),
        "{t}"
    );
    let id: i64 = t
        .split("【通知 ")
        .nth(1)
        .unwrap()
        .split('】')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let (t, _) = call(&m, "my_notifications", json!({ "after": id })).await;
    assert_eq!(t, "没有新通知。");
    let (t, _) = call(&m, "my_notifications", json!({})).await;
    assert_eq!(t, "没有新通知。", "after 已经 ack 了");

    // delete_page
    let (t, err) = call(&m, "delete_page", json!({ "page_id": child })).await;
    assert!(!err && t.contains("回收站"), "{t}");
    assert_eq!(json(c.get(s.api(&format!("/pages/{child}")))).await.0, 404);
    let (t, err) = call(&m, "read_page", json!({ "page_id": child })).await;
    assert!(err && t.contains("找不到"), "{t}");

    // 只读的 Agent：能读不能写
    let r = mcp_client(&s, &read_tok).await.unwrap();
    let (t, err) = call(&r, "read_page", json!({ "page_id": page })).await;
    assert!(!err, "{t}");
    let (t, err) = call(
        &r,
        "edit_page",
        json!({ "page_id": page, "ops": [{ "op": "append", "markdown": "x" }] }),
    )
    .await;
    assert!(err && t.contains("没有权限"), "{t}");
    let _ = r.cancel().await;
    let _ = m.cancel().await;
    s.stop().await;
}

async fn t_read(m: &Mcp, page: &str) -> String {
    call(m, "read_page", json!({ "page_id": page })).await.0
}
