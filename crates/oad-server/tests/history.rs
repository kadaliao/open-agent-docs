//! 历史：自动存版本（停下来、Agent 的每次写入、卸载前）、看某一版、按块对比（谁改的）、恢复（新的一次改动）、手动存；
//! 数据库的格子 / 行 / 表结构历史和撤销。MCP 的 page_history、restore_version。
mod common;
use common::*;
use oad_core::{AuthConfig, HubConfig};
use serde_json::{Value, json};
use std::time::Duration;

fn hub() -> HubConfig {
    HubConfig {
        flush_window: Duration::from_millis(5),
        version_idle: Duration::from_millis(150),
        ..HubConfig::default()
    }
}

async fn versions(s: &Server, c: &reqwest::Client, page: &str) -> Vec<Value> {
    let (st, v) = json(c.get(s.api(&format!("/pages/{page}/versions")))).await;
    assert_eq!(st, 200, "{v}");
    v["versions"].as_array().unwrap().clone()
}

async fn put_md(s: &Server, c: &reqwest::Client, page: &str, md: &str, token: Option<&str>) {
    let mut r = c
        .put(s.api(&format!("/pages/{page}/content")))
        .json(&json!({ "markdown": md }));
    if let Some(t) = token {
        r = r.bearer_auth(t);
    }
    let res = r.send().await.unwrap();
    assert_eq!(res.status().as_u16(), 204);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn page_versions() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, hub()).await;
    let c = http();
    let agents = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (_, token, _) = make_agent(c.post(&agents), "改稿助手", "write", None).await;

    // 人写了两次：停下来之后自动存一版（两次合成一版）
    let page = page_with(&s, &c, "历史页", "第一段\n\n第二段").await;
    put_md(&s, &c, &page, "第一段\n\n第二段\n\n第三段", None).await;
    eventually("自动存一版", async || {
        versions(&s, &c, &page).await.len() == 1
    })
    .await;
    let v1 = versions(&s, &c, &page).await[0].clone();
    assert_eq!(v1["kind"], "auto");
    assert_eq!(v1["authors"][0]["kind"], "user");

    // Agent 写：单独成一版；之前人没存的改动先存一版
    put_md(
        &s,
        &c,
        &page,
        "第一段\n\n第二段改了\n\n第三段\n\n第四段",
        None,
    )
    .await;
    put_md(
        &s,
        &c,
        &page,
        "第一段\n\n第二段被 Agent 改了\n\n第三段\n\n第四段",
        Some(&token),
    )
    .await;
    let vs = versions(&s, &c, &page).await;
    assert_eq!(vs.len(), 3, "{vs:?}");
    assert_eq!(
        (vs[0]["kind"].as_str(), vs[1]["kind"].as_str()),
        (Some("agent"), Some("auto"))
    );
    assert_eq!(
        vs[0]["authors"],
        json!([{ "kind": "agent", "id": vs[0]["created_by"], "name": "改稿助手" }])
    );
    let (agent_v, before_v) = (vs[0]["id"].as_i64().unwrap(), vs[1]["id"].as_i64().unwrap());

    // 看某一版
    let (st, v) = json(c.get(s.api(&format!("/pages/{page}/versions/{}", v1["id"])))).await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(
        v["markdown"].as_str().unwrap().trim(),
        "第一段\n\n第二段\n\n第三段"
    );
    assert_eq!(v["version"]["id"], v1["id"]);

    // 对比：Agent 那一版比前一版改了第二段，作者是 Agent
    let (st, d) = json(c.get(s.api(&format!(
        "/pages/{page}/versions/{before_v}/diff?to={agent_v}"
    ))))
    .await;
    assert_eq!(st, 200, "{d}");
    let changed: Vec<&Value> = d["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["op"] != "same")
        .collect();
    assert_eq!(changed.len(), 1, "{d}");
    assert_eq!(changed[0]["op"], "changed");
    assert_eq!(changed[0]["after"], "第二段被 Agent 改了");
    assert_eq!(changed[0]["by"][0]["name"], "改稿助手");
    assert_eq!(changed[0]["type"], "paragraph");
    // 和现在比
    let (_, d) = json(c.get(s.api(&format!(
        "/pages/{page}/versions/{}/diff?to=current",
        v1["id"]
    ))))
    .await;
    let ops: Vec<&str> = d["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, vec!["same", "changed", "same", "added"]);

    // 恢复到第一版：内容回去，是一次新的改动，后来的版本都还在
    let (st, r) =
        json(c.post(s.api(&format!("/pages/{page}/versions/{}/restore", v1["id"])))).await;
    assert_eq!(st, 200, "{r}");
    assert_eq!(r["version"]["kind"], "restore");
    let (_, now) = json(c.get(s.api(&format!("/pages/{page}/content")))).await;
    assert_eq!(
        now["markdown"].as_str().unwrap().trim(),
        "第一段\n\n第二段\n\n第三段"
    );
    let vs = versions(&s, &c, &page).await;
    assert!(vs.len() >= 4);
    assert!(vs.iter().any(|v| v["id"] == agent_v));
    // 还能恢复回 Agent 那一版
    let (st, _) = json(c.post(s.api(&format!("/pages/{page}/versions/{agent_v}/restore")))).await;
    assert_eq!(st, 200);
    let (_, now) = json(c.get(s.api(&format!("/pages/{page}/content")))).await;
    assert!(now["markdown"].as_str().unwrap().contains("被 Agent 改了"));

    // 手动存一版，带名字；没改也能存
    let (st, m) = json(
        c.post(s.api(&format!("/pages/{page}/versions")))
            .json(&json!({ "label": "定稿" })),
    )
    .await;
    assert_eq!(st, 201, "{m}");
    assert_eq!(
        (m["kind"].as_str(), m["label"].as_str()),
        (Some("manual"), Some("定稿"))
    );
    let (st, _) = json(c.get(s.api(&format!("/pages/{page}/versions/99999")))).await;
    assert_eq!(st, 404);
    s.stop().await;
}

/// 没停下来就卸载 / 关机：存一版。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn version_on_unload() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(
        dir.path(),
        AuthConfig::None,
        HubConfig {
            flush_window: Duration::from_millis(5),
            version_idle: Duration::from_secs(3600),
            ..HubConfig::default()
        },
    )
    .await;
    let c = http();
    let page = page_with(&s, &c, "关机前", "写了一半").await;
    assert!(versions(&s, &c, &page).await.is_empty());
    s.stop().await;
    let s = Server::start(dir.path(), AuthConfig::None, hub()).await;
    let vs = versions(&s, &c, &page).await;
    assert_eq!(vs.len(), 1);
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcp_history_tools() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, hub()).await;
    let c = http();
    let agents = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (_, token, _) = make_agent(c.post(&agents), "历史助手", "write", None).await;
    let m = mcp_client(&s, &token).await.unwrap();
    let (t, _) = call(
        &m,
        "create_page",
        json!({ "title": "MCP 历史", "markdown": "原文" }),
    )
    .await;
    let page = field(&t, "page_id");
    let (t, err) = call(
        &m,
        "rewrite_page",
        json!({ "page_id": page, "markdown": "改过了" }),
    )
    .await;
    assert!(!err, "{t}");
    let (t, err) = call(&m, "page_history", json!({ "page_id": page })).await;
    assert!(!err, "{t}");
    assert!(t.contains("Agent 写入") && t.contains("历史助手"), "{t}");
    assert!(t.contains("改 1 块") || t.contains("加 1 块"), "{t}");
    let oldest: i64 = t
        .lines()
        .filter_map(|l| l.strip_prefix("- #"))
        .next_back()
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let (t, err) = call(
        &m,
        "restore_version",
        json!({ "page_id": page, "version": oldest }),
    )
    .await;
    assert!(!err, "{t}");
    assert!(t.contains("已恢复"), "{t}");
    let (_, now) = json(c.get(s.api(&format!("/pages/{page}/content")))).await;
    assert_eq!(now["markdown"].as_str().unwrap().trim(), "原文");
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn database_history_and_undo() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, hub()).await;
    let c = http();
    let (_, db) = json(c.post(s.api(&format!("/workspaces/{}/databases", s.ws_id()))).json(&json!({
        "title": "库", "schema": { "props": [{ "name": "名称", "type": "title" }, { "name": "价格", "type": "number" }] }
    })))
    .await;
    let db = db["page"]["id"].as_str().unwrap().to_string();
    let (_, add) = json(c.post(s.api(&format!("/databases/{db}/rows"))).json(
        &json!({ "rows": [{ "props": { "名称": "a", "价格": 1 } }, { "props": { "名称": "b" } }] }),
    ))
    .await;
    let (ids, add_seq) = (add["ids"].clone(), add["seq"].as_i64().unwrap());
    let (r0, r1) = (
        ids[0].as_str().unwrap().to_string(),
        ids[1].as_str().unwrap().to_string(),
    );
    let (_, p1) = json(
        c.patch(s.api(&format!("/databases/{db}/rows")))
            .json(&json!({ "ops": [{ "row_id": r0, "set": { "价格": 2, "名称": "a2" } }] })),
    )
    .await;
    let p1 = p1["seq"].as_i64().unwrap();
    let (_, p2) = json(
        c.patch(s.api(&format!("/databases/{db}/rows")))
            .json(&json!({ "ops": [{ "row_id": r0, "set": { "价格": 3 } }] })),
    )
    .await;
    let p2 = p2["seq"].as_i64().unwrap();

    // 一行一个属性的历史（属性写名字）
    let (st, h) = json(c.get(s.api(&format!("/databases/{db}/history?row={r0}&prop=价格")))).await;
    assert_eq!(st, 200, "{h}");
    let items = h["items"].as_array().unwrap();
    let vals: Vec<Value> = items.iter().map(|o| o["new"].clone()).collect();
    assert_eq!(vals, vec![json!(3), json!(2)]);
    assert_eq!(items[0]["kind"], "cell");
    assert_eq!(items[0]["prop_name"], "价格");
    assert_eq!(items[0]["row_title"], "a2");
    assert_eq!(items[0]["by"]["kind"], "user");
    let (_, all) = json(c.get(s.api(&format!("/databases/{db}/history")))).await;
    let all_items = all["items"].as_array().unwrap();
    assert!(
        all_items
            .iter()
            .any(|i| i["kind"] == "row" && i["summary"] == "加了一行")
    );
    let key = |name: &str| -> String {
        all_items.iter().find(|i| i["prop_name"] == name).unwrap()["prop"]
            .as_str()
            .unwrap()
            .to_string()
    };

    // 撤销 p1：价格之后又被改成 3，不动；名称换回 a（行页面的标题也是）
    let (st, u) = json(
        c.post(s.api(&format!("/databases/{db}/history/undo")))
            .json(&json!({ "seq": p1 })),
    )
    .await;
    assert_eq!(st, 200, "{u}");
    let (_, rows) = json(c.get(s.api(&format!("/databases/{db}/rows")))).await;
    let row0 = rows["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == r0.as_str())
        .unwrap()
        .clone();
    assert_eq!(row0["props"][key("价格")], 3);
    assert_eq!(row0["props"][key("名称")], "a");
    let (_, pg) = json(c.get(s.api(&format!("/pages/{r0}")))).await;
    assert_eq!(pg["title"], "a");
    let _ = p2;

    // 删一行再撤销：放回来；撤销加行：两行都删掉
    let (_, del) = json(
        c.delete(s.api(&format!("/databases/{db}/rows")))
            .json(&json!({ "ids": [r1] })),
    )
    .await;
    let (st, _) = json(
        c.post(s.api(&format!("/databases/{db}/history/undo")))
            .json(&json!({ "seq": del["seq"] })),
    )
    .await;
    assert_eq!(st, 200);
    let (_, rows) = json(c.get(s.api(&format!("/databases/{db}/rows")))).await;
    assert_eq!(rows["rows"].as_array().unwrap().len(), 2);
    let (st, _) = json(c.get(s.api(&format!("/pages/{r1}")))).await;
    assert_eq!(st, 200, "行页面也回来了");
    let (st, _) = json(
        c.post(s.api(&format!("/databases/{db}/history/undo")))
            .json(&json!({ "seq": add_seq })),
    )
    .await;
    assert_eq!(st, 200);
    let (_, rows) = json(c.get(s.api(&format!("/databases/{db}/rows")))).await;
    assert!(rows["rows"].as_array().unwrap().is_empty());

    // 表结构改动也在历史里，也能经这里撤销
    let (_, sc) = json(
        c.patch(s.api(&format!("/databases/{db}/schema")))
            .json(&json!({ "ops": [{ "op": "add", "name": "备注", "type": "text" }] })),
    )
    .await;
    let (_, all) = json(c.get(s.api(&format!("/databases/{db}/history")))).await;
    assert_eq!(all["items"][0]["seq"], sc["seq"]);
    assert_eq!(all["items"][0]["kind"], "schema");
    assert_eq!(all["items"][0]["summary"], "加了「备注」");
    let (st, _) = json(
        c.post(s.api(&format!("/databases/{db}/history/undo")))
            .json(&json!({ "seq": sc["seq"] })),
    )
    .await;
    assert_eq!(st, 200);
    let (_, info) = json(c.get(s.api(&format!("/databases/{db}")))).await;
    assert!(!info["schema"].to_string().contains("备注"));
    let (_, all) = json(c.get(s.api(&format!("/databases/{db}/history")))).await;
    let added = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["seq"] == sc["seq"])
        .unwrap()
        .clone();
    assert_eq!(added["undone"], true);
    let (st, _) = json(
        c.post(s.api(&format!("/databases/{db}/history/undo")))
            .json(&json!({ "seq": 9999 })),
    )
    .await;
    assert_eq!(st, 404);
    s.stop().await;
}
