//! 格子：存的 JSON 按属性类型解析好放着（选项换成序号、日期换成分钟数），筛选、排序、分组直接用。
use crate::date::{self, Clock, DateTime};
use crate::schema::{PropType, RowData};
use rustc_hash::FxHashMap;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Cell {
    Empty,
    /// 原文和小写（和原文一样时不另存）
    Text(Box<str>, Option<Box<str>>),
    Num(f64),
    Bool(bool),
    Date(DateTime, Option<DateTime>),
    /// 毫秒时间戳（创建时间、最后编辑时间）
    Ms(i64),
    /// 选项的序号
    Opt(u32),
    Opts(Vec<u32>),
    /// 人、关联的行
    Ids(Vec<Box<str>>),
    /// 文件：名字、网址
    Files(Vec<(Box<str>, Box<str>)>),
}

impl Cell {
    pub fn text(s: &str) -> Cell {
        if s.is_empty() {
            return Cell::Empty;
        }
        let low = s.to_lowercase();
        Cell::Text(s.into(), (low != s).then(|| low.into()))
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Cell::Empty => true,
            Cell::Opts(v) => v.is_empty(),
            Cell::Ids(v) => v.is_empty(),
            Cell::Files(v) => v.is_empty(),
            Cell::Bool(b) => !b,
            _ => false,
        }
    }

    /// 文字的小写（筛选用）。
    pub fn lower(&self) -> Option<&str> {
        match self {
            Cell::Text(s, l) => Some(l.as_deref().unwrap_or(s)),
            _ => None,
        }
    }
}

/// 选项：id → 序号、名字（小写）→ 序号。
#[derive(Default, Debug)]
pub(crate) struct OptIndex {
    pub by_id: FxHashMap<Box<str>, u32>,
    pub by_name: FxHashMap<String, u32>,
}

impl OptIndex {
    pub fn get(&self, s: &str) -> Option<u32> {
        self.by_id
            .get(s)
            .or_else(|| self.by_name.get(&s.trim().to_lowercase()))
            .copied()
    }
}

fn as_str(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn ids(v: &Value) -> Vec<Box<str>> {
    match v {
        Value::Array(a) => a.iter().filter_map(as_str).map(Into::into).collect(),
        v => as_str(v).into_iter().map(Into::into).collect(),
    }
}

/// 日期属性存的值：`{ start, end? }` 或者一个字符串。
pub(crate) fn date_of(v: &Value, clock: &Clock) -> Option<(DateTime, Option<DateTime>)> {
    let (s, e) = match v {
        Value::Object(o) => (
            o.get("start")?.as_str()?,
            o.get("end").and_then(Value::as_str),
        ),
        Value::String(s) => (s.as_str(), None),
        _ => return None,
    };
    let start = date::parse_stored(s, clock)?;
    let time = matches!(v, Value::Object(o) if o.get("time") == Some(&Value::Bool(true)));
    let start = DateTime {
        min: start.min,
        time: start.time || time,
    };
    let end = e.and_then(|e| date::parse_stored(e, clock));
    Some((start, end))
}

/// 一格存的值 → 格子。
pub(crate) fn cell_of(
    kind: PropType,
    v: Option<&Value>,
    row: &RowData,
    opts: &OptIndex,
    clock: &Clock,
) -> Cell {
    use PropType::*;
    match kind {
        CreatedTime => return row.created_at.map_or(Cell::Empty, |x| Cell::Ms(x as i64)),
        UpdatedTime => {
            return row
                .updated_at
                .or(row.created_at)
                .map_or(Cell::Empty, |x| Cell::Ms(x as i64));
        }
        CreatedBy => {
            return row
                .created_by
                .as_deref()
                .map_or(Cell::Empty, |x| Cell::Ids(vec![x.into()]));
        }
        UpdatedBy => {
            return row
                .updated_by
                .as_deref()
                .or(row.created_by.as_deref())
                .map_or(Cell::Empty, |x| Cell::Ids(vec![x.into()]));
        }
        UniqueId => {
            return v
                .and_then(Value::as_f64)
                .or(row.unique_id.map(|x| x as f64))
                .map_or(Cell::Empty, Cell::Num);
        }
        _ => {}
    }
    let Some(v) = v.filter(|v| !v.is_null()) else {
        return Cell::Empty;
    };
    match kind {
        Title | Text | Url | Email | Phone => as_str(v).map_or(Cell::Empty, |s| Cell::text(&s)),
        Number => match v {
            Value::Number(n) => n.as_f64().map_or(Cell::Empty, Cell::Num),
            Value::String(s) => crate::num::parse(s).map_or(Cell::Empty, Cell::Num),
            _ => Cell::Empty,
        },
        Select | Status => {
            let s = match v {
                Value::Array(a) => a.first().and_then(as_str),
                v => as_str(v),
            };
            s.and_then(|s| opts.get(&s)).map_or(Cell::Empty, Cell::Opt)
        }
        MultiSelect => {
            let mut out: Vec<u32> = vec![];
            for id in ids(v) {
                if let Some(i) = opts.get(&id)
                    && !out.contains(&i)
                {
                    out.push(i);
                }
            }
            if out.is_empty() {
                Cell::Empty
            } else {
                Cell::Opts(out)
            }
        }
        Date => date_of(v, clock).map_or(Cell::Empty, |(s, e)| Cell::Date(s, e)),
        Checkbox => match v {
            Value::Bool(b) => Cell::Bool(*b),
            Value::Number(n) => Cell::Bool(n.as_f64() != Some(0.0)),
            Value::String(s) => Cell::Bool(crate::normalize::truthy(s) == Some(true)),
            _ => Cell::Empty,
        },
        Person | Relation => {
            let v = ids(v);
            if v.is_empty() {
                Cell::Empty
            } else {
                Cell::Ids(v)
            }
        }
        Files => {
            let items = match v {
                Value::Array(a) => a.clone(),
                v => vec![v.clone()],
            };
            let out: Vec<(Box<str>, Box<str>)> = items
                .iter()
                .filter_map(|x| match x {
                    Value::Object(o) => {
                        let url = o.get("url").and_then(Value::as_str).unwrap_or("");
                        let name = o.get("name").and_then(Value::as_str).unwrap_or(url);
                        (!url.is_empty() || !name.is_empty()).then(|| (name.into(), url.into()))
                    }
                    Value::String(s) => {
                        Some((s.rsplit('/').next().unwrap_or(s).into(), s.as_str().into()))
                    }
                    _ => None,
                })
                .collect();
            if out.is_empty() {
                Cell::Empty
            } else {
                Cell::Files(out)
            }
        }
        _ => Cell::Empty,
    }
}
