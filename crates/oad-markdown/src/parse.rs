//! Markdown → 文档结构。用 comrak 解析（GFM + 提示框 + 公式 + 高亮 + 中日韩友好的强调规则），再转成节点。
//!
//! 约定（和 serialize.rs 对得上，保证来回不走样）：
//! - 单独一行、什么都不包的网址（`https://…`）是 embed（链接卡片）；`<https://…>` 是行内提及 linkMention；
//!   `[文字](网址)` 和正文里裸露的网址是普通链接。
//! - 段落里的单个换行是 hardBreak（Agent 写的多行文字，本意就是换行）。
//! - 只有图片的段落拆成 image 块。
//! - `<details><summary>标题</summary> … </details>` 是折叠块；`<br>` 单独一行是空段落。
//! - 不认的 HTML 当文字留着，不渲染。
use crate::node::{Mark, Node, merge_texts, sort_marks};
use comrak::nodes::{AlertType, ListType, NodeValue};
use comrak::{Arena, Options, parse_document};

pub fn options() -> Options<'static> {
    let mut o = Options::default();
    let e = &mut o.extension;
    e.strikethrough = true;
    e.table = true;
    e.autolink = true;
    e.tasklist = true;
    e.alerts = true;
    e.math_dollars = true;
    e.highlight = true;
    e.cjk_friendly_emphasis = true;
    o.parse.relaxed_tasklist_matching = true;
    o.render.sourcepos = true;
    o
}

thread_local! {
    /// 正在解析的原文（按行），行内转换时看链接原来是不是 `<网址>` 的写法。
    static SOURCE: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// 行内节点在原文里的第一个字符。
fn source_char(n: N) -> Option<char> {
    let p = n.data.borrow().sourcepos.start;
    SOURCE.with(|s| {
        let s = s.borrow();
        let line = s.get(p.line.checked_sub(1)?)?;
        line.get(p.column.checked_sub(1)?..)?.chars().next()
    })
}

pub fn parse(md: &str) -> Node {
    let md = md.replace("\r\n", "\n");
    SOURCE.with(|s| *s.borrow_mut() = md.lines().map(str::to_string).collect());
    let doc = parse_inner(&md);
    SOURCE.with(|s| s.borrow_mut().clear());
    doc
}

fn parse_inner(md: &str) -> Node {
    let arena = Arena::new();
    let opts = options();
    let root = parse_document(&arena, md, &opts);
    let lines: Vec<&str> = md.split('\n').collect();
    let cx = Cx { lines };
    Node::new("doc").children(cx.blocks(root))
}

type N<'a> = &'a comrak::nodes::AstNode<'a>;

struct Cx<'s> {
    lines: Vec<&'s str>,
}

fn is_url(s: &str) -> bool {
    (s.starts_with("https://") || s.starts_with("http://"))
        && !s.chars().any(char::is_whitespace)
        && s.len() > 8
}

fn html_text(node: N) -> Option<String> {
    match &node.data.borrow().value {
        NodeValue::HtmlBlock(h) => Some(h.literal.clone()),
        _ => None,
    }
}

impl<'s> Cx<'s> {
    /// 节点在原文里的那几行（去掉开头的列偏移）。
    fn source(&self, from: N, to: N) -> String {
        let a = from.data.borrow().sourcepos;
        let b = to.data.borrow().sourcepos;
        let col = a.start.column.saturating_sub(1);
        (a.start.line..=b.end.line)
            .filter_map(|l| self.lines.get(l - 1))
            .map(|l| {
                if l.len() >= col && l.is_char_boundary(col) {
                    &l[col..]
                } else {
                    l.trim_start()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn blocks(&self, parent: N) -> Vec<Node> {
        let kids: Vec<N> = parent.children().collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < kids.len() {
            // <details> … </details>：找到配对的结束块，中间的原文去掉标签后再解析一遍
            if let Some(h) = html_text(kids[i])
                && h.trim_start().to_ascii_lowercase().starts_with("<details")
            {
                let mut depth = 0i32;
                let mut j = i;
                loop {
                    if let Some(t) = html_text(kids[j]) {
                        let low = t.to_ascii_lowercase();
                        depth += low.matches("<details").count() as i32;
                        depth -= low.matches("</details>").count() as i32;
                    }
                    if depth <= 0 || j + 1 == kids.len() {
                        break;
                    }
                    j += 1;
                }
                out.push(details(&self.source(kids[i], kids[j])));
                i = j + 1;
                continue;
            }
            out.extend(self.block(kids[i]));
            i += 1;
        }
        out
    }

    fn block(&self, n: N) -> Vec<Node> {
        let v = n.data.borrow().value.clone();
        match v {
            NodeValue::Paragraph => self.paragraph(n),
            NodeValue::Heading(h) => {
                vec![
                    Node::new("heading")
                        .with("level", h.level as i64)
                        .children(inline_only(n)),
                ]
            }
            NodeValue::List(l) => {
                let task = n
                    .children()
                    .any(|c| matches!(c.data.borrow().value, NodeValue::TaskItem(_)));
                let items: Vec<Node> = n
                    .children()
                    .map(|item| {
                        let checked = match &item.data.borrow().value {
                            NodeValue::TaskItem(t) => t.symbol.is_some_and(|c| c != ' '),
                            _ => false,
                        };
                        let mut body = self.blocks(item);
                        if body.is_empty() || body[0].ty != "paragraph" {
                            body.insert(0, Node::new("paragraph"));
                        }
                        if task {
                            Node::new("taskItem")
                                .with("checked", checked)
                                .children(body)
                        } else {
                            Node::new("listItem").children(body)
                        }
                    })
                    .collect();
                let list = if task {
                    Node::new("taskList")
                } else if l.list_type == ListType::Ordered {
                    let n = Node::new("orderedList");
                    if l.start != 1 {
                        n.with("start", l.start as i64)
                    } else {
                        n
                    }
                } else {
                    Node::new("bulletList")
                };
                vec![list.children(items)]
            }
            NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => {
                vec![Node::new("blockquote").children(non_empty(self.blocks(n)))]
            }
            NodeValue::Alert(a) => {
                let kind = match a.alert_type {
                    AlertType::Note => "note",
                    AlertType::Tip => "tip",
                    AlertType::Important => "important",
                    AlertType::Warning => "warning",
                    AlertType::Caution => "caution",
                    _ => "note",
                };
                let c = Node::new("callout").with("kind", kind);
                let c = match a.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                    Some(t) => c.with("title", t),
                    None => c,
                };
                vec![c.children(non_empty(self.blocks(n)))]
            }
            NodeValue::CodeBlock(c) => {
                let lang = c.info.split_whitespace().next().map(str::to_string);
                let text = c
                    .literal
                    .strip_suffix('\n')
                    .unwrap_or(&c.literal)
                    .to_string();
                let mut cb = Node::new("codeBlock");
                if let Some(l) = lang {
                    cb = cb.with("language", l);
                }
                if !text.is_empty() {
                    cb.content.push(Node::text(text, vec![]));
                }
                vec![cb]
            }
            NodeValue::MathBlock(m) => vec![math_block(&m.literal)],
            NodeValue::ThematicBreak(_) => vec![Node::new("horizontalRule")],
            NodeValue::Table(_) => {
                let rows: Vec<Node> = n
                    .children()
                    .map(|row| {
                        let header =
                            matches!(&row.data.borrow().value, NodeValue::TableRow(r) if r.header);
                        let cells = row
                            .children()
                            .map(|cell| {
                                let para = Node::new("paragraph").children(inline_only(cell));
                                Node::new(if header { "tableHeader" } else { "tableCell" })
                                    .children(vec![para])
                            })
                            .collect();
                        Node::new("tableRow").children(cells)
                    })
                    .collect();
                vec![Node::new("table").children(rows)]
            }
            NodeValue::HtmlBlock(h) => {
                let t = h.literal.trim();
                if matches!(t.to_ascii_lowercase().as_str(), "<br>" | "<br/>" | "<br />") {
                    vec![Node::new("paragraph")]
                } else {
                    // 不认的 HTML：原样当文字
                    vec![Node::new("paragraph").children(hard_lines(t))]
                }
            }
            NodeValue::FootnoteDefinition(_) | NodeValue::FrontMatter(_) => vec![],
            _ => self.blocks(n),
        }
    }

    fn paragraph(&self, n: N) -> Vec<Node> {
        let kids: Vec<N> = n.children().collect();
        // 单独一行的网址 → 链接卡片（原文就是网址本身，不带尖括号、不是 [文字](网址)）
        if kids.len() == 1 {
            if let NodeValue::Link(l) = &kids[0].data.borrow().value {
                let raw = self.source(n, n);
                if raw.trim() == l.url && is_url(&l.url) {
                    return vec![Node::new("embed").with("url", l.url.clone())];
                }
            }
            // 单独的 $$…$$
            if let NodeValue::Math(m) = &kids[0].data.borrow().value
                && m.display_math
            {
                return vec![math_block(&m.literal)];
            }
        }
        // 图片拆出来当块，前后的文字各自成段
        let mut out = Vec::new();
        let mut cur: Vec<Node> = Vec::new();
        for k in inline_children(n, &[]) {
            if k.ty == "image" {
                flush(&mut cur, &mut out);
                out.push(k);
            } else {
                cur.push(k);
            }
        }
        flush(&mut cur, &mut out);
        out
    }
}

fn flush(cur: &mut Vec<Node>, out: &mut Vec<Node>) {
    // 去掉图片两边留下的换行
    while cur.first().is_some_and(|n| n.ty == "hardBreak") {
        cur.remove(0);
    }
    while cur.last().is_some_and(|n| n.ty == "hardBreak") {
        cur.pop();
    }
    let content = merge_texts(std::mem::take(cur));
    if content
        .iter()
        .any(|n| !n.is_text() || !n.text.as_deref().unwrap_or("").trim().is_empty())
    {
        out.push(Node::new("paragraph").children(content));
    }
}

fn non_empty(mut v: Vec<Node>) -> Vec<Node> {
    if v.is_empty() {
        v.push(Node::new("paragraph"));
    }
    v
}

fn math_block(latex: &str) -> Node {
    Node::new("mathBlock").with("latex", latex.trim_matches('\n').to_string())
}

fn hard_lines(t: &str) -> Vec<Node> {
    let mut v = Vec::new();
    for (i, l) in t.split('\n').enumerate() {
        if i > 0 {
            v.push(Node::new("hardBreak"));
        }
        if !l.is_empty() {
            v.push(Node::text(l, vec![]));
        }
    }
    v
}

/// `<details>` 原文 → 折叠块。
fn details(src: &str) -> Node {
    let low = src.to_ascii_lowercase();
    let open_end = low.find('>').map(|i| i + 1).unwrap_or(0);
    let open = low[..open_end].contains(" open");
    let mut rest_start = open_end;
    let mut title = String::new();
    if let Some(s) = low[open_end..].find("<summary") {
        let s = open_end + s;
        if let (Some(gt), Some(e)) = (low[s..].find('>'), low[s..].find("</summary>")) {
            title = src[s + gt + 1..s + e].trim().to_string();
            rest_start = s + e + "</summary>".len();
        }
    }
    let end = low.rfind("</details>").unwrap_or(src.len());
    let inner = if rest_start <= end {
        &src[rest_start..end]
    } else {
        ""
    };
    let body = parse(inner.trim_matches('\n')).content;
    let title = unescape_html(&title);
    let summary = Node::new("detailsSummary").children(if title.is_empty() {
        vec![]
    } else {
        vec![Node::text(title, vec![])]
    });
    let d = Node::new("details").children(vec![
        summary,
        Node::new("detailsContent").children(non_empty(body)),
    ]);
    if open { d.with("open", true) } else { d }
}

pub fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn plain(n: N) -> String {
    let mut s = String::new();
    for d in n.descendants() {
        match &d.data.borrow().value {
            NodeValue::Text(t) => s.push_str(t),
            NodeValue::Code(c) => s.push_str(&c.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => s.push(' '),
            _ => {}
        }
    }
    s
}

/// 只能放行内内容的地方（标题、表格）：图片退成文字。
fn inline_only(n: N) -> Vec<Node> {
    let v = inline_children(n, &[])
        .into_iter()
        .map(|k| {
            if k.ty == "image" {
                Node::text(
                    format!(
                        "![{}]({})",
                        k.attr_str("alt").unwrap_or(""),
                        k.attr_str("src").unwrap_or("")
                    ),
                    vec![],
                )
            } else {
                k
            }
        })
        .collect();
    merge_texts(v)
}

fn with_mark(marks: &[Mark], m: Mark) -> Vec<Mark> {
    let mut v: Vec<Mark> = marks.iter().filter(|x| x.ty != m.ty).cloned().collect();
    v.push(m);
    sort_marks(&mut v);
    v
}

fn inline_children(n: N, marks: &[Mark]) -> Vec<Node> {
    let mut out = Vec::new();
    // 行内 HTML 的成对标签（<u>…</u>、<mark>…</mark>…）按开关处理
    let mut extra: Vec<Mark> = Vec::new();
    for k in n.children() {
        if let NodeValue::HtmlInline(h) = &k.data.borrow().value {
            let tag = h.trim().to_ascii_lowercase();
            let name = tag
                .trim_start_matches("</")
                .trim_start_matches('<')
                .trim_end_matches('>')
                .trim_end_matches('/')
                .trim();
            let closing = tag.starts_with("</");
            let mark = match name {
                "u" | "ins" => Some("underline"),
                "mark" => Some("highlight"),
                "s" | "del" | "strike" => Some("strike"),
                "b" | "strong" => Some("bold"),
                "i" | "em" => Some("italic"),
                _ => None,
            };
            if let Some(m) = mark {
                if closing {
                    extra.retain(|x| x.ty != m);
                } else {
                    extra.push(Mark::new(m));
                }
                continue;
            }
            if name == "br" {
                out.push(Node::new("hardBreak"));
                continue;
            }
        }
        let mut ms = marks.to_vec();
        for e in &extra {
            ms = with_mark(&ms, e.clone());
        }
        out.extend(inline(k, &ms));
    }
    merge_texts(out)
}

fn inline(n: N, marks: &[Mark]) -> Vec<Node> {
    let v = n.data.borrow().value.clone();
    match v {
        NodeValue::Text(t) => vec![Node::text(t.to_string(), marks.to_vec())],
        NodeValue::SoftBreak | NodeValue::LineBreak => vec![Node::new("hardBreak")],
        NodeValue::Code(c) => {
            // 行内代码里不再套别的格式，只保留链接
            let mut ms: Vec<Mark> = marks.iter().filter(|m| m.ty == "link").cloned().collect();
            ms.push(Mark::new("code"));
            vec![Node::text(c.literal, ms)]
        }
        NodeValue::Emph(_) => inline_children(n, &with_mark(marks, Mark::new("italic"))),
        NodeValue::Strong(_) => inline_children(n, &with_mark(marks, Mark::new("bold"))),
        NodeValue::Strikethrough(_) => inline_children(n, &with_mark(marks, Mark::new("strike"))),
        NodeValue::Highlight => inline_children(n, &with_mark(marks, Mark::new("highlight"))),
        NodeValue::Underline => inline_children(n, &with_mark(marks, Mark::new("underline"))),
        NodeValue::Link(l) => {
            // `<https://…>`：行内提及
            if source_char(n) == Some('<')
                && is_url(&l.url)
                && plain(n) == l.url
                && marks.is_empty()
            {
                return vec![Node::new("linkMention").with("url", l.url.clone())];
            }
            let m = Mark::new("link").with("href", l.url.clone());
            let m = if l.title.is_empty() {
                m
            } else {
                m.with("title", l.title.clone())
            };
            inline_children(n, &with_mark(marks, m))
        }
        NodeValue::Image(img) => {
            // 段落会把它拆成图片块；标题、表格里的由 inline_only 退成文字
            let alt = plain(n);
            let mut im = Node::new("image").with("src", img.url.clone());
            if !alt.is_empty() {
                im = im.with("alt", alt);
            }
            if !img.title.is_empty() {
                im = im.with("title", img.title.clone());
            }
            vec![im]
        }
        NodeValue::Math(m) => vec![Node::new("mathInline").with("latex", m.literal)],
        NodeValue::HtmlInline(h) => vec![Node::text(h, marks.to_vec())],
        NodeValue::FootnoteReference(f) => {
            vec![Node::text(format!("[^{}]", f.name), marks.to_vec())]
        }
        _ => inline_children(n, marks),
    }
}
