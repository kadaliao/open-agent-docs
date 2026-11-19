//! 10 万段的搜索延迟（默认不跑）：`cargo test --release -p oad-search --test bench -- --ignored --nocapture`
use oad_search::{Doc, Index, Kind, Op, SearchQuery};
use std::time::{Duration, Instant};

const WORDS: &[&str] = &[
    "季度",
    "计划",
    "搜索",
    "设计",
    "数据库",
    "表格",
    "协作",
    "文档",
    "评论",
    "通知",
    "版本",
    "历史",
    "分享",
    "服务器",
    "浏览器",
    "性能",
    "测试",
    "发布",
    "会议",
    "客户",
    "需求",
    "预算",
    "进度",
    "风险",
    "团队",
    "search",
    "engine",
    "running",
    "release",
    "meeting",
    "budget",
    "design",
    "review",
    "agent",
    "index",
];

fn para(i: usize) -> String {
    let mut s = String::new();
    let mut x = i.wrapping_mul(2654435761);
    for _ in 0..30 {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        s.push_str(WORDS[(x >> 33) % WORDS.len()]);
        s.push(if x.is_multiple_of(7) { '，' } else { ' ' });
    }
    s
}

fn pct(v: &mut [Duration], p: f64) -> Duration {
    v.sort();
    v[((v.len() as f64 - 1.0) * p) as usize]
}

#[test]
#[ignore]
fn search_100k() {
    let dir = tempfile::tempdir().unwrap();
    let ix = Index::open(Some(dir.path())).unwrap();
    let t = Instant::now();
    for chunk in 0..10 {
        let ops = (0..10_000)
            .map(|j| {
                let i = chunk * 10_000 + j;
                Op::Put(Doc {
                    page_id: format!("p{}", i / 100),
                    block_id: Some(format!("b{i}")),
                    db_id: None,
                    workspace_id: if i % 10 == 0 {
                        "other".into()
                    } else {
                        "w".into()
                    },
                    kind: Kind::Page,
                    title: String::new(),
                    text: para(i),
                })
            })
            .collect();
        ix.apply(ops, |_| {}).unwrap();
    }
    println!("索引 10 万段：{:?}，{} 条", t.elapsed(), ix.num_docs());
    for text in [
        "季度 计划",
        "search",
        "数据库 性能 测试",
        "sear",
        "预算",
        "不存在的词",
    ] {
        let mut times = vec![];
        let mut n = 0;
        for _ in 0..50 {
            let t = Instant::now();
            let hits = ix
                .search(&SearchQuery {
                    workspace_id: "w",
                    text,
                    kind: None,
                    limit: 20,
                    per_page: true,
                })
                .unwrap();
            times.push(t.elapsed());
            n = hits.len();
        }
        println!(
            "搜「{text}」：{n} 条，p50 {:?}，p95 {:?}",
            pct(&mut times, 0.5),
            pct(&mut times, 0.95)
        );
    }
    // 改一段（增量）
    let mut times = vec![];
    for i in 0..20 {
        let t = Instant::now();
        ix.apply(
            vec![Op::Put(Doc {
                page_id: "p1".into(),
                block_id: Some(format!("b{}", 100 + i)),
                db_id: None,
                workspace_id: "w".into(),
                kind: Kind::Page,
                title: String::new(),
                text: format!("改过的段落 {i}"),
            })],
            |_| {},
        )
        .unwrap();
        times.push(t.elapsed());
    }
    println!(
        "改一段（提交 + 能搜到）：p50 {:?}，p95 {:?}",
        pct(&mut times, 0.5),
        pct(&mut times, 0.95)
    );
}
