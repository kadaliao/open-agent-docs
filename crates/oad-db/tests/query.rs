//! 查询：每种条件、嵌套的且 / 或、排序、分组（子分组、日期分段）、汇总行、搜索、分页；随机数据上的不变量。
mod common;
use common::*;
use oad_db::PropType::*;
use oad_db::{Engine, Group};
use serde_json::{Value, json};

fn tasks() -> Engine {
    let mut uid = p("uid", "编号", UniqueId);
    uid.prefix = Some("T-".into());
    let s = schema(vec![
        p("title", "名称", Title),
        status("st", "状态"),
        p("n", "分", Number),
        with_options(p("tags", "标签", MultiSelect), &["a", "b", "c"]),
        p("due", "截止", Date),
        p("done", "完成", Checkbox),
        p("note", "备注", Text),
        p("who", "人", Person),
        p("files", "附件", Files),
        uid,
        p("ct", "创建时间", CreatedTime),
        formula("f", "双倍", "prop(\"分\") * 2"),
        formula("ft", "标记", "if(prop(\"完成\"), \"做完了\", \"还没\")"),
        formula("fb", "大", "prop(\"分\") > 2"),
        formula("fd", "后一天", "dateAdd(prop(\"截止\"), 1, \"days\")"),
        with_options(p("lv", "级别", Select), &["高", "中", "低"]),
    ]);
    let mut rows = vec![
        row(
            "r1",
            json!({"title": "甲", "st": "s2", "n": 3, "tags": ["o1", "o2"], "due": {"start": "2026-11-14"}, "done": true,
                         "note": "Hello World", "who": ["user:1"], "files": [{"name": "报告.pdf", "url": "/f/1"}], "lv": "o1"}),
        ),
        row(
            "r2",
            json!({"title": "乙", "st": "s1", "n": 10, "tags": ["o2"], "due": {"start": "2026-11-16"}, "who": ["user:2"], "lv": "o3"}),
        ),
        row(
            "r3",
            json!({"title": "丙", "st": "s3", "n": -1, "due": {"start": "2026-10-01", "end": "2026-12-01"}, "lv": "o2"}),
        ),
        row("r4", json!({"title": "丁"})),
    ];
    for (i, r) in rows.iter_mut().enumerate() {
        r.unique_id = Some(i as i64 + 1);
        r.created_at = Some((NOW - i as i64 * 86_400_000 * 10) as f64);
    }
    engine(s, rows)
}

fn q(e: &Engine, prop: &str, cond: &str, value: Value) -> Vec<String> {
    titles(
        e,
        json!({"filter": {"op": "and", "items": [{"prop": prop, "cond": cond, "value": value}]}}),
    )
}

#[test]
fn legacy_query_cases() {
    let e = tasks();
    assert_eq!(q(&e, "状态", "eq", json!("进行中")), ["甲"]);
    assert_eq!(q(&e, "状态", "neq", json!("进行中")), ["乙", "丙", "丁"]);
    assert_eq!(q(&e, "标签", "contains", json!("b")), ["甲", "乙"]);
    assert_eq!(q(&e, "分", "gt", json!(0)), ["甲", "乙"]);
    assert_eq!(q(&e, "截止", "lt", json!("today")), ["甲", "丙"]);
    assert_eq!(q(&e, "截止", "gte", json!("今天")), ["乙"]);
    assert_eq!(q(&e, "完成", "eq", json!(true)), ["甲"]);
    assert_eq!(q(&e, "完成", "eq", json!(false)), ["乙", "丙", "丁"]);
    assert_eq!(q(&e, "备注", "contains", json!("world")), ["甲"]);
    assert_eq!(q(&e, "人", "contains", json!("张三")), ["甲"]);
    assert_eq!(q(&e, "分", "empty", json!(null)), ["丁"]);
    assert_eq!(q(&e, "标签", "not_empty", json!(null)), ["甲", "乙"]);
    assert_eq!(q(&e, "名称", "eq", json!("丙")), ["丙"]);
    assert_eq!(
        titles(
            &e,
            json!({"filter": [{"prop": "分", "cond": "gte", "value": 0}, {"prop": "标签", "cond": "contains", "value": "a"}]})
        ),
        ["甲"]
    );
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "分", "desc": true}]})),
        ["乙", "甲", "丙", "丁"]
    ); // 空的在最后
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "状态"}]})),
        ["乙", "甲", "丙", "丁"]
    ); // 按选项的顺序
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "名称", "dir": "desc"}]})),
        ["甲", "乙", "丙", "丁"]
    ); // 文字按码位
    let err = |f: Value| e.query(&view(json!({"filter": f}))).unwrap_err().msg;
    assert!(err(json!([{"prop": "不存在", "cond": "eq", "value": 1}])).contains("没有属性"));
    assert!(err(json!([{"prop": "状态", "cond": "eq", "value": "搁置"}])).contains("没有选项"));
    assert!(
        err(json!([{"prop": "分", "cond": "starts_with", "value": 1}]))
            .contains("不能用条件「starts_with」")
    );
    assert!(
        e.query(&view(json!({"sorts": [{"prop": "没有"}]})))
            .unwrap_err()
            .msg
            .contains("排序里没有属性")
    );
}

#[test]
fn text_conditions() {
    let e = tasks();
    assert_eq!(q(&e, "备注", "eq", json!("hello world")), ["甲"]);
    assert_eq!(
        q(&e, "备注", "neq", json!("hello world")),
        ["乙", "丙", "丁"]
    );
    assert_eq!(
        q(&e, "备注", "not_contains", json!("hello")),
        ["乙", "丙", "丁"]
    );
    assert_eq!(q(&e, "备注", "starts_with", json!("HELLO")), ["甲"]);
    assert_eq!(q(&e, "备注", "ends_with", json!("world")), ["甲"]);
    assert_eq!(q(&e, "名称", "gt", json!("乙")), ["甲"]);
    assert_eq!(q(&e, "备注", "empty", json!(null)), ["乙", "丙", "丁"]);
    // 没填值的条件不筛
    assert_eq!(
        q(&e, "备注", "contains", json!("")),
        ["甲", "乙", "丙", "丁"]
    );
    assert_eq!(q(&e, "状态", "eq", json!(null)), ["甲", "乙", "丙", "丁"]);
    // 条件别名
    assert_eq!(q(&e, "备注", "包含", json!("world")), ["甲"]);
    assert_eq!(q(&e, "分", ">", json!(5)), ["乙"]);
}

#[test]
fn number_and_id_conditions() {
    let e = tasks();
    assert_eq!(q(&e, "分", "eq", json!("3")), ["甲"]);
    assert_eq!(q(&e, "分", "neq", json!(3)), ["乙", "丙", "丁"]);
    assert_eq!(q(&e, "分", "gte", json!(3)), ["甲", "乙"]);
    assert_eq!(q(&e, "分", "lt", json!(3)), ["丙"]);
    assert_eq!(q(&e, "分", "lte", json!("¥3")), ["甲", "丙"]);
    assert_eq!(q(&e, "编号", "gte", json!("T-2")), ["乙", "丙", "丁"]);
    assert_eq!(q(&e, "编号", "eq", json!(4)), ["丁"]);
    assert!(
        e.query(&view(
            json!({"filter": [{"prop": "分", "cond": "eq", "value": "很多"}]})
        ))
        .is_err()
    );
}

#[test]
fn option_people_files_conditions() {
    let e = tasks();
    assert_eq!(
        q(&e, "状态", "eq", json!(["未开始", "已完成"])),
        ["乙", "丙"]
    ); // 任一
    assert_eq!(q(&e, "状态", "in_group", json!("done")), ["丙"]);
    assert_eq!(
        q(&e, "状态", "in_group", json!(["未开始", "进行中"])),
        ["甲", "乙"]
    );
    assert_eq!(q(&e, "st", "eq", json!("s1")), ["乙"]);
    assert_eq!(q(&e, "级别", "neq", json!("高")), ["乙", "丙", "丁"]);
    assert_eq!(
        q(&e, "标签", "not_contains", json!("a")),
        ["乙", "丙", "丁"]
    );
    assert_eq!(q(&e, "标签", "eq", json!("a, c")), ["甲"]);
    assert_eq!(
        q(&e, "人", "not_contains", json!("user:1")),
        ["乙", "丙", "丁"]
    );
    assert_eq!(q(&e, "人", "eq", json!(["张三", "李四"])), ["甲", "乙"]);
    assert!(
        e.query(&view(
            json!({"filter": [{"prop": "人", "cond": "eq", "value": "谁"}]})
        ))
        .unwrap_err()
        .msg
        .contains("没有叫")
    );
    assert_eq!(q(&e, "附件", "contains", json!("报告")), ["甲"]);
    assert_eq!(q(&e, "附件", "empty", json!(null)), ["乙", "丙", "丁"]);
    assert_eq!(q(&e, "完成", "checked", json!(null)), ["甲"]);
    assert_eq!(q(&e, "完成", "unchecked", json!(null)), ["乙", "丙", "丁"]);
    assert_eq!(q(&e, "完成", "neq", json!("是")), ["乙", "丙", "丁"]);
}

#[test]
fn date_conditions() {
    let e = tasks(); // 今天 2026-11-15
    assert_eq!(q(&e, "截止", "eq", json!("2026-11-14")), ["甲", "丙"]); // 丙的范围里也有这天
    assert_eq!(q(&e, "截止", "eq", json!("2026-11-01")), ["丙"]); // 在范围里
    assert_eq!(q(&e, "截止", "neq", json!("2026-11-14")), ["乙", "丁"]);
    assert_eq!(q(&e, "截止", "gt", json!("yesterday")), ["乙"]);
    assert_eq!(
        q(&e, "截止", "lte", json!({"start": "2026-11-14"})),
        ["甲", "丙"]
    );
    assert_eq!(q(&e, "截止", "is_within", json!("past_week")), ["甲", "丙"]); // 范围和过去一周有重叠
    assert_eq!(q(&e, "截止", "is_within", json!("next_week")), ["乙", "丙"]);
    assert_eq!(q(&e, "截止", "is_within", json!("this_week")), ["甲", "丙"]); // 周一 11-09 到周日 11-15
    assert_eq!(
        q(&e, "截止", "is_within", json!("this_month")),
        ["甲", "乙", "丙"]
    );
    assert_eq!(
        q(&e, "截止", "is_within", json!("next_1_days")),
        ["乙", "丙"]
    );
    assert_eq!(q(&e, "截止", "is_within", json!("today")), ["丙"]);
    assert_eq!(q(&e, "截止", "gt", json!("one_week_ago")), ["甲", "乙"]);
    assert_eq!(q(&e, "截止", "lt", json!("one_month_ago")), ["丙"]);
    assert_eq!(q(&e, "截止", "empty", json!(null)), ["丁"]);
    assert!(
        e.query(&view(
            json!({"filter": [{"prop": "截止", "cond": "is_within", "value": "someday"}]})
        ))
        .is_err()
    );
    assert!(
        e.query(&view(
            json!({"filter": [{"prop": "截止", "cond": "eq", "value": "下周三"}]})
        ))
        .is_err()
    );
    // 创建时间：甲今天、乙 10 天前、丙 20 天前、丁 30 天前
    assert_eq!(
        q(&e, "创建时间", "is_within", json!("past_month")),
        ["甲", "乙", "丙", "丁"]
    );
    assert_eq!(q(&e, "创建时间", "is_within", json!("past_week")), ["甲"]);
    assert_eq!(q(&e, "创建时间", "lt", json!("2026-11-01")), ["丙", "丁"]);
}

#[test]
fn formula_conditions() {
    let e = tasks();
    assert_eq!(q(&e, "双倍", "gt", json!(5)), ["甲", "乙"]);
    assert_eq!(q(&e, "双倍", "eq", json!("20")), ["乙"]);
    assert_eq!(q(&e, "双倍", "empty", json!(null)), ["丁"]);
    assert_eq!(q(&e, "双倍", "neq", json!(6)), ["乙", "丙", "丁"]);
    assert_eq!(q(&e, "标记", "contains", json!("做完")), ["甲"]);
    assert_eq!(
        q(&e, "标记", "starts_with", json!("还")),
        ["乙", "丙", "丁"]
    );
    assert_eq!(q(&e, "大", "checked", json!(null)), ["甲", "乙"]);
    assert_eq!(q(&e, "大", "eq", json!(false)), ["丙", "丁"]);
    assert_eq!(q(&e, "后一天", "eq", json!("2026-11-15")), ["甲"]);
    assert_eq!(
        q(&e, "后一天", "is_within", json!("next_week")),
        ["甲", "乙"]
    ); // dateAdd 只留开始
    assert_eq!(
        titles(
            &e,
            json!({"filter": [{"prop": "双倍", "cond": "gt", "value": 0}], "sorts": [{"prop": "双倍", "dir": "desc"}]})
        ),
        ["乙", "甲"]
    );
}

#[test]
fn nested_and_or() {
    let e = tasks();
    let f = json!({"op": "or", "items": [
        {"prop": "状态", "cond": "eq", "value": "已完成"},
        {"op": "and", "items": [{"prop": "分", "cond": "gt", "value": 5}, {"prop": "标签", "cond": "contains", "value": "b"}]}
    ]});
    assert_eq!(titles(&e, json!({ "filter": f })), ["乙", "丙"]);
    let f = json!({"op": "and", "items": [
        {"op": "or", "items": [{"prop": "名称", "cond": "eq", "value": "甲"}, {"prop": "名称", "cond": "eq", "value": "丁"}]},
        {"op": "or", "items": []}
    ]});
    assert_eq!(titles(&e, json!({ "filter": f })), ["甲", "丁"]); // 空的“或”不筛
    assert_eq!(
        titles(
            &e,
            json!({"filter": {"or": [{"prop": "分", "cond": "lt", "value": 0}, {"prop": "分", "cond": "gt", "value": 9}]}})
        ),
        ["乙", "丙"]
    );
    assert_eq!(
        titles(&e, json!({"filter": {"op": "and", "items": []}})),
        ["甲", "乙", "丙", "丁"]
    );
    // 旧写法：{prop, op, value}
    assert_eq!(
        titles(
            &e,
            json!({"filter": [{"prop": "分", "op": "gt", "value": 5}]})
        ),
        ["乙"]
    );
}

#[test]
fn sorts() {
    let e = tasks();
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "截止"}]})),
        ["丙", "甲", "乙", "丁"]
    );
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "截止", "dir": "desc"}]})),
        ["乙", "甲", "丙", "丁"]
    );
    assert_eq!(
        titles(
            &e,
            json!({"sorts": [{"prop": "完成", "dir": "desc"}, {"prop": "分"}]})
        ),
        ["甲", "丙", "乙", "丁"]
    );
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "级别"}]})),
        ["甲", "丙", "乙", "丁"]
    );
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "标记"}, {"prop": "名称"}]})),
        ["甲", "丁", "丙", "乙"]
    ); // 按码位：做 < 还，丁 < 丙 < 乙
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "人"}]})),
        ["甲", "乙", "丙", "丁"]
    );
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "创建时间"}]})),
        ["丁", "丙", "乙", "甲"]
    );
    assert_eq!(
        titles(&e, json!({"sorts": [{"prop": "编号", "dir": "desc"}]})),
        ["丁", "丙", "乙", "甲"]
    );
    // 自然顺序：任务2 在 任务10 前面
    let s = schema(vec![p("title", "名称", Title)]);
    let rows = ["任务10", "任务2", "任务1", "Apple", "apple 2", "任务02"]
        .iter()
        .enumerate()
        .map(|(i, t)| row(&format!("r{i}"), json!({"title": t})))
        .collect();
    let e2 = engine(s, rows);
    assert_eq!(
        titles(&e2, json!({"sorts": [{"prop": "名称"}]})),
        ["Apple", "apple 2", "任务1", "任务2", "任务02", "任务10"]
    );
}

fn names(g: &[Group]) -> Vec<(String, usize)> {
    g.iter().map(|x| (x.name.clone(), x.count)).collect()
}

#[test]
fn grouping() {
    let e = tasks();
    let r = e.query(&view(json!({"group_by": "st"}))).unwrap();
    assert_eq!(
        names(r.groups.as_ref().unwrap()),
        [
            ("未开始".into(), 1),
            ("进行中".into(), 1),
            ("已完成".into(), 1),
            ("无状态".into(), 1)
        ]
    );
    assert_eq!(r.groups.as_ref().unwrap()[1].color.as_deref(), Some("blue"));
    assert_eq!(r.groups.as_ref().unwrap()[3].key, "");
    // 多选：一行落在几个分组里；没有行的选项也列出来
    let g = e
        .query(&view(json!({"group_by": "标签"})))
        .unwrap()
        .groups
        .unwrap();
    assert_eq!(
        names(&g),
        [
            ("a".into(), 1),
            ("b".into(), 2),
            ("c".into(), 0),
            ("无标签".into(), 2)
        ]
    );
    let g = e
        .query(&view(
            json!({"group_by": "标签", "hide_empty_groups": true}),
        ))
        .unwrap()
        .groups
        .unwrap();
    assert_eq!(g.len(), 3);
    let g = e
        .query(&view(json!({"group_by": "完成"})))
        .unwrap()
        .groups
        .unwrap();
    assert_eq!(names(&g), [("已勾选".into(), 1), ("未勾选".into(), 3)]);
    assert_eq!((g[0].key.as_str(), g[1].key.as_str()), ("true", "false"));
    let g = e
        .query(&view(json!({"group_by": "人"})))
        .unwrap()
        .groups
        .unwrap();
    assert_eq!(
        names(&g),
        [("张三".into(), 1), ("李四".into(), 1), ("无人".into(), 2)]
    );
    let g = e
        .query(&view(json!({"group_by": "分"})))
        .unwrap()
        .groups
        .unwrap();
    assert_eq!(
        g.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(),
        ["-1", "3", "10", ""]
    );
    let g = e
        .query(&view(json!({"group_by": "大"})))
        .unwrap()
        .groups
        .unwrap();
    assert_eq!(names(&g), [("是".into(), 2), ("否".into(), 2)]); // 空 > 2 是 false
    // 组里的行按排序
    let g = e
        .query(&view(
            json!({"group_by": "完成", "sorts": [{"prop": "分", "dir": "desc"}]}),
        ))
        .unwrap()
        .groups
        .unwrap();
    assert_eq!(g[1].ids, ["r2", "r3", "r4"]);
}

#[test]
fn date_buckets() {
    let e = tasks();
    let g = |how: &str| {
        e.query(&view(json!({"group_by": "截止", "group_date": how})))
            .unwrap()
            .groups
            .unwrap()
            .iter()
            .map(|x| (x.key.clone(), x.name.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        g("month"),
        [
            ("2026-10".into(), "2026年10月".into()),
            ("2026-11".into(), "2026年11月".into()),
            (String::new(), "无截止".into())
        ]
    );
    assert_eq!(g("day")[0], ("2026-10-01".into(), "2026年10月1日".into()));
    assert_eq!(
        g("week").iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
        ["2026-W40", "2026-W46", "2026-W47", ""]
    );
    assert_eq!(g("year")[0], ("2026".into(), "2026年".into()));
    assert_eq!(
        g("relative")
            .iter()
            .map(|x| x.1.as_str())
            .collect::<Vec<_>>(),
        ["2026年10月", "昨天", "明天", "无截止"]
    );
}

#[test]
fn sub_groups_and_totals() {
    let e = tasks();
    let r = e
        .query(&view(json!({"group_by": "完成", "sub_group_by": "状态", "totals": {"分": "sum", "名称": "count", "标签": "count_values"}})))
        .unwrap();
    let g = r.groups.unwrap();
    let sub = g[1].groups.as_ref().unwrap();
    assert_eq!(
        names(sub),
        [
            ("未开始".into(), 1),
            ("进行中".into(), 0),
            ("已完成".into(), 1),
            ("无状态".into(), 1)
        ]
    );
    assert_eq!(g[1].totals.as_ref().unwrap()["分"], json!(9));
    assert_eq!(r.totals.as_ref().unwrap()["分"], json!(12));
    assert_eq!(r.totals.unwrap()["名称"], json!(4));
}

#[test]
fn every_total() {
    let e = tasks();
    let t = |key: &str, f: &str| {
        e.query(&view(json!({"totals": { key: f }})))
            .unwrap()
            .totals
            .unwrap()[key]
            .clone()
    };
    assert_eq!(t("分", "count"), json!(4));
    assert_eq!(t("分", "count_values"), json!(3));
    assert_eq!(t("标签", "count_values"), json!(3));
    assert_eq!(t("标签", "unique"), json!(2));
    assert_eq!(t("标签", "count_unique"), json!(2));
    assert_eq!(t("分", "empty"), json!(1));
    assert_eq!(t("分", "not_empty"), json!(3));
    assert_eq!(t("分", "percent_empty"), json!(0.25));
    assert_eq!(t("分", "percent_not_empty"), json!(0.75));
    assert_eq!(t("分", "sum"), json!(12));
    assert_eq!(t("分", "avg"), json!(4));
    assert_eq!(t("分", "min"), json!(-1));
    assert_eq!(t("分", "max"), json!(10));
    assert_eq!(t("分", "median"), json!(3));
    assert_eq!(t("分", "range"), json!(11));
    assert_eq!(t("截止", "earliest"), json!({"start": "2026-10-01"}));
    assert_eq!(t("截止", "latest"), json!({"start": "2026-11-16"}));
    assert_eq!(t("截止", "range"), json!(46));
    assert_eq!(t("完成", "checked"), json!(1));
    assert_eq!(t("完成", "unchecked"), json!(3));
    assert_eq!(t("完成", "percent_checked"), json!(0.25));
    assert_eq!(t("完成", "percent_unchecked"), json!(0.75));
    assert_eq!(t("双倍", "sum"), json!(24));
    assert_eq!(t("状态", "count_unique"), json!(3));
    assert_eq!(
        t("状态", "show_unique"),
        json!(["进行中", "未开始", "已完成"])
    );
    // 只算筛出来的行；写错的算法、属性不报错（视图里旧的设置）
    let r = e.query(&view(json!({"filter": [{"prop": "分", "cond": "gt", "value": 0}], "totals": {"分": "sum", "没有": "sum", "备注": "magic"}}))).unwrap();
    assert_eq!(r.totals.unwrap(), *json!({"分": 13}).as_object().unwrap());
}

#[test]
fn search_and_paging() {
    let e = tasks();
    assert_eq!(titles(&e, json!({"search": "WORLD"})), ["甲"]);
    assert_eq!(titles(&e, json!({"search": "进行"})), ["甲"]); // 选项名
    assert_eq!(titles(&e, json!({"search": "李"})), ["乙"]); // 人名
    assert_eq!(titles(&e, json!({"search": "t-3"})), ["丙"]); // 编号
    assert_eq!(
        titles(&e, json!({"search": "  "})),
        ["甲", "乙", "丙", "丁"]
    );
    let r = e
        .query(&view(
            json!({"sorts": [{"prop": "分"}], "offset": 1, "limit": 2}),
        ))
        .unwrap();
    assert_eq!((r.ids, r.total), (vec!["r1".to_string(), "r2".into()], 4));
    let r = e.query(&view(json!({"offset": 10}))).unwrap();
    assert!(r.ids.is_empty());
}

/// 很小的伪随机数（测试里不要依赖外面的 crate）。
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn random_engine(seed: u64, n: usize) -> Engine {
    let mut g = Lcg(seed);
    let s = schema(vec![
        p("title", "名称", Title),
        p("n", "分", Number),
        with_options(p("sel", "级", Select), &["高", "中", "低"]),
        with_options(p("tags", "标签", MultiSelect), &["a", "b", "c", "d"]),
        p("d", "日", Date),
        p("ok", "好", Checkbox),
        formula("f", "式", "prop(\"分\") % 7"),
    ]);
    let rows = (0..n)
        .map(|i| {
            let mut o = serde_json::Map::new();
            if g.pick(5) > 0 {
                o.insert("title".into(), json!(format!("行{}", g.pick(50))));
            }
            if g.pick(4) > 0 {
                o.insert("n".into(), json!(g.pick(100) as i64 - 30));
            }
            if g.pick(3) > 0 {
                o.insert("sel".into(), json!(format!("o{}", g.pick(3) + 1)));
            }
            let tags: Vec<String> = (1..=4)
                .filter(|_| g.pick(3) == 0)
                .map(|k| format!("o{k}"))
                .collect();
            if !tags.is_empty() {
                o.insert("tags".into(), json!(tags));
            }
            if g.pick(3) > 0 {
                o.insert(
                    "d".into(),
                    json!({"start": format!("2026-{:02}-{:02}", g.pick(12) + 1, g.pick(28) + 1)}),
                );
            }
            if g.pick(2) == 0 {
                o.insert("ok".into(), json!(true));
            }
            row(&format!("r{i}"), Value::Object(o))
        })
        .collect();
    engine(s, rows)
}

fn random_cond(g: &mut Lcg) -> Value {
    let pick = |g: &mut Lcg, xs: &[Value]| xs[g.pick(xs.len() as u64) as usize].clone();
    match g.pick(7) {
        0 => {
            json!({"prop": "分", "cond": pick(g, &[json!("gt"), json!("lt"), json!("eq"), json!("empty")]), "value": g.pick(60) as i64 - 20})
        }
        1 => {
            json!({"prop": "级", "cond": pick(g, &[json!("eq"), json!("neq"), json!("not_empty")]), "value": pick(g, &[json!("高"), json!("中"), json!(["低", "高"])])})
        }
        2 => {
            json!({"prop": "标签", "cond": pick(g, &[json!("contains"), json!("not_contains"), json!("empty")]), "value": pick(g, &[json!("a"), json!("b, c")])})
        }
        3 => match g.pick(3) {
            0 => {
                json!({"prop": "日", "cond": "is_within", "value": pick(g, &[json!("past_year"), json!("this_month"), json!("next_3_days")])})
            }
            1 => json!({"prop": "日", "cond": "empty"}),
            _ => {
                json!({"prop": "日", "cond": pick(g, &[json!("lt"), json!("gte"), json!("eq")]), "value": pick(g, &[json!("2026-06-01"), json!("today")])})
            }
        },
        4 => json!({"prop": "好", "cond": pick(g, &[json!("checked"), json!("unchecked")])}),
        5 => {
            json!({"prop": "名称", "cond": pick(g, &[json!("contains"), json!("starts_with"), json!("empty")]), "value": pick(g, &[json!("1"), json!("行2")])})
        }
        _ => {
            json!({"prop": "式", "cond": pick(g, &[json!("gt"), json!("eq"), json!("empty")]), "value": g.pick(7)})
        }
    }
}

#[test]
fn random_filters_compose() {
    // “且”是交集，“或”是并集，“空 / 不空”互补；排序是同一些行的重排、按键有序；分组的行合起来是全部
    for seed in 1..40 {
        let e = random_engine(seed, 120);
        let mut g = Lcg(seed * 7919);
        let a = random_cond(&mut g);
        let b = random_cond(&mut g);
        let set = |f: Value| -> std::collections::BTreeSet<String> {
            ids(&e, json!({ "filter": f })).into_iter().collect()
        };
        let (sa, sb) = (set(json!([a.clone()])), set(json!([b.clone()])));
        let and = set(json!({"op": "and", "items": [a.clone(), b.clone()]}));
        let or = set(json!({"op": "or", "items": [a.clone(), b.clone()]}));
        assert_eq!(and, sa.intersection(&sb).cloned().collect(), "{a} and {b}");
        assert_eq!(or, sa.union(&sb).cloned().collect(), "{a} or {b}");
        let empty = set(json!([{"prop": "分", "cond": "empty"}]));
        let filled = set(json!([{"prop": "分", "cond": "not_empty"}]));
        assert_eq!(empty.len() + filled.len(), 120);
        assert!(empty.is_disjoint(&filled));
        // 排序
        let sorted = ids(
            &e,
            json!({"filter": [a.clone()], "sorts": [{"prop": "分", "dir": "desc"}, {"prop": "名称"}]}),
        );
        let as_set: std::collections::BTreeSet<String> = sorted.iter().cloned().collect();
        assert_eq!(as_set, sa);
        let nums: Vec<Option<f64>> = sorted.iter().map(|id| e.value(id, "n").as_f64()).collect();
        let first_none = nums.iter().position(Option::is_none).unwrap_or(nums.len());
        assert!(nums[first_none..].iter().all(Option::is_none), "空的在最后");
        assert!(nums[..first_none].windows(2).all(|w| w[0] >= w[1]));
        // 分组
        let r = e
            .query(&view(json!({"filter": [a.clone()], "group_by": "级"})))
            .unwrap();
        let mut all: Vec<String> = r.groups.unwrap().into_iter().flat_map(|g| g.ids).collect();
        all.sort();
        let mut want: Vec<String> = sa.iter().cloned().collect();
        want.sort();
        assert_eq!(all, want);
    }
}
