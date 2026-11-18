//! 性能测量（默认不跑）：`cargo test --release -p oad-server --test bench -- --ignored --nocapture`
mod common;
use common::*;
use oad_core::{AuthConfig, HubConfig};
use std::time::{Duration, Instant};

fn md(n: usize) -> String {
    (0..n)
        .map(|i| format!("第 {i} 段，有 **加粗** 和 [链接](https://a.com/{i})，还有一些中文和 English words.\n\n"))
        .collect()
}

fn pct(v: &mut [Duration], p: f64) -> Duration {
    v.sort();
    v[((v.len() as f64 - 1.0) * p) as usize]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn bench() {
    let dir = tempfile::tempdir().unwrap();
    let c = http();
    let hub = HubConfig {
        flush_window: Duration::from_millis(20),
        ..HubConfig::default()
    };
    let mut pages = Vec::new();
    {
        let s = Server::start(dir.path(), AuthConfig::None, hub.clone()).await;
        for n in [1000, 10000] {
            pages.push((n, page_with(&s, &c, &format!("{n} 段"), &md(n)).await));
        }
        s.stop().await; // 关机时压缩成快照
    }
    let s = Server::start(dir.path(), AuthConfig::None, hub.clone()).await;
    for (n, page) in &pages {
        // 冷打开：从 SQLite 读快照、建 Yrs 文档、编码整份状态
        let t = Instant::now();
        let state = s.core.hub.read(page, |d| d.encode_state()).await.unwrap();
        let cold = t.elapsed();
        // 已加载时编码
        let t = Instant::now();
        let _ = s.core.hub.read(page, |d| d.encode_state()).await.unwrap();
        let warm = t.elapsed();
        eprintln!(
            "{n} 段：冷打开（读库 + 加载 + 编码）{cold:?}，已加载再编码 {warm:?}，状态 {} KB",
            state.len() / 1024
        );
    }
    s.stop().await;
    // 浏览器视角：页面没加载时 WebSocket 连上到同步完（先用一个小页面把连接池、线程热起来）
    let s = Server::start(dir.path(), AuthConfig::None, hub.clone()).await;
    let warm = page_with(&s, &c, "热身", "一段\n").await;
    let mut w = YClient::new(9999);
    w.connect(&s.ws(&warm), &[]).await;
    eventually("热身", async || w.synced()).await;
    for (n, page) in &pages {
        let mut y = YClient::new(10_000 + *n as u32);
        let t = Instant::now();
        y.connect(&s.ws(page), &[]).await;
        while !y.synced() {
            tokio::task::yield_now().await;
        }
        eprintln!("{n} 段：WebSocket 连上到同步完（冷）{:?}", t.elapsed());
    }

    // 每次按键：A 打一个字，到 B 收到并合并（经过服务器合并 + 广播）
    let page = &pages[0].1;
    let para = block_id(&s.server_doc(page).await, 500);
    let mut a = YClient::new(20001);
    let mut b = YClient::new(20002);
    a.connect(&s.ws(page), &[]).await;
    b.connect(&s.ws(page), &[]).await;
    eventually("同步上", async || a.synced() && b.synced()).await;
    let mut lat = Vec::new();
    for i in 0..300 {
        let before = b.state.lock().unwrap().applied;
        let t = Instant::now();
        a.type_at(&para, i, "x");
        while b.state.lock().unwrap().applied == before {
            tokio::task::yield_now().await;
        }
        lat.push(t.elapsed());
    }
    eprintln!(
        "1000 段的页面，按键 → 另一个客户端收到：p50 {:?}，p95 {:?}，max {:?}",
        pct(&mut lat, 0.5),
        pct(&mut lat, 0.95),
        pct(&mut lat, 1.0)
    );
    // 同时 Agent 每 20ms 改一段
    let s2 = &s;
    let c2 = &c;
    let agent_page = page.clone();
    let blocks: Vec<String> = (0..50)
        .map(|i| block_id(&pages_doc(s2, &agent_page), i * 10))
        .collect();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let st = stop.clone();
    let agent = async move {
        let mut i = 0;
        let mut lat = Vec::new();
        while !st.load(std::sync::atomic::Ordering::Relaxed) {
            let b = &blocks[i % blocks.len()];
            let t = Instant::now();
            let _ = c2
                .put(s2.api(&format!("/pages/{agent_page}/blocks/{b}")))
                .json(&serde_json::json!({ "markdown": format!("Agent 第 {i} 次改写") }))
                .send()
                .await;
            lat.push(t.elapsed());
            i += 1;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        lat
    };
    let typing = async {
        let mut lat = Vec::new();
        for i in 0..300 {
            let t = Instant::now();
            a.type_at(&para, 300 + i, "y");
            let want = "y".repeat(i as usize + 1);
            while !b.doc.block(&para).unwrap().plain_text().contains(&want) {
                tokio::task::yield_now().await;
            }
            lat.push(t.elapsed());
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        lat
    };
    let (mut edits, mut lat) = tokio::join!(agent, typing);
    eprintln!(
        "同时 Agent 每 20ms 改一段（{} 次，HTTP 往返 p50 {:?}，p95 {:?}）：按键 → 另一个客户端收到 p50 {:?}，p95 {:?}",
        edits.len(),
        pct(&mut edits, 0.5),
        pct(&mut edits, 0.95),
        pct(&mut lat, 0.5),
        pct(&mut lat, 0.95)
    );
    s.stop().await;
}

fn pages_doc(s: &Server, page: &str) -> oad_core::Node {
    let core = s.core.clone();
    let page = page.to_string();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(async move { core.hub.read(&page, |d| d.to_node()).await.unwrap() })
    })
}

/// 服务器端写入，经 HTTP（和网页 / Agent 一样）：`cargo test --release -p oad-server --test bench writes -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn writes() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, HubConfig::default()).await;
    let c = http();
    for n in [1000usize, 10000] {
        let (_, p) = json(
            c.post(s.api(&format!("/workspaces/{}/pages", s.ws_id())))
                .json(&serde_json::json!({ "title": format!("{n}") })),
        )
        .await;
        let page = p["id"].as_str().unwrap().to_string();
        let body = serde_json::json!({ "markdown": md(n) });
        let t = Instant::now();
        let r = c
            .put(s.api(&format!("/pages/{page}/content")))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 204);
        let put = t.elapsed();
        let doc = s.server_doc(&page).await;
        let (mut plain, mut based) = (Vec::new(), Vec::new());
        for i in 0..100 {
            let b = block_id(&doc, (i * 97) % n);
            let url = s.api(&format!("/pages/{page}/blocks/{b}"));
            let t = Instant::now();
            let (st, _) = json(
                c.put(&url)
                    .json(&serde_json::json!({ "markdown": format!("第 {i} 次改") })),
            )
            .await;
            assert_eq!(st, 200);
            plain.push(t.elapsed());
            let (_, g) = json(c.get(&url)).await;
            let base = g["version"].as_str().unwrap().to_string();
            let t = Instant::now();
            let (st, _) = json(c.put(&url).json(
                &serde_json::json!({ "markdown": format!("第 {i} 次带 base 改"), "base": base }),
            ))
            .await;
            assert_eq!(st, 200);
            based.push(t.elapsed());
        }
        let t = Instant::now();
        let (st, _) = json(c.get(s.api(&format!("/pages/{page}/content?format=json")))).await;
        assert_eq!(st, 200);
        let read = t.elapsed();
        eprintln!(
            "{n} 段（HTTP）：PUT content {put:?}；PUT block p50 {:?} p95 {:?}；带 base p50 {:?} p95 {:?}；GET content(json) {read:?}",
            pct(&mut plain, 0.5),
            pct(&mut plain, 0.95),
            pct(&mut based, 0.5),
            pct(&mut based, 0.95)
        );
    }
    s.stop().await;
}
