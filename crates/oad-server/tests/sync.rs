//! WebSocket 同步：两个客户端同时改同一段、离线后重连合并、HTTP 按块改和人同时打字。
mod common;
use common::*;
use oad_core::AuthConfig;
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_clients_converge() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let page = page_with(&s, &c, "并发", "第一段原来的文字。\n\n第二段\n").await;
    let para = block_id(&s.server_doc(&page).await, 0);

    let mut a = YClient::new(1001);
    let mut b = YClient::new(1002);
    a.connect(&s.ws(&page), &[]).await;
    b.connect(&s.ws(&page), &[]).await;
    eventually("两边都同步上", async || a.synced() && b.synced()).await;
    assert_eq!(a.node(), s.server_doc(&page).await);

    // 两边同时在同一段里打字，交错着来，大部分按键都和对方的并发
    let (pa, pb) = (para.clone(), para.clone());
    let (da, db) = (&a, &b);
    let ta = async move {
        for _ in 0..40 {
            da.type_at(&pa, 2, "a");
            tokio::task::yield_now().await;
        }
    };
    let tb = async move {
        for _ in 0..40 {
            db.type_at(&pb, 100, "b");
            tokio::task::yield_now().await;
        }
    };
    tokio::join!(ta, tb);

    eventually("两个客户端和服务器一致", async || {
        let server = s.server_doc(&page).await;
        a.node() == server && b.node() == server
    })
    .await;
    let text = a.node().content[0].plain_text();
    assert_eq!(text.matches('a').count(), 40, "{text}");
    assert_eq!(text.matches('b').count(), 40, "{text}");
    assert!(text.starts_with("第一"), "{text}");
    assert_eq!(a.node().content[0].attr_str("id"), Some(para.as_str()));
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offline_edit_merges_on_reconnect() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let page = page_with(&s, &c, "离线", "第一段\n\n第二段\n").await;
    let doc = s.server_doc(&page).await;
    let (p1, p2) = (block_id(&doc, 0), block_id(&doc, 1));

    let mut a = YClient::new(2001);
    let mut b = YClient::new(2002);
    a.connect(&s.ws(&page), &[]).await;
    b.connect(&s.ws(&page), &[]).await;
    eventually("同步上", async || a.synced() && b.synced()).await;

    // a 断网后改第二段；同时 b 改第一段（在线）
    a.disconnect();
    a.type_at(&p2, 3, "（a 离线写的）");
    b.type_at(&p1, 3, "（b 在线写的）");
    eventually("服务器收到 b 的", async || {
        s.server_doc(&page)
            .await
            .plain_text()
            .contains("b 在线写的")
    })
    .await;
    assert!(!a.node().plain_text().contains("b 在线写的"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !s.server_doc(&page)
            .await
            .plain_text()
            .contains("a 离线写的")
    );

    // 重连：双方交换 step1 / step2，合并
    a.connect(&s.ws(&page), &[]).await;
    eventually("三方一致且都有两边的字", async || {
        let server = s.server_doc(&page).await;
        let t = server.plain_text();
        a.node() == server
            && b.node() == server
            && t.contains("a 离线写的")
            && t.contains("b 在线写的")
    })
    .await;
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_block_edit_while_typing() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let page = page_with(
        &s,
        &c,
        "Agent",
        "第一段原来的文字，这里是中间，这里是结尾。\n\n第二段\n",
    )
    .await;
    let para = block_id(&s.server_doc(&page).await, 0);

    let mut human = YClient::new(3001);
    human.connect(&s.ws(&page), &[]).await;
    eventually("同步上", async || human.synced()).await;

    // Agent 先读这一块（拿到版本），人接着在中间打字，打的字已经到了服务器
    let (st, blk) = json(c.get(s.api(&format!("/pages/{page}/blocks/{para}")))).await;
    assert_eq!(st, 200);
    assert_eq!(
        blk["markdown"],
        "第一段原来的文字，这里是中间，这里是结尾。"
    );
    let version = blk["version"].as_str().unwrap().to_string();
    human.type_at(&para, 14, "【人打的字】");
    eventually("服务器收到人打的字", async || {
        s.server_doc(&page)
            .await
            .plain_text()
            .contains("【人打的字】")
    })
    .await;

    // Agent 按读到的版本改开头、加结尾：人打的字不能丢
    let (st, r) = json(
        c.put(s.api(&format!("/pages/{page}/blocks/{para}")))
            .json(&json!({ "markdown": "Agent 改了开头，这里是中间，这里是结尾。（补一句）", "base": version })),
    )
    .await;
    assert_eq!(st, 200, "{r}");
    assert_eq!(r["ids"], json!([para]));
    eventually("客户端收到 Agent 的修改", async || {
        human.node().content[0]
            .plain_text()
            .starts_with("Agent 改了开头")
    })
    .await;
    let t = human.node().content[0].plain_text();
    assert!(
        t.contains("【人打的字】") && t.ends_with("（补一句）"),
        "{t}"
    );
    assert_eq!(human.node(), s.server_doc(&page).await);

    // 不给版本：按服务器当前内容改；人同时在打的字（已在服务器上的）没被 Agent 改到的部分保留
    human.type_at(&para, 0, "★");
    let (st, _) = json(c.put(s.api(&format!("/pages/{page}/blocks/{para}"))).json(
        &json!({ "markdown": "★Agent 改了开头，这里是中间【人打的字】，这里是结尾。（再补一句）" }),
    ))
    .await;
    assert_eq!(st, 200);
    eventually("一致", async || {
        let server = s.server_doc(&page).await;
        human.node() == server && server.content[0].plain_text().ends_with("（再补一句）")
    })
    .await;

    // 服务器替 Agent 放了在线状态，带着它在写的块
    let aw = human.state.lock().unwrap().awareness.clone();
    assert!(
        aw.iter()
            .any(|j| j.contains(&para) && j.contains("\"kind\":\"user\"")),
        "{aw:?}"
    );

    // 插入、删除也通过同步中心
    let (st, r) = json(
        c.post(s.api(&format!("/pages/{page}/blocks")))
            .json(&json!({ "markdown": "插在第一段后面", "after": para })),
    )
    .await;
    assert_eq!(st, 200);
    let new_id = r["ids"][0].as_str().unwrap().to_string();
    eventually("客户端收到插入", async || {
        human.node().content.get(1).map(|n| n.plain_text()) == Some("插在第一段后面".into())
    })
    .await;
    let r = c
        .delete(s.api(&format!("/pages/{page}/blocks/{new_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    eventually("客户端收到删除", async || {
        human.node().content.len() == 2
    })
    .await;
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn title_syncs_both_ways() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let page = page_with(&s, &c, "原标题", "正文\n").await;
    let mut a = YClient::new(4001);
    a.connect(&s.ws(&page), &[]).await;
    eventually("同步上", async || a.synced()).await;
    assert_eq!(a.doc.title(), "原标题");
    // 网页改 Y.Text title → pages.title
    a.doc.set_title("网页改的标题");
    eventually("pages.title 跟上", async || {
        let (_, p) = json(c.get(s.api(&format!("/pages/{page}")))).await;
        p["title"] == "网页改的标题"
    })
    .await;
    // HTTP 改标题 → 网页收到
    let (st, p) = json(
        c.patch(s.api(&format!("/pages/{page}")))
            .json(&json!({ "title": "接口改的标题" })),
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(p["title"], "接口改的标题");
    eventually("网页收到标题", async || {
        a.doc.title() == "接口改的标题"
    })
    .await;
    s.stop().await;
}
