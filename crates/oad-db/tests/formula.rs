//! 公式：语法、优先级、报错（位置）、每个函数、方法写法、let、按行算和缓存。
mod common;
use common::*;
use oad_db::PropType::*;
use oad_db::formula;
use serde_json::{Value, json};

fn base() -> Vec<oad_db::Prop> {
    vec![
        p("title", "名称", Title),
        p("price", "单价", Number),
        p("qty", "数量", Number),
        p("done", "完成", Checkbox),
        p("due", "截止", Date),
        with_options(p("tags", "标签", MultiSelect), &["甲", "乙", "丙"]),
        status("st", "状态"),
        p("note", "备注", Text),
        p("who", "负责人", Person),
        p("empty", "空数", Number),
        with_options(p("sel", "级别", Select), &["高", "低"]),
    ]
}

fn row1() -> oad_db::RowData {
    row(
        "r1",
        json!({"title": "甲", "price": 2.5, "qty": 4, "due": {"start": "2026-11-20"}, "tags": ["o1", "o2"], "st": "s2",
               "note": "Hello World", "who": ["user:1", "agent:7"], "sel": "o1"}),
    )
}

/// 在一行上算一个公式。
fn f(src: &str) -> Value {
    let mut props = base();
    props.push(formula("f", "式", src));
    let e = engine(schema(props), vec![row1()]);
    assert!(
        e.schema_errors().is_empty(),
        "{src}: {:?}",
        e.schema_errors()
    );
    e.computed("r1").get("f").cloned().unwrap_or(Value::Null)
}

/// 公式写错的报错。
fn err(src: &str) -> String {
    let e = engine(schema(base()), vec![]);
    let x = e.check_formula(src, None).expect_err(src);
    x.describe(src.trim_end())
}

fn err_at(src: &str) -> usize {
    let e = engine(schema(base()), vec![]);
    e.check_formula(src, None).expect_err(src).char_pos(src) - 1
}

#[test]
fn literals_and_arith() {
    assert_eq!(f("1 + 2 * 3"), json!(7));
    assert_eq!(f("(1 + 2) * 3"), json!(9));
    assert_eq!(f("2 ^ 3 ^ 2"), json!(512)); // 右结合
    assert_eq!(f("-2 ^ 2"), json!(-4));
    assert_eq!(f("-2 * 3"), json!(-6));
    assert_eq!(f("10 / 4"), json!(2.5));
    assert_eq!(f("10 % 4"), json!(2));
    assert_eq!(f("-7 % 3"), json!(-1));
    assert_eq!(f("0.1 + 0.2"), json!(0.3));
    assert_eq!(f("1e3 + 1"), json!(1001));
    assert_eq!(f("1 / 0"), Value::Null);
    assert_eq!(f("5 % 0"), Value::Null);
    assert_eq!(f("2 ^ 2000"), Value::Null); // 次方太大
    assert_eq!(f("(-8) ^ 0.5"), Value::Null); // 不是一个数
    assert_eq!(f("\"a\" + 1"), json!("a1"));
    assert_eq!(
        f("'单引号' + \"双\" + “中文” + 「直角」"),
        json!("单引号双中文直角")
    );
    assert_eq!(f(r#""换\n行\"""#), json!("换\n行\""));
    assert_eq!(f("true + 1"), json!(2));
    assert_eq!(f("+5"), json!(5));
    assert_eq!(f("prop(\"空数\") + 1"), Value::Null);
    assert_eq!(f("prop(\"单价\") * prop(\"数量\")"), json!(10));
    assert_eq!(f("（1 + 2）× 3"), json!(9)); // 全角括号、乘号
}

#[test]
fn logic_and_compare() {
    assert_eq!(f("1 < 2 and 2 < 3"), json!(true));
    assert_eq!(f("1 > 2 or 2 > 3"), json!(false));
    assert_eq!(f("1 > 2 || 3 > 2"), json!(true));
    assert_eq!(f("true && false"), json!(false));
    assert_eq!(f("not true"), json!(false));
    assert_eq!(f("!(1 == 2)"), json!(true));
    assert_eq!(f("1 = 1"), json!(true));
    assert_eq!(f("1 != 1"), json!(false));
    assert_eq!(f("1 ≠ 2"), json!(true));
    assert_eq!(f("\"b\" > \"a\""), json!(true));
    assert_eq!(f("\"10\" > \"9\""), json!(true)); // 两边都像数按数比
    assert_eq!(f("1 == \"1\""), json!(true));
    assert_eq!(f("true == 1"), json!(true));
    assert_eq!(f("prop(\"截止\") > parseDate(\"2026-11-15\")"), json!(true));
    assert_eq!(f("prop(\"截止\") == \"2026-11-20\""), json!(true));
    assert_eq!(f("prop(\"截止\") == prop(\"空数\")"), json!(false));
    assert_eq!(f("prop(\"截止\") != prop(\"空数\")"), json!(true));
    assert_eq!(f("1 < 2 == true"), json!(true)); // 比较比等于紧
    assert_eq!(f("1 + 1 > 1 and 2 * 2 == 4"), json!(true));
    assert_eq!(f("prop(\"完成\") ? \"是\" : \"否\""), json!("否"));
    assert_eq!(f("1 > 2 ? \"a\" : 2 > 1 ? \"b\" : \"c\""), json!("b"));
    assert_eq!(f("if(prop(\"数量\") > 3, \"多\", \"少\")"), json!("多"));
    assert_eq!(
        f("ifs(prop(\"数量\") > 10, \"很多\", prop(\"数量\") > 3, \"多\", \"少\")"),
        json!("多")
    );
    assert_eq!(f("ifs(false, 1, false, 2)"), Value::Null);
    assert_eq!(f("and(true, 1, \"x\")"), json!(true));
    assert_eq!(f("or(false, 0, \"\")"), json!(false));
    assert_eq!(f("not(prop(\"备注\"))"), json!(false));
    assert_eq!(
        f("empty(prop(\"空数\")) and empty(0) and empty(\"\") and not empty(prop(\"标签\"))"),
        json!(true)
    );
    // and / or 短路：后面除以 0 不算
    assert_eq!(f("false and 1 / 0 > 1"), json!(false));
}

#[test]
fn text_functions() {
    assert_eq!(f("length(prop(\"备注\"))"), json!(11));
    assert_eq!(f("length(\"中文😀\")"), json!(3));
    assert_eq!(f("length(prop(\"标签\"))"), json!(2));
    assert_eq!(f("concat(\"a\", 1, true)"), json!("a1true"));
    assert_eq!(f("concat([1], [2, 3])"), json!(["1", "2", "3"]));
    assert_eq!(f("join(prop(\"标签\"), \"/\")"), json!("甲/乙"));
    assert_eq!(f("join(prop(\"标签\"))"), json!("甲乙"));
    assert_eq!(f("contains(prop(\"备注\"), \"World\")"), json!(true));
    assert_eq!(f("contains(prop(\"标签\"), \"乙\")"), json!(true));
    assert_eq!(f("contains(prop(\"标签\"), \"丙\")"), json!(false));
    assert_eq!(f("includes(prop(\"标签\"), \"甲\")"), json!(true));
    assert_eq!(f("replace(\"a-b-c\", \"-\", \"+\")"), json!("a+b+c"));
    assert_eq!(f("replaceAll(\"aaa\", \"a\", \"b\")"), json!("bbb"));
    assert_eq!(f("lower(\"AbC\") + upper(\"x\")"), json!("abcX"));
    assert_eq!(f("trim(\"  x  \")"), json!("x"));
    assert_eq!(f("slice(\"abcdef\", 1, 3)"), json!("bc"));
    assert_eq!(f("slice(\"abcdef\", -2)"), json!("ef"));
    assert_eq!(f("slice(\"abc\", 5)"), Value::Null);
    assert_eq!(f("substring(\"中文字符\", 1, 3)"), json!("文字"));
    assert_eq!(f("slice([1, 2, 3], 1)"), json!(["2", "3"]));
    assert_eq!(f("split(\"a,b,c\", \",\")"), json!(["a", "b", "c"]));
    assert_eq!(f("split(\"a b  c\")"), json!(["a", "b", "c"]));
    assert_eq!(f("split(\"中文\", \"\")"), json!(["中", "文"]));
    assert_eq!(f("repeat(\"ab\", 3)"), json!("ababab"));
    assert_eq!(
        f("startsWith(\"hello\", \"he\") and endsWith(\"hello\", \"lo\")"),
        json!(true)
    );
    assert_eq!(f("padStart(\"7\", 3, \"0\")"), json!("007"));
    assert_eq!(f("padEnd(\"7\", 3)"), json!("7  "));
    assert_eq!(f("toNumber(\"1,234.5\")"), json!(1234.5));
    assert_eq!(f("toNumber(\"abc\")"), Value::Null);
    assert_eq!(f("toNumber(true)"), json!(1));
    assert_eq!(f("toText(1.5) + toText(true)"), json!("1.5true"));
    assert_eq!(f("format(10 / 3)"), json!("3.333333333"));
    assert_eq!(f("toText(prop(\"截止\"))"), json!("2026-11-20"));
    assert_eq!(f("toText(prop(\"标签\"))"), json!("甲, 乙"));
    assert_eq!(
        f("\"还差 \" + toText(10 - prop(\"数量\"))"),
        json!("还差 6")
    );
    assert_eq!(f("prop(\"状态\")"), json!("进行中"));
    assert_eq!(f("prop(\"级别\") + \"级\""), json!("高级"));
    assert_eq!(f("prop(\"负责人\")"), json!(["张三", "Writer"]));
    assert_eq!(f("prop(\"名称\")"), json!("甲"));
    assert_eq!(f("prop(\"空数\")"), Value::Null);
}

#[test]
fn math_functions() {
    assert_eq!(f("round(2.5)"), json!(3));
    assert_eq!(f("round(-2.5)"), json!(-3)); // 四舍五入，不是银行家
    assert_eq!(f("round(1.23456, 2)"), json!(1.23));
    assert_eq!(f("round(1234, -2)"), json!(1200));
    assert_eq!(f("floor(2.7) + ceil(2.1)"), json!(5));
    assert_eq!(f("abs(-3)"), json!(3));
    assert_eq!(f("sqrt(16)"), json!(4));
    assert_eq!(f("sqrt(-1)"), Value::Null);
    assert_eq!(f("cbrt(27)"), json!(3));
    assert_eq!(f("pow(2, 10)"), json!(1024));
    assert_eq!(f("mod(7, 3)"), json!(1));
    assert_eq!(f("sign(-5) + sign(0) + sign(9)"), json!(0));
    assert_eq!(f("ln(e())"), json!(1));
    assert_eq!(f("exp(0)"), json!(1));
    assert_eq!(f("log10(1000)"), json!(3));
    assert_eq!(f("log2(8)"), json!(3));
    assert_eq!(f("ln(0)"), Value::Null);
    assert_eq!(f("pi() > 3.1415 and pi() < 3.1416"), json!(true));
    assert_eq!(f("min(3, 1, 2)"), json!(1));
    assert_eq!(f("max(3, \"7\", 2)"), json!(7));
    assert_eq!(f("min(\"x\")"), Value::Null);
    assert_eq!(f("sum(1, 2, [3, 4])"), json!(10));
    assert_eq!(f("sum(\"x\")"), json!(0));
    assert_eq!(f("average(1, 2, 3, 4)"), json!(2.5));
    assert_eq!(f("mean([2, 4])"), json!(3));
    assert_eq!(f("median(5, 1, 3)"), json!(3));
    assert_eq!(f("median(1, 2, 3, 4)"), json!(2.5));
    assert_eq!(f("average(\"x\")"), Value::Null);
}

#[test]
fn date_functions() {
    // 现在是 2026-11-15 10:30（北京时间）
    assert_eq!(
        f("now()"),
        json!({"start": "2026-11-15T10:30", "time": true})
    );
    assert_eq!(f("today()"), json!({"start": "2026-11-15"}));
    assert_eq!(
        f("dateAdd(prop(\"截止\"), 2, \"days\")"),
        json!({"start": "2026-11-22"})
    );
    assert_eq!(
        f("dateAdd(prop(\"截止\"), 1, \"months\")"),
        json!({"start": "2026-12-20"})
    );
    assert_eq!(
        f("dateAdd(parseDate(\"2026-01-31\"), 1, \"月\")"),
        json!({"start": "2026-02-28"})
    );
    assert_eq!(
        f("dateAdd(prop(\"截止\"), 1, \"年\")"),
        json!({"start": "2027-11-20"})
    );
    assert_eq!(
        f("dateAdd(prop(\"截止\"), 1, \"quarters\")"),
        json!({"start": "2027-02-20"})
    );
    assert_eq!(
        f("dateAdd(prop(\"截止\"), 2, \"weeks\")"),
        json!({"start": "2026-12-04"})
    );
    assert_eq!(
        f("dateAdd(prop(\"截止\"), 90, \"minutes\")"),
        json!({"start": "2026-11-20T01:30", "time": true})
    );
    assert_eq!(
        f("dateAdd(prop(\"截止\"), 1, \"小时\")"),
        json!({"start": "2026-11-20T01:00", "time": true})
    );
    assert_eq!(f("dateAdd(prop(\"截止\"), 1, \"fortnights\")"), Value::Null);
    assert_eq!(
        f("dateSubtract(prop(\"截止\"), 5, \"days\")"),
        json!({"start": "2026-11-15"})
    );
    assert_eq!(
        f("dateBetween(prop(\"截止\"), parseDate(\"2026-11-15\"), \"days\")"),
        json!(5)
    );
    assert_eq!(
        f("dateBetween(parseDate(\"2026-11-15\"), prop(\"截止\"), \"days\")"),
        json!(-5)
    );
    assert_eq!(
        f("dateBetween(parseDate(\"2026-03-10\"), parseDate(\"2026-01-20\"), \"months\")"),
        json!(1)
    );
    assert_eq!(
        f("dateBetween(parseDate(\"2026-01-20\"), parseDate(\"2026-03-10\"), \"months\")"),
        json!(-1)
    );
    assert_eq!(
        f("dateBetween(parseDate(\"2028-02-29\"), parseDate(\"2026-03-01\"), \"years\")"),
        json!(1)
    );
    assert_eq!(
        f(
            "dateBetween(parseDate(\"2026-11-15 18:00\"), parseDate(\"2026-11-15 08:30\"), \"hours\")"
        ),
        json!(9)
    );
    assert_eq!(
        f("dateBetween(prop(\"截止\"), today(), \"weeks\")"),
        json!(0)
    );
    assert_eq!(
        f("dateBetween(prop(\"空数\"), today(), \"days\")"),
        Value::Null
    );
    assert_eq!(f("prop(\"截止\") - parseDate(\"2026-11-19\")"), json!(1));
    assert_eq!(
        f("formatDate(prop(\"截止\"), \"YYYY年M月D日\")"),
        json!("2026年11月20日")
    );
    assert_eq!(
        f("formatDate(now(), \"HH:mm dddd\")"),
        json!("10:30 星期日")
    );
    assert_eq!(f("formatDate(prop(\"截止\"))"), json!("2026-11-20"));
    assert_eq!(f("formatDate(prop(\"空数\"))"), Value::Null);
    assert_eq!(
        f("parseDate(\"2026年11月15日\")"),
        json!({"start": "2026-11-15"})
    );
    assert_eq!(f("parseDate(\"不是日期\")"), Value::Null);
    assert_eq!(
        f("year(prop(\"截止\")) * 10000 + month(prop(\"截止\")) * 100 + day(prop(\"截止\"))"),
        json!(20261120)
    );
    assert_eq!(f("date(prop(\"截止\"))"), json!(20));
    assert_eq!(f("weekday(prop(\"截止\"))"), json!(5)); // 周五
    assert_eq!(f("week(prop(\"截止\"))"), json!(47));
    assert_eq!(f("hour(now()) * 100 + minute(now())"), json!(1030));
    assert_eq!(f("timestamp(now())"), json!(NOW));
    assert_eq!(
        f("fromTimestamp(timestamp(now()))"),
        json!({"start": "2026-11-15T10:30", "time": true})
    );
    assert_eq!(
        f("dateRange(today(), prop(\"截止\"))"),
        json!({"start": "2026-11-15", "end": "2026-11-20"})
    );
    assert_eq!(
        f("dateEnd(dateRange(today(), prop(\"截止\")))"),
        json!({"start": "2026-11-20"})
    );
    assert_eq!(
        f("dateStart(dateRange(today(), prop(\"截止\")))"),
        json!({"start": "2026-11-15"})
    );
    assert_eq!(f("prop(\"截止\") + 1"), Value::Null); // 日期要用 dateAdd
}

#[test]
fn lists_methods_and_let() {
    assert_eq!(f("prop(\"标签\").length()"), json!(2));
    assert_eq!(f("prop(\"备注\").lower().contains(\"world\")"), json!(true));
    assert_eq!(f("prop(\"标签\").join(\"+\").upper()"), json!("甲+乙"));
    assert_eq!(f("(1 + 2).toText().length()"), json!(1));
    assert_eq!(f("-prop(\"单价\").abs()"), json!(-2.5));
    assert_eq!(f("[3, 1, 2].sort()"), json!(["1", "2", "3"]));
    assert_eq!(f("[\"b\", \"a\"].sort().first()"), json!("a"));
    assert_eq!(f("[1, 2, 3].last()"), json!(3));
    assert_eq!(f("[1, 2, 3].at(-1)"), json!(3));
    assert_eq!(f("[1, 2, 3].at(5)"), Value::Null);
    assert_eq!(f("[1, 2, 3].reverse()"), json!(["3", "2", "1"]));
    assert_eq!(f("[1, 1, 2].unique()"), json!(["1", "2"]));
    assert_eq!(f("[[1, 2], [3]].flat().length()"), json!(3));
    assert_eq!(f("[].length()"), json!(0));
    assert_eq!(f("[]"), Value::Null);
    assert_eq!(f("[1, 2, 3].map(current * 2)"), json!(["2", "4", "6"]));
    assert_eq!(f("map([1, 2, 3], current + index)"), json!(["1", "3", "5"]));
    assert_eq!(
        f("[1, 2, 3, 4].filter(current % 2 == 0)"),
        json!(["2", "4"])
    );
    assert_eq!(f("[1, 2, 3].find(current > 1)"), json!(2));
    assert_eq!(f("[1, 2, 3].find(current > 5)"), Value::Null);
    assert_eq!(f("[1, 2, 3].findIndex(current > 1)"), json!(1));
    assert_eq!(f("[1, 2, 3].findIndex(current > 5)"), json!(-1));
    assert_eq!(f("[1, 2, 3].some(current > 2)"), json!(true));
    assert_eq!(f("[1, 2, 3].every(current > 2)"), json!(false));
    assert_eq!(
        f("prop(\"标签\").map(current + \"!\").join()"),
        json!("甲!乙!")
    );
    assert_eq!(
        f("[[1, 2], [3]].map(current.map(current * 10).sum()).sum()"),
        json!(60)
    ); // current 是最里面那层的
    assert_eq!(f("let(x, 2, x * x)"), json!(4));
    assert_eq!(f("let(x, prop(\"数量\"), let(y, x + 1, x * y))"), json!(20));
    assert_eq!(f("lets(a, 1, b, a + 1, c, b * 10, a + b + c)"), json!(23));
    assert_eq!(f("let(x, 1, let(x, 2, x) + x)"), json!(3)); // 里面的 x 盖住外面的
    assert_eq!(
        f("let(总价, prop(\"单价\") * prop(\"数量\"), 总价 > 5 ? \"贵\" : \"便宜\")"),
        json!("贵")
    );
    assert_eq!(f("LENGTH(\"ab\") + Length(\"c\")"), json!(3)); // 函数名不分大小写
    assert_eq!(f("prop(\"状态\") == \"进行中\""), json!(true));
    assert_eq!(f("PROP(\"数量\")"), json!(4));
    assert_eq!(f("prop(\" 数量 \")"), json!(4)); // 属性名两边的空格不算
}

#[test]
fn errors_say_where() {
    assert_eq!(err("1 +"), "公式第 4 个字：公式没写完");
    assert!(err("(1 + 2").contains("少了「)」"));
    assert!(err("1 + 2)").contains("多了「)」"));
    assert!(err("prop(\"没有\")").contains("没有属性「没有」，现在有：名称、单价"));
    assert!(err("foo(1)").contains("没有函数「foo」"));
    assert!(err("lenght(1)").contains("是不是 length"));
    assert!(err("round()").contains("round 的参数个数不对（round(数, 小数位)）"));
    assert!(err("if(1, 2)").contains("if 的参数个数不对"));
    assert!(err("数量 + 1").contains("「数量」不是属性也不是变量：取属性的值写 prop(\"数量\")"));
    assert!(err("prop(1)").contains("prop 里写属性名"));
    assert!(err("\"没结束").contains("引号没有配对"));
    assert!(err("1 # 2").contains("这里看不懂：# 2"));
    assert!(err("let(1, 2, 3)").contains("变量名要写成名字"));
    assert!(err("lets(a, 1, b, 2)").contains("参数个数不对"));
    assert!(err("x.length()").contains("「x」不是属性也不是变量"));
    assert!(err("\"a\".nope()").contains("没有函数「nope」"));
    assert!(err("\"a\".length").contains("后面要跟括号"));
    assert!(err("current + 1").contains("不是属性也不是变量")); // current 只在 map 这些里面有
    assert!(err("").contains("公式是空的"));
    assert!(err(&"1+".repeat(600)).contains("公式太长"));
    assert!(err(&format!("{}1{}", "(".repeat(50), ")".repeat(50))).contains("嵌套太深"));
    assert!(err("1 ? 2").contains("少了「:」"));
    // 位置（第几个字，从 0 起）
    assert_eq!(err_at("1 + foo(2)"), 4);
    assert_eq!(err_at("prop(\"数量\") + prop(\"没有\")"), 18);
    assert_eq!(err_at("中文 + 1"), 0);
}

#[test]
fn wasm_style_error_position_is_utf16() {
    let e = engine(schema(base()), vec![]);
    let src = "\"😀\" + foo(1)";
    let x = e.check_formula(src, None).unwrap_err();
    assert_eq!(x.utf16_pos(src), 7); // 😀 占两个
    assert_eq!(x.char_pos(src), 7);
}

#[test]
fn self_reference_and_cycles() {
    let mut props = base();
    props.push(formula("a", "甲式", "1"));
    props.push(formula("b", "乙式", "prop(\"甲式\") + 1"));
    let e = engine(schema(props.clone()), vec![row1()]);
    assert_eq!(e.computed("r1")["b"], json!(2));
    assert!(
        e.check_formula("prop(\"乙式\")", Some("a"))
            .unwrap_err()
            .msg
            .contains("成了环")
    );
    assert!(
        e.check_formula("prop(\"甲式\") * 2", Some("a"))
            .unwrap_err()
            .msg
            .contains("它自己")
    );
    assert!(e.check_formula("prop(\"乙式\")", Some("price")).is_ok());
    // 表结构里已经成环：两个都标成错的，值是空的
    props[11] = formula("a", "甲式", "prop(\"乙式\")");
    let e = engine(schema(props), vec![row1()]);
    let errs = e.schema_errors();
    assert_eq!(errs.len(), 2);
    assert!(errs[0].msg.contains("成了环"));
    assert_eq!(e.computed("r1")["a"], Value::Null);
    assert_eq!(e.computed("r1")["b"], Value::Null);
}

#[test]
fn chained_formulas_and_cache() {
    let mut props = base();
    props.push(formula("total", "金额", "prop(\"单价\") * prop(\"数量\")"));
    props.push(formula(
        "tag",
        "标记",
        "if(prop(\"完成\"), \"✓\", \"还差 \" + toText(10 - prop(\"数量\")))",
    ));
    props.push(formula("big", "大单", "prop(\"金额\") > 5"));
    let mut e = engine(
        schema(props),
        vec![
            row1(),
            row(
                "r2",
                json!({"title": "乙", "price": 10, "qty": 1, "done": true}),
            ),
        ],
    );
    assert_eq!(e.computed("r1")["total"], json!(10));
    assert_eq!(e.computed("r1")["tag"], json!("还差 6"));
    assert_eq!(e.computed("r2")["tag"], json!("✓"));
    assert_eq!(e.computed("r1")["big"], json!(true));
    // 改了才重算
    let changed = e.apply(vec![
        serde_json::from_value(json!({"row_id": "r1", "props": {"qty": 1}})).unwrap(),
    ]);
    assert_eq!(changed, vec!["r1"]);
    assert_eq!(e.computed("r1")["total"], json!(2.5));
    assert_eq!(e.computed("r1")["big"], json!(false));
    assert_eq!(e.computed("r2")["total"], json!(10));
}

#[test]
fn clock_formulas_follow_time() {
    let mut props = base();
    props.push(formula(
        "left",
        "剩几天",
        "dateBetween(prop(\"截止\"), today(), \"days\")",
    ));
    let mut e = engine(schema(props), vec![row1()]);
    assert_eq!(e.computed("r1")["left"], json!(5));
    e.set_clock(oad_db::Clock::new(NOW + 86_400_000, 480));
    assert_eq!(e.computed("r1")["left"], json!(4));
}

#[test]
fn refs_and_rename() {
    assert_eq!(
        formula::refs("prop(\"a\") + prop(\"B\") * prop(\"a\")").unwrap(),
        vec!["a", "B"]
    );
    assert!(formula::refs("1 +").is_err());
    assert_eq!(
        formula::rename_prop(
            "prop(\"数量\") * 2 + PROP( \"数量\" ) + \"数量\"",
            "数量",
            "件数"
        ),
        "prop(\"件数\") * 2 + PROP( \"件数\" ) + \"数量\""
    );
    assert_eq!(
        formula::rename_prop("prop('a')", "a", "x\"y"),
        "prop(\"x\\\"y\")"
    );
    assert_eq!(formula::rename_prop("1 + #", "a", "b"), "1 + #");
}

#[test]
fn every_function_has_an_example() {
    // 每个函数都有一个能算的例子（新加函数忘了测这里会失败）
    let examples = [
        ("if", "if(true, 1, 2)", json!(1)),
        ("ifs", "ifs(false, 1, 2)", json!(2)),
        ("and", "and(true, true)", json!(true)),
        ("or", "or(false, true)", json!(true)),
        ("not", "not(false)", json!(true)),
        ("empty", "isEmpty(\"\")", json!(true)),
        ("let", "let(x, 1, x)", json!(1)),
        ("lets", "lets(x, 1, y, 2, x + y)", json!(3)),
        ("length", "len(\"abc\")", json!(3)),
        ("concat", "concat(\"a\", \"b\")", json!("ab")),
        ("join", "join([1, 2], \"-\")", json!("1-2")),
        ("contains", "contains(\"abc\", \"b\")", json!(true)),
        ("replace", "replace(\"abc\", \"b\", \"x\")", json!("axc")),
        ("lower", "lower(\"A\")", json!("a")),
        ("upper", "upper(\"a\")", json!("A")),
        ("trim", "trim(\" a \")", json!("a")),
        ("slice", "slice(\"abc\", 1)", json!("bc")),
        ("split", "split(\"a b\")", json!(["a", "b"])),
        ("repeat", "repeat(\"a\", 2)", json!("aa")),
        ("startsWith", "startsWith(\"ab\", \"a\")", json!(true)),
        ("endsWith", "endsWith(\"ab\", \"a\")", json!(false)),
        ("padStart", "padStart(\"1\", 2, \"0\")", json!("01")),
        ("padEnd", "padEnd(\"1\", 2, \"0\")", json!("10")),
        ("toNumber", "toNumber(\"2\")", json!(2)),
        ("toText", "toText(2)", json!("2")),
        ("round", "round(1.5)", json!(2)),
        ("floor", "floor(1.5)", json!(1)),
        ("ceil", "ceil(1.5)", json!(2)),
        ("abs", "abs(-1)", json!(1)),
        ("sqrt", "sqrt(4)", json!(2)),
        ("cbrt", "cbrt(8)", json!(2)),
        ("pow", "pow(2, 3)", json!(8)),
        ("mod", "mod(5, 3)", json!(2)),
        ("sign", "sign(-3)", json!(-1)),
        ("exp", "round(exp(2), 2)", json!(7.39)),
        ("ln", "ln(1)", json!(0)),
        ("log10", "log10(100)", json!(2)),
        ("log2", "log2(4)", json!(2)),
        ("pi", "floor(pi())", json!(3)),
        ("e", "floor(e())", json!(2)),
        ("min", "min(1, 2)", json!(1)),
        ("max", "max(1, 2)", json!(2)),
        ("sum", "add(1, 2)", json!(3)),
        ("average", "avg(1, 3)", json!(2)),
        ("median", "median(1, 9, 3)", json!(3)),
        ("now", "minute(now())", json!(30)),
        ("today", "day(today())", json!(15)),
        ("dateAdd", "day(dateAdd(today(), 1, \"days\"))", json!(16)),
        (
            "dateSubtract",
            "day(dateSubtract(today(), 1, \"天\"))",
            json!(14),
        ),
        (
            "dateBetween",
            "dateBetween(today(), today(), \"days\")",
            json!(0),
        ),
        (
            "dateRange",
            "dateBetween(dateEnd(dateRange(today(), dateAdd(today(), 3, \"days\"))), today(), \"days\")",
            json!(3),
        ),
        ("dateStart", "day(start(today()))", json!(15)),
        ("dateEnd", "day(end(today()))", json!(15)),
        (
            "formatDate",
            "formatDate(today(), \"MM/DD\")",
            json!("11/15"),
        ),
        ("parseDate", "year(parseDate(\"2030-1-2\"))", json!(2030)),
        ("year", "year(today())", json!(2026)),
        ("month", "month(today())", json!(11)),
        ("day", "day(today())", json!(15)),
        ("weekday", "weekday(today())", json!(7)),
        ("week", "week(today())", json!(46)),
        ("hour", "hour(now())", json!(10)),
        ("minute", "minute(now())", json!(30)),
        (
            "timestamp",
            "timestamp(today()) < timestamp(now())",
            json!(true),
        ),
        ("fromTimestamp", "year(fromTimestamp(0))", json!(1970)),
        ("first", "first([1, 2])", json!(1)),
        ("last", "last([1, 2])", json!(2)),
        ("at", "at([1, 2], 1)", json!(2)),
        ("reverse", "reverse([1, 2]).first()", json!(2)),
        ("sort", "sort([2, 1]).first()", json!(1)),
        ("unique", "unique([1, 1]).length()", json!(1)),
        ("flat", "flat([[1], [2]]).length()", json!(2)),
        ("map", "map([1], current + 1).first()", json!(2)),
        ("filter", "filter([1, 2], current > 1).first()", json!(2)),
        ("find", "find([1, 2], current > 1)", json!(2)),
        ("findIndex", "findIndex([1, 2], current > 1)", json!(1)),
        ("some", "some([1, 2], current > 1)", json!(true)),
        ("every", "every([1, 2], current > 1)", json!(false)),
    ];
    for info in formula::FUNCS {
        assert!(
            examples.iter().any(|(n, _, _)| *n == info.name),
            "{} 没有例子",
            info.name
        );
    }
    for (name, src, want) in examples {
        assert_eq!(f(src), want, "{name}: {src}");
    }
}
