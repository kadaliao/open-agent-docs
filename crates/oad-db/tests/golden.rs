//! 浏览器和服务器算出来一样：同一组库、行、视图、公式在 Rust 里（经 JSON 接口）跑一遍，结果写进
//! `packages/db-wasm/test/golden.json`；`packages/db-wasm/test/golden.test.mjs` 用 WASM 版跑同样的步骤比对。
//! 编好了 WASM（`sh packages/db-wasm/build.sh`）、有 node 时这里顺便跑 node 那边；没有就只生成文件。
mod common;
use common::*;
use oad_db::PropType::*;
use oad_db::api::JsonEngine;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;

/// 一个用例：一个库（加上关联到的库），一串步骤。
struct Case {
    name: &'static str,
    schema: Value,
    opts: Value,
    rows: Value,
    related: Vec<(String, Value, Value)>,
    steps: Vec<Value>,
}

fn run(c: &Case) -> Value {
    let mut e = JsonEngine::new(&c.schema.to_string(), &c.opts.to_string()).unwrap();
    for (id, s, r) in &c.related {
        e.load_related(id, &s.to_string(), &r.to_string()).unwrap();
    }
    e.load_rows(&c.rows.to_string()).unwrap();
    let mut steps = vec![];
    for st in &c.steps {
        let s = |k: &str| st[k].as_str().unwrap_or("").to_string();
        let want: Value = match st["op"].as_str().unwrap() {
            "query" => match e.query(&st["view"].to_string()) {
                Ok(x) => serde_json::from_str(&x).unwrap(),
                Err(m) => json!({ "error": m }),
            },
            "computed" => serde_json::from_str(&e.computed(&s("row"))).unwrap(),
            "displayRow" => serde_json::from_str(&e.display_row(&s("row"))).unwrap(),
            "display" => json!(e.display(&s("key"), &st["value"].to_string()).unwrap()),
            "normalize" => match e.normalize(&s("key"), &st["input"].to_string()) {
                Ok(x) => json!({ "value": serde_json::from_str::<Value>(&x).unwrap() }),
                Err(m) => json!({ "error": m }),
            },
            "checkFormula" => {
                serde_json::from_str(&e.check_formula(&s("src"), st["key"].as_str())).unwrap()
            }
            "apply" => serde_json::from_str(&e.apply(&st["changes"].to_string()).unwrap()).unwrap(),
            "setSchema" => {
                e.set_schema(&st["schema"].to_string()).unwrap();
                Value::Null
            }
            "schemaErrors" => serde_json::from_str(&e.schema_errors()).unwrap(),
            op => panic!("{op}"),
        };
        let mut st = st.clone();
        st["want"] = want;
        steps.push(st);
    }
    let related: Vec<Value> = c
        .related
        .iter()
        .map(|(id, s, r)| json!({"id": id, "schema": s, "rows": r}))
        .collect();
    json!({"name": c.name, "schema": c.schema, "opts": c.opts, "rows": c.rows, "related": related, "steps": steps})
}

fn q(v: Value) -> Value {
    json!({"op": "query", "view": v})
}

fn tasks_case() -> Case {
    let mut uid = p("uid", "编号", UniqueId);
    uid.prefix = Some("T-".into());
    let mut money = p("price", "单价", Number);
    money.format = Some("cny".into());
    let s = schema(vec![
        p("title", "名称", Title),
        status("st", "状态"),
        money,
        p("qty", "数量", Number),
        with_options(p("tags", "标签", MultiSelect), &["a", "b", "c"]),
        p("due", "截止", Date),
        p("done", "完成", Checkbox),
        p("note", "备注", Text),
        p("who", "人", Person),
        p("files", "附件", Files),
        uid,
        p("ct", "创建时间", CreatedTime),
        p("cb", "创建人", CreatedBy),
        formula("total", "金额", "prop(\"单价\") * prop(\"数量\")"),
        formula(
            "left",
            "剩几天",
            "dateBetween(prop(\"截止\"), today(), \"days\")",
        ),
        formula(
            "label",
            "标记",
            "if(prop(\"完成\"), \"✓\", \"还差 \" + toText(10 - prop(\"数量\")))",
        ),
        formula("tagn", "标签数", "prop(\"标签\").length()"),
        formula("when", "后天", "dateAdd(today(), 2, \"days\")"),
        formula("list", "列表", "prop(\"标签\").map(upper(current))"),
        formula("bad", "坏的", "prop(\"没有\") + 1"),
        with_options(p("lv", "级别", Select), &["高", "中", "低"]),
    ]);
    let mut rows = vec![
        row(
            "r1",
            json!({"title": "甲", "st": "s2", "price": 2.5, "qty": 4, "tags": ["o1", "o2"], "due": {"start": "2026-11-20"}, "done": true,
                         "note": "Hello World", "who": ["user:1", "agent:7"], "files": [{"name": "报告.pdf", "url": "/f/1"}], "lv": "o1"}),
        ),
        row(
            "r2",
            json!({"title": "乙", "st": "s1", "price": 10, "qty": 1, "tags": ["o2"], "due": {"start": "2026-11-16T09:30", "time": true}, "who": ["user:2"], "lv": "o3"}),
        ),
        row(
            "r3",
            json!({"title": "丙", "st": "s3", "price": 0.1, "qty": 3, "due": {"start": "2026-10-01", "end": "2026-12-01"}, "lv": "o2"}),
        ),
        row("r4", json!({"title": "丁"})),
        row(
            "r5",
            json!({"title": "任务10", "qty": 12, "tags": ["o3"], "note": "中文 备注", "st": "s2"}),
        ),
        row(
            "r6",
            json!({"title": "任务2", "price": 1234.5, "qty": 2, "done": true}),
        ),
    ];
    for (i, r) in rows.iter_mut().enumerate() {
        r.unique_id = Some(i as i64 + 1);
        r.created_at = Some((NOW - i as i64 * 86_400_000 * 6) as f64);
    }
    let views = vec![
        json!({}),
        json!({"sorts": [{"prop": "title"}]}),
        json!({"sorts": [{"prop": "price", "dir": "desc"}, {"prop": "title"}]}),
        json!({"sorts": [{"prop": "st"}, {"prop": "due", "dir": "desc"}]}),
        json!({"sorts": [{"prop": "total"}]}),
        json!({"sorts": [{"prop": "label", "dir": "desc"}]}),
        json!({"sorts": [{"prop": "who"}, {"prop": "ct"}]}),
        json!({"filter": {"op": "and", "items": [{"prop": "st", "cond": "neq", "value": "已完成"}, {"op": "or", "items": [{"prop": "qty", "cond": "gt", "value": 3}, {"prop": "tags", "cond": "contains", "value": "b"}]}]}}),
        json!({"filter": [{"prop": "due", "cond": "is_within", "value": "next_week"}]}),
        json!({"filter": [{"prop": "due", "cond": "lt", "value": "today"}]}),
        json!({"filter": [{"prop": "ct", "cond": "is_within", "value": "past_week"}]}),
        json!({"filter": [{"prop": "total", "cond": "gte", "value": 10}]}),
        json!({"filter": [{"prop": "label", "cond": "contains", "value": "还差"}]}),
        json!({"filter": [{"prop": "note", "cond": "contains", "value": "world"}]}),
        json!({"filter": [{"prop": "人", "cond": "contains", "value": "张三"}]}),
        json!({"filter": [{"prop": "编号", "cond": "gte", "value": "T-3"}]}),
        json!({"filter": [{"prop": "附件", "cond": "not_empty"}]}),
        json!({"filter": [{"prop": "完成", "cond": "checked"}]}),
        json!({"filter": [{"prop": "状态", "cond": "in_group", "value": ["todo", "doing"]}]}),
        json!({"filter": [{"prop": "状态", "cond": "eq", "value": "搁置"}]}),
        json!({"filter": [{"prop": "nope", "cond": "eq", "value": 1}]}),
        json!({"group_by": "st", "totals": {"price": "sum", "qty": "avg", "done": "percent_checked", "due": "earliest", "tags": "count_values"}}),
        json!({"group_by": "tags", "sub_group_by": "done"}),
        json!({"group_by": "due", "group_date": "week", "hide_empty_groups": true}),
        json!({"group_by": "due", "group_date": "relative"}),
        json!({"group_by": "who"}),
        json!({"group_by": "label"}),
        json!({"group_by": "qty", "sorts": [{"prop": "title"}]}),
        json!({"search": "任务", "sorts": [{"prop": "title"}]}),
        json!({"sorts": [{"prop": "title"}], "offset": 2, "limit": 3}),
        json!({"totals": {"price": "median", "qty": "range", "st": "count_unique", "lv": "empty", "title": "count", "total": "max", "due": "latest"}}),
    ];
    let mut steps: Vec<Value> = views.into_iter().map(q).collect();
    for r in ["r1", "r2", "r3", "r4", "r6"] {
        steps.push(json!({"op": "computed", "row": r}));
        steps.push(json!({"op": "displayRow", "row": r}));
    }
    for (k, v) in [
        ("price", json!(1200)),
        ("price", json!(-2.5)),
        ("due", json!({"start": "2026-11-15", "end": "2026-11-20"})),
        ("due", json!({"start": "2026-11-15T09:05", "time": true})),
        ("who", json!(["user:1", "user:9"])),
        ("tags", json!(["o2", "o1"])),
        ("st", json!("s3")),
        ("done", json!(true)),
        ("uid", json!(7)),
        ("ct", json!(NOW)),
        ("total", json!(10.5)),
        ("label", json!("✓")),
        ("when", json!({"start": "2026-11-17"})),
        ("files", json!([{"name": "a.png", "url": "/x"}])),
        ("note", json!("文字")),
    ] {
        steps.push(json!({"op": "display", "key": k, "value": v}));
    }
    for (k, v) in [
        ("price", json!("¥1,200")),
        ("price", json!("很多")),
        ("price", json!("50%")),
        ("tags", json!("a、b")),
        ("tags", json!(["c", "zzz"])),
        ("st", json!("已完成")),
        ("st", json!("搁置")),
        ("who", json!("张三, Writer")),
        ("who", json!("谁")),
        ("due", json!("2026/11/5")),
        ("due", json!("明天")),
        ("due", json!("2026年11月15日 14:30")),
        ("due", json!("2026-11-15 → 2026-11-20")),
        ("due", json!("下周")),
        ("done", json!("是")),
        ("done", json!("否")),
        ("note", json!("  文字  ")),
        ("title", json!(" 两个   空格 ")),
        ("files", json!("https://a.com/x.pdf")),
        ("total", json!(1)),
        ("lv", json!("低")),
        ("lv", json!(null)),
    ] {
        steps.push(json!({"op": "normalize", "key": k, "input": v}));
    }
    for (src, key) in [
        ("prop(\"单价\") * 2", None),
        ("1 +", None),
        ("prop(\"没有\")", None),
        ("lenght(1)", None),
        ("\"😀\" + foo(1)", None),
        ("let(x, 1, x + y)", None),
        ("prop(\"金额\") + 1", Some("total")),
        ("prop(\"金额\") + 1", Some("left")),
        ("[1, 2].map(current * index)", None),
    ] {
        steps.push(json!({"op": "checkFormula", "src": src, "key": key}));
    }
    steps.push(json!({"op": "schemaErrors"}));
    steps.push(json!({"op": "apply", "changes": [{"row_id": "r1", "props": {"qty": 9, "done": null}}, {"row_id": "r7", "props": {"title": "新", "qty": 1}}, {"row_id": "r4", "deleted": true}]}));
    steps.push(q(
        json!({"sorts": [{"prop": "qty"}], "totals": {"qty": "sum"}}),
    ));
    steps.push(json!({"op": "computed", "row": "r1"}));
    let mut s2 = s.clone();
    s2.props.retain(|p| p.key != "bad");
    s2.props[13].formula = Some("prop(\"单价\") * prop(\"数量\") * 2".into());
    steps.push(json!({"op": "setSchema", "schema": s2}));
    steps.push(json!({"op": "computed", "row": "r1"}));
    steps.push(json!({"op": "schemaErrors"}));
    Case {
        name: "任务库",
        schema: serde_json::to_value(&s).unwrap(),
        opts: json!({"id": "tasks", "now": NOW, "tz": 480, "people": people()}),
        rows: serde_json::to_value(&rows).unwrap(),
        related: vec![],
        steps,
    }
}

fn relations_case() -> Case {
    let proj = schema(vec![
        p("title", "名称", Title),
        p("budget", "预算", Number),
        mirror("tasks", "任务", "task_db", "proj"),
        rollup("hours", "总工时", "tasks", "h", "sum"),
        rollup("rate", "完成率", "tasks", "done", "percent_checked"),
        rollup("names", "任务名", "tasks", "title", "show_original"),
        rollup("first", "最早截止", "tasks", "due", "earliest"),
        rollup("dbl", "双倍工时", "tasks", "h2", "average"),
        formula(
            "left",
            "剩余预算",
            "prop(\"预算\") - prop(\"总工时\") * 100",
        ),
        relation("parent", "上级项目", "proj_db"),
        mirror("kids", "子项目", "proj_db", "parent"),
        rollup("kb", "子项目预算", "kids", "budget", "sum"),
    ]);
    let task = schema(vec![
        p("title", "名称", Title),
        p("h", "工时", Number),
        p("done", "完成", Checkbox),
        p("due", "截止", Date),
        relation("proj", "项目", "proj_db"),
        formula("h2", "双倍", "prop(\"工时\") * 2"),
    ]);
    let task_rows = json!([
        {"id": "t1", "props": {"title": "设计", "h": 5, "done": true, "due": {"start": "2026-11-20"}, "proj": ["p1"]}},
        {"id": "t2", "props": {"title": "开发", "h": 8, "due": {"start": "2026-11-18"}, "proj": ["p1", "p2"]}},
        {"id": "t3", "props": {"title": "孤儿", "h": 1}},
    ]);
    let rows = json!([
        {"id": "p1", "props": {"title": "官网改版", "budget": 2000}},
        {"id": "p2", "props": {"title": "发布会", "budget": 500, "parent": ["p1"]}},
        {"id": "p3", "props": {"title": "没任务", "parent": ["p1"], "budget": 100}},
    ]);
    let mut steps = vec![];
    for r in ["p1", "p2", "p3"] {
        steps.push(json!({"op": "computed", "row": r}));
        steps.push(json!({"op": "displayRow", "row": r}));
    }
    for v in [
        json!({"filter": [{"prop": "任务", "cond": "contains", "value": "开"}]}),
        json!({"filter": [{"prop": "总工时", "cond": "gt", "value": 10}]}),
        json!({"filter": [{"prop": "子项目", "cond": "not_empty"}]}),
        json!({"sorts": [{"prop": "任务"}]}),
        json!({"sorts": [{"prop": "rate", "dir": "desc"}]}),
        json!({"group_by": "tasks", "totals": {"hours": "sum", "budget": "sum"}}),
        json!({"group_by": "parent"}),
    ] {
        steps.push(q(v));
    }
    steps.push(json!({"op": "display", "key": "任务", "value": ["t2", "t1", "gone"]}));
    steps.push(json!({"op": "display", "key": "完成率", "value": 0.5}));
    steps.push(json!({"op": "normalize", "key": "上级项目", "input": "发布会"}));
    steps.push(json!({"op": "normalize", "key": "上级项目", "input": ["#p3", "官网改版"]}));
    steps.push(json!({"op": "normalize", "key": "上级项目", "input": "没有这个"}));
    steps.push(json!({"op": "normalize", "key": "任务", "input": "t1"}));
    steps.push(json!({"op": "apply", "changes": [{"row_id": "p3", "props": {"parent": ["p2"], "budget": 300}}]}));
    steps.push(json!({"op": "computed", "row": "p1"}));
    steps.push(json!({"op": "computed", "row": "p2"}));
    Case {
        name: "关联和汇总",
        schema: serde_json::to_value(&proj).unwrap(),
        opts: json!({"id": "proj_db", "now": NOW, "tz": 480}),
        rows,
        related: vec![(
            "task_db".into(),
            serde_json::to_value(&task).unwrap(),
            task_rows,
        )],
        steps,
    }
}

/// 随机的行和视图：各种条件、排序、分组随便组合。
fn random_case() -> Case {
    let mut seed: u64 = 2026;
    let mut r = move |m: u64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % m
    };
    let s = schema(vec![
        p("title", "名称", Title),
        p("n", "分", Number),
        with_options(p("sel", "级", Select), &["高", "中", "低"]),
        with_options(p("tags", "标签", MultiSelect), &["a", "b", "c", "d"]),
        status("st", "状态"),
        p("d", "日", Date),
        p("ok", "好", Checkbox),
        p("who", "人", Person),
        formula("f", "式", "prop(\"分\") % 7"),
        formula("g", "文", "prop(\"级\") + toText(prop(\"分\"))"),
    ]);
    let rows: Vec<Value> = (0..150)
        .map(|i| {
            let mut o = serde_json::Map::new();
            if r(6) > 0 {
                o.insert("title".into(), json!(format!("行{}", r(40))));
            }
            if r(4) > 0 {
                o.insert("n".into(), json!(r(200) as f64 / 4.0 - 10.0));
            }
            if r(3) > 0 {
                o.insert("sel".into(), json!(format!("o{}", r(3) + 1)));
            }
            let tags: Vec<String> = (1..=4).filter(|_| r(3) == 0).map(|k| format!("o{k}")).collect();
            if !tags.is_empty() {
                o.insert("tags".into(), json!(tags));
            }
            if r(4) > 0 {
                o.insert("st".into(), json!(format!("s{}", r(3) + 1)));
            }
            if r(3) > 0 {
                let mo = r(4) + 10;
                o.insert("d".into(), json!({"start": format!("2026-{mo:02}-{:02}", r(28) + 1)}));
            }
            if r(2) == 0 {
                o.insert("ok".into(), json!(true));
            }
            if r(2) == 0 {
                o.insert("who".into(), json!([format!("user:{}", r(2) + 1)]));
            }
            json!({"id": format!("x{i}"), "props": o, "created_at": NOW - r(60) as i64 * 86_400_000})
        })
        .collect();
    let conds = [
        json!({"prop": "n", "cond": "gt", "value": 20}),
        json!({"prop": "n", "cond": "lte", "value": 5.5}),
        json!({"prop": "n", "cond": "empty"}),
        json!({"prop": "sel", "cond": "eq", "value": "高"}),
        json!({"prop": "sel", "cond": "neq", "value": ["中", "低"]}),
        json!({"prop": "tags", "cond": "contains", "value": "a, b"}),
        json!({"prop": "tags", "cond": "not_contains", "value": "c"}),
        json!({"prop": "st", "cond": "in_group", "value": "done"}),
        json!({"prop": "d", "cond": "is_within", "value": "past_month"}),
        json!({"prop": "d", "cond": "is_within", "value": "next_30_days"}),
        json!({"prop": "d", "cond": "gte", "value": "2026-12-01"}),
        json!({"prop": "ok", "cond": "unchecked"}),
        json!({"prop": "who", "cond": "contains", "value": "李四"}),
        json!({"prop": "title", "cond": "contains", "value": "1"}),
        json!({"prop": "f", "cond": "eq", "value": 3}),
        json!({"prop": "g", "cond": "starts_with", "value": "高"}),
    ];
    let sorts = [
        "n", "sel", "tags", "st", "d", "ok", "who", "title", "f", "g",
    ];
    let groups = ["sel", "tags", "st", "d", "ok", "who", "g", "n"];
    let mut steps = vec![];
    for _ in 0..40 {
        let mut items = vec![];
        for _ in 0..r(3) + 1 {
            if r(4) == 0 {
                let a = conds[r(conds.len() as u64) as usize].clone();
                let b = conds[r(conds.len() as u64) as usize].clone();
                items.push(json!({"op": "or", "items": [a, b]}));
            } else {
                items.push(conds[r(conds.len() as u64) as usize].clone());
            }
        }
        let mut v = json!({"filter": {"op": if r(3) == 0 { "or" } else { "and" }, "items": items}});
        let ns = r(3);
        v["sorts"] = json!((0..ns).map(|_| json!({"prop": sorts[r(sorts.len() as u64) as usize], "dir": if r(2) == 0 { "asc" } else { "desc" }})).collect::<Vec<_>>());
        if r(2) == 0 {
            v["group_by"] = json!(groups[r(groups.len() as u64) as usize]);
            let how = ["day", "week", "month", "year", "relative"][r(5) as usize];
            v["group_date"] = json!(how);
            if r(3) == 0 {
                v["sub_group_by"] = json!(groups[r(groups.len() as u64) as usize]);
            }
        }
        if r(3) == 0 {
            let f = ["sum", "avg", "median", "min", "max", "range"][r(6) as usize];
            v["totals"] =
                json!({"n": f, "tags": "count_values", "ok": "percent_checked", "d": "earliest"});
        }
        steps.push(q(v));
    }
    Case {
        name: "随机",
        schema: serde_json::to_value(&s).unwrap(),
        opts: json!({"now": NOW, "tz": 480, "people": people()}),
        rows: Value::Array(rows),
        related: vec![],
        steps,
    }
}

#[test]
fn golden_matches_wasm() {
    let cases = [tasks_case(), relations_case(), random_case()];
    let out = Value::Array(cases.iter().map(run).collect());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/db-wasm");
    let path = root.join("test/golden.json");
    let text = serde_json::to_string_pretty(&out).unwrap() + "\n";
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, &text).unwrap();
        eprintln!("更新了 {}", path.display());
    }
    // 没有一步是意外出错的（报错的步骤是故意的）
    let n: usize = out
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["steps"].as_array().unwrap().len())
        .sum();
    eprintln!("{} 个用例，{n} 步", cases.len());
    if !root.join("dist/oad_db_bg.wasm").exists() {
        eprintln!("没有编好的 WASM（sh packages/db-wasm/build.sh），跳过对照");
        return;
    }
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("没有 node，跳过对照");
        return;
    }
    let st = Command::new("node")
        .arg("--test")
        .arg(root.join("test/golden.test.mjs"))
        .status()
        .unwrap();
    assert!(
        st.success(),
        "WASM 版和 Rust 版结果不一样（packages/db-wasm/test/golden.test.mjs）"
    );
}
