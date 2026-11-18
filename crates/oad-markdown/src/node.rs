//! 文档结构：和 ProseMirror 的 JSON 同形（type / attrs / content / text / marks），浏览器的 Tiptap 和服务器的 Yrs 都按这个存。
//!
//! 块：paragraph、heading{level}、bulletList / orderedList{start} / listItem、taskList / taskItem{checked}、
//! blockquote、callout{kind, title}、codeBlock{language, wrap}、mathBlock{latex}、horizontalRule、
//! image{src, alt, title, width, caption}、embed{url, view}、table / tableRow / tableHeader / tableCell、
//! details{open} / detailsSummary / detailsContent。
//! 行内：text（marks：bold、italic、strike、code、link{href, title}、underline、highlight）、hardBreak、mathInline{latex}、
//! linkMention{url}（行内提及，Markdown 写成 `<url>`）。
//! 每个块级节点可以有 attrs.id（块编号），这里不生成，由调用方分配。
//!
//! **展示属性**（[`PRESENTATION_ATTRS`]）：只影响显示、Markdown 里没有的属性。从 Markdown 解析出来的节点不带它们；
//! 服务器按 Markdown 改写时，沿用了编号的块保留旧的展示属性（见 oad-doc 的 reconcile）。
//! - `embed.view`：`"embed"`（默认，按网站画富卡片 / 播放器）| `"bookmark"`（书签卡片）
//! - `image.width`：占正文宽度的百分比（10–100），不写 = 原始大小但不超过正文宽
//! - `image.caption`：图注（纯文字）
//! - `codeBlock.wrap`：自动换行
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 展示属性：(节点类型, 属性名)。前后端共用这一份。
pub const PRESENTATION_ATTRS: &[(&str, &str)] = &[
    ("embed", "view"),
    ("image", "width"),
    ("image", "caption"),
    ("codeBlock", "wrap"),
];

/// 这个属性是不是这种节点的展示属性。
pub fn is_presentation(ty: &str, attr: &str) -> bool {
    PRESENTATION_ATTRS
        .iter()
        .any(|(t, a)| *t == ty && *a == attr)
}

/// 行内的原子节点（没有文字、没有块编号）。
pub fn is_inline_atom(ty: &str) -> bool {
    matches!(ty, "hardBreak" | "mathInline" | "linkMention")
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Node {
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub attrs: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Node>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<Mark>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Mark {
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub attrs: Map<String, Value>,
}

impl Mark {
    pub fn new(ty: &str) -> Self {
        Mark {
            ty: ty.into(),
            attrs: Map::new(),
        }
    }
    pub fn with(mut self, k: &str, v: impl Into<Value>) -> Self {
        let v = v.into();
        if !v.is_null() {
            self.attrs.insert(k.into(), v);
        }
        self
    }
    pub fn attr_str(&self, k: &str) -> Option<&str> {
        self.attrs.get(k).and_then(|v| v.as_str())
    }
}

impl Node {
    pub fn new(ty: &str) -> Self {
        Node {
            ty: ty.into(),
            attrs: Map::new(),
            content: Vec::new(),
            text: None,
            marks: Vec::new(),
        }
    }
    pub fn text(s: impl Into<String>, marks: Vec<Mark>) -> Self {
        Node {
            ty: "text".into(),
            attrs: Map::new(),
            content: Vec::new(),
            text: Some(s.into()),
            marks,
        }
    }
    pub fn with(mut self, k: &str, v: impl Into<Value>) -> Self {
        let v = v.into();
        if !v.is_null() {
            self.attrs.insert(k.into(), v);
        }
        self
    }
    pub fn children(mut self, c: Vec<Node>) -> Self {
        self.content = c;
        self
    }
    pub fn attr_str(&self, k: &str) -> Option<&str> {
        self.attrs.get(k).and_then(|v| v.as_str())
    }
    pub fn attr_i64(&self, k: &str) -> Option<i64> {
        self.attrs.get(k).and_then(|v| v.as_i64())
    }
    pub fn attr_bool(&self, k: &str) -> bool {
        self.attrs.get(k).and_then(|v| v.as_bool()).unwrap_or(false)
    }
    pub fn is_text(&self) -> bool {
        self.ty == "text"
    }
    /// 纯文字内容（mark 去掉，hardBreak 当换行）。
    pub fn plain_text(&self) -> String {
        let mut s = String::new();
        fn walk(n: &Node, s: &mut String) {
            if let Some(t) = &n.text {
                s.push_str(t);
            } else if n.ty == "hardBreak" {
                s.push('\n');
            } else if n.ty == "mathInline" {
                s.push_str(n.attr_str("latex").unwrap_or(""));
            } else if n.ty == "linkMention" {
                s.push_str(n.attr_str("url").unwrap_or(""));
            }
            for c in &n.content {
                walk(c, s);
            }
        }
        walk(self, &mut s);
        s
    }
    /// 去掉所有展示属性（和 Markdown 来回的结果比较时用）。
    pub fn without_presentation(&self) -> Node {
        let mut n = self.clone();
        let ty = n.ty.clone();
        n.attrs.retain(|k, _| !is_presentation(&ty, k));
        n.content = n.content.iter().map(|c| c.without_presentation()).collect();
        n
    }

    /// 去掉所有 attrs.id（比较内容时用）。
    pub fn without_ids(&self) -> Node {
        let mut n = self.clone();
        n.attrs.remove("id");
        n.content = n.content.iter().map(|c| c.without_ids()).collect();
        n
    }
}

/// 相邻、marks 一样的 text 合成一个；去掉空 text。
pub fn merge_texts(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(nodes.len());
    for n in nodes {
        if n.is_text() {
            if n.text.as_deref().unwrap_or("").is_empty() {
                continue;
            }
            if let Some(last) = out.last_mut()
                && last.is_text()
                && last.marks == n.marks
            {
                last.text
                    .as_mut()
                    .unwrap()
                    .push_str(n.text.as_deref().unwrap());
                continue;
            }
        }
        out.push(n);
    }
    out
}

/// mark 在一串 marks 里的固定顺序（链接在最外层，行内代码在最里层），比较和输出都按这个排。
pub fn mark_rank(ty: &str) -> u8 {
    match ty {
        "link" => 0,
        "bold" => 1,
        "italic" => 2,
        "strike" => 3,
        "underline" => 4,
        "highlight" => 5,
        "code" => 9,
        _ => 8,
    }
}

pub fn sort_marks(marks: &mut [Mark]) {
    marks.sort_by_key(|m| mark_rank(&m.ty));
}
