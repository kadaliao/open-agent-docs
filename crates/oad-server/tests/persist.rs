//! 重启后内容和标题都在；增量够多时压缩成快照；空闲卸载时也压缩。
mod common;
use common::*;
use oad_core::{AuthConfig, HubConfig};
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restart_keeps_content_and_title() {
    let dir = tempfile::tempdir().unwrap();
    let c = http();
    let (page, before) = {
        let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
        let page = page_with(&s, &c, "会留下来的标题", "# 标题\n\n第一段\n\n- 一\n- 二\n").await;
        let para = block_id(&s.server_doc(&page).await, 1);
        let mut a = YClient::new(5001);
        a.connect(&s.ws(&page), &[]).await;
        eventually("同步上", async || a.synced()).await;
        // 先改标题再打字：同一条连接上的更新按顺序到，等到正文时标题一定也到了（反过来在慢机器上会只等到正文就关机）
        a.doc.set_title("网页改的标题");
        a.type_at(&para, 3, "（网页打的）");
        eventually("服务器收到", async || {
            s.server_doc(&page).await.plain_text().contains("网页打的")
        })
        .await;
        let before = s.server_doc(&page).await;
        // 没等攒够时间就关：关机时会存
        s.stop().await;
        (page, before)
    };
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    assert_eq!(s.server_doc(&page).await, before);
    let (_, p) = json(c.get(s.api(&format!("/pages/{page}")))).await;
    assert_eq!(p["title"], "网页改的标题");
    let (_, md) = json(c.get(s.api(&format!("/pages/{page}/content")))).await;
    assert!(
        md["markdown"]
            .as_str()
            .unwrap()
            .contains("第一段（网页打的）")
    );
    // 新连上的客户端拿到同样的内容
    let mut b = YClient::new(5002);
    b.connect(&s.ws(&page), &[]).await;
    eventually("同步上", async || b.synced()).await;
    assert_eq!(b.node(), before);
    assert_eq!(b.doc.title(), "网页改的标题");
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compaction() {
    let dir = tempfile::tempdir().unwrap();
    let hub = HubConfig {
        flush_window: Duration::from_millis(1),
        compact_after: 5,
        idle_unload: Duration::from_millis(300),
        ..HubConfig::default()
    };
    let c = http();
    let s = Server::start(dir.path(), AuthConfig::None, hub.clone()).await;
    let page = page_with(&s, &c, "压缩", "第一段\n").await;
    let para = block_id(&s.server_doc(&page).await, 0);
    for i in 0..23 {
        let (st, _) = json(
            c.put(s.api(&format!("/pages/{page}/blocks/{para}")))
                .json(&json!({ "markdown": format!("第 {i} 次改") })),
        )
        .await;
        assert_eq!(st, 200);
        tokio::time::sleep(Duration::from_millis(5)).await;
        s.core.hub.flush(&page).await.unwrap();
    }
    let log = s.core.storage.load_doc(&page).await.unwrap();
    assert!(log.snapshot.is_some());
    assert!(
        log.updates.len() < 5,
        "压缩后增量应该少于 5 条，现在 {}",
        log.updates.len()
    );
    assert!(!log.updates.is_empty(), "23 次改，最后几条还没压缩");
    // 空闲卸载时把剩下的也压缩掉
    eventually("卸载", async || s.core.hub.loaded() == 0).await;
    let log = s.core.storage.load_doc(&page).await.unwrap();
    assert_eq!(log.updates.len(), 0);
    // 只靠快照加载
    assert_eq!(s.server_doc(&page).await.plain_text(), "第 22 次改");
    s.stop().await;
    let s = Server::start(dir.path(), AuthConfig::None, hub).await;
    assert_eq!(s.server_doc(&page).await.plain_text(), "第 22 次改");
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unload_while_connected_is_safe() {
    // 有连接时不卸载；连接断了才卸载，卸载后再连能接着改
    let dir = tempfile::tempdir().unwrap();
    let hub = HubConfig {
        flush_window: Duration::from_millis(1),
        idle_unload: Duration::from_millis(100),
        ..HubConfig::default()
    };
    let c = http();
    let s = Server::start(dir.path(), AuthConfig::None, hub).await;
    let page = page_with(&s, &c, "卸载", "第一段\n").await;
    let para = block_id(&s.server_doc(&page).await, 0);
    let mut a = YClient::new(6001);
    a.connect(&s.ws(&page), &[]).await;
    eventually("同步上", async || a.synced()).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(s.core.hub.loaded(), 1);
    a.type_at(&para, 0, "A");
    a.disconnect();
    eventually("卸载", async || s.core.hub.loaded() == 0).await;
    a.connect(&s.ws(&page), &[]).await;
    eventually("重新同步", async || a.synced()).await;
    a.type_at(&para, 0, "B");
    eventually("服务器收到", async || {
        s.server_doc(&page).await.plain_text() == "BA第一段"
    })
    .await;
    s.stop().await;
}
