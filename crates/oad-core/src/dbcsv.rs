//! CSV 导入（猜每一列的类型）和导出（给人看的文字）。
use crate::db::{DbInfo, NewDb, RowInput};
use crate::dbengine as eng;
use crate::model::*;
use crate::{Core, Error, Result};
use oad_db::schema::{Prop, PropType};
use oad_db::{Bare, Clock, Lookup};
use serde_json::{Map, Value, json};
use std::collections::HashSet;

/// 最多导入几行、几列。
pub const MAX_IMPORT_ROWS: usize = 50_000;
pub const MAX_IMPORT_COLS: usize = 200;

/// 解析 CSV：去掉 BOM，自动认 `,` / `;` / 制表符分隔。返回 (表头, 行)。
pub fn parse_csv(data: &[u8]) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let data = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
    let text = String::from_utf8_lossy(data);
    let first = text.lines().next().unwrap_or("");
    let delim = b",\t;"
        .iter()
        .copied()
        .max_by_key(|d| first.bytes().filter(|b| b == d).count())
        .unwrap_or(b',');
    let mut r = csv::ReaderBuilder::new()
        .delimiter(delim)
        .flexible(true)
        .has_headers(false)
        .from_reader(text.as_bytes());
    let mut it = r.records();
    let header: Vec<String> = match it.next() {
        Some(h) => h
            .map_err(|e| Error::BadRequest(format!("CSV 读不了：{e}")))?
            .iter()
            .map(|s| s.trim().to_string())
            .collect(),
        None => return Err(Error::BadRequest("CSV 是空的".into())),
    };
    if header.len() > MAX_IMPORT_COLS {
        return Err(Error::BadRequest(format!("最多 {MAX_IMPORT_COLS} 列")));
    }
    let mut rows = Vec::new();
    for rec in it {
        let rec = rec.map_err(|e| Error::BadRequest(format!("CSV 读不了：{e}")))?;
        if rec.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if rows.len() >= MAX_IMPORT_ROWS {
            return Err(Error::BadRequest(format!("最多导入 {MAX_IMPORT_ROWS} 行")));
        }
        let mut row: Vec<String> = rec.iter().map(str::to_string).collect();
        row.resize(header.len(), String::new());
        rows.push(row);
    }
    Ok((header, rows))
}

/// 一列值的样子（不含空的）猜类型。
pub fn guess_type(values: &[&str], clock: &Clock) -> PropType {
    let vals: Vec<&str> = values
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if vals.is_empty() {
        return PropType::Text;
    }
    let all = |f: &dyn Fn(&str) -> bool| vals.iter().all(|v| f(v));
    let boolish = |s: &str| {
        matches!(
            s.to_lowercase().as_str(),
            "true"
                | "false"
                | "yes"
                | "no"
                | "是"
                | "否"
                | "✓"
                | "✔"
                | "✗"
                | "✘"
                | "√"
                | "×"
                | "checked"
                | "unchecked"
                | "x"
        )
    };
    if all(&boolish) {
        return PropType::Checkbox;
    }
    if all(&|s| oad_db::num::parse(s).is_some()) {
        return PropType::Number;
    }
    if all(&|s| oad_db::date::parse(s, clock).is_some() || date_range(s, clock)) {
        return PropType::Date;
    }
    if all(&|s| {
        (s.starts_with("http://") || s.starts_with("https://")) && !s.contains(char::is_whitespace)
    }) {
        return PropType::Url;
    }
    if all(&|s| {
        let mut p = s.split('@');
        matches!((p.next(), p.next(), p.next()), (Some(a), Some(b), None) if !a.is_empty() && b.contains('.') && !s.contains(' '))
    }) {
        return PropType::Email;
    }
    if all(&|s| {
        let d = s.chars().filter(char::is_ascii_digit).count();
        (7..=15).contains(&d)
            && s.chars().all(|c| c.is_ascii_digit() || " +-()".contains(c))
            && s.starts_with(['+', '('])
    }) {
        return PropType::Phone;
    }
    // 选项：值短、重复多
    let short = all(&|s| s.chars().count() <= 30);
    if short && vals.len() >= 4 {
        let parts = |s: &str| -> Vec<String> {
            s.split([',', '，', '、', ';', '；'])
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        };
        let multi = vals.iter().any(|v| parts(v).len() > 1);
        let distinct: HashSet<String> = if multi {
            vals.iter().flat_map(|v| parts(v)).collect()
        } else {
            vals.iter().map(|v| v.to_string()).collect()
        };
        let n = if multi {
            vals.iter().map(|v| parts(v).len()).sum()
        } else {
            vals.len()
        };
        let limit = (n / 2).clamp(1, 50);
        if distinct.len() <= limit {
            return if multi {
                PropType::MultiSelect
            } else {
                PropType::Select
            };
        }
    }
    PropType::Text
}

fn date_range(s: &str, clock: &Clock) -> bool {
    let (a, b) = oad_db::date::split_range(s);
    b.is_some_and(|b| {
        oad_db::date::parse(a, clock).is_some() && oad_db::date::parse(b, clock).is_some()
    })
}

/// 表头 → 名字：去掉空白；空的叫“列 N”；重名的加序号。
fn column_names(header: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    header
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let base = h.split_whitespace().collect::<Vec<_>>().join(" ");
            let base = if base.is_empty() {
                format!("列 {}", i + 1)
            } else {
                base.chars().take(100).collect()
            };
            let mut n = base.clone();
            let mut k = 2;
            while !seen.insert(n.to_lowercase()) {
                n = format!("{base} {k}");
                k += 1;
            }
            n
        })
        .collect()
}

/// 标题列：叫名称 / 标题 / name / title 的，没有就第一列。
fn title_column(names: &[String]) -> usize {
    names
        .iter()
        .position(|n| {
            matches!(
                n.to_lowercase().as_str(),
                "名称" | "标题" | "名字" | "name" | "title"
            )
        })
        .unwrap_or(0)
}

/// 一格 CSV 文字 → 存的样子（认不出的留空）。
fn cell_value(p: &Prop, s: &str, look: &dyn Lookup) -> (Option<Value>, Vec<oad_db::SelectOption>) {
    let s = s.trim();
    if s.is_empty() {
        return (None, vec![]);
    }
    match oad_db::normalize::normalize(p, &Value::String(s.into()), look, true) {
        Ok(n) if !n.value.is_null() => (Some(n.value), n.new_options),
        _ => (None, vec![]),
    }
}

/// 导出时一格的文字（多值用逗号分开，日期 ISO，勾选“是 / 否”）。
pub fn export_cell(p: &Prop, v: &Value, look: &dyn Lookup) -> String {
    if v.is_null() {
        return String::new();
    }
    match p.kind {
        PropType::Checkbox => {
            if v.as_bool().unwrap_or(false) {
                "是".into()
            } else {
                "否".into()
            }
        }
        PropType::Date => {
            let s = v["start"].as_str().unwrap_or("").to_string();
            match v["end"].as_str() {
                Some(e) => format!("{s} → {e}"),
                None => s,
            }
        }
        PropType::Number => v.as_f64().map(oad_db::num::text).unwrap_or_default(),
        _ => oad_db::display::display(p, v, look),
    }
}

pub fn write_csv(header: &[String], rows: &[Vec<String>]) -> Result<String> {
    let mut w = csv::WriterBuilder::new().from_writer(Vec::new());
    w.write_record(header)
        .map_err(|e| Error::Internal(e.to_string()))?;
    for r in rows {
        w.write_record(r)
            .map_err(|e| Error::Internal(e.to_string()))?;
    }
    let bytes = w.into_inner().map_err(|e| Error::Internal(e.to_string()))?;
    // 带 BOM，Excel 打开不乱码
    Ok(format!("\u{FEFF}{}", String::from_utf8_lossy(&bytes)))
}

impl Core {
    /// 导入 CSV 成一个新库：每列猜类型（勾选、数字、日期、网址、邮箱、电话、单选、多选，别的是文字），标题列是
    /// 叫“名称 / name / title”的那列（没有就第一列）。
    pub async fn import_csv(
        &self,
        workspace_id: &str,
        who: &Principal,
        title: Option<String>,
        parent_id: Option<String>,
        data: &[u8],
    ) -> Result<DbInfo> {
        self.require(workspace_id, who, true).await?;
        let (header, rows) = parse_csv(data)?;
        let names = column_names(&header);
        let tcol = title_column(&names);
        let clock = self.db_clock();
        let look = Bare(clock);
        let mut props: Vec<Prop> = Vec::with_capacity(names.len());
        let mut taken = HashSet::new();
        for (i, n) in names.iter().enumerate() {
            let kind = if i == tcol {
                PropType::Title
            } else {
                let col: Vec<&str> = rows.iter().map(|r| r[i].as_str()).collect();
                guess_type(&col, &clock)
            };
            let key = eng::fresh_key(&taken);
            taken.insert(key.clone());
            let mut p = Prop::new(&key, n, kind);
            if kind == PropType::Number {
                let col: Vec<&str> = rows
                    .iter()
                    .map(|r| r[i].trim())
                    .filter(|s| !s.is_empty())
                    .collect();
                p.format = number_format(&col);
            }
            props.push(p);
        }
        // 每格换成存的样子；单选、多选的选项边读边加
        let mut inputs = Vec::with_capacity(rows.len());
        for r in &rows {
            let mut m = Map::new();
            for (i, p) in props.iter_mut().enumerate() {
                let (v, opts) = cell_value(p, &r[i], &look);
                p.options.extend(opts);
                if let Some(v) = v {
                    m.insert(p.key.clone(), v);
                }
            }
            inputs.push(m);
        }
        // 标题放第一个
        let t = props.remove(tcol);
        props.insert(0, t);
        let schema = json!({ "props": props, "next_id": 1 });
        let schema = eng::check_schema(schema)?;
        let db = self
            .create_database(
                workspace_id,
                who,
                NewDb {
                    title: title.or(Some("导入的表格".into())),
                    parent_id,
                    schema: Some(schema),
                    ..Default::default()
                },
            )
            .await?;
        if !inputs.is_empty() {
            let rows = inputs
                .into_iter()
                .map(|props| RowInput {
                    props,
                    after: None,
                    body: None,
                })
                .collect();
            self.add_db_rows_raw(&db.page, who, rows).await?;
        }
        self.database_info(&db.page.id, who).await
    }

    /// 导出 CSV：表头是属性名，按库里的顺序，算出来的值（公式、汇总）也导出；按钮不导出。返回 (库标题, CSV)。
    pub async fn export_csv(&self, db_id: &str, who: &Principal) -> Result<(String, String)> {
        let (page, meta) = self.db_for(db_id, who, false).await?;
        let c = self.engine(&page, &meta).await?;
        let cols: Vec<Prop> = c
            .eng
            .schema()
            .props
            .iter()
            .filter(|p| p.kind != PropType::Button)
            .cloned()
            .collect();
        let header: Vec<String> = cols.iter().map(|p| p.name.clone()).collect();
        let ids = c.eng.row_ids();
        let mut out = Vec::with_capacity(ids.len());
        for id in &ids {
            out.push(
                cols.iter()
                    .map(|p| export_cell(p, &c.eng.value(id, &p.key), &c.eng))
                    .collect(),
            );
        }
        drop(c);
        let csv = tokio::task::spawn_blocking(move || write_csv(&header, &out))
            .await
            .map_err(|e| Error::Internal(e.to_string()))??;
        Ok((page.title, csv))
    }
}

/// 数字列的格式：都带 % 的是百分比，带 ¥ / $ 的是货币，有千分位的用千分位。
fn number_format(col: &[&str]) -> Option<String> {
    let all = |f: &dyn Fn(&str) -> bool| !col.is_empty() && col.iter().all(|s| f(s));
    if all(&|s| s.ends_with('%') || s.ends_with('％')) {
        Some("percent".into())
    } else if all(&|s| s.starts_with('¥') || s.starts_with('￥')) {
        Some("cny".into())
    } else if all(&|s| s.starts_with('$')) {
        Some("usd".into())
    } else if col.iter().any(|s| s.contains(',')) {
        Some("comma".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_types() {
        let c = Clock::new(1_763_000_000_000, 480);
        let g = |v: &[&str]| guess_type(v, &c);
        assert_eq!(g(&["是", "否", "", "是"]), PropType::Checkbox);
        assert_eq!(g(&["1", "0", "1"]), PropType::Number, "0 / 1 是数字");
        assert_eq!(g(&["1,200", "¥3", "50%"]), PropType::Number);
        assert_eq!(
            g(&["2026-11-01", "2026/11/02 14:00", "2026年11月3日"]),
            PropType::Date
        );
        assert_eq!(g(&["https://a.com", "http://b.cn/x"]), PropType::Url);
        assert_eq!(g(&["a@b.cn", "x.y@z.com"]), PropType::Email);
        assert_eq!(
            g(&["+86 138 0000 0000", "(010) 1234-5678"]),
            PropType::Phone
        );
        assert_eq!(g(&["红", "绿", "红", "红", "绿"]), PropType::Select);
        assert_eq!(g(&["红,甜", "甜", "脆", "甜、脆"]), PropType::MultiSelect);
        assert_eq!(
            g(&["一", "二", "三", "四"]),
            PropType::Text,
            "不重复的不是选项"
        );
        assert_eq!(g(&["", " "]), PropType::Text);
    }

    #[test]
    fn parses_csv() {
        let (h, rows) = parse_csv("\u{feff}a;b\n1;\"x;y\"\n;\n2\n".as_bytes()).unwrap();
        assert_eq!(h, vec!["a", "b"]);
        assert_eq!(
            rows,
            vec![
                vec!["1".to_string(), "x;y".into()],
                vec!["2".into(), "".into()]
            ]
        );
        assert!(parse_csv(b"").is_err());
        assert_eq!(
            column_names(&["".into(), "x".into(), "X".into()]),
            vec!["列 1", "x", "X 2"]
        );
        assert_eq!(title_column(&["价格".into(), "Name".into()]), 1);
        let csv = write_csv(&["a".into()], &[vec!["有,逗号".into()]]).unwrap();
        assert_eq!(csv, "\u{feff}a\n\"有,逗号\"\n");
    }
}
