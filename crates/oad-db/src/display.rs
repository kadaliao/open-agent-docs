//! 给人和 Agent 看的文字：选项写名字、人写名字、关联写对方行的标题、数字按格式、日期按显示方式。空的是 ""。
use crate::Lookup;
use crate::aggregate::Agg;
use crate::date::{self, DateTime};
use crate::schema::{Prop, PropType};
use crate::value::date_of;
use serde_json::Value;

fn join<I: IntoIterator<Item = String>>(it: I) -> String {
    it.into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn items(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(a) => a.iter().collect(),
        v => vec![v],
    }
}

fn plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => crate::num::text(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => if *b { "是" } else { "否" }.into(),
        Value::Null => String::new(),
        Value::Array(a) => join(a.iter().map(plain)),
        Value::Object(o) => o
            .get("name")
            .or(o.get("start"))
            .map(plain)
            .unwrap_or_default(),
    }
}

fn date_text(prop: &Prop, v: &Value, look: &dyn Lookup) -> String {
    let clock = look.clock();
    match date_of(v, clock) {
        Some((s, e)) => {
            let style = prop.date_format.as_deref();
            let a = date::format(&s, style, clock);
            match e {
                Some(e) => format!("{a} → {}", date::format(&e, style, clock)),
                None => a,
            }
        }
        None => plain(v),
    }
}

/// 公式、汇总算出来的值给人看的样子。
fn calc_text(prop: &Prop, v: &Value, look: &dyn Lookup) -> String {
    let agg = prop.rollup.as_ref().and_then(|r| Agg::parse(&r.func));
    match v {
        Value::Bool(b) => if *b { "是" } else { "否" }.into(),
        Value::Number(n) => {
            let n = n.as_f64().unwrap_or(0.0);
            let fmt = match (prop.format.as_deref(), agg) {
                (Some(f), _) => Some(f),
                (None, Some(a)) if a.is_percent() => Some("percent"),
                _ => None,
            };
            crate::num::format(n, fmt, prop.precision)
        }
        Value::Object(o) if o.contains_key("start") => date_text(prop, v, look),
        Value::Array(a) => join(a.iter().map(|x| calc_text(prop, x, look))),
        v => plain(v),
    }
}

/// 一格的值（存的样子，公式、汇总、反向关联是算出来的样子）→ 文字。
pub fn display(prop: &Prop, v: &Value, look: &dyn Lookup) -> String {
    if v.is_null() || v.as_str() == Some("") || v.as_array().is_some_and(|a| a.is_empty()) {
        return String::new();
    }
    use PropType::*;
    match prop.kind {
        Formula | Rollup => calc_text(prop, v, look),
        Relation => {
            let db = prop
                .relation
                .as_ref()
                .map(|r| r.db_id.as_str())
                .unwrap_or("");
            let loaded = look.has_table(db);
            join(items(v).into_iter().filter_map(|x| {
                let id = plain(x);
                if !loaded {
                    return Some(id);
                }
                look.row_title(db, &id).map(|t| {
                    if t.is_empty() {
                        "无标题".to_string()
                    } else {
                        t
                    }
                })
            }))
        }
        Files => join(items(v).into_iter().map(plain)),
        UniqueId => format!("{}{}", prop.prefix.as_deref().unwrap_or(""), plain(v)),
        Button => String::new(),
        Number => match v.as_f64() {
            Some(n) => crate::num::format(n, prop.format.as_deref(), prop.precision),
            None => plain(v),
        },
        Select | Status | MultiSelect => join(items(v).into_iter().filter_map(|x| {
            let id = x.as_str()?;
            prop.options
                .iter()
                .find(|o| o.id == id)
                .map(|o| o.name.clone())
        })),
        Date => date_text(prop, v, look),
        Person | CreatedBy | UpdatedBy => join(items(v).into_iter().map(|x| {
            let id = plain(x);
            look.person_name(&id).unwrap_or(id)
        })),
        Checkbox => match v {
            Value::Bool(true) => "是".into(),
            Value::Bool(false) => String::new(),
            v => plain(v),
        },
        CreatedTime | UpdatedTime => match v.as_f64() {
            Some(ms) => {
                let clock = look.clock();
                let d = DateTime {
                    min: clock.local_min(ms as i64),
                    time: true,
                };
                date::format(&d, prop.date_format.as_deref(), clock)
            }
            None => date_text(prop, v, look),
        },
        _ => plain(v),
    }
}
