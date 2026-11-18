//! 性能：1 万行 × 20 列，筛选 + 排序 + 分组（+ 汇总行）。目标：release 下 < 20ms。
//! `cargo test -p oad-db --release --test bench -- --ignored --nocapture`
mod common;
use common::*;
use oad_db::PropType::*;
use oad_db::{Change, Engine, RowData};
use serde_json::{Value, json};
use std::time::Instant;

pub fn big_schema() -> oad_db::Schema {
    let mut money = p("price", "单价", Number);
    money.format = Some("cny".into());
    schema(vec![
        p("title", "名称", Title),
        status("st", "状态"),
        with_options(p("pri", "优先级", Select), &["高", "中", "低"]),
        with_options(
            p("tags", "标签", MultiSelect),
            &["前端", "后端", "设计", "测试", "运维", "文档"],
        ),
        p("who", "负责人", Person),
        p("due", "截止", Date),
        p("start", "开始", Date),
        money,
        p("qty", "数量", Number),
        p("score", "评分", Number),
        p("hours", "工时", Number),
        p("done", "完成", Checkbox),
        p("flag", "置顶", Checkbox),
        p("note", "备注", Text),
        p("url", "网址", Url),
        p("ct", "创建时间", CreatedTime),
        formula("total", "金额", "prop(\"单价\") * prop(\"数量\")"),
        formula(
            "left",
            "剩几天",
            "dateBetween(prop(\"截止\"), today(), \"days\")",
        ),
        formula(
            "label",
            "标记",
            "if(prop(\"完成\"), \"✓\", prop(\"优先级\") + \"·\" + toText(prop(\"评分\")))",
        ),
        with_options(p("area", "区域", Select), &["华北", "华东", "华南", "西南"]),
    ])
}

pub fn big_rows(n: usize) -> Vec<RowData> {
    let mut s: u64 = 42;
    let mut r = move |m: u64| {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (s >> 33) % m
    };
    (0..n)
        .map(|i| {
            let mut tags = vec![];
            for k in 1..=6 {
                if r(4) == 0 {
                    tags.push(format!("o{k}"));
                }
            }
            let mut o = json!({
                "title": format!("任务 {i}"),
                "st": format!("s{}", r(3) + 1),
                "pri": format!("o{}", r(3) + 1),
                "who": [format!("user:{}", r(2) + 1)],
                "due": {"start": format!("2026-{:02}-{:02}", r(12) + 1, r(28) + 1)},
                "start": {"start": format!("2026-{:02}-{:02}", r(12) + 1, r(28) + 1)},
                "price": r(10000) as f64 / 100.0,
                "qty": r(50),
                "score": r(100),
                "hours": r(40),
                "note": format!("备注 {} 关键词{}", r(1000), r(20)),
                "url": format!("https://example.com/{}", r(100)),
                "area": format!("o{}", r(4) + 1),
            });
            if !tags.is_empty() {
                o["tags"] = json!(tags);
            }
            if r(2) == 0 {
                o["done"] = json!(true);
            }
            if r(10) == 0 {
                o["flag"] = json!(true);
            }
            let mut row = row(&format!("r{i}"), o);
            row.created_at = Some((NOW - r(1000) as i64 * 3_600_000) as f64);
            row
        })
        .collect()
}

fn time<T>(label: &str, runs: usize, mut f: impl FnMut() -> T) -> T {
    let mut out = None;
    let mut ts = vec![];
    for _ in 0..runs {
        let t = Instant::now();
        out = Some(f());
        ts.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    eprintln!(
        "{label:<40} 中位 {:>7.2}ms  最慢 {:>7.2}ms",
        ts[ts.len() / 2],
        ts[ts.len() - 1]
    );
    out.unwrap()
}

#[test]
#[ignore]
fn bench_10k() {
    let rows = big_rows(10_000);
    let json_rows = serde_json::to_string(&rows).unwrap();
    eprintln!("1 万行 JSON {:.1} MB", json_rows.len() as f64 / 1e6);
    let mut e = Engine::new(big_schema());
    e.set_clock(clock());
    e.set_people(people());
    time("加载（解析 JSON + 建格子）", 5, || {
        let rows: Vec<RowData> = serde_json::from_str(&json_rows).unwrap();
        e.load_rows(rows);
    });
    let v = view(json!({
        "filter": {"op": "and", "items": [
            {"prop": "st", "cond": "neq", "value": "已完成"},
            {"op": "or", "items": [{"prop": "score", "cond": "gt", "value": 30}, {"prop": "tags", "cond": "contains", "value": "前端"}]},
            {"prop": "due", "cond": "is_within", "value": "this_year"}
        ]},
        "sorts": [{"prop": "pri"}, {"prop": "due", "dir": "desc"}],
        "group_by": "st",
    }));
    let r = time("筛选 + 排序 + 分组", 20, || e.query(&v).unwrap());
    eprintln!(
        "  → {} 行，{} 组",
        r.total,
        r.groups.as_ref().unwrap().len()
    );
    let mut v2 = v.clone();
    v2.totals = json!({"price": "sum", "score": "avg", "tags": "count_values", "done": "percent_checked", "due": "earliest"}).as_object().unwrap().clone();
    v2.sub_group_by = Some("pri".into());
    time("…再加子分组 + 汇总行", 20, || {
        e.query(&v2).unwrap()
    });
    let vf = view(
        json!({"filter": [{"prop": "total", "cond": "gt", "value": 1000}, {"prop": "label", "cond": "contains", "value": "高"}], "sorts": [{"prop": "left"}]}),
    );
    let mut cold = Engine::new(big_schema());
    cold.set_clock(clock());
    cold.load_rows(rows.clone());
    time(
        "按公式筛选排序（第一次，要算公式）",
        1,
        || cold.query(&vf).unwrap(),
    );
    time("按公式筛选排序（有缓存）", 20, || {
        cold.query(&vf).unwrap()
    });
    time("搜索", 20, || {
        e.query(&view(
            json!({"search": "关键词7", "sorts": [{"prop": "title"}]}),
        ))
        .unwrap()
    });
    time("按名称排序（文字）", 20, || {
        e.query(&view(json!({"sorts": [{"prop": "title", "dir": "desc"}]})))
            .unwrap()
    });
    let mut k = 0;
    time("改一格 + 重新查", 20, || {
        k += 1;
        let c: Vec<Change> = serde_json::from_value(
            json!([{"row_id": format!("r{k}"), "props": {"score": k % 100, "qty": k % 7}}]),
        )
        .unwrap();
        e.apply(c);
        e.query(&vf).unwrap()
    });
    time("computed（一行）", 20, || e.computed("r5000"));
    let r = time("查询结果转 JSON", 20, || {
        serde_json::to_string(&e.query(&v).unwrap()).unwrap()
    });
    eprintln!("  → {:.0} KB", r.len() as f64 / 1e3);
    let _: Value = json!(null);
}
