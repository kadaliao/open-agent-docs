//! 查询：筛选（可嵌套的且 / 或）、搜索、排序（几列，空的总在最后）、分组（带子分组）、每列底部的汇总。
use crate::DbError;
use crate::aggregate::{Agg, aggregate};
use crate::date::{self, DateTime};
use crate::engine::{Engine, v_json};
use crate::formula::V;
use crate::schema::{PropType, View};
use crate::value::Cell;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::cmp::Ordering;

/// 查询结果。`ids` 按排序（给了 limit / offset 是那一段），`total` 是符合条件的总行数。
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct QueryResult {
    pub ids: Vec<String>,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<Group>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totals: Option<Map<String, Value>>,
}

/// 一个分组。空值的分组 `key` 是 ""，排在最后。
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Group {
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub ids: Vec<String>,
    pub count: usize,
    /// 子分组（视图有 sub_group_by 时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<Group>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totals: Option<Map<String, Value>>,
}

// ---------- 筛选 ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cond {
    Eq,
    Neq,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    Gt,
    Gte,
    Lt,
    Lte,
    Empty,
    NotEmpty,
    Within,
    Checked,
    Unchecked,
    InGroup,
}

const CONDS: &[(Cond, &str, &[&str])] = &[
    (Cond::Eq, "eq", &["is", "=", "==", "equals", "等于", "是"]),
    (
        Cond::Neq,
        "neq",
        &["is_not", "!=", "ne", "not_equals", "不等于", "不是"],
    ),
    (Cond::Contains, "contains", &["has", "包含", "含"]),
    (
        Cond::NotContains,
        "not_contains",
        &["does_not_contain", "不包含"],
    ),
    (Cond::StartsWith, "starts_with", &["开头是"]),
    (Cond::EndsWith, "ends_with", &["结尾是"]),
    (
        Cond::Gt,
        "gt",
        &[">", "after", "greater_than", "大于", "晚于"],
    ),
    (
        Cond::Gte,
        "gte",
        &[">=", "on_or_after", "大于等于", "不早于"],
    ),
    (
        Cond::Lt,
        "lt",
        &["<", "before", "less_than", "小于", "早于"],
    ),
    (
        Cond::Lte,
        "lte",
        &["<=", "on_or_before", "小于等于", "不晚于"],
    ),
    (Cond::Empty, "empty", &["is_empty", "为空", "空"]),
    (
        Cond::NotEmpty,
        "not_empty",
        &["is_not_empty", "不为空", "不空"],
    ),
    (Cond::Within, "is_within", &["within", "在范围内"]),
    (Cond::Checked, "checked", &["已勾选"]),
    (Cond::Unchecked, "unchecked", &["未勾选"]),
    (Cond::InGroup, "in_group", &["group", "状态组"]),
];

impl Cond {
    fn parse(s: &str) -> Option<Cond> {
        let s = s.trim().to_lowercase().replace(' ', "_");
        CONDS
            .iter()
            .find(|(_, k, al)| *k == s || al.contains(&s.as_str()))
            .map(|x| x.0)
    }
    fn key(self) -> &'static str {
        CONDS.iter().find(|x| x.0 == self).unwrap().1
    }
    fn needs_value(self) -> bool {
        !matches!(
            self,
            Cond::Empty | Cond::NotEmpty | Cond::Checked | Cond::Unchecked
        )
    }
    fn negated(self) -> bool {
        matches!(self, Cond::Neq | Cond::NotContains)
    }
}

/// 每种类型能用的条件。
fn allowed(kind: PropType) -> &'static [Cond] {
    use Cond::*;
    use PropType::*;
    match kind {
        Title | Text | Url | Email | Phone => &[
            Eq,
            Neq,
            Contains,
            NotContains,
            StartsWith,
            EndsWith,
            Gt,
            Gte,
            Lt,
            Lte,
            Empty,
            NotEmpty,
        ],
        Number | UniqueId => &[Eq, Neq, Gt, Gte, Lt, Lte, Empty, NotEmpty],
        Select => &[Eq, Neq, Contains, NotContains, Empty, NotEmpty],
        Status => &[Eq, Neq, Contains, NotContains, InGroup, Empty, NotEmpty],
        MultiSelect | Person | CreatedBy | UpdatedBy | Relation | Files => {
            &[Contains, NotContains, Eq, Neq, Empty, NotEmpty]
        }
        Checkbox => &[Checked, Unchecked, Eq, Neq],
        Date | CreatedTime | UpdatedTime => &[Eq, Neq, Gt, Gte, Lt, Lte, Within, Empty, NotEmpty],
        Formula | Rollup => &[
            Eq,
            Neq,
            Contains,
            NotContains,
            StartsWith,
            EndsWith,
            Gt,
            Gte,
            Lt,
            Lte,
            Empty,
            NotEmpty,
            Within,
            Checked,
            Unchecked,
        ],
        Button | PropType::Unknown => &[],
    }
}

enum Test {
    Empty(bool),
    Text(Cond, String),
    Num(Cond, f64),
    /// 选项序号（选中算命中）
    Opts(Vec<bool>, bool),
    Ids(FxHashSet<String>, bool),
    /// 关联：标题或 id；第二个是“包含”（不是“等于”）
    Rel(String, bool, bool),
    Files(String, bool),
    Check(bool),
    /// 日期（按天）：条件、[开始, 结束]
    Date(Cond, i64, i64),
    Calc(Cond, CalcTarget),
}

struct CalcTarget {
    text: String,
    num: Option<f64>,
    day: Option<(i64, i64)>,
    bool: Option<bool>,
}

enum Node {
    And(Vec<Node>),
    Or(Vec<Node>),
    True,
    Leaf(usize, Test),
}

fn blank(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => crate::num::text(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => b.to_string(),
        Value::Object(o) => o
            .get("start")
            .or(o.get("name"))
            .map(scalar)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn list(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a.iter().map(scalar).filter(|s| !s.is_empty()).collect(),
        Value::String(s) => s
            .split([',', '，', '、', ';', '；'])
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect(),
        v => vec![scalar(v)],
    }
}

impl Engine {
    fn day_of(&self, v: &Value) -> Option<i64> {
        let s = scalar(v);
        let today = self.clock.today();
        let t = DateTime::from_day(today);
        Some(match s.to_lowercase().replace(' ', "_").as_str() {
            "one_week_ago" | "一周前" => today - 7,
            "one_week_from_now" | "一周后" => today + 7,
            "one_month_ago" | "一个月前" => t.add_months(-1).day(),
            "one_month_from_now" | "一个月后" => t.add_months(1).day(),
            _ => date::parse(&s, &self.clock)?.day(),
        })
    }

    /// is_within 的范围（按天，含两头）。
    fn within(&self, v: &Value) -> Option<(i64, i64)> {
        let s = scalar(v).to_lowercase().replace(' ', "_");
        let today = self.clock.today();
        let t = DateTime::from_day(today);
        let (y, m, _) = date::civil_from_days(today);
        Some(match s.as_str() {
            "today" | "今天" => (today, today),
            "this_week" | "本周" => (date::week_start(today), date::week_start(today) + 6),
            "this_month" | "本月" => (
                date::month_start(today),
                date::days_from_civil(y, m, date::days_in_month(y, m)),
            ),
            "this_year" | "今年" => (
                date::days_from_civil(y, 1, 1),
                date::days_from_civil(y, 12, 31),
            ),
            "past_week" | "过去一周" | "过去_7_天" => (today - 7, today),
            "past_month" | "过去一个月" => (t.add_months(-1).day(), today),
            "past_year" | "过去一年" => (t.add_months(-12).day(), today),
            "next_week" | "未来一周" | "未来_7_天" => (today, today + 7),
            "next_month" | "未来一个月" => (today, t.add_months(1).day()),
            "next_year" | "未来一年" => (today, t.add_months(12).day()),
            other => {
                let n = |p: &str| {
                    other
                        .strip_prefix(p)?
                        .strip_suffix("_days")?
                        .parse::<i64>()
                        .ok()
                };
                match n("past_") {
                    Some(k) => (today - k, today),
                    None => (today, today + n("next_")?),
                }
            }
        })
    }

    fn compile_filter(&self, f: &Value, depth: usize) -> Result<Node, DbError> {
        if depth > 20 {
            return Err(DbError::new("筛选嵌套太深"));
        }
        let group = |op: &str, items: &[Value]| -> Result<Node, DbError> {
            let kids = items
                .iter()
                .map(|x| self.compile_filter(x, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(if op.eq_ignore_ascii_case("or") {
                Node::Or(kids)
            } else {
                Node::And(kids)
            })
        };
        match f {
            Value::Null => Ok(Node::True),
            Value::Array(items) => group("and", items),
            Value::Object(o) => {
                if let Some(Value::Array(items)) = o.get("items") {
                    let op = o.get("op").and_then(Value::as_str).unwrap_or("and");
                    return group(op, items);
                }
                for op in ["and", "or"] {
                    if let Some(Value::Array(items)) = o.get(op) {
                        return group(op, items);
                    }
                }
                let Some(prop) = o.get("prop").map(scalar) else {
                    return Err(DbError::new("筛选要指明属性（prop）"));
                };
                let cond = o
                    .get("cond")
                    .or(o.get("op"))
                    .map(scalar)
                    .unwrap_or_else(|| "eq".into());
                self.leaf(&prop, &cond, o.get("value").unwrap_or(&Value::Null))
            }
            _ => Err(DbError::new(
                "筛选的格式不对：{ op: \"and\", items: [{ prop, cond, value }] }",
            )),
        }
    }

    fn leaf(&self, prop: &str, cond: &str, value: &Value) -> Result<Node, DbError> {
        let t = &self.tables[0];
        let Some(p) = t.find(prop) else {
            return Err(DbError::new(format!("筛选里没有属性「{prop}」")));
        };
        let pr = &t.schema.props[p];
        let kind = if pr.is_mirror() {
            PropType::Relation
        } else {
            pr.kind
        };
        let ok = allowed(kind);
        let c = Cond::parse(cond)
            .filter(|c| ok.contains(c))
            .ok_or_else(|| {
                let names: Vec<&str> = ok.iter().map(|c| c.key()).collect();
                DbError::new(format!(
                    "「{}」（{}）不能用条件「{cond}」，能用：{}",
                    pr.name,
                    kind.zh(),
                    names.join("、")
                ))
            })?;
        if c.needs_value() && blank(value) {
            return Ok(Node::True); // 还没填值的条件不筛
        }
        let neg = c.negated();
        use PropType::*;
        let test = match (c, kind) {
            (Cond::Empty, _) => Test::Empty(true),
            (Cond::NotEmpty, _) => Test::Empty(false),
            (_, Formula | Rollup) => Test::Calc(
                c,
                CalcTarget {
                    text: scalar(value).to_lowercase(),
                    num: crate::num::parse(&scalar(value)).or(value.as_f64()),
                    day: if c == Cond::Within {
                        self.within(value)
                    } else {
                        self.day_of(value).map(|d| (d, d))
                    },
                    bool: match value {
                        Value::Bool(b) => Some(*b),
                        v => crate::normalize::truthy(&scalar(v)),
                    },
                },
            ),
            (Cond::Checked, _) => Test::Check(true),
            (Cond::Unchecked, _) => Test::Check(false),
            (_, Checkbox) => {
                let b = match value {
                    Value::Bool(b) => Some(*b),
                    v => crate::normalize::truthy(&scalar(v)),
                }
                .ok_or_else(|| {
                    DbError::new(format!("「{}」要写 是 / 否（true / false）", pr.name))
                })?;
                Test::Check(if c == Cond::Neq { !b } else { b })
            }
            (_, Title | Text | Url | Email | Phone) => Test::Text(c, scalar(value).to_lowercase()),
            (_, Number) => {
                let n = value.as_f64().or_else(|| crate::num::parse(&scalar(value)));
                Test::Num(
                    c,
                    n.ok_or_else(|| {
                        DbError::new(format!(
                            "「{}」要写数字，不是「{}」",
                            pr.name,
                            scalar(value)
                        ))
                    })?,
                )
            }
            (_, UniqueId) => {
                let s = scalar(value);
                let digits: String = s
                    .chars()
                    .rev()
                    .take_while(char::is_ascii_digit)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                let n = digits.parse::<f64>().map_err(|_| {
                    DbError::new(format!(
                        "「{}」写编号，比如 {}12",
                        pr.name,
                        pr.prefix.as_deref().unwrap_or("")
                    ))
                })?;
                Test::Num(c, n)
            }
            (Cond::InGroup, _) => {
                let want: Vec<String> = list(value)
                    .into_iter()
                    .map(|g| match g.to_lowercase().as_str() {
                        "未开始" | "待办" | "to_do" | "to-do" => "todo".into(),
                        "进行中" | "in_progress" => "doing".into(),
                        "已完成" | "完成" | "complete" => "done".into(),
                        x => x.to_string(),
                    })
                    .collect();
                Test::Opts(
                    pr.options
                        .iter()
                        .map(|o| o.group.as_ref().is_some_and(|g| want.contains(g)))
                        .collect(),
                    false,
                )
            }
            (_, Select | Status | MultiSelect) => {
                let mut set = vec![false; pr.options.len()];
                for s in list(value) {
                    let i = t.rt[p].opts.get(&s).ok_or_else(|| {
                        let names: Vec<&str> = pr.options.iter().map(|o| o.name.as_str()).collect();
                        DbError::new(format!(
                            "「{}」没有选项「{s}」，可选：{}",
                            pr.name,
                            names.join("、")
                        ))
                    })?;
                    set[i as usize] = true;
                }
                Test::Opts(set, neg)
            }
            (_, Person | CreatedBy | UpdatedBy) => {
                let mut set = FxHashSet::default();
                for s in list(value) {
                    let id = if self.people.contains_key(&s) || s.contains(':') {
                        s
                    } else {
                        self.people_by_name
                            .get(&s.to_lowercase())
                            .cloned()
                            .ok_or_else(|| {
                                DbError::new(format!("「{}」没有叫「{s}」的人", pr.name))
                            })?
                    };
                    set.insert(id);
                }
                Test::Ids(set, neg)
            }
            (_, Relation) => {
                let s = scalar(value);
                let s = s.strip_prefix('#').unwrap_or(&s).to_lowercase();
                Test::Rel(s, matches!(c, Cond::Contains | Cond::NotContains), neg)
            }
            (_, Files) => Test::Files(scalar(value).to_lowercase(), neg),
            (Cond::Within, _) => {
                let (a, b) = self.within(value).ok_or_else(|| {
                    DbError::new(format!(
                        "「{}」的范围写 today、this_week、this_month、this_year、past_week、past_month、past_year、next_week、next_month、next_year 或 past_N_days / next_N_days，不是「{}」",
                        pr.name,
                        scalar(value)
                    ))
                })?;
                Test::Date(c, a, b)
            }
            (_, Date | CreatedTime | UpdatedTime) => {
                let d = self.day_of(value).ok_or_else(|| {
                    DbError::new(format!(
                        "「{}」的日期写成 2026-11-15 或 today / 明天 / one_week_ago…，不是「{}」",
                        pr.name,
                        scalar(value)
                    ))
                })?;
                Test::Date(c, d, d)
            }
            _ => return Err(DbError::new(format!("「{}」不能筛选", pr.name))),
        };
        Ok(Node::Leaf(p, test))
    }

    fn cell_days(&self, cell: &Cell) -> Option<(i64, i64)> {
        match cell {
            Cell::Date(s, e) => Some((s.day(), e.map_or(s.day(), |e| e.day().max(s.day())))),
            Cell::Ms(ms) => {
                let d = self.clock.local_min(*ms).div_euclid(1440);
                Some((d, d))
            }
            _ => None,
        }
    }

    fn date_test(c: Cond, have: Option<(i64, i64)>, a: i64, b: i64) -> bool {
        let Some((s, e)) = have else {
            return c == Cond::Neq;
        };
        match c {
            Cond::Eq => s <= a && a <= e,
            Cond::Neq => !(s <= a && a <= e),
            Cond::Gt => s > a,
            Cond::Gte => s >= a,
            Cond::Lt => s < a,
            Cond::Lte => s <= a,
            Cond::Within => s <= b && e >= a,
            _ => false,
        }
    }

    fn num_test(c: Cond, x: Option<f64>, y: f64) -> bool {
        let Some(x) = x else { return c.negated() };
        match c {
            Cond::Eq | Cond::Contains => x == y,
            Cond::Neq | Cond::NotContains => x != y,
            Cond::Gt => x > y,
            Cond::Gte => x >= y,
            Cond::Lt => x < y,
            Cond::Lte => x <= y,
            _ => false,
        }
    }

    fn text_test(c: Cond, s: &str, t: &str) -> bool {
        match c {
            Cond::Eq => s == t,
            Cond::Neq => s != t,
            Cond::Contains => s.contains(t),
            Cond::NotContains => !s.contains(t),
            Cond::StartsWith => s.starts_with(t),
            Cond::EndsWith => s.ends_with(t),
            Cond::Gt => s > t,
            Cond::Gte => s >= t,
            Cond::Lt => s < t,
            Cond::Lte => s <= t,
            _ => false,
        }
    }

    fn is_empty_at(&self, r: usize, p: usize) -> bool {
        let t = &self.tables[0];
        let prop = &t.schema.props[p];
        if prop.is_computed() {
            let v = self.calc(0, r, p, 0);
            return matches!(&v, V::Null)
                || matches!(&v, V::Str(s) if s.is_empty())
                || matches!(&v, V::List(l) if l.is_empty());
        }
        if prop.kind == PropType::Relation {
            return self.links(0, r, p).is_empty();
        }
        t.rows[r].cells[p].is_empty()
    }

    fn test(&self, node: &Node, r: usize) -> bool {
        match node {
            Node::True => true,
            Node::And(k) => k.iter().all(|n| self.test(n, r)),
            Node::Or(k) => k.is_empty() || k.iter().any(|n| self.test(n, r)),
            Node::Leaf(p, t) => self.leaf_test(*p, t, r),
        }
    }

    fn leaf_test(&self, p: usize, test: &Test, r: usize) -> bool {
        let tb = &self.tables[0];
        let cell = &tb.rows[r].cells[p];
        match test {
            Test::Empty(want) => self.is_empty_at(r, p) == *want,
            Test::Text(c, t) => Self::text_test(*c, cell.lower().unwrap_or(""), t),
            Test::Num(c, y) => Self::num_test(
                *c,
                if let Cell::Num(x) = cell {
                    Some(*x)
                } else {
                    None
                },
                *y,
            ),
            Test::Check(want) => matches!(cell, Cell::Bool(true)) == *want,
            Test::Opts(set, neg) => {
                let hit = match cell {
                    Cell::Opt(i) => set.get(*i as usize).copied().unwrap_or(false),
                    Cell::Opts(v) => v
                        .iter()
                        .any(|i| set.get(*i as usize).copied().unwrap_or(false)),
                    _ => false,
                };
                hit != *neg
            }
            Test::Ids(set, neg) => {
                let hit = matches!(cell, Cell::Ids(v) if v.iter().any(|x| set.contains(&**x)));
                hit != *neg
            }
            Test::Rel(s, contains, neg) => {
                let Some(tt) = tb.rt[p].target else {
                    return *neg;
                };
                let ot = &self.tables[tt];
                let hit = self.links(0, r, p).into_iter().any(|i| {
                    let title = ot
                        .title
                        .map(|ti| &ot.rows[i].cells[ti])
                        .and_then(Cell::lower)
                        .unwrap_or("");
                    ot.rows[i].data.id.to_lowercase() == *s
                        || if *contains {
                            title.contains(s.as_str())
                        } else {
                            title == s
                        }
                });
                hit != *neg
            }
            Test::Files(s, neg) => {
                let hit = matches!(cell, Cell::Files(v) if v.iter().any(|(n, _)| n.to_lowercase().contains(s.as_str())));
                hit != *neg
            }
            Test::Date(c, a, b) => Self::date_test(*c, self.cell_days(cell), *a, *b),
            Test::Calc(c, t) => {
                let v = self.calc(0, r, p, 0);
                match &v {
                    V::Bool(b) => match c {
                        Cond::Checked => *b,
                        Cond::Unchecked => !*b,
                        Cond::Eq | Cond::Contains => t.bool.is_some_and(|w| w == *b),
                        Cond::Neq | Cond::NotContains => t.bool.is_none_or(|w| w != *b),
                        _ => false,
                    },
                    V::Num(x) => match t.num {
                        Some(y) => Self::num_test(*c, Some(*x), y),
                        None => c.negated(),
                    },
                    V::Date(s, e) => match t.day {
                        Some((a, b)) => {
                            let have = Some((s.day(), e.map_or(s.day(), |e| e.day())));
                            let c = match c {
                                Cond::Contains => Cond::Eq,
                                Cond::NotContains => Cond::Neq,
                                c => *c,
                            };
                            Self::date_test(c, have, a, b)
                        }
                        None => c.negated(),
                    },
                    V::Null => c.negated(),
                    V::List(l) if matches!(c, Cond::Eq | Cond::Neq) => {
                        let hit = l.iter().any(|x| x.text().to_lowercase() == t.text);
                        hit != (*c == Cond::Neq)
                    }
                    v => Self::text_test(*c, &v.text().to_lowercase(), &t.text),
                }
            }
        }
    }

    // ---------- 搜索 ----------

    fn search_matches(&self, r: usize, q: &str, low_opts: &[Vec<String>]) -> bool {
        let t = &self.tables[0];
        t.schema
            .props
            .iter()
            .enumerate()
            .any(|(p, prop)| match &t.rows[r].cells[p] {
                Cell::Text(..) => t.rows[r].cells[p].lower().is_some_and(|s| s.contains(q)),
                Cell::Opt(i) => low_opts[p].get(*i as usize).is_some_and(|s| s.contains(q)),
                Cell::Opts(v) => v
                    .iter()
                    .any(|i| low_opts[p].get(*i as usize).is_some_and(|s| s.contains(q))),
                Cell::Ids(v) if prop.kind == PropType::Person => {
                    v.iter().any(|x| self.person(x).to_lowercase().contains(q))
                }
                Cell::Num(x) if prop.kind == PropType::UniqueId => format!(
                    "{}{}",
                    prop.prefix.as_deref().unwrap_or(""),
                    crate::num::text(*x)
                )
                .to_lowercase()
                .contains(q),
                _ => false,
            })
    }

    // ---------- 排序 ----------

    fn sort_key(&self, r: usize, p: usize) -> SKey<'_> {
        let t = &self.tables[0];
        let prop = &t.schema.props[p];
        let cell = &t.rows[r].cells[p];
        use PropType::*;
        if prop.kind.is_calculated() {
            return match self.calc(0, r, p, 0) {
                V::Null => SKey::Empty,
                V::Num(x) => SKey::Mixed(0, x, Cow::Borrowed("")),
                V::Date(d, _) => SKey::Mixed(1, d.min as f64, Cow::Borrowed("")),
                V::Bool(b) => SKey::Mixed(3, b as u8 as f64, Cow::Borrowed("")),
                v => {
                    let s = v.text().to_lowercase();
                    if s.is_empty() {
                        SKey::Empty
                    } else {
                        SKey::Mixed(2, 0.0, Cow::Owned(s))
                    }
                }
            };
        }
        let joined = |v: Vec<String>| {
            let s = v.join(", ").to_lowercase();
            if s.is_empty() {
                SKey::Empty
            } else {
                SKey::Text(Cow::Owned(s))
            }
        };
        if prop.kind == Relation {
            let Some(tt) = t.rt[p].target else {
                return SKey::Empty;
            };
            let ot = &self.tables[tt];
            return joined(
                self.links(0, r, p)
                    .into_iter()
                    .map(|i| ot.title_of(i).to_string())
                    .collect(),
            );
        }
        match (prop.kind, cell) {
            (Checkbox, c) => SKey::Num(matches!(c, Cell::Bool(true)) as u8 as f64),
            (_, Cell::Empty) => SKey::Empty,
            (_, Cell::Text(..)) => SKey::Text(Cow::Borrowed(cell.lower().unwrap())),
            (_, Cell::Num(x)) => SKey::Num(*x),
            (_, Cell::Opt(i)) => SKey::Num(*i as f64),
            (_, Cell::Opts(v)) => joined(
                v.iter()
                    .map(|i| prop.options[*i as usize].name.clone())
                    .collect(),
            ),
            (_, Cell::Date(d, _)) => SKey::Num(d.min as f64),
            (_, Cell::Ms(ms)) => SKey::Num(*ms as f64),
            (_, Cell::Ids(v)) => joined(v.iter().map(|x| self.person(x)).collect()),
            (_, Cell::Files(v)) => joined(v.iter().map(|(n, _)| n.to_string()).collect()),
            (_, Cell::Bool(b)) => SKey::Num(*b as u8 as f64),
        }
    }

    // ---------- 分组 ----------

    fn bucket(&self, day: i64, how: &str) -> (String, String, i64) {
        let (y, m, d) = date::civil_from_days(day);
        match how {
            "day" => {
                let s = DateTime::from_day(day).iso();
                (s.clone(), format!("{y}年{m}月{d}日"), day)
            }
            "week" => {
                let (wy, w) = date::iso_week(day);
                (
                    format!("{wy}-W{w:02}"),
                    format!("{wy}年第{w}周"),
                    date::week_start(day),
                )
            }
            "year" => (
                y.to_string(),
                format!("{y}年"),
                date::days_from_civil(y, 1, 1),
            ),
            "relative" => {
                let today = self.clock.today();
                let (k, n, o) = match day - today {
                    0 => ("today", "今天", today),
                    1 => ("tomorrow", "明天", today + 1),
                    -1 => ("yesterday", "昨天", today - 1),
                    2..=7 => ("next_7_days", "未来 7 天", today + 2),
                    -7..=-2 => ("past_7_days", "过去 7 天", today - 7),
                    8..=30 => ("next_30_days", "未来 30 天", today + 8),
                    -30..=-8 => ("past_30_days", "过去 30 天", today - 30),
                    _ => return self.bucket(day, "month"),
                };
                (k.into(), n.into(), o)
            }
            _ => (
                format!("{y}-{m:02}"),
                format!("{y}年{m}月"),
                date::month_start(day),
            ),
        }
    }

    /// 一行落在哪些分组（多选、人员、关联可能落在几个里）：(key, name, color, 排序用的)。
    fn group_keys(
        &self,
        r: usize,
        p: usize,
        how: &str,
    ) -> Vec<(String, String, Option<String>, GOrder)> {
        let t = &self.tables[0];
        let prop = &t.schema.props[p];
        let cell = &t.rows[r].cells[p];
        let opt = |i: u32| {
            let o = &prop.options[i as usize];
            (
                o.id.clone(),
                o.name.clone(),
                Some(o.color.clone()).filter(|c| !c.is_empty()),
                GOrder::Pre,
            )
        };
        let date = |d: i64| {
            let (k, n, o) = self.bucket(d, how);
            (k, n, None, GOrder::Num(o as f64))
        };
        use PropType::*;
        if prop.kind.is_calculated() {
            return match self.calc(0, r, p, 0) {
                V::Null => vec![],
                V::Bool(b) => vec![(
                    b.to_string(),
                    if b { "是" } else { "否" }.into(),
                    None,
                    GOrder::Num(!b as u8 as f64),
                )],
                V::Num(x) => vec![(
                    crate::num::text(x),
                    crate::display::display(prop, &crate::num::to_json(x), self),
                    None,
                    GOrder::Num(x),
                )],
                V::Date(d, _) => vec![date(d.day())],
                v => {
                    let s = v.text();
                    if s.is_empty() {
                        vec![]
                    } else {
                        vec![(s.clone(), s.clone(), None, GOrder::Text(s.to_lowercase()))]
                    }
                }
            };
        }
        if prop.kind == Relation {
            let Some(tt) = t.rt[p].target else {
                return vec![];
            };
            let ot = &self.tables[tt];
            return self
                .links(0, r, p)
                .into_iter()
                .map(|i| {
                    let title = ot.title_of(i).to_string();
                    let name = if title.is_empty() {
                        "无标题".to_string()
                    } else {
                        title
                    };
                    (
                        ot.rows[i].data.id.clone(),
                        name.clone(),
                        None,
                        GOrder::Text(name.to_lowercase()),
                    )
                })
                .collect();
        }
        match (prop.kind, cell) {
            (Checkbox, c) => {
                let b = matches!(c, Cell::Bool(true));
                vec![(b.to_string(), String::new(), None, GOrder::Pre)]
            }
            (_, Cell::Opt(i)) => vec![opt(*i)],
            (_, Cell::Opts(v)) => v.iter().map(|i| opt(*i)).collect(),
            (_, Cell::Date(d, _)) => vec![date(d.day())],
            (_, Cell::Ms(ms)) => vec![date(self.clock.local_min(*ms).div_euclid(1440))],
            (_, Cell::Ids(v)) => v
                .iter()
                .map(|x| {
                    let n = self.person(x);
                    (
                        x.to_string(),
                        n.clone(),
                        None,
                        GOrder::Text(n.to_lowercase()),
                    )
                })
                .collect(),
            (_, Cell::Num(x)) => vec![(
                crate::num::text(*x),
                crate::display::display(prop, &crate::num::to_json(*x), self),
                None,
                GOrder::Num(*x),
            )],
            (_, Cell::Text(s, _)) => vec![(
                s.to_string(),
                s.to_string(),
                None,
                GOrder::Text(cell.lower().unwrap().to_string()),
            )],
            (_, Cell::Files(v)) => v
                .iter()
                .map(|(n, _)| {
                    (
                        n.to_string(),
                        n.to_string(),
                        None,
                        GOrder::Text(n.to_lowercase()),
                    )
                })
                .collect(),
            _ => vec![],
        }
    }

    fn group(
        &self,
        rows: &[usize],
        p: usize,
        how: &str,
        view: &View,
        sub: Option<(usize, &str)>,
        totals: &[(String, usize, Agg)],
    ) -> Vec<Group> {
        let t = &self.tables[0];
        let prop = &t.schema.props[p];
        struct Slot {
            g: Group,
            order: GOrder,
            rows: Vec<usize>,
        }
        let mut slots: Vec<Slot> = vec![];
        let mut at: FxHashMap<String, usize> = FxHashMap::default();
        let mut add = |slots: &mut Vec<Slot>,
                       key: String,
                       name: String,
                       color: Option<String>,
                       order: GOrder|
         -> usize {
            if let Some(&i) = at.get(&key) {
                return i;
            }
            at.insert(key.clone(), slots.len());
            slots.push(Slot {
                g: Group {
                    key,
                    name,
                    color,
                    ..Default::default()
                },
                order,
                rows: vec![],
            });
            slots.len() - 1
        };
        // 选项按表结构里的顺序全列出来（看板上空的列也要有）；勾选是“已勾选 / 未勾选”
        if prop.kind.has_options() && !prop.is_computed() {
            for o in &prop.options {
                add(
                    &mut slots,
                    o.id.clone(),
                    o.name.clone(),
                    Some(o.color.clone()).filter(|c| !c.is_empty()),
                    GOrder::Pre,
                );
            }
        }
        if prop.kind == PropType::Checkbox {
            add(
                &mut slots,
                "true".into(),
                "已勾选".into(),
                None,
                GOrder::Pre,
            );
            add(
                &mut slots,
                "false".into(),
                "未勾选".into(),
                None,
                GOrder::Pre,
            );
        }
        let mut empty: Vec<usize> = vec![];
        for &r in rows {
            let keys = self.group_keys(r, p, how);
            if keys.is_empty() {
                empty.push(r);
            }
            let mut seen: FxHashSet<usize> = FxHashSet::default();
            for (k, n, c, o) in keys {
                let i = add(&mut slots, k, n, c, o);
                if seen.insert(i) {
                    slots[i].rows.push(r);
                }
            }
        }
        // 预先列好的按顺序在前，别的按值排
        let pre = slots
            .iter()
            .take_while(|s| matches!(s.order, GOrder::Pre))
            .count();
        slots[pre..].sort_by(|a, b| a.order.cmp(&b.order));
        if !empty.is_empty() {
            slots.push(Slot {
                g: Group {
                    key: String::new(),
                    name: format!("无{}", prop.name),
                    ..Default::default()
                },
                order: GOrder::Pre,
                rows: empty,
            });
        }
        slots
            .into_iter()
            .filter(|s| !view.hide_empty_groups || !s.rows.is_empty())
            .map(|s| {
                let mut g = s.g;
                g.count = s.rows.len();
                g.ids = s
                    .rows
                    .iter()
                    .map(|&r| self.tables[0].rows[r].data.id.clone())
                    .collect();
                if let Some((sp, shw)) = sub {
                    g.groups = Some(self.group(&s.rows, sp, shw, view, None, totals));
                }
                if !totals.is_empty() {
                    g.totals = Some(self.totals(&s.rows, totals));
                }
                g
            })
            .collect()
    }

    fn totals(&self, rows: &[usize], spec: &[(String, usize, Agg)]) -> Map<String, Value> {
        let mut out = Map::new();
        for (key, p, agg) in spec {
            let vals: Vec<V> = if *agg == Agg::Count {
                vec![]
            } else {
                rows.iter().map(|&r| self.fval(0, r, *p, 0)).collect()
            };
            let v = crate::formula::finish(aggregate(*agg, &vals, rows.len()));
            out.insert(key.clone(), v_json(&v));
        }
        out
    }

    /// 按视图查：筛选、搜索、排序、分组、汇总行。
    pub fn query(&self, view: &View) -> Result<QueryResult, DbError> {
        let t = &self.tables[0];
        let filter = match &view.filter {
            Some(f) => self.compile_filter(f, 0)?,
            None => Node::True,
        };
        let q = view
            .search
            .as_deref()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
        let low_opts: Vec<Vec<String>> = t
            .schema
            .props
            .iter()
            .map(|p| p.options.iter().map(|o| o.name.to_lowercase()).collect())
            .collect();
        let mut rows: Vec<usize> = (0..t.rows.len())
            .filter(|&r| {
                self.test(&filter, r)
                    && q.as_deref()
                        .is_none_or(|q| self.search_matches(r, q, &low_opts))
            })
            .collect();
        // 排序
        let mut sorts = vec![];
        for s in &view.sorts {
            let p = t
                .find(&s.prop)
                .ok_or_else(|| DbError::new(format!("排序里没有属性「{}」", s.prop)))?;
            sorts.push((p, s.is_desc()));
        }
        if !sorts.is_empty() {
            let keys: Vec<Vec<SKey>> = sorts
                .iter()
                .map(|(p, _)| rows.iter().map(|&r| self.sort_key(r, *p)).collect())
                .collect();
            let mut perm: Vec<usize> = (0..rows.len()).collect();
            perm.sort_by(|&a, &b| {
                for (k, (_, desc)) in keys.iter().zip(&sorts) {
                    let o = cmp_keys(&k[a], &k[b], *desc);
                    if o != Ordering::Equal {
                        return o;
                    }
                }
                Ordering::Equal
            });
            rows = perm.into_iter().map(|i| rows[i]).collect();
        }
        // 汇总行
        let mut spec: Vec<(String, usize, Agg)> = vec![];
        for (k, f) in &view.totals {
            let (Some(p), Some(agg)) = (t.find(k), f.as_str().and_then(Agg::parse)) else {
                continue;
            };
            spec.push((k.clone(), p, agg));
        }
        let totals = (!spec.is_empty()).then(|| self.totals(&rows, &spec));
        // 分组
        let find = |k: &Option<String>| -> Result<Option<usize>, DbError> {
            match k.as_deref().filter(|k| !k.is_empty()) {
                None => Ok(None),
                Some(k) => t
                    .find(k)
                    .map(Some)
                    .ok_or_else(|| DbError::new(format!("分组里没有属性「{k}」"))),
            }
        };
        let gp = find(&view.group_by)?;
        let sp = find(&view.sub_group_by)?;
        let groups = gp.map(|p| {
            let how = view.group_date.as_deref().unwrap_or("month");
            let sub = sp.map(|q| (q, view.sub_group_date.as_deref().unwrap_or("month")));
            self.group(&rows, p, how, view, sub, &spec)
        });
        let total = rows.len();
        let off = view.offset.unwrap_or(0).min(total);
        let end = view.limit.map_or(total, |l| (off + l).min(total));
        Ok(QueryResult {
            ids: rows[off..end]
                .iter()
                .map(|&r| t.rows[r].data.id.clone())
                .collect(),
            total,
            groups,
            totals,
        })
    }
}

/// 排序用的值。
enum SKey<'a> {
    Empty,
    Num(f64),
    Text(Cow<'a, str>),
    /// 公式、汇总：类型不一样的分开排（数在前，然后日期、文字、真假）
    Mixed(u8, f64, Cow<'a, str>),
}

fn cmp_keys(a: &SKey, b: &SKey, desc: bool) -> Ordering {
    let o = match (a, b) {
        (SKey::Empty, SKey::Empty) => return Ordering::Equal,
        (SKey::Empty, _) => return Ordering::Greater, // 空的总在最后
        (_, SKey::Empty) => return Ordering::Less,
        (SKey::Num(x), SKey::Num(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (SKey::Text(x), SKey::Text(y)) => natural(x, y),
        (SKey::Mixed(ra, xa, sa), SKey::Mixed(rb, xb, sb)) => ra
            .cmp(rb)
            .then(xa.partial_cmp(xb).unwrap_or(Ordering::Equal))
            .then_with(|| natural(sa, sb)),
        (SKey::Num(_), _) => Ordering::Less,
        (_, SKey::Num(_)) => Ordering::Greater,
        (SKey::Text(_), _) => Ordering::Less,
        (_, SKey::Text(_)) => Ordering::Greater,
    };
    if desc { o.reverse() } else { o }
}

/// 自然顺序：文字里的数按数比（任务2 在 任务10 前面），别的按码位。
pub(crate) fn natural(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let x = std::str::from_utf8(&a[si..i])
                .unwrap()
                .trim_start_matches('0');
            let y = std::str::from_utf8(&b[sj..j])
                .unwrap()
                .trim_start_matches('0');
            let o = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
            if o != Ordering::Equal {
                return o;
            }
            continue;
        }
        if a[i] != b[j] {
            return a[i].cmp(&b[j]);
        }
        i += 1;
        j += 1;
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// 分组排序用的：预先列好的（选项、勾选）按列的先后；别的按值。
#[derive(Clone, Debug, PartialEq)]
enum GOrder {
    Pre,
    Num(f64),
    Text(String),
}

impl GOrder {
    fn cmp(&self, o: &GOrder) -> Ordering {
        match (self, o) {
            (GOrder::Num(a), GOrder::Num(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (GOrder::Text(a), GOrder::Text(b)) => natural(a, b),
            (GOrder::Num(_), _) => Ordering::Less,
            (_, GOrder::Num(_)) => Ordering::Greater,
            _ => Ordering::Equal,
        }
    }
}
