//! 浏览器用的 WASM 接口（`wasm` feature）：和服务器同一份代码，结果一致。
use wasm_bindgen::prelude::*;

/// Markdown → 文档（ProseMirror JSON 字符串）。
#[wasm_bindgen]
pub fn parse(md: &str) -> String {
    serde_json::to_string(&crate::parse(md)).unwrap_or_else(|_| "null".into())
}

/// 文档（JSON 字符串）→ Markdown。
#[wasm_bindgen]
pub fn serialize(doc: &str) -> Result<String, JsError> {
    let n: crate::Node =
        serde_json::from_str(doc).map_err(|e| JsError::new(&format!("文档格式不对：{e}")))?;
    Ok(crate::serialize(&n))
}

/// 一块 → Markdown（复制一块时用）。
#[wasm_bindgen(js_name = serializeBlock)]
pub fn serialize_block(block: &str) -> Result<String, JsError> {
    let n: crate::Node =
        serde_json::from_str(block).map_err(|e| JsError::new(&format!("块格式不对：{e}")))?;
    Ok(crate::serialize_block(&n))
}
