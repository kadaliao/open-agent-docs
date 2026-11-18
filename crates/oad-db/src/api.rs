//! 和 [`Engine`] 一样的接口，进出都是 JSON 字符串：WASM 版（`wasm` feature）就是包了这一层，
//! 测试里用它生成对照结果（`packages/db-wasm/test/golden.json`），保证浏览器和服务器算出来一样。
//!
//! 为什么用 JSON 字符串而不是 serde-wasm-bindgen：1 万行的数据从网络来本来就是 JSON 文字，直接交给 Rust 解析一遍最快；
//! 逐个对象过 JS ↔ WASM 边界（serde-wasm-bindgen）每个字段都要调一次 JS，大数据量慢得多。结果（1 万个 id）也是一次 JSON.parse。
use crate::Engine;
use crate::date::Clock;
use crate::schema::{Change, Person, RowData, Schema, View};
use serde::Deserialize;
use serde_json::{Value, json};

/// 建引擎时的选项。
#[derive(Deserialize, Default)]
pub struct Options {
    /// 主库自己的 id（关联指向自己时用）
    #[serde(default)]
    pub id: Option<String>,
    /// 固定的“现在”（毫秒时间戳，测试用）；不给就用系统时间（WASM 里每次调用前取浏览器的时间）
    #[serde(default)]
    pub now: Option<f64>,
    /// 时区：当地时间比 UTC 快几分钟（北京时间 480）
    #[serde(default)]
    pub tz: Option<i32>,
    #[serde(default)]
    pub people: Option<Vec<Person>>,
}

fn parse<T: for<'de> Deserialize<'de>>(s: &str, what: &str) -> Result<T, String> {
    serde_json::from_str(s).map_err(|e| format!("{what}的格式不对：{e}"))
}

pub struct JsonEngine {
    pub eng: Engine,
    /// 建的时候给了 now：时钟固定，不跟着系统走
    pub fixed_clock: bool,
}

impl JsonEngine {
    pub fn new(schema: &str, opts: &str) -> Result<JsonEngine, String> {
        let schema: Schema = parse(schema, "表结构")?;
        let opts: Options = if opts.trim().is_empty() {
            Options::default()
        } else {
            parse(opts, "选项")?
        };
        let mut eng = Engine::new(schema);
        if let Some(id) = &opts.id {
            eng.set_id(id);
        }
        let tz = opts.tz.unwrap_or(0);
        let now = opts.now.map(|x| x as i64).unwrap_or(eng.clock().now_ms);
        eng.set_clock(Clock::new(now, tz));
        if let Some(p) = opts.people {
            eng.set_people(p);
        }
        Ok(JsonEngine {
            eng,
            fixed_clock: opts.now.is_some(),
        })
    }

    pub fn load_rows(&mut self, rows: &str) -> Result<(), String> {
        let rows: Vec<RowData> = parse(rows, "行")?;
        self.eng.load_rows(rows);
        Ok(())
    }

    pub fn load_related(&mut self, db: &str, schema: &str, rows: &str) -> Result<(), String> {
        let schema: Schema = parse(schema, "表结构")?;
        let rows: Vec<RowData> = parse(rows, "行")?;
        self.eng.load_related(db, schema, rows);
        Ok(())
    }

    pub fn apply(&mut self, changes: &str) -> Result<String, String> {
        let changes: Vec<Change> = parse(changes, "改动")?;
        Ok(json!(self.eng.apply(changes)).to_string())
    }

    pub fn set_schema(&mut self, schema: &str) -> Result<(), String> {
        let schema: Schema = parse(schema, "表结构")?;
        self.eng.set_schema(schema);
        Ok(())
    }

    pub fn set_people(&mut self, people: &str) -> Result<(), String> {
        let people: Vec<Person> = parse(people, "人员")?;
        self.eng.set_people(people);
        Ok(())
    }

    pub fn set_clock(&mut self, now_ms: f64, tz: i32) {
        self.eng.set_clock(Clock::new(now_ms as i64, tz));
    }

    pub fn query(&self, view: &str) -> Result<String, String> {
        let view: View = if view.trim().is_empty() {
            View::default()
        } else {
            parse(view, "视图")?
        };
        let r = self.eng.query(&view).map_err(|e| e.msg)?;
        Ok(serde_json::to_string(&r).unwrap())
    }

    pub fn computed(&self, row: &str) -> String {
        Value::Object(self.eng.computed(row)).to_string()
    }

    pub fn display(&self, key: &str, value: &str) -> Result<String, String> {
        let v: Value = parse(value, "值")?;
        Ok(self.eng.display(key, &v))
    }

    pub fn display_row(&self, row: &str) -> String {
        Value::Object(self.eng.display_row(row)).to_string()
    }

    /// `{ "ok": true }` 或 `{ "error": "…", "at": 下标 }`（UTF-16，和 JS 字符串一致）。
    pub fn check_formula(&self, src: &str, key: Option<&str>) -> String {
        match self.eng.check_formula(src, key) {
            Ok(()) => json!({ "ok": true }),
            Err(e) => {
                json!({ "error": e.describe(src.trim_end()), "at": e.utf16_pos(src.trim_end()) })
            }
        }
        .to_string()
    }

    pub fn normalize(&self, key: &str, input: &str) -> Result<String, String> {
        let v: Value = parse(input, "值")?;
        self.eng
            .normalize(key, &v)
            .map(|v| v.to_string())
            .map_err(|e| e.msg)
    }

    /// 表结构里有问题的属性：`[{ key, error, at? }]`。
    pub fn schema_errors(&self) -> String {
        Value::Array(
            self.eng
                .schema_errors()
                .into_iter()
                .map(|e| match e.at {
                    Some(at) => json!({ "key": e.key, "error": e.msg, "at": at }),
                    None => json!({ "key": e.key, "error": e.msg }),
                })
                .collect(),
        )
        .to_string()
    }
}
