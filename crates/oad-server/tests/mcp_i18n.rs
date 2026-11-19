//! MCP 双语：Agent 设了 locale: en 就拿英文的工具说明、参数说明、服务说明、结果和出错。
mod common;
use common::*;
use oad_core::AuthConfig;
use serde_json::{Value, json};

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3000..=0x303F | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xFF00..=0xFFEF)
}

fn no_cjk(s: &str) -> bool {
    !s.chars().any(is_cjk)
}

fn descs(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                match x.as_str() {
                    Some(d) if k == "description" => out.push(d.to_string()),
                    _ => descs(x, out),
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| descs(x, out)),
        _ => {}
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn english_agent() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let agents = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (agent, token, _) = make_agent(c.post(&agents), "writer", "write", None).await;
    let (_, zh_token, _) = make_agent(c.post(&agents), "写作助手", "write", None).await;
    let (st, v) = json(
        c.patch(s.api(&format!("/agents/{agent}")))
            .json(&json!({ "locale": "en" })),
    )
    .await;
    assert_eq!(st, 200, "{v}");

    let m = mcp_client(&s, &token).await.unwrap();
    // 服务说明、工具说明、参数说明都是英文
    let info = m.peer_info().unwrap();
    let ins = info.instructions.as_deref().unwrap();
    assert!(no_cjk(ins) && ins.contains("⟦block_id⟧"), "{ins}");
    let tools = m.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 19);
    let mut n = 0;
    for t in &tools {
        let d = t.description.as_deref().unwrap();
        assert!(no_cjk(d), "{}: {d}", t.name);
        let mut ps = Vec::new();
        descs(&Value::Object((*t.input_schema).clone()), &mut ps);
        for p in &ps {
            assert!(no_cjk(p), "{}: {p}", t.name);
        }
        n += ps.len();
    }
    assert!(n > 40, "参数说明太少：{n}");

    // create_page
    let (t, err) = call(
        &m,
        "create_page",
        json!({ "title": "Weekly", "markdown": "Shipped the login page.\n\n- one\n- two\n" }),
    )
    .await;
    assert!(!err, "{t}");
    assert!(
        t.starts_with("Created page \"Weekly\"") && no_cjk(&t),
        "{t}"
    );
    let page = field(&t, "page_id");

    // read_page
    let (t, _) = call(&m, "read_page", json!({ "page_id": page })).await;
    assert!(
        t.contains("· 2 blocks") && t.contains("updated ") && no_cjk(&t),
        "{t}"
    );
    let (t, _) = call(&m, "read_page", json!({ "page_id": page, "max_blocks": 1 })).await;
    assert!(
        t.contains("(1 more blocks. Continue with: read_page(") && no_cjk(&t),
        "{t}"
    );

    // list_pages
    let (t, _) = call(&m, "list_pages", json!({})).await;
    assert!(
        t.contains(&format!("- Weekly (page_id: {page}, updated ")) && no_cjk(&t),
        "{t}"
    );

    // search_pages 没结果
    let (t, _) = call(&m, "search_pages", json!({ "query": "nothingmatches" })).await;
    assert_eq!(t, "No pages found for \"nothingmatches\".");

    // edit_page
    let (t, _) = call(&m, "read_page", json!({ "page_id": page })).await;
    let base = field(&t, "base");
    let (t, err) = call(
        &m,
        "edit_page",
        json!({ "page_id": page, "base": base, "ops": [{ "op": "append", "markdown": "More." }] }),
    )
    .await;
    assert!(
        !err && t.starts_with("Done (1 steps):") && t.contains("New base: ") && no_cjk(&t),
        "{t}"
    );

    // db_create、db_add、db_query：固定的部分是英文（表结构那几行是 core 给的）
    let (t, err) = call(
        &m,
        "db_create",
        json!({ "title": "Tasks", "props": [{ "name": "Name", "type": "title" }, { "name": "Price", "type": "number" }] }),
    )
    .await;
    assert!(!err, "{t}");
    assert!(
        t.starts_with("Created database \"Tasks\"") && t.contains("\nProperties:\n"),
        "{t}"
    );
    let db = field(&t, "db_id");
    let (t, _) = call(&m, "db_query", json!({ "db_id": db })).await;
    assert!(
        t.contains("No matching rows.") && t.contains("0 rows in total"),
        "{t}"
    );
    let (t, err) = call(
        &m,
        "db_add",
        json!({ "db_id": db, "rows": [{ "Name": "Write report", "Price": 3 }] }),
    )
    .await;
    assert!(!err && t.starts_with("Added 1 rows, row_id:"), "{t}");
    let (t, _) = call(&m, "db_query", json!({ "db_id": db })).await;
    assert!(
        t.contains("| row_id | Name | Price |")
            && t.contains("1 rows in total; these are rows 1–1")
            && no_cjk(&t),
        "{t}"
    );

    // 出错也是英文
    let (t, err) = call(&m, "read_page", json!({ "page_id": "nope" })).await;
    assert!(err && t.starts_with("Not found: ") && no_cjk(&t), "{t}");
    let (t, err) = call(
        &m,
        "edit_page",
        json!({ "page_id": page, "ops": [{ "op": "replace" }] }),
    )
    .await;
    assert_eq!(
        (t.as_str(), err),
        ("Bad arguments: Step 1 needs block", true)
    );
    let (t, err) = call(&m, "db_query", json!({ "db_id": db, "view": "Nope" })).await;
    assert_eq!((t.as_str(), err), ("Not found: No view \"Nope\"", true));

    // 没设语言的 Agent 照旧是中文
    let z = mcp_client(&s, &zh_token).await.unwrap();
    assert!(
        z.peer_info()
            .unwrap()
            .instructions
            .as_deref()
            .unwrap()
            .contains("⟦块编号⟧")
    );
    let zt = z.list_all_tools().await.unwrap();
    assert!(
        zt.iter()
            .any(|t| t.description.as_deref().is_some_and(|d| !no_cjk(d)))
    );
    let (t, _) = call(&z, "search_pages", json!({ "query": "nothingmatches" })).await;
    assert_eq!(t, "没有找到和「nothingmatches」有关的页面。");
    let (t, _) = call(&z, "read_page", json!({ "page_id": "nope" })).await;
    assert!(t.starts_with("找不到："), "{t}");

    // 不是 Agent：按 Accept-Language
    let r = c
        .post(s.mcp())
        .header("accept-language", "en-US,en;q=0.9")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 401);
    let v: Value = r.json().await.unwrap();
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("An agent token is required")
    );
}
