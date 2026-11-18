//! 服务器写入的性能（默认不跑）：`cargo test --release -p oad-core --test write_bench -- --ignored --nocapture`
use oad_core::storage::sqlite::Sqlite;
use oad_core::{AuthConfig, AuthRequest, Core, HubConfig, Principal};
use std::sync::Arc;
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

async fn who(core: &Core) -> Principal {
    let h = http::HeaderMap::new();
    core.authenticate(&AuthRequest {
        headers: &h,
        query_token: None,
    })
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn writes() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(Sqlite::open(dir.path().join("oad.db")).unwrap());
    let core = Core::open(storage, AuthConfig::None, HubConfig::default())
        .await
        .unwrap();
    let me = who(&core).await;
    let ws = core.default_workspace.id.clone();
    for n in [1000usize, 10000] {
        let page = core
            .create_page(&ws, &me, None, None, Some(format!("{n}")), None)
            .await
            .unwrap()
            .id;
        let doc = oad_markdown::parse(&md(n));
        let t = Instant::now();
        core.set_content(&page, &me, doc.clone(), None)
            .await
            .unwrap();
        let put = t.elapsed();
        // 整页再交一次（只有一处不同）
        let md2 = md(n).replacen("第 5 段", "第 5 段（改）", 1);
        let t = Instant::now();
        core.set_content(&page, &me, oad_markdown::parse(&md2), None)
            .await
            .unwrap();
        let reput = t.elapsed();
        let (node, _) = core.content(&page, &me).await.unwrap();
        let ids: Vec<String> = node
            .content
            .iter()
            .map(|b| b.attr_str("id").unwrap().to_string())
            .collect();
        let mut plain = Vec::new();
        let mut based = Vec::new();
        for i in 0..60 {
            let b = &ids[(i * 97) % n];
            let nodes = oad_markdown::parse(&format!("Agent 第 {i} 次改写的一段")).content;
            let t = Instant::now();
            core.replace_block(&page, b, &me, nodes.clone(), None)
                .await
                .unwrap();
            plain.push(t.elapsed());
            let base = core.version(&page, &me).await.unwrap();
            let t = Instant::now();
            core.replace_block(&page, b, &me, nodes, Some(base))
                .await
                .unwrap();
            based.push(t.elapsed());
        }
        // 旧版本：读了以后别处有人改过
        let base = core.version(&page, &me).await.unwrap();
        core.replace_block(
            &page,
            &ids[1],
            &me,
            oad_markdown::parse("别处的改动").content,
            None,
        )
        .await
        .unwrap();
        let t = Instant::now();
        core.replace_block(
            &page,
            &ids[2],
            &me,
            oad_markdown::parse("按旧版本改").content,
            Some(base),
        )
        .await
        .unwrap();
        let stale = t.elapsed();
        // 旧版本，而且要改的那块在那之后被别人改过：要从 base 复制一份
        let base = core.version(&page, &me).await.unwrap();
        let (ag, _, _) = core
            .create_agent(
                &ws,
                &me,
                &format!("别人{n}"),
                None,
                oad_core::Scope::Write,
                None,
            )
            .await
            .unwrap();
        let other = Principal {
            user_id: ag.id,
            name: ag.name,
            kind: oad_core::PrincipalKind::Agent,
        };
        core.replace_block(
            &page,
            &ids[3],
            &other,
            oad_markdown::parse("别人先改了这块").content,
            None,
        )
        .await
        .unwrap();
        let t = Instant::now();
        core.replace_block(
            &page,
            &ids[3],
            &me,
            oad_markdown::parse("按旧版本改同一块").content,
            Some(base),
        )
        .await
        .unwrap();
        let conflict = t.elapsed();
        // 读整页：第一次（缓存是空的）和之后
        let t = Instant::now();
        core.content(&page, &me).await.unwrap();
        let read_cold = t.elapsed();
        let t = Instant::now();
        core.content(&page, &me).await.unwrap();
        let read_warm = t.elapsed();
        let t = Instant::now();
        core.version(&page, &me).await.unwrap();
        let ver = t.elapsed();
        let t = Instant::now();
        core.hub.read(&page, |d| d.to_node()).await.unwrap();
        let uncached = t.elapsed();
        eprintln!("  版本 {ver:?}，不用缓存读 {uncached:?}");
        eprintln!(
            "{n} 段：整页写入 {put:?}，整页再写（改一处）{reput:?}；按块改 p50 {:?} p95 {:?}；带 base p50 {:?} p95 {:?}；\
旧 base（那块没变）{stale:?}，旧 base（那块被别人改过）{conflict:?}；读整页 首次 {read_cold:?} 之后 {read_warm:?}",
            pct(&mut plain, 0.5),
            pct(&mut plain, 0.95),
            pct(&mut based, 0.5),
            pct(&mut based, 0.95)
        );
    }
    core.shutdown().await;
}
