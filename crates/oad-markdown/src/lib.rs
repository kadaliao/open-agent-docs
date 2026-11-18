//! Markdown 和文档结构互转。Agent 读写的都是 Markdown，服务器用这里把它和文档（ProseMirror 形状的节点树）来回转换；
//! 同一份代码编译成 WASM 给浏览器用（复制粘贴、导出），两边结果一致。
//!
//! 保证：`parse(&serialize(&doc))` 和 `doc` 一样（块编号除外）；`serialize(&parse(md))` 再来一遍不变。
pub mod node;
mod parse;
mod serialize;
#[cfg(feature = "wasm")]
pub mod wasm;

pub use node::{Mark, Node};
pub use parse::parse;
pub use serialize::{serialize, serialize_block};

/// 能当链接卡片的网址：http(s) 开头、没有空白。
pub fn is_bare_url(s: &str) -> bool {
    (s.starts_with("https://") || s.starts_with("http://"))
        && s.len() > 8
        && !s.chars().any(char::is_whitespace)
}
