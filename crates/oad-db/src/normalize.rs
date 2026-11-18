//! 值的规范化：给的值 → 存的样子。网页写存的样子（选项 id、`{start, end}`、`user:1`），Agent 写名字
//! （“进行中”、“张三”、“2026-11-15”、“是”），两种都认。空（null、""、[]）是清空，返回 null。认不出报错，说清楚该怎么写。
use crate::date::{self, DateTime};
use crate::schema::{Prop, PropType, SelectOption};
use crate::{DbError, Lookup};
use serde_json::{Map, Value, json};

pub const MAX_TEXT: usize = 2000;
pub const MAX_LINKS: usize = 100;
pub const MAX_FILES: usize = 20;
pub const MAX_OPTIONS: usize = 100;
pub const NAME_LEN: usize = 60;
pub const COLORS: [&str; 10] = [
    "default", "gray", "brown", "orange", "yellow", "green", "blue", "purple", "pink", "red",
];

/// 规范化的结果：存的值，和为它新加的选项（单选、多选里没有的名字；调用方负责把它们写回表结构）。
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Normalized {
    pub value: Value,
    pub new_options: Vec<SelectOption>,
}

/// 勾选：是 / 否、true / false、✓、yes / no、1 / 0…；认不出是 None。
pub fn truthy(s: &str) -> Option<bool> {
    match s.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "是" | "✓" | "✔" | "☑" | "x" | "[x]" | "done" | "对"
        | "checked" | "on" | "勾" | "已勾选" => Some(true),
        "0" | "false" | "no" | "n" | "否" | "" | "[ ]" | "不是" | "unchecked" | "off" | "✗"
        | "✘" | "未勾选" => Some(false),
        _ => None,
    }
}

/// 日期存的样子：`{ start, end?, time? }`（有几点时 `time: true`）。
pub fn date_value(start: DateTime, end: Option<DateTime>) -> Value {
    let mut o = Map::new();
    o.insert("start".into(), Value::String(start.iso()));
    if let Some(e) = end {
        o.insert("end".into(), Value::String(e.iso()));
    }
    if start.shows_time() || end.is_some_and(|e| e.shows_time()) {
        o.insert("time".into(), Value::Bool(true));
    }
    Value::Object(o)
}

fn is_blank(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(crate::num::text(n.as_f64().unwrap_or(0.0))),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// 列表，或者用 `,，、;；` 分开的一串。
fn split_items(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.clone(),
        Value::String(s) => s
            .split([',', '，', '、', ';', '；', '\n'])
            .map(|x| Value::String(x.trim().to_string()))
            .filter(|x| !is_blank(x))
            .collect(),
        v => vec![v.clone()],
    }
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 新选项的 id：由名字算出来（同样的输入同样的 id），撞了往后挪。
fn option_id(prop: &Prop, taken: &[SelectOption], name: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in prop.key.bytes().chain([0]).chain(name.bytes()) {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    loop {
        let id = format!("o{:04x}", h & 0xffff);
        if !taken.iter().any(|o| o.id == id) {
            return id;
        }
        h = h.wrapping_mul(0x100000001b3).wrapping_add(1);
    }
}

struct Opts<'a> {
    prop: &'a Prop,
    added: Vec<SelectOption>,
    create: bool,
}

impl Opts<'_> {
    fn all(&self) -> impl Iterator<Item = &SelectOption> {
        self.prop.options.iter().chain(&self.added)
    }

    fn pick(&mut self, v: &Value) -> Result<String, DbError> {
        let p = self.prop;
        let s = scalar(v).ok_or_else(|| DbError::new(format!("「{}」写选项的名字", p.name)))?;
        let s = s.trim();
        if let Some(o) = self.all().find(|o| o.id == s) {
            return Ok(o.id.clone());
        }
        let low = s.to_lowercase();
        if let Some(o) = self.all().find(|o| o.name.to_lowercase() == low) {
            return Ok(o.id.clone());
        }
        if !self.create || p.kind == PropType::Status {
            let names: Vec<&str> = self.all().map(|o| o.name.as_str()).collect();
            let names = if names.is_empty() {
                "（还没有选项）".to_string()
            } else {
                names.join("、")
            };
            return Err(DbError::new(format!(
                "「{}」没有选项「{s}」，可选：{names}",
                p.name
            )));
        }
        let name = collapse(s);
        if name.chars().count() > NAME_LEN {
            return Err(DbError::new(format!(
                "「{}」的选项名不能超过 {NAME_LEN} 字",
                p.name
            )));
        }
        let n = p.options.len() + self.added.len();
        if n >= MAX_OPTIONS {
            return Err(DbError::new(format!(
                "「{}」最多 {MAX_OPTIONS} 个选项",
                p.name
            )));
        }
        let taken: Vec<SelectOption> = self.all().cloned().collect();
        let o = SelectOption {
            id: option_id(p, &taken, &name),
            name,
            color: COLORS[1 + n % (COLORS.len() - 1)].into(),
            group: None,
            extra: Map::new(),
        };
        let id = o.id.clone();
        self.added.push(o);
        Ok(id)
    }
}

fn person_id(s: &str) -> bool {
    let Some((kind, rest)) = s.split_once(':') else {
        return false;
    };
    matches!(kind, "user" | "agent" | "bot")
        && !rest.is_empty()
        && !rest.contains(char::is_whitespace)
}

/// 把给的值变成存的样子。`create`：单选、多选里没有的名字新加一个选项（状态不加），放在 `new_options` 里。
pub fn normalize(
    prop: &Prop,
    v: &Value,
    look: &dyn Lookup,
    create: bool,
) -> Result<Normalized, DbError> {
    let name = &prop.name;
    let done = |value: Value| {
        Ok(Normalized {
            value,
            new_options: vec![],
        })
    };
    if prop.is_computed() || prop.kind.is_auto() {
        return Err(DbError::new(format!("「{name}」是自动算的，不能改")));
    }
    if prop.kind == PropType::Button {
        return Err(DbError::new(format!("「{name}」是按钮，没有值")));
    }
    if is_blank(v) {
        return done(Value::Null);
    }
    use PropType::*;
    match prop.kind {
        Title | Text | Url | Email | Phone => {
            let Some(s) = scalar(v) else {
                return Err(DbError::new(format!("「{name}」要写一段文字")));
            };
            let s = if prop.kind == Text {
                s.trim().to_string()
            } else {
                collapse(&s)
            };
            if s.chars().count() > MAX_TEXT {
                return Err(DbError::new(format!(
                    "「{name}」太长（最多 {MAX_TEXT} 字）"
                )));
            }
            if prop.kind == Email && !looks_like_email(&s) {
                return Err(DbError::new(format!("「{name}」不像邮箱：{s}")));
            }
            let s = if prop.kind == Url && !has_scheme(&s) {
                format!("https://{s}")
            } else {
                s
            };
            done(Value::String(s))
        }
        Number => {
            let n = match v {
                Value::Number(n) => n.as_f64(),
                Value::String(s) => crate::num::parse(s),
                _ => None,
            };
            match n {
                Some(n) if n.is_finite() => done(crate::num::to_json(n)),
                _ => Err(DbError::new(format!(
                    "「{name}」要写数字，不是「{}」",
                    show(v)
                ))),
            }
        }
        Select | Status => {
            let one = match v {
                Value::Array(a) if a.len() == 1 => &a[0],
                Value::Array(_) => return Err(DbError::new(format!("「{name}」只能选一个"))),
                v => v,
            };
            let mut o = Opts {
                prop,
                added: vec![],
                create,
            };
            let id = o.pick(one)?;
            Ok(Normalized {
                value: Value::String(id),
                new_options: o.added,
            })
        }
        MultiSelect => {
            let mut o = Opts {
                prop,
                added: vec![],
                create,
            };
            let mut out: Vec<Value> = vec![];
            for x in split_items(v) {
                if is_blank(&x) {
                    continue;
                }
                let id = Value::String(o.pick(&x)?);
                if !out.contains(&id) {
                    out.push(id);
                }
            }
            let value = if out.is_empty() {
                Value::Null
            } else {
                Value::Array(out)
            };
            Ok(Normalized {
                value,
                new_options: o.added,
            })
        }
        Date => {
            let clock = look.clock();
            let (start, end) = match v {
                Value::Object(o) => (
                    o.get("start").and_then(scalar).unwrap_or_default(),
                    o.get("end")
                        .and_then(scalar)
                        .filter(|s| !s.trim().is_empty()),
                ),
                v => {
                    let s = scalar(v).unwrap_or_default();
                    let (a, b) = date::split_range(&s);
                    (a.to_string(), b.map(str::to_string))
                }
            };
            let hint = "写成 2026-11-15、2026-11-15 14:00、2026年11月15日 或 今天 / 明天 / 昨天";
            let Some(a) = date::parse(&start, clock) else {
                return Err(DbError::new(format!(
                    "「{name}」的日期{hint}，不是「{start}」"
                )));
            };
            let b = match end {
                None => None,
                Some(e) => match date::parse(&e, clock) {
                    Some(b) => Some(b),
                    None => {
                        return Err(DbError::new(format!(
                            "「{name}」的结束日期{hint}，不是「{e}」"
                        )));
                    }
                },
            };
            if b.is_some_and(|b| b.min < a.min) {
                return Err(DbError::new(format!("「{name}」的结束日期在开始日期前面")));
            }
            done(date_value(a, b))
        }
        Person => {
            let mut out: Vec<Value> = vec![];
            for x in split_items(v) {
                let Some(s) = scalar(&x) else { continue };
                let s = s.trim();
                if s.is_empty() {
                    continue;
                }
                let who = if person_id(s) {
                    s.to_string()
                } else {
                    look.person_by_name(s)
                        .ok_or_else(|| DbError::new(format!("「{name}」没有叫「{s}」的人")))?
                };
                let who = Value::String(who);
                if !out.contains(&who) {
                    out.push(who);
                }
            }
            done(if out.is_empty() {
                Value::Null
            } else {
                Value::Array(out)
            })
        }
        Checkbox => {
            let b = match v {
                Value::Bool(b) => Some(*b),
                Value::Number(n) => Some(n.as_f64() != Some(0.0)),
                Value::String(s) => truthy(s),
                _ => None,
            };
            match b {
                // 没勾就是空的
                Some(b) => done(if b { Value::Bool(true) } else { Value::Null }),
                None => Err(DbError::new(format!(
                    "「{name}」要写 是 / 否（true / false）"
                ))),
            }
        }
        Relation => links(prop, v, look),
        Files => files(prop, v),
        _ => Err(DbError::new(format!("不认识的类型：{}", prop.kind.key()))),
    }
}

fn show(v: &Value) -> String {
    scalar(v).unwrap_or_else(|| v.to_string())
}

fn has_scheme(s: &str) -> bool {
    if s.starts_with('/') || s.to_lowercase().starts_with("mailto:") {
        return true;
    }
    let Some(i) = s.find("://") else { return false };
    let scheme = &s[..i];
    !scheme.is_empty()
        && scheme.chars().next().unwrap().is_ascii_alphabetic()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
}

fn looks_like_email(s: &str) -> bool {
    let Some((a, b)) = s.split_once('@') else {
        return false;
    };
    let ok = |x: &str| !x.is_empty() && !x.contains(char::is_whitespace) && !x.contains('@');
    ok(a)
        && ok(b)
        && b.split_once('.')
            .is_some_and(|(x, y)| !x.is_empty() && !y.is_empty())
}

/// 关联：行 id（`#id` 也认）或者对方库里行的名称，列表或逗号分开。对方库加载了就只认那里有的行。
fn links(prop: &Prop, v: &Value, look: &dyn Lookup) -> Result<Normalized, DbError> {
    let name = &prop.name;
    let db = prop
        .relation
        .as_ref()
        .map(|r| r.db_id.as_str())
        .unwrap_or("");
    let loaded = look.has_table(db);
    let mut out: Vec<Value> = vec![];
    for x in split_items(v) {
        let Some(s) = scalar(&x) else {
            return Err(DbError::new(format!("「{name}」写行 id 或者那一行的名称")));
        };
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        let bare = s.strip_prefix('#').unwrap_or(s);
        let id = if !loaded || look.row_title(db, bare).is_some() {
            bare.to_string()
        } else if let Some(id) = look.row_by_title(db, s) {
            id
        } else {
            return Err(DbError::new(format!(
                "「{name}」关联的库里没有叫「{s}」的行"
            )));
        };
        let id = Value::String(id);
        if !out.contains(&id) {
            out.push(id);
        }
    }
    if out.len() > MAX_LINKS {
        return Err(DbError::new(format!("「{name}」最多关联 {MAX_LINKS} 行")));
    }
    Ok(Normalized {
        value: if out.is_empty() {
            Value::Null
        } else {
            Value::Array(out)
        },
        new_options: vec![],
    })
}

/// 文件：网址（http(s)://…，或者站内的 /…），可以写成 `{ url, name }`。
fn files(prop: &Prop, v: &Value) -> Result<Normalized, DbError> {
    let name = &prop.name;
    let items = match v {
        Value::Array(a) => a.clone(),
        Value::String(s) => s.split_whitespace().map(|x| json!(x)).collect(),
        v => vec![v.clone()],
    };
    let mut out = vec![];
    for x in items {
        let (url, nm) = match &x {
            Value::Object(o) => (
                o.get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                o.get("name").and_then(Value::as_str).map(str::to_string),
            ),
            Value::String(s) => (s.trim().to_string(), None),
            _ => (String::new(), None),
        };
        if url.is_empty() {
            continue;
        }
        let low = url.to_lowercase();
        if !(low.starts_with("https://") || low.starts_with("http://") || url.starts_with('/'))
            || url.contains(char::is_whitespace)
        {
            return Err(DbError::new(format!(
                "「{name}」要写网址（https://…）或者传上来的文件，不是「{url}」"
            )));
        }
        let nm = nm
            .map(|n| collapse(&n))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| {
                let path = url
                    .split(['?', '#'])
                    .next()
                    .unwrap_or(&url)
                    .trim_end_matches('/');
                path.rsplit('/')
                    .next()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&url)
                    .to_string()
            });
        out.push(json!({ "name": nm.chars().take(200).collect::<String>(), "url": url.chars().take(2000).collect::<String>() }));
    }
    if out.len() > MAX_FILES {
        return Err(DbError::new(format!("「{name}」最多 {MAX_FILES} 个文件")));
    }
    Ok(Normalized {
        value: if out.is_empty() {
            Value::Null
        } else {
            Value::Array(out)
        },
        new_options: vec![],
    })
}
