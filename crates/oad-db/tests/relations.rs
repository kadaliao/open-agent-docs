//! 关联、双向关联（反向的那一半）、汇总；跨库、关联自己；改动后缓存失效。
mod common;
use common::*;
use oad_db::PropType::*;
use oad_db::{Change, Engine};
use serde_json::{Value, json};

fn ch(v: Value) -> Vec<Change> {
    serde_json::from_value(v).unwrap()
}

/// 项目库（主库）：反向关联“任务”（任务库的“项目”），汇总总工时、完成率、个数。任务库是关联到的库。
fn projects() -> Engine {
    let proj = schema(vec![
        p("title", "名称", Title),
        p("budget", "预算", Number),
        mirror("tasks", "任务", "task_db", "proj"),
        rollup("hours", "总工时", "tasks", "h", "sum"),
        rollup("rate", "完成率", "tasks", "done", "percent_checked"),
        rollup("cnt", "几个", "tasks", "", "count"),
        rollup("names", "任务名", "tasks", "title", "show_original"),
        rollup("first", "最早截止", "tasks", "due", "earliest"),
        rollup("dbl", "双倍工时", "tasks", "h2", "sum"),
        formula(
            "left",
            "剩余预算",
            "prop(\"预算\") - prop(\"总工时\") * 100",
        ),
        formula("tn", "任务数", "prop(\"任务\").length()"),
    ]);
    let task = schema(vec![
        p("title", "名称", Title),
        p("h", "工时", Number),
        p("done", "完成", Checkbox),
        p("due", "截止", Date),
        relation("proj", "项目", "proj_db"),
        formula("h2", "双倍", "prop(\"工时\") * 2"),
    ]);
    let mut e = Engine::new(proj);
    e.set_id("proj_db");
    e.set_clock(clock());
    e.load_rows(vec![
        row("p1", json!({"title": "官网改版", "budget": 2000})),
        row("p2", json!({"title": "发布会", "budget": 500})),
        row("p3", json!({"title": "没任务"})),
    ]);
    e.load_related(
        "task_db",
        task,
        vec![
            row("t1", json!({"title": "设计", "h": 5, "done": true, "due": {"start": "2026-11-20"}, "proj": ["p1"]})),
            row("t2", json!({"title": "开发", "h": 8, "due": {"start": "2026-11-18"}, "proj": ["p1", "p2"]})),
            row("t3", json!({"title": "孤儿", "h": 1})),
        ],
    );
    e
}

#[test]
fn mirror_and_rollups() {
    let e = projects();
    let c = e.computed("p1");
    assert_eq!(c["tasks"], json!(["t1", "t2"]));
    assert_eq!(c["hours"], json!(13));
    assert_eq!(c["rate"], json!(0.5));
    assert_eq!(c["cnt"], json!(2));
    assert_eq!(c["names"], json!(["设计", "开发"]));
    assert_eq!(c["first"], json!({"start": "2026-11-18"}));
    assert_eq!(c["dbl"], json!(26)); // 对方库里的公式
    assert_eq!(c["left"], json!(700));
    assert_eq!(c["tn"], json!(2));
    let c = e.computed("p2");
    assert_eq!(
        (c["tasks"].clone(), c["hours"].clone(), c["rate"].clone()),
        (json!(["t2"]), json!(8), json!(0))
    );
    let c = e.computed("p3");
    assert_eq!(
        (
            c["tasks"].clone(),
            c["hours"].clone(),
            c["cnt"].clone(),
            c["rate"].clone()
        ),
        (Value::Null, Value::Null, json!(0), Value::Null)
    );
    assert_eq!(e.display("任务", &json!(["t1", "t2"])), "设计, 开发");
    assert_eq!(e.display("完成率", &json!(0.5)), "50%");
    assert_eq!(e.display("几个", &json!(2)), "2");
    assert!(e.schema_errors().is_empty(), "{:?}", e.schema_errors());
}

#[test]
fn query_on_relations_and_rollups() {
    let e = projects();
    let q = |f: Value| titles(&e, json!({ "filter": [f] }));
    assert_eq!(
        q(json!({"prop": "任务", "cond": "contains", "value": "开"})),
        ["官网改版", "发布会"]
    );
    assert_eq!(
        q(json!({"prop": "任务", "cond": "eq", "value": "设计"})),
        ["官网改版"]
    );
    assert_eq!(
        q(json!({"prop": "任务", "cond": "contains", "value": "#t2"})),
        ["官网改版", "发布会"]
    );
    assert_eq!(q(json!({"prop": "任务", "cond": "empty"})), ["没任务"]);
    assert_eq!(
        q(json!({"prop": "总工时", "cond": "gt", "value": 10})),
        ["官网改版"]
    );
    assert_eq!(
        q(json!({"prop": "完成率", "cond": "eq", "value": "50%"})),
        ["官网改版"]
    );
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "总工时", "dir": "desc"}]})),
        ["官网改版", "发布会", "没任务"]
    );
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "任务"}]})),
        ["发布会", "官网改版", "没任务"]
    );
    let g = e
        .query(&view(json!({"group_by": "任务"})))
        .unwrap()
        .groups
        .unwrap();
    assert_eq!(
        g.iter()
            .map(|x| (x.name.as_str(), x.count))
            .collect::<Vec<_>>(),
        [("开发", 2), ("设计", 1), ("无任务", 1)]
    );
    let t = e
        .query(&view(
            json!({"totals": {"总工时": "sum", "任务": "count_values"}}),
        ))
        .unwrap()
        .totals
        .unwrap();
    assert_eq!(
        (t["总工时"].clone(), t["任务"].clone()),
        (json!(21), json!(3))
    );
}

#[test]
fn related_db_reload_invalidates() {
    let mut e = projects();
    assert_eq!(e.computed("p1")["hours"], json!(13));
    // 任务库变了（网页收到 db.changed 后重新给一次）
    let task = schema(vec![
        p("title", "名称", Title),
        p("h", "工时", Number),
        p("done", "完成", Checkbox),
        p("due", "截止", Date),
        relation("proj", "项目", "proj_db"),
        formula("h2", "双倍", "prop(\"工时\") * 2"),
    ]);
    e.load_related(
        "task_db",
        task,
        vec![row(
            "t1",
            json!({"title": "设计", "h": 6, "done": true, "proj": ["p1", "p3"]}),
        )],
    );
    assert_eq!(e.computed("p1")["hours"], json!(6));
    assert_eq!(e.computed("p1")["rate"], json!(1));
    assert_eq!(e.computed("p2")["tasks"], Value::Null);
    assert_eq!(e.computed("p3")["tasks"], json!(["t1"]));
}

#[test]
fn stored_relation_side() {
    // 任务库做主库：存的关联、汇总对方的预算
    let task = schema(vec![
        p("title", "名称", Title),
        p("h", "工时", Number),
        relation("proj", "项目", "proj_db"),
        rollup("budget", "项目预算", "proj", "budget", "sum"),
        formula("pn", "项目名", "prop(\"项目\").join(\"/\")"),
    ]);
    let proj = schema(vec![
        p("title", "名称", Title),
        p("budget", "预算", Number),
        mirror("tasks", "任务", "task_db", "proj"),
    ]);
    let mut e = Engine::new(task);
    e.set_id("task_db");
    e.set_clock(clock());
    e.load_related(
        "proj_db",
        proj,
        vec![
            row("p1", json!({"title": "官网改版", "budget": 2000})),
            row("p2", json!({"title": "发布会", "budget": 500})),
        ],
    );
    e.load_rows(vec![
        row("t1", json!({"title": "设计", "proj": ["p1"]})),
        row("t2", json!({"title": "开发", "proj": ["p1", "p2", "gone"]})),
    ]);
    assert_eq!(e.computed("t2")["budget"], json!(2500));
    assert_eq!(e.computed("t2")["pn"], json!("官网改版/发布会"));
    assert_eq!(
        e.display("项目", &json!(["p2", "p1", "gone"])),
        "发布会, 官网改版"
    );
    // 写名称、写 id 都认；对方库里没有的不认
    assert_eq!(
        e.normalize("项目", &json!("官网改版")).unwrap(),
        json!(["p1"])
    );
    assert_eq!(
        e.normalize("项目", &json!(["#p2", "发布会", "p1"]))
            .unwrap(),
        json!(["p2", "p1"])
    );
    assert!(
        e.normalize("项目", &json!("不存在的项目"))
            .unwrap_err()
            .msg
            .contains("没有叫「不存在的项目」的行")
    );
    // 改了关联：这一行的汇总、公式重算
    let changed = e.apply(ch(json!([{"row_id": "t1", "props": {"proj": ["p2"]}}])));
    assert_eq!(changed, ["t1"]);
    assert_eq!(e.computed("t1")["budget"], json!(500));
    assert_eq!(e.computed("t1")["pn"], json!("发布会"));
    // 对方库没加载时：id 原样认，显示 id
    let mut lone = Engine::new(schema(vec![
        p("title", "名称", Title),
        relation("x", "外链", "other"),
    ]));
    lone.set_clock(clock());
    assert_eq!(
        lone.normalize("外链", &json!("a, b")).unwrap(),
        json!(["a", "b"])
    );
    assert_eq!(lone.display("外链", &json!(["a"])), "a");
}

/// 关联自己（任务的上级任务），双向：反向的是“子任务”；汇总子任务的工时。
fn tree() -> Engine {
    let s = schema(vec![
        p("title", "名称", Title),
        p("h", "工时", Number),
        relation("parent", "上级", "db1"),
        mirror("kids", "子任务", "db1", "parent"),
        rollup("kh", "子任务工时", "kids", "h", "sum"),
        rollup("ph", "上级工时", "parent", "h", "sum"),
        rollup("pkh", "上级的子任务工时", "parent", "kh", "sum"),
    ]);
    engine(
        s,
        vec![
            row("a", json!({"title": "根", "h": 1})),
            row("b", json!({"title": "甲", "h": 2, "parent": ["a"]})),
            row("c", json!({"title": "乙", "h": 3, "parent": ["a"]})),
            row("d", json!({"title": "甲一", "h": 4, "parent": ["b"]})),
        ],
    )
}

#[test]
fn self_relation_two_way() {
    let mut e = tree();
    assert_eq!(e.computed("a")["kids"], json!(["b", "c"]));
    assert_eq!(e.computed("a")["kh"], json!(5));
    assert_eq!(e.computed("b")["kh"], json!(4));
    assert_eq!(e.computed("d")["ph"], json!(2));
    assert_eq!(e.computed("d")["pkh"], json!(4)); // 汇总套汇总
    assert_eq!(e.computed("b")["pkh"], json!(5));
    assert_eq!(e.display("子任务", &json!(["b", "c"])), "甲, 乙");
    // 改 d 的工时：b 的子任务工时、d 自己的都变；受影响的行有 d 指着的 b
    let changed = e.apply(ch(json!([{"row_id": "d", "props": {"h": 10}}])));
    assert_eq!(changed, ["d", "b"]);
    assert_eq!(e.computed("b")["kh"], json!(10));
    // 把 c 挪到 b 下面：a、b 的子任务都变
    let changed = e.apply(ch(json!([{"row_id": "c", "props": {"parent": ["b"]}}])));
    assert_eq!(changed, ["c", "a", "b"]);
    assert_eq!(e.computed("a")["kids"], json!(["b"]));
    assert_eq!(e.computed("b")["kids"], json!(["c", "d"]));
    assert_eq!(e.computed("a")["kh"], json!(2));
    assert_eq!(e.computed("b")["kh"], json!(13));
    assert_eq!(e.computed("d")["pkh"], json!(13));
    // 删行、加行
    e.apply(ch(json!([{"row_id": "d", "deleted": true}, {"row_id": "e", "row": {"id": "e", "props": {"title": "新", "h": 7, "parent": ["a"]}}}])));
    assert_eq!(e.computed("a")["kids"], json!(["b", "e"]));
    assert_eq!(e.computed("b")["kh"], json!(3));
    assert_eq!(e.row_ids(), ["a", "b", "c", "e"]);
    let q = titles(
        &e,
        json!({"filter": [{"prop": "子任务", "cond": "not_empty"}]}),
    );
    assert_eq!(q, ["根", "甲"]);
}

#[test]
fn rollup_cycles_are_errors() {
    // 汇总互相引用成环（沿关联自己）：标成错的，值是空的，不会死循环
    let s = schema(vec![
        p("title", "名称", Title),
        relation("rel", "关联", "db1"),
        rollup("x", "甲", "rel", "y", "sum"),
        rollup("y", "乙", "rel", "x", "sum"),
        formula("z", "丙", "prop(\"甲\") + 1"),
    ]);
    let e = engine(s, vec![row("a", json!({"title": "a", "rel": ["a"]}))]);
    let errs = e.schema_errors();
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert!(errs.iter().all(|x| x.msg.contains("成了环")));
    assert_eq!(e.computed("a")["x"], Value::Null);
    assert_eq!(e.computed("a")["z"], Value::Null);
    // 汇总的关联不在了
    let s = schema(vec![
        p("title", "名称", Title),
        rollup("x", "甲", "nope", "y", "sum"),
    ]);
    let e = engine(s, vec![row("a", json!({}))]);
    assert!(e.schema_errors()[0].msg.contains("关联属性不存在"));
}

#[test]
fn cross_db_cycle_is_caught() {
    // A 的汇总取 B 的汇总，B 的汇总又取 A 的那个汇总
    let a = schema(vec![
        p("title", "名称", Title),
        relation("ab", "到B", "B"),
        rollup("ra", "A汇总", "ab", "rb", "sum"),
    ]);
    let b = schema(vec![
        p("title", "名称", Title),
        relation("ba", "到A", "A"),
        rollup("rb", "B汇总", "ba", "ra", "sum"),
    ]);
    let mut e = Engine::new(a);
    e.set_id("A");
    e.load_rows(vec![row("a1", json!({"ab": ["b1"]}))]);
    e.load_related("B", b, vec![row("b1", json!({"ba": ["a1"]}))]);
    assert!(e.schema_errors()[0].msg.contains("成了环"));
    assert_eq!(e.computed("a1")["ra"], Value::Null);
}

#[test]
fn every_rollup_fn() {
    let mut proj = schema(vec![
        p("title", "名称", Title),
        mirror("tasks", "任务", "task_db", "proj"),
    ]);
    let fns = [
        ("show_original", "tag", json!(["x", "y", "x"])),
        ("show_unique", "tag", json!(["x", "y"])),
        ("count", "h", json!(3)),
        ("count_values", "h", json!(2)),
        ("count_unique", "tag", json!(2)),
        ("count_empty", "h", json!(1)),
        ("count_not_empty", "h", json!(2)),
        ("percent_empty", "h", json!(0.3333333333)),
        ("percent_not_empty", "h", json!(0.6666666667)),
        ("sum", "h", json!(9)),
        ("average", "h", json!(4.5)),
        ("median", "h", json!(4.5)),
        ("min", "h", json!(3)),
        ("max", "h", json!(6)),
        ("range", "h", json!(3)),
        ("earliest", "due", json!({"start": "2026-11-01"})),
        ("latest", "due", json!({"start": "2026-11-11"})),
        (
            "date_range",
            "due",
            json!({"start": "2026-11-01", "end": "2026-11-11"}),
        ),
        ("range", "due", json!(10)),
        ("checked", "done", json!(1)),
        ("unchecked", "done", json!(2)),
        ("percent_checked", "done", json!(0.3333333333)),
        ("percent_unchecked", "done", json!(0.6666666667)),
        ("求和", "h", json!(9)),
    ];
    for (i, (f, target, _)) in fns.iter().enumerate() {
        proj.props.push(rollup(
            &format!("r{i}"),
            &format!("汇总{i}"),
            "tasks",
            target,
            f,
        ));
    }
    let task = schema(vec![
        p("title", "名称", Title),
        p("h", "工时", Number),
        p("done", "完成", Checkbox),
        p("due", "截止", Date),
        with_options(p("tag", "标签", Select), &["x", "y"]),
        relation("proj", "项目", "proj_db"),
    ]);
    let mut e = Engine::new(proj);
    e.set_id("proj_db");
    e.load_rows(vec![row("p", json!({"title": "P"}))]);
    e.load_related(
        "task_db",
        task,
        vec![
            row("t1", json!({"h": 3, "done": true, "due": {"start": "2026-11-11"}, "tag": "o1", "proj": ["p"]})),
            row("t2", json!({"h": 6, "tag": "o2", "proj": ["p"]})),
            row("t3", json!({"due": {"start": "2026-11-01"}, "tag": "o1", "proj": ["p"]})),
        ],
    );
    let c = e.computed("p");
    for (i, (f, target, want)) in fns.iter().enumerate() {
        assert_eq!(c[&format!("r{i}")], *want, "{f}({target})");
    }
}

#[test]
fn apply_kinds() {
    let s = schema(vec![
        p("title", "名称", Title),
        p("n", "数", Number),
        formula("f", "式", "prop(\"数\") + 1"),
    ]);
    let mut e = engine(
        s,
        vec![
            row("a", json!({"title": "a", "n": 1})),
            row("b", json!({"title": "b", "n": 2})),
        ],
    );
    assert_eq!(e.computed("a")["f"], json!(2));
    // props 只改给了的格子；null / "" / [] 是清空
    e.apply(ch(json!([{"row_id": "a", "props": {"n": 5}}])));
    assert_eq!(e.value("a", "title"), json!("a"));
    assert_eq!(e.computed("a")["f"], json!(6));
    e.apply(ch(
        json!([{"row_id": "a", "set": {"n": null, "title": ""}}]),
    ));
    assert_eq!(e.value("a", "n"), Value::Null);
    assert_eq!(e.value("a", "title"), Value::Null);
    // row 整行换掉（同步来的），没有的行是加
    e.apply(ch(
        json!([{"row_id": "b", "row": {"id": "b", "seq": 9, "props": {"title": "B"}}}]),
    ));
    assert_eq!(e.value("b", "n"), Value::Null);
    assert_eq!(e.row("b").unwrap().seq, 9);
    assert_eq!(
        e.apply(ch(
            json!([{"row_id": "c", "props": {"title": "c", "n": 0}}])
        )),
        ["c"]
    );
    assert_eq!(e.computed("c")["f"], json!(1));
    assert_eq!(e.row_ids(), ["a", "b", "c"]);
    // 删的行不存在：没事
    assert!(
        e.apply(ch(json!([{"row_id": "zzz", "deleted": true}])))
            .contains(&"zzz".to_string())
    );
    e.apply(ch(json!([{"row_id": "a", "deleted": true}])));
    assert_eq!(e.row_ids(), ["b", "c"]);
    assert_eq!(e.computed("c")["f"], json!(1));
    assert!(e.computed("a").is_empty());
    // 改表结构：公式重算
    let s = schema(vec![
        p("title", "名称", Title),
        p("n", "数", Number),
        formula("f", "式", "prop(\"数\") * 10"),
    ]);
    e.set_schema(s);
    assert_eq!(e.computed("c")["f"], json!(0));
}

#[test]
fn computed_includes_auto_props() {
    let s = schema(vec![
        p("title", "名称", Title),
        p("ct", "创建时间", CreatedTime),
        p("ub", "最后编辑人", UpdatedBy),
        p("uid", "编号", UniqueId),
    ]);
    let mut r = row("a", json!({}));
    r.unique_id = Some(12);
    r.updated_by = Some("agent:7".into());
    let e = engine(s, vec![r]);
    let c = e.computed("a");
    assert_eq!(c["ct"], json!({"start": "2026-11-15T10:30", "time": true}));
    assert_eq!(c["ub"], json!("agent:7"));
    assert_eq!(c["uid"], json!(12));
    assert_eq!(e.display_row("a")["ub"], json!("Writer"));
}

#[test]
fn engine_is_send() {
    // 服务器把它放进 Mutex 跨线程用（缓存用 RefCell，所以不是 Sync）
    fn send<T: Send>() {}
    send::<Engine>();
    send::<oad_db::api::JsonEngine>();
}
