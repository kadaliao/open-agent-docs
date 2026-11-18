//! 值：规范化（名字 → 存的样子）、显示、改类型时的转换。
mod common;
use common::*;
use oad_db::PropType::*;
use oad_db::{Engine, Prop};
use serde_json::{Value, json};

fn schema_all() -> oad_db::Schema {
    let mut num = p("n", "数", Number);
    num.format = Some("cny".into());
    let mut uid = p("uid", "编号", UniqueId);
    uid.prefix = Some("T-".into());
    let mut zh = p("dz", "中文日", Date);
    zh.date_format = Some("zh".into());
    schema(vec![
        p("title", "名称", Title),
        num,
        with_options(p("tags", "标签", MultiSelect), &["甲", "乙"]),
        status("st", "状态"),
        p("who", "人", Person),
        p("d", "日", Date),
        p("ok", "好", Checkbox),
        p("url", "网", Url),
        p("mail", "信", Email),
        p("tel", "电话", Phone),
        with_options(p("sel", "选", Select), &["高", "低"]),
        p("note", "备注", Text),
        p("files", "附件", Files),
        p("ct", "创建时间", CreatedTime),
        p("cb", "创建人", CreatedBy),
        uid,
        zh,
        formula("f", "式", "1"),
        p("btn", "按钮", Button),
    ])
}

fn e() -> Engine {
    engine(schema_all(), vec![])
}

fn n(key: &str, v: Value) -> Result<Value, String> {
    e().normalize(key, &v).map_err(|x| x.msg)
}

#[test]
fn numbers() {
    assert_eq!(n("n", json!("¥1,200")), Ok(json!(1200)));
    assert_eq!(n("n", json!(3.5)), Ok(json!(3.5)));
    assert_eq!(n("n", json!("50%")), Ok(json!(0.5)));
    assert_eq!(n("n", json!(" 1 234 ")), Ok(json!(1234)));
    assert_eq!(n("n", json!(null)), Ok(Value::Null));
    assert_eq!(n("n", json!("")), Ok(Value::Null));
    assert!(
        n("n", json!("很多"))
            .unwrap_err()
            .contains("要写数字，不是「很多」")
    );
    assert!(n("n", json!(true)).is_err());
}

#[test]
fn options() {
    let en = e();
    assert_eq!(
        en.normalize("标签", &json!("甲、乙")).unwrap(),
        json!(["o1", "o2"])
    );
    assert_eq!(
        en.normalize("tags", &json!(["o2", "甲", "甲"])).unwrap(),
        json!(["o2", "o1"])
    );
    // 没有的选项：默认报错；create 时新加（给出新选项，由调用方写回表结构）
    assert!(
        en.normalize("tags", &json!(["丙"]))
            .unwrap_err()
            .msg
            .contains("没有选项「丙」，可选：甲、乙")
    );
    let got = en
        .normalize_with("tags", &json!("甲, 丙, 丁, 丙"), true)
        .unwrap();
    assert_eq!(
        got.new_options
            .iter()
            .map(|o| o.name.as_str())
            .collect::<Vec<_>>(),
        ["丙", "丁"]
    );
    assert_eq!(
        got.value,
        json!(["o1", got.new_options[0].id, got.new_options[1].id])
    );
    assert_ne!(got.new_options[0].id, got.new_options[1].id);
    // 状态：名字、id 都认，不自动加
    assert_eq!(en.normalize("状态", &json!("进行中")).unwrap(), json!("s2"));
    assert_eq!(en.normalize("st", &json!("s3")).unwrap(), json!("s3"));
    let err = en
        .normalize_with("st", &json!("搁置"), true)
        .unwrap_err()
        .msg;
    assert!(err.contains("可选：未开始、进行中、已完成"), "{err}");
    assert_eq!(en.normalize("sel", &json!(["低"])).unwrap(), json!("o2"));
    assert!(
        en.normalize("sel", &json!(["高", "低"]))
            .unwrap_err()
            .msg
            .contains("只能选一个")
    );
    assert!(en.normalize("sel", &json!("HIGH")).is_err());
}

#[test]
fn people_and_dates() {
    let en = e();
    assert_eq!(
        en.normalize("人", &json!(["张三", "writer"])).unwrap(),
        json!(["user:1", "agent:7"])
    );
    assert_eq!(
        en.normalize("who", &json!("agent:7")).unwrap(),
        json!(["agent:7"])
    );
    assert_eq!(
        en.normalize("who", &json!("李四、张三")).unwrap(),
        json!(["user:2", "user:1"])
    );
    assert!(
        en.normalize("who", &json!("不存在的人"))
            .unwrap_err()
            .msg
            .contains("没有叫「不存在的人」的人")
    );
    let d = |v: Value| en.normalize("d", &v).map_err(|x| x.msg);
    assert_eq!(d(json!("2026/11/5")), Ok(json!({"start": "2026-11-05"})));
    assert_eq!(
        d(json!("2026-11-15 9:30")),
        Ok(json!({"start": "2026-11-15T09:30", "time": true}))
    );
    assert_eq!(
        d(json!("2026-11-15 → 2026-11-20")),
        Ok(json!({"start": "2026-11-15", "end": "2026-11-20"}))
    );
    assert_eq!(
        d(json!("2026年11月15日 到 2026年11月20日")),
        Ok(json!({"start": "2026-11-15", "end": "2026-11-20"}))
    );
    assert_eq!(
        d(json!({"start": "today"})),
        Ok(json!({"start": "2026-11-15"}))
    );
    assert_eq!(d(json!("明天")), Ok(json!({"start": "2026-11-16"})));
    assert_eq!(d(json!("yesterday")), Ok(json!({"start": "2026-11-14"})));
    assert_eq!(
        d(json!({"start": "2026-11-15T14:00", "time": true})),
        Ok(json!({"start": "2026-11-15T14:00", "time": true}))
    );
    assert!(d(json!("下周")).unwrap_err().contains("不是「下周」"));
    assert!(
        d(json!({"start": "2026-11-20", "end": "2026-11-01"}))
            .unwrap_err()
            .contains("前面")
    );
    assert!(
        d(json!({"start": "2026-11-20", "end": "坏的"}))
            .unwrap_err()
            .contains("结束日期")
    );
}

#[test]
fn text_like_and_checkbox() {
    assert_eq!(n("好", json!("是")), Ok(json!(true)));
    assert_eq!(n("ok", json!("✓")), Ok(json!(true)));
    assert_eq!(n("ok", json!("否")), Ok(Value::Null)); // 没勾就是空的
    assert_eq!(n("ok", json!(false)), Ok(Value::Null));
    assert_eq!(n("ok", json!(1)), Ok(json!(true)));
    assert!(n("ok", json!("也许")).is_err());
    assert_eq!(
        n("网", json!("example.com")),
        Ok(json!("https://example.com"))
    );
    assert_eq!(n("url", json!("ftp://x.org/a")), Ok(json!("ftp://x.org/a")));
    assert_eq!(n("url", json!("/files/a")), Ok(json!("/files/a")));
    assert!(
        n("信", json!("not-an-email"))
            .unwrap_err()
            .contains("不像邮箱")
    );
    assert_eq!(n("mail", json!(" a@b.co ")), Ok(json!("a@b.co")));
    assert_eq!(n("title", json!("  两个   空格  ")), Ok(json!("两个 空格")));
    assert_eq!(
        n("note", json!("  多行\n\n留着  ")),
        Ok(json!("多行\n\n留着"))
    );
    assert_eq!(n("note", json!(12)), Ok(json!("12")));
    assert!(n("note", json!({"a": 1})).is_err());
    assert!(
        n("note", json!("长".repeat(2001)))
            .unwrap_err()
            .contains("太长")
    );
    assert_eq!(n("tel", json!("138 0000 0000")), Ok(json!("138 0000 0000")));
}

#[test]
fn files_and_readonly() {
    assert_eq!(
        n(
            "附件",
            json!(["/api/files/abc", "https://example.com/a/报告.pdf?x=1", {"url": "https://x.com/y", "name": "  起  名 "}])
        ),
        Ok(
            json!([{"name": "abc", "url": "/api/files/abc"}, {"name": "报告.pdf", "url": "https://example.com/a/报告.pdf?x=1"},
                  {"name": "起 名", "url": "https://x.com/y"}])
        )
    );
    assert!(n("files", json!("ftp://x")).unwrap_err().contains("网址"));
    for k in ["ct", "cb", "uid", "f"] {
        assert!(n(k, json!(1)).unwrap_err().contains("自动算"), "{k}");
    }
    assert!(n("btn", json!(1)).unwrap_err().contains("按钮"));
    assert!(n("没有", json!(1)).unwrap_err().contains("没有属性"));
}

#[test]
fn normalize_row_by_names() {
    let en = e();
    let given = json!({"名称": "任务", "状态": "已完成", "标签": "甲, 新的", "人": "张三", "日": "2026-11-15"});
    let (props, added) = en.normalize_row(given.as_object().unwrap(), true).unwrap();
    assert_eq!(props["title"], json!("任务"));
    assert_eq!(props["st"], json!("s3"));
    assert_eq!(props["who"], json!(["user:1"]));
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].0, "tags");
    assert_eq!(props["tags"], json!(["o1", added[0].1[0].id]));
    let bad = json!({"不存在": 1});
    assert!(
        en.normalize_row(bad.as_object().unwrap(), true)
            .unwrap_err()
            .msg
            .contains("没有属性「不存在」，现在有：名称、数")
    );
}

#[test]
fn display() {
    let mut rows = vec![row("r1", json!({"title": "甲"}))];
    rows[0].unique_id = Some(3);
    let en = engine(schema_all(), rows);
    let d = |k: &str, v: Value| en.display(k, &v);
    assert_eq!(d("n", json!(1200)), "¥1,200");
    assert_eq!(d("n", json!(-2.5)), "-¥2.50");
    assert_eq!(
        d("d", json!({"start": "2026-11-15", "end": "2026-11-20"})),
        "2026-11-15 → 2026-11-20"
    );
    assert_eq!(
        d("d", json!({"start": "2026-11-15T09:05", "time": true})),
        "2026-11-15 09:05"
    );
    assert_eq!(d("dz", json!({"start": "2026-11-15"})), "2026年11月15日");
    assert_eq!(
        d("who", json!(["user:1", "agent:7", "user:99"])),
        "张三, Writer, user:99"
    );
    assert_eq!(d("tags", json!(["o2", "o1", "gone"])), "乙, 甲");
    assert_eq!(d("st", json!("s2")), "进行中");
    assert_eq!(d("ok", json!(true)), "是");
    assert_eq!(d("ok", json!(null)), "");
    assert_eq!(d("files", json!([{"name": "a.png", "url": "/x"}])), "a.png");
    assert_eq!(d("uid", json!(3)), "T-3");
    assert_eq!(d("ct", json!(NOW)), "2026-11-15 10:30");
    assert_eq!(d("cb", json!("user:2")), "李四");
    assert_eq!(d("f", json!(true)), "是");
    assert_eq!(d("f", json!({"start": "2026-11-15"})), "2026-11-15");
    assert_eq!(d("f", json!(["a", "b"])), "a, b");
    assert_eq!(d("btn", json!(1)), "");
    // 每格给人看的样子（空的不列）
    let row = en.display_row("r1");
    assert_eq!(row["title"], json!("甲"));
    assert_eq!(row["uid"], json!("T-3"));
    assert_eq!(row["cb"], json!("张三"));
    assert_eq!(row["f"], json!("1"));
    assert!(!row.contains_key("n"));
}

#[test]
fn number_formats() {
    let mk = |f: &str| {
        let mut x = p("x", "x", Number);
        x.format = Some(f.into());
        engine(schema(vec![p("title", "名称", Title), x]), vec![])
    };
    assert_eq!(mk("comma").display("x", &json!(1234567.5)), "1,234,567.5");
    assert_eq!(mk("percent").display("x", &json!(0.256)), "25.6%");
    assert_eq!(mk("usd").display("x", &json!(1234)), "$1,234");
    assert_eq!(mk("number").display("x", &json!(1.0 / 3.0)), "0.3333333333");
    let mut x = p("x", "x", Number);
    x.precision = Some(2);
    let en = engine(schema(vec![p("title", "名称", Title), x]), vec![]);
    assert_eq!(en.display("x", &json!(3)), "3.00");
}

fn convert(old: Prop, new: Prop, vals: Vec<Value>) -> (Vec<Value>, Vec<String>) {
    let rows = vals
        .iter()
        .enumerate()
        .map(|(i, v)| row(&format!("r{i}"), json!({"x": v})))
        .collect();
    let mut new = new;
    new.key = "x".into();
    let mut old = old;
    old.key = "x".into();
    let en = engine(schema(vec![p("title", "名称", Title), old]), rows);
    let c = en.convert("x", &new).unwrap();
    assert_eq!(
        c.prop.options.len(),
        new.options.len() + c.new_options.len()
    );
    (
        c.values.into_iter().map(|x| x.1).collect(),
        c.new_options.into_iter().map(|o| o.name).collect(),
    )
}

#[test]
fn change_type() {
    // 文字变单选：加了选项
    let (v, added) = convert(
        p("x", "说明", Text),
        p("x", "说明", Select),
        vec![json!("紧急"), json!("不急"), json!("紧急")],
    );
    assert_eq!(added, ["紧急", "不急"]);
    assert_eq!(v[0], v[2]);
    // 多选变单选留第一个；单选变多选
    let tags = with_options(p("x", "标签", MultiSelect), &["a", "b"]);
    let (v, added) = convert(
        tags.clone(),
        with_options(p("x", "标签", Select), &["a", "b"]),
        vec![json!(["o2", "o1"])],
    );
    assert_eq!((v, added.len()), (vec![json!("o2")], 0));
    let (v, _) = convert(
        with_options(p("x", "级", Select), &["高"]),
        with_options(p("x", "级", MultiSelect), &["低", "高"]),
        vec![json!("o1")],
    );
    assert_eq!(v, vec![json!(["o2"])]); // 按名字对上新选项
    // 数字变文字、文字变数字（转不了的清空）
    let (v, _) = convert(
        p("x", "分", Number),
        p("x", "分", Text),
        vec![json!(7), json!(2.5)],
    );
    assert_eq!(v, vec![json!("7"), json!("2.5")]);
    let (v, _) = convert(
        p("x", "分", Text),
        p("x", "分", Number),
        vec![json!("¥1,200"), json!("很多"), json!("50%")],
    );
    assert_eq!(v, vec![json!(1200), Value::Null, json!(0.5)]);
    let mut cny = p("x", "钱", Number);
    cny.format = Some("cny".into());
    let (v, _) = convert(cny, p("x", "钱", Number), vec![json!(1200)]);
    assert_eq!(v, vec![json!(1200)]);
    // 勾选 ↔ 文字
    let (v, _) = convert(
        p("x", "好", Checkbox),
        p("x", "好", Text),
        vec![json!(true)],
    );
    assert_eq!(v, vec![json!("是")]);
    let (v, _) = convert(
        p("x", "好", Text),
        p("x", "好", Checkbox),
        vec![json!("yes"), json!("no"), json!("嗯")],
    );
    assert_eq!(v, vec![json!(true), Value::Null, Value::Null]);
    // 日期 ↔ 文字
    let (v, _) = convert(
        p("x", "日", Date),
        p("x", "日", Text),
        vec![json!({"start": "2026-11-15", "end": "2026-11-20"})],
    );
    assert_eq!(v, vec![json!("2026-11-15 → 2026-11-20")]);
    let (v, _) = convert(
        p("x", "日", Text),
        p("x", "日", Date),
        vec![json!("2026年11月15日"), json!("哪天")],
    );
    assert_eq!(v, vec![json!({"start": "2026-11-15"}), Value::Null]);
    // 人员 → 文字写名字；多选 → 文字
    let (v, _) = convert(
        p("x", "人", Person),
        p("x", "人", Text),
        vec![json!(["user:1", "user:2"])],
    );
    assert_eq!(v, vec![json!("张三, 李四")]);
    let (v, _) = convert(tags, p("x", "标签", Text), vec![json!(["o1", "o2"])]);
    assert_eq!(v, vec![json!("a, b")]);
    // 文字 → 多选按逗号拆
    let (v, added) = convert(
        p("x", "t", Text),
        p("x", "t", MultiSelect),
        vec![json!("x, y")],
    );
    assert_eq!(added, ["x", "y"]);
    assert_eq!(v[0].as_array().unwrap().len(), 2);
    // 变成算出来的：全清
    let (v, _) = convert(p("x", "t", Text), formula("x", "t", "1"), vec![json!("a")]);
    assert_eq!(v, vec![Value::Null]);
    // 一样的类型原样
    let (v, _) = convert(p("x", "t", Text), p("x", "t2", Text), vec![json!("a")]);
    assert_eq!(v, vec![json!("a")]);
}
