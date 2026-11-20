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

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = JSON, js_name = stringify, catch)]
    fn json_stringify(v: &JsValue) -> Result<JsValue, JsValue>;
}

/// 对象 → JSON 字符串（文档、选项是 JS 对象时用）。
fn to_json(v: &JsValue) -> Result<String, JsError> {
    json_stringify(v)
        .ok()
        .and_then(|s| s.as_string())
        .ok_or_else(|| JsError::new("转不成 JSON"))
}

/// Markdown（字符串）或文档（ProseMirror JSON 对象）→ 只读 HTML，和服务器 `?format=html` 逐字一样。
/// `options.baseUrl`：`/` 开头的地址（上传的文件、页面链接）前面拼上它。
#[wasm_bindgen(js_name = toHtml)]
pub fn to_html(
    #[wasm_bindgen(unchecked_param_type = "string | object")] input: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "{ baseUrl?: string }")] options: Option<
        JsValue,
    >,
) -> Result<String, JsError> {
    let doc = match input.as_string() {
        Some(md) => crate::parse(&md),
        None if input.is_object() => serde_json::from_str(&to_json(&input)?)
            .map_err(|e| JsError::new(&format!("文档格式不对：{e}")))?,
        None => return Err(JsError::new("要 Markdown 字符串或文档对象")),
    };
    let opts: crate::HtmlOptions = if let Some(o) = options.filter(|o| o.is_object()) {
        serde_json::from_str(&to_json(&o)?).map_err(|e| JsError::new(&format!("选项不对：{e}")))?
    } else {
        crate::HtmlOptions::default()
    };
    Ok(crate::to_html(&doc, &opts))
}

/// 只读 HTML 的样式表（和服务器 `/embed/content.css`、包里的 `content.css` 同一份）。
#[wasm_bindgen(js_name = contentCss)]
pub fn content_css() -> String {
    crate::CONTENT_CSS.into()
}
