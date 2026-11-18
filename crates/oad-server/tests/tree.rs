//! 页面树：建、排序、移动、不能成环、连子页一起删；SSE 收到事件。
mod common;
use common::*;
use futures_util::StreamExt;
use oad_core::AuthConfig;
use serde_json::{Value, json};
use std::time::Duration;

/// 读 SSE，直到收到 n 个事件。
async fn read_events(res: reqwest::Response, n: usize) -> Vec<(String, Value)> {
    let mut stream = res.bytes_stream();
    let mut buf = String::new();
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while out.len() < n {
        let chunk = tokio::time::timeout_at(deadline, stream.next())
            .await
            .expect("等事件超时")
            .unwrap()
            .unwrap();
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(i) = buf.find("\n\n") {
            let block: String = buf.drain(..i + 2).collect();
            let mut ev = None;
            let mut data = String::new();
            for line in block.lines() {
                if let Some(e) = line.strip_prefix("event:") {
                    ev = Some(e.trim().to_string());
                } else if let Some(d) = line.strip_prefix("data:") {
                    data.push_str(d.trim());
                }
            }
            if let Some(ev) = ev {
                out.push((ev, serde_json::from_str(&data).unwrap()));
            }
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn page_tree_and_events() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let ws = s.ws_id();
    let sse = c
        .get(s.api(&format!("/workspaces/{ws}/events")))
        .send()
        .await
        .unwrap();
    assert_eq!(sse.status().as_u16(), 200);
    let events = tokio::spawn(read_events(sse, 8));

    let create = async |body: Value| {
        let (st, p) = json(
            c.post(s.api(&format!("/workspaces/{ws}/pages")))
                .json(&body),
        )
        .await;
        assert_eq!(st, 201, "{p}");
        p["id"].as_str().unwrap().to_string()
    };
    let a = create(json!({ "title": "A" })).await;
    let b = create(json!({ "title": "B" })).await;
    // 插在 A 后面（A 和 B 中间）
    let ab = create(json!({ "title": "AB", "after": a })).await;
    let child = create(json!({ "title": "A 的子页", "parent_id": a })).await;
    let grand = create(json!({ "title": "孙页", "parent_id": child })).await;

    let list = async || {
        let (_, l) = json(c.get(s.api(&format!("/workspaces/{ws}/pages")))).await;
        l["pages"].as_array().unwrap().clone()
    };
    let roots = |pages: &[Value]| {
        let mut r: Vec<(String, String)> = pages
            .iter()
            .filter(|p| p["parent_id"].is_null())
            .map(|p| {
                (
                    p["position"].as_str().unwrap().to_string(),
                    p["title"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        r.sort();
        r.into_iter().map(|(_, t)| t).collect::<Vec<_>>()
    };
    assert_eq!(roots(&list().await), ["A", "AB", "B"]);

    // 移到最前（after: null）
    let (st, p) = json(
        c.patch(s.api(&format!("/pages/{b}")))
            .json(&json!({ "after": null })),
    )
    .await;
    assert_eq!(st, 200, "{p}");
    assert_eq!(roots(&list().await), ["B", "A", "AB"]);

    // 成环：A 移到自己的孙页下面、移到自己下面
    let (st, e) = json(
        c.patch(s.api(&format!("/pages/{a}")))
            .json(&json!({ "parent_id": grand })),
    )
    .await;
    assert_eq!(st, 400, "{e}");
    let (st, _) = json(
        c.patch(s.api(&format!("/pages/{a}")))
            .json(&json!({ "parent_id": a })),
    )
    .await;
    assert_eq!(st, 400);
    // after 不是同级
    let (st, _) = json(
        c.patch(s.api(&format!("/pages/{ab}")))
            .json(&json!({ "after": child })),
    )
    .await;
    assert_eq!(st, 400);

    // 子页移到根、放在 A 后面
    let (st, p) = json(
        c.patch(s.api(&format!("/pages/{child}")))
            .json(&json!({ "parent_id": null, "after": a })),
    )
    .await;
    assert_eq!(st, 200);
    assert!(p["parent_id"].is_null());
    assert_eq!(roots(&list().await), ["B", "A", "A 的子页", "AB"]);
    // 再移回 A 下面
    let (st, p) = json(
        c.patch(s.api(&format!("/pages/{child}")))
            .json(&json!({ "parent_id": a })),
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(p["parent_id"], a.as_str());

    // 连子页、孙页一起删
    let (st, d) = json(c.delete(s.api(&format!("/pages/{a}")))).await;
    assert_eq!(st, 200);
    let mut deleted: Vec<String> = d["deleted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    deleted.sort();
    let mut want = vec![a.clone(), child.clone(), grand.clone()];
    want.sort();
    assert_eq!(deleted, want);
    let titles: Vec<String> = list()
        .await
        .iter()
        .map(|p| p["title"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(titles.len(), 2, "{titles:?}");
    let (st, _) = json(c.get(s.api(&format!("/pages/{grand}")))).await;
    assert_eq!(st, 404);
    // 删掉的页面不能再改、不能同步
    let (st, _) = json(c.get(s.api(&format!("/pages/{a}/content")))).await;
    assert_eq!(st, 404);
    let mut y = YClient::new(9001);
    y.connect(&s.ws(&a), &[]).await;
    eventually("被拒绝", async || {
        y.state.lock().unwrap().denied.is_some()
    })
    .await;

    let evs = events.await.unwrap();
    let kinds: Vec<&str> = evs.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "page.created",
            "page.created",
            "page.created",
            "page.created",
            "page.created",
            "page.moved",
            "page.moved",
            "page.moved",
        ]
    );
    assert_eq!(evs[0].1["title"], "A");
    assert_eq!(evs[5].1["id"], b.as_str());
    // 删除事件
    let sse = c
        .get(s.api(&format!("/workspaces/{ws}/events")))
        .send()
        .await
        .unwrap();
    let events = tokio::spawn(read_events(sse, 2));
    let (st, _) = json(c.delete(s.api(&format!("/pages/{ab}")))).await;
    assert_eq!(st, 200);
    let (st, _) = json(
        c.patch(s.api(&format!("/pages/{b}")))
            .json(&json!({ "icon": "📄" })),
    )
    .await;
    assert_eq!(st, 200);
    let evs = events.await.unwrap();
    assert_eq!(evs[0].0, "page.deleted");
    assert_eq!(evs[0].1["id"], ab.as_str());
    assert_eq!(evs[1].0, "page.updated");
    assert_eq!(evs[1].1["icon"], "📄");

    // 不是成员的工作区
    let (st, _) = json(c.get(s.api("/workspaces/nope/pages"))).await;
    assert_eq!(st, 403);
    s.stop().await;
}
