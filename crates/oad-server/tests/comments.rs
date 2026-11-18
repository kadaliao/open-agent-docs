//! 评论：增删改、解决、实时事件、人的通知（只推给本人）、叫 Agent 的每一条规则。
mod common;
use common::*;
use futures_util::StreamExt;
use oad_core::AuthConfig;
use serde_json::{Value, json};
use std::time::Duration;

const SECRET: &str = "header-secret";

fn as_user(r: reqwest::RequestBuilder, id: &str) -> reqwest::RequestBuilder {
    r.header("X-OAD-Secret", SECRET)
        .header("X-OAD-User-Id", id)
        .header("X-OAD-User-Name", id)
}

async fn header_server(dir: &std::path::Path) -> Server {
    Server::start(
        dir,
        AuthConfig::Header {
            secret: SECRET.into(),
        },
        fast_hub(),
    )
    .await
}

/// 后台读 SSE，收到的 (事件名, 数据) 放进共享的列表。
fn collect_sse(res: reqwest::Response) -> std::sync::Arc<std::sync::Mutex<Vec<(String, Value)>>> {
    let out: std::sync::Arc<std::sync::Mutex<Vec<(String, Value)>>> = Default::default();
    let o = out.clone();
    tokio::spawn(async move {
        let mut stream = res.bytes_stream();
        let mut buf = String::new();
        while let Some(Ok(chunk)) = stream.next().await {
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(i) = buf.find("\n\n") {
                let block: String = buf.drain(..i + 2).collect();
                let (mut ev, mut data) = (None, String::new());
                for line in block.lines() {
                    if let Some(e) = line.strip_prefix("event:") {
                        ev = Some(e.trim().to_string());
                    } else if let Some(d) = line.strip_prefix("data:") {
                        data.push_str(d.trim());
                    }
                }
                if let Some(ev) = ev {
                    o.lock()
                        .unwrap()
                        .push((ev, serde_json::from_str(&data).unwrap_or(Value::Null)));
                }
            }
        }
    });
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn comments_events_and_inbox() {
    let dir = tempfile::tempdir().unwrap();
    let s = header_server(dir.path()).await;
    let c = http();
    let ws = s.ws_id();
    // alice 第一个来，是 owner；bob 是 editor
    let (_, alice) = json(as_user(c.get(s.api("/me")), "alice")).await;
    let (_, bob) = json(as_user(c.get(s.api("/me")), "bob")).await;
    let (alice, bob) = (
        alice["user"]["id"].as_str().unwrap().to_string(),
        bob["user"]["id"].as_str().unwrap().to_string(),
    );
    let (st, p) = json(
        as_user(c.post(s.api(&format!("/workspaces/{ws}/pages"))), "alice")
            .json(&json!({ "title": "评论页" })),
    )
    .await;
    assert_eq!(st, 201);
    let page = p["id"].as_str().unwrap().to_string();
    let r = as_user(c.put(s.api(&format!("/pages/{page}/content"))), "alice")
        .json(&json!({ "markdown": "第一段文字\n\n第二段\n" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    let block = block_id(&s.server_doc(&page).await, 0);

    let sse_a = collect_sse(
        as_user(c.get(s.api(&format!("/workspaces/{ws}/events"))), "alice")
            .send()
            .await
            .unwrap(),
    );
    let sse_b = collect_sse(
        as_user(c.get(s.api(&format!("/workspaces/{ws}/events"))), "bob")
            .send()
            .await
            .unwrap(),
    );
    tokio::time::sleep(Duration::from_millis(100)).await;

    // alice 选中一段文字评论，@ bob
    let anchor = json!({ "block_id": block, "start": "AQID", "end": "BAUG", "quote": "第一段" });
    let (st, t) = json(
        as_user(c.post(s.api(&format!("/pages/{page}/comments"))), "alice")
            .json(&json!({ "anchor": anchor, "body": format!("@[bob](user:{bob}) 看看这句") })),
    )
    .await;
    assert_eq!(st, 201, "{t}");
    let thread = t["id"].as_str().unwrap().to_string();
    assert_eq!(t["anchor"], anchor);
    assert_eq!(t["created_by"]["name"], "alice");
    assert_eq!(
        t["comments"][0]["author"],
        json!({ "kind": "user", "id": alice, "name": "alice" })
    );

    // bob 回复：alice 收到 reply；bob 不该收到自己的
    let (st, cm) = json(
        as_user(c.post(s.api(&format!("/threads/{thread}/comments"))), "bob")
            .json(&json!({ "body": "好的，我改" })),
    )
    .await;
    assert_eq!(st, 201);
    let reply_id = cm["id"].as_str().unwrap().to_string();
    // 只能改自己的
    let (st, _) = json(
        as_user(c.patch(s.api(&format!("/comments/{reply_id}"))), "alice")
            .json(&json!({ "body": "x" })),
    )
    .await;
    assert_eq!(st, 403);
    let (st, ed) = json(
        as_user(c.patch(s.api(&format!("/comments/{reply_id}"))), "bob")
            .json(&json!({ "body": "好的，马上改" })),
    )
    .await;
    assert_eq!(st, 200);
    assert!(ed["edited_at"].is_i64());
    // alice 解决：bob 收到 resolved
    let (st, th) = json(
        as_user(c.patch(s.api(&format!("/threads/{thread}"))), "alice")
            .json(&json!({ "resolved": true })),
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(th["resolved_by"]["id"], alice.as_str());
    // 已解决的过滤
    let (_, open) = json(as_user(
        c.get(s.api(&format!("/pages/{page}/comments"))),
        "bob",
    ))
    .await;
    assert_eq!(open["threads"].as_array().unwrap().len(), 0);
    let (_, all) = json(as_user(
        c.get(s.api(&format!("/pages/{page}/comments?resolved=all"))),
        "bob",
    ))
    .await;
    assert_eq!(all["threads"][0]["comments"].as_array().unwrap().len(), 2);
    // 删评论
    let r = as_user(c.delete(s.api(&format!("/comments/{reply_id}"))), "bob")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    let (_, all) = json(as_user(
        c.get(s.api(&format!("/pages/{page}/comments?resolved=true"))),
        "bob",
    ))
    .await;
    assert_eq!(all["threads"][0]["comments"][1]["body"], "");
    assert!(all["threads"][0]["comments"][1]["deleted_at"].is_i64());

    // 收件箱
    let (_, inbox_b) = json(as_user(c.get(s.api("/me/notifications")), "bob")).await;
    let kinds_b: Vec<&str> = inbox_b["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds_b, ["resolved", "mention"], "新的在前");
    assert_eq!(inbox_b["unread"], 2);
    assert_eq!(inbox_b["notifications"][1]["data"]["quote"], "第一段");
    assert_eq!(
        inbox_b["notifications"][0]["data"]["by"]["id"],
        alice.as_str()
    );
    let (_, inbox_a) = json(as_user(c.get(s.api("/me/notifications")), "alice")).await;
    assert_eq!(inbox_a["notifications"].as_array().unwrap().len(), 1);
    assert_eq!(inbox_a["notifications"][0]["type"], "reply");
    let first = inbox_b["notifications"][1]["id"].as_i64().unwrap();
    let (_, r) = json(
        as_user(c.post(s.api("/me/notifications/read")), "bob").json(&json!({ "ids": [first] })),
    )
    .await;
    assert_eq!(r["read"], 1);
    let (_, r) =
        json(as_user(c.post(s.api("/me/notifications/read")), "bob").json(&json!({ "all": true })))
            .await;
    assert_eq!(r["read"], 1);
    let (_, inbox_b) = json(as_user(c.get(s.api("/me/notifications")), "bob")).await;
    assert_eq!(inbox_b["unread"], 0);

    // 实时事件：两个人都收到评论事件；notification 只推给本人
    let want = [
        "comment.created",
        "comment.created",
        "comment.updated",
        "thread.updated",
        "comment.deleted",
    ];
    eventually("SSE 收到评论事件", async || {
        let names = |v: &Vec<(String, Value)>| {
            v.iter()
                .filter(|(e, _)| e.starts_with("comment.") || e.starts_with("thread."))
                .count()
        };
        names(&sse_a.lock().unwrap()) == want.len() && names(&sse_b.lock().unwrap()) == want.len()
    })
    .await;
    let a = sse_a.lock().unwrap().clone();
    let got: Vec<&str> = a
        .iter()
        .map(|(e, _)| e.as_str())
        .filter(|e| *e != "notification")
        .collect();
    assert_eq!(got, want);
    assert!(
        a.iter()
            .all(|(e, d)| e == "notification" || d["page_id"] == page.as_str())
    );
    let notes_a: Vec<&Value> = a
        .iter()
        .filter(|(e, _)| e == "notification")
        .map(|(_, d)| d)
        .collect();
    let b = sse_b.lock().unwrap().clone();
    let notes_b: Vec<&Value> = b
        .iter()
        .filter(|(e, _)| e == "notification")
        .map(|(_, d)| d)
        .collect();
    assert_eq!(notes_a.len(), 1);
    assert_eq!(notes_a[0]["type"], "reply");
    assert_eq!(notes_b.len(), 2);
    assert_eq!(notes_b[0]["type"], "mention");
    s.stop().await;
}

/// admin token 看到的、after 之后新来的通知：(给谁, 评论内容)。
async fn new_notes(
    s: &Server,
    c: &reqwest::Client,
    admin: &str,
    after: &mut i64,
) -> Vec<(String, String)> {
    tokio::time::sleep(Duration::from_millis(50)).await;
    let (st, v) = json(
        c.get(s.api(&format!("/agent/notifications?after={after}&limit=100")))
            .bearer_auth(admin),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    let list = v["notifications"].as_array().unwrap();
    if let Some(last) = list.last() {
        *after = last["id"].as_i64().unwrap();
    }
    list.iter()
        .map(|n| {
            (
                n["agent"]["name"].as_str().unwrap().to_string(),
                n["comment"]["body"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn notify_rules() {
    let dir = tempfile::tempdir().unwrap();
    let s = header_server(dir.path()).await;
    let c = http();
    let ws = s.ws_id();
    json(as_user(c.get(s.api("/me")), "alice")).await;
    let agents_url = s.api(&format!("/workspaces/{ws}/agents"));
    let mk = async |name: &str, scope: &str| {
        make_agent(as_user(c.post(&agents_url), "alice"), name, scope, None).await
    };
    let (a1, t1, _) = mk("甲", "write").await;
    let (a2, t2, _) = mk("乙", "write").await;
    let (_a3, t3, _) = mk("丙", "write").await;
    let (_a4, t4, _) = mk("丁", "write").await;
    let (_rd, _, _) = mk("读者", "read").await;
    let (rv, _, _) = mk("撤了", "write").await;
    let (_, admin, _) = mk("管家", "admin").await;
    let r = as_user(c.delete(s.api(&format!("/agents/{rv}"))), "alice")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 200);

    // 人建的页面
    let page = {
        let (_, p) = json(
            as_user(c.post(s.api(&format!("/workspaces/{ws}/pages"))), "alice")
                .json(&json!({ "title": "规则" })),
        )
        .await;
        p["id"].as_str().unwrap().to_string()
    };
    let r = as_user(c.put(s.api(&format!("/pages/{page}/content"))), "alice")
        .json(&json!({ "markdown": "人写的一段\n" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    let comment = async |who: &str, page: &str, body: Value| {
        let (st, t) =
            json(as_user(c.post(s.api(&format!("/pages/{page}/comments"))), who).json(&body)).await;
        assert_eq!(st, 201, "{t}");
        t["id"].as_str().unwrap().to_string()
    };
    let mut cur = 0i64;

    // 什么都对不上：谁也不叫
    comment("alice", &page, json!({ "body": "随便说一句" })).await;
    assert!(new_notes(&s, &c, &admin, &mut cur).await.is_empty());

    // 1. 明确指定 > @
    comment(
        "alice",
        &page,
        json!({ "body": "@甲 看看", "notify": [format!("agent:{a2}")] }),
    )
    .await;
    assert_eq!(
        new_notes(&s, &c, &admin, &mut cur).await,
        [("乙".to_string(), "@甲 看看".to_string())]
    );
    comment(
        "alice",
        &page,
        json!({ "body": "@甲 先别动", "notify": false }),
    )
    .await;
    assert!(new_notes(&s, &c, &admin, &mut cur).await.is_empty());

    // 2. @ 到的 Agent，最多 3 个；只读的、撤销的不叫
    comment(
        "alice",
        &page,
        json!({ "body": "@读者 @撤了 @甲 @乙 @丙 @丁 都来" }),
    )
    .await;
    let got: Vec<String> = new_notes(&s, &c, &admin, &mut cur)
        .await
        .into_iter()
        .map(|n| n.0)
        .collect();
    assert_eq!(got, ["甲", "乙", "丙"]);
    comment(
        "alice",
        &page,
        json!({ "body": format!("@[甲](agent:{a1}) 用链接写法") }),
    )
    .await;
    assert_eq!(new_notes(&s, &c, &admin, &mut cur).await[0].0, "甲");

    // 3. 回复时叫这串里最后说话的 Agent；Agent 写的评论不叫任何 Agent
    let t = comment("alice", &page, json!({ "body": "开个头", "notify": false })).await;
    let (st, _) = json(
        c.post(s.api(&format!("/threads/{t}/comments")))
            .bearer_auth(&t3)
            .json(&json!({ "body": "@甲 我是丙，顺便叫一下甲" })),
    )
    .await;
    assert_eq!(st, 201);
    assert!(
        new_notes(&s, &c, &admin, &mut cur).await.is_empty(),
        "Agent 写的不叫 Agent"
    );
    json(
        as_user(c.post(s.api(&format!("/threads/{t}/comments"))), "alice")
            .json(&json!({ "body": "丙你继续" })),
    )
    .await;
    assert_eq!(
        new_notes(&s, &c, &admin, &mut cur).await,
        [("丙".to_string(), "丙你继续".to_string())]
    );

    // 4. 这段的作者是 Agent
    let (_, r) = json(
        c.post(s.api(&format!("/pages/{page}/blocks")))
            .bearer_auth(&t4)
            .json(&json!({ "markdown": "丁写的一段话，比较长" })),
    )
    .await;
    let ding_block = r["ids"][0].as_str().unwrap().to_string();
    comment(
        "alice",
        &page,
        json!({ "body": "这段有问题", "anchor": { "block_id": ding_block, "quote": "丁写的" } }),
    )
    .await;
    assert_eq!(new_notes(&s, &c, &admin, &mut cur).await[0].0, "丁");
    // 人写的那段：不叫（页面也是人建的）
    let human_block = block_id(&s.server_doc(&page).await, 0);
    comment(
        "alice",
        &page,
        json!({ "body": "这段也看看", "anchor": { "block_id": human_block } }),
    )
    .await;
    assert!(new_notes(&s, &c, &admin, &mut cur).await.is_empty());

    // 5. 建页面的是 Agent
    let (_, p) = json(
        c.post(s.api(&format!("/workspaces/{ws}/pages")))
            .bearer_auth(&t1)
            .json(&json!({ "title": "甲建的页" })),
    )
    .await;
    let agent_page = p["id"].as_str().unwrap().to_string();
    comment("alice", &agent_page, json!({ "body": "整页的意见" })).await;
    let n = new_notes(&s, &c, &admin, &mut cur).await;
    assert_eq!(n, [("甲".to_string(), "整页的意见".to_string())]);

    // 通知内容：带 prompt、页面链接、作者
    let (_, v) = json(c.get(s.api("/agent/notifications")).bearer_auth(&t1)).await;
    let last = v["notifications"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["type"], "comment");
    assert_eq!(last["agent"]["id"], a1.as_str());
    assert_eq!(last["page"]["title"], "甲建的页");
    assert!(
        last["page"]["url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/p/{agent_page}"))
    );
    assert_eq!(last["comment"]["author"]["kind"], "user");
    let prompt = last["prompt"].as_str().unwrap();
    assert!(
        prompt.contains("甲建的页")
            && prompt.contains(last["thread_id"].as_str().unwrap())
            && prompt.contains("comment("),
        "{prompt}"
    );
    // 普通 Agent 只看到自己的；admin 看到所有的
    assert!(
        v["notifications"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["agent"]["id"] == a1.as_str())
    );
    let (_, v2) = json(c.get(s.api("/agent/notifications")).bearer_auth(&t2)).await;
    assert!(
        v2["notifications"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["agent"]["id"] == a2.as_str())
    );
    // 人不能取 Agent 通知
    let (st, _) = json(as_user(c.get(s.api("/agent/notifications")), "alice")).await;
    assert_eq!(st, 403);
    s.stop().await;
}
