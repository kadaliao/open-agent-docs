//! 浏览器用的 WASM 接口（`wasm` feature）：包着 [`crate::api::JsonEngine`]，进出都是 JSON 字符串。
//! 包里的 index.js 再包一层，对外是约定里的 `DbEngine`（接受对象，返回对象）。
use crate::api::JsonEngine;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct RawEngine {
    inner: JsonEngine,
}

fn err(e: String) -> JsError {
    JsError::new(&e)
}

#[wasm_bindgen]
impl RawEngine {
    /// `schema`、`opts`（`{ id?, now?, tz?, people? }`）都是 JSON。
    #[wasm_bindgen(constructor)]
    pub fn new(schema: &str, opts: &str) -> Result<RawEngine, JsError> {
        let mut inner = JsonEngine::new(schema, opts).map_err(err)?;
        if !inner.fixed_clock {
            let tz = -(js_sys::Date::new_0().get_timezone_offset() as i32);
            inner.set_clock(js_sys::Date::now(), tz);
        }
        Ok(RawEngine { inner })
    }

    /// 没固定时钟就跟上浏览器的时间（到了下一分钟，用了 now() / today() 的公式重算）。
    fn tick(&mut self) {
        if !self.inner.fixed_clock {
            let tz = -(js_sys::Date::new_0().get_timezone_offset() as i32);
            self.inner.set_clock(js_sys::Date::now(), tz);
        }
    }

    #[wasm_bindgen(js_name = loadRows)]
    pub fn load_rows(&mut self, rows: &str) -> Result<(), JsError> {
        self.inner.load_rows(rows).map_err(err)
    }

    #[wasm_bindgen(js_name = loadRelated)]
    pub fn load_related(&mut self, db: &str, schema: &str, rows: &str) -> Result<(), JsError> {
        self.inner.load_related(db, schema, rows).map_err(err)
    }

    pub fn apply(&mut self, changes: &str) -> Result<String, JsError> {
        self.inner.apply(changes).map_err(err)
    }

    #[wasm_bindgen(js_name = setSchema)]
    pub fn set_schema(&mut self, schema: &str) -> Result<(), JsError> {
        self.inner.set_schema(schema).map_err(err)
    }

    #[wasm_bindgen(js_name = setPeople)]
    pub fn set_people(&mut self, people: &str) -> Result<(), JsError> {
        self.inner.set_people(people).map_err(err)
    }

    /// 固定时钟（毫秒时间戳、时区分钟数）；之后不再跟着浏览器走。
    #[wasm_bindgen(js_name = setClock)]
    pub fn set_clock(&mut self, now_ms: f64, tz: i32) {
        self.inner.fixed_clock = true;
        self.inner.set_clock(now_ms, tz);
    }

    pub fn query(&mut self, view: &str) -> Result<String, JsError> {
        self.tick();
        self.inner.query(view).map_err(err)
    }

    pub fn computed(&mut self, row: &str) -> String {
        self.tick();
        self.inner.computed(row)
    }

    pub fn display(&mut self, key: &str, value: &str) -> Result<String, JsError> {
        self.tick();
        self.inner.display(key, value).map_err(err)
    }

    #[wasm_bindgen(js_name = displayRow)]
    pub fn display_row(&mut self, row: &str) -> String {
        self.tick();
        self.inner.display_row(row)
    }

    #[wasm_bindgen(js_name = checkFormula)]
    pub fn check_formula(&self, src: &str, key: Option<String>) -> String {
        self.inner.check_formula(src, key.as_deref())
    }

    pub fn normalize(&mut self, key: &str, input: &str) -> Result<String, JsError> {
        self.tick();
        self.inner.normalize(key, input).map_err(err)
    }

    #[wasm_bindgen(js_name = schemaErrors)]
    pub fn schema_errors(&self) -> String {
        self.inner.schema_errors()
    }
}
