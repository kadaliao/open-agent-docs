//! 文档结构 → 只读 HTML（嵌到别人的网页里直接显示，配 `content.css`）。服务器 `?format=html` 和 WASM 的 `toHtml` 都走这里，输出逐字一样。
//!
//! 安全：HTML 是按节点一个个拼出来的，不透传任何原始 HTML。
//! - 文字、属性值都转义（属性值连引号一起转）；
//! - 只出固定的标签和属性，没有 `<script>`、`style` 以外的内联样式（只有图片宽度，是夹在 10–100 之间的整数）、事件属性；
//! - 链接只认 http / https / mailto / tel 和相对地址，图片只认 http / https 和相对地址；别的（`javascript:`、`data:`……）去掉链接、留下文字。
//!
//! 需要脚本才画得出来的块（公式、链接卡片、内嵌库）出成带 `data-*` 的占位，文字留着：宿主想画就照着 `data-*` 自己画。
//! 根相对的地址（`/api/v1/files/…`、`/p/…`）在给了 `base_url` 时拼成完整地址（嵌在别的域名下也打得开）。
//! 服务器还会给本站的文件、图片带上签名（`signed`，`?exp=…&sig=…`），不登录也打得开；浏览器里转的没有签名。
use crate::node::{Mark, Node, mark_rank};
use std::collections::HashMap;

/// 转 HTML 的选项。
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HtmlOptions {
    /// 对外的地址（如 `https://docs.example.com`）：`/` 开头的地址前面拼上它。不给就保持相对地址。
    #[serde(default)]
    pub base_url: Option<String>,
    /// 本站文件、图片的签名（只有服务器给）：文件 id 或媒体 hash → `exp=…&sig=…`。
    /// 根相对的 `/api/v1/files/<id>…`、`/api/v1/media/<hash>` 在这里查得到的就把它加到地址后面。
    #[serde(skip)]
    pub signed: HashMap<String, String>,
}

/// 整个文档 → `<div class="oad-content">…</div>`。传进来的不是 `doc` 也行（当成一个块）。
pub fn to_html(doc: &Node, opts: &HtmlOptions) -> String {
    let base = opts
        .base_url
        .as_deref()
        .map(|b| b.trim().trim_end_matches('/'))
        .filter(|b| !b.is_empty() && safe_url(b, Kind::Image, None).is_some());
    let cx = Cx {
        base,
        signed: &opts.signed,
    };
    let mut out = String::from("<div class=\"oad-content\">");
    if doc.ty == "doc" {
        blocks(&doc.content, &cx, &mut out);
    } else {
        block(doc, &cx, &mut out);
    }
    out.push_str("</div>");
    out
}

struct Cx<'a> {
    base: Option<&'a str>,
    signed: &'a HashMap<String, String>,
}

impl Cx<'_> {
    /// 能放进 href / src 的地址：[`safe_url`]，本站文件有签名的带上签名，根相对的拼上 base。
    fn url(&self, raw: &str, kind: Kind) -> Option<String> {
        if self.signed.is_empty() {
            return safe_url(raw, kind, self.base);
        }
        let u = safe_url(raw, kind, None)?;
        let u = match own_asset(&u).and_then(|k| self.signed.get(k)) {
            Some(q) => with_query(&u, q),
            None => u,
        };
        Some(match self.base {
            Some(b) if root_relative(&u) => format!("{b}{u}"),
            _ => u,
        })
    }
}

fn root_relative(u: &str) -> bool {
    u.starts_with('/') && !u[1..].starts_with(['/', '\\'])
}

/// 根相对的 `/api/v1/files/<id>…` → id；`/api/v1/media/<hash>` → hash。
fn own_asset(u: &str) -> Option<&str> {
    if !root_relative(u) {
        return None;
    }
    let rest = u
        .strip_prefix("/api/v1/files/")
        .or_else(|| u.strip_prefix("/api/v1/media/"))?;
    let key = rest.split(['/', '?', '#']).next()?;
    (!key.is_empty()).then_some(key)
}

/// 地址后面加查询参数（`#…` 之前）。
fn with_query(u: &str, q: &str) -> String {
    let (main, frag) = match u.split_once('#') {
        Some((m, f)) => (m, Some(f)),
        None => (u, None),
    };
    let sep = if main.contains('?') { '&' } else { '?' };
    match frag {
        Some(f) => format!("{main}{sep}{q}#{f}"),
        None => format!("{main}{sep}{q}"),
    }
}

const CALLOUT_KINDS: &[(&str, &str)] = &[
    ("note", "i"),
    ("tip", "✦"),
    ("important", "!"),
    ("warning", "⚠"),
    ("caution", "⛔"),
];

const REL: &str = " target=\"_blank\" rel=\"noopener noreferrer nofollow\"";

fn blocks(nodes: &[Node], cx: &Cx, out: &mut String) {
    for n in nodes {
        block(n, cx, out);
    }
}

fn block(n: &Node, cx: &Cx, out: &mut String) {
    match n.ty.as_str() {
        "paragraph" => {
            out.push_str("<p>");
            inline(&n.content, cx, out);
            out.push_str("</p>");
        }
        "heading" => {
            let l = n.attr_i64("level").unwrap_or(1).clamp(1, 6);
            out.push_str(&format!("<h{l}>"));
            inline(&n.content, cx, out);
            out.push_str(&format!("</h{l}>"));
        }
        "blockquote" => {
            out.push_str("<blockquote>");
            blocks(&n.content, cx, out);
            out.push_str("</blockquote>");
        }
        "callout" => {
            let (kind, icon) = CALLOUT_KINDS
                .iter()
                .find(|(k, _)| Some(*k) == n.attr_str("kind"))
                .copied()
                .unwrap_or(CALLOUT_KINDS[0]);
            out.push_str(&format!(
                "<div class=\"oad-callout\" data-kind=\"{kind}\"><span class=\"oad-callout-icon\" aria-hidden=\"true\">{icon}</span><div class=\"oad-callout-body\">"
            ));
            if let Some(t) = n.attr_str("title").filter(|t| !t.is_empty()) {
                out.push_str("<p class=\"oad-callout-title\">");
                out.push_str(&esc(t));
                out.push_str("</p>");
            }
            blocks(&n.content, cx, out);
            out.push_str("</div></div>");
        }
        "bulletList" => {
            out.push_str("<ul>");
            items(n, cx, out);
            out.push_str("</ul>");
        }
        "orderedList" => {
            match n.attr_i64("start") {
                Some(s) if s != 1 => out.push_str(&format!("<ol start=\"{s}\">")),
                _ => out.push_str("<ol>"),
            }
            items(n, cx, out);
            out.push_str("</ol>");
        }
        "taskList" => {
            out.push_str("<ul class=\"oad-tasks\">");
            for item in &n.content {
                let done = item.attr_bool("checked");
                out.push_str(if done {
                    "<li class=\"oad-task\" data-checked=\"true\"><input type=\"checkbox\" disabled checked><div>"
                } else {
                    "<li class=\"oad-task\" data-checked=\"false\"><input type=\"checkbox\" disabled><div>"
                });
                blocks(&item.content, cx, out);
                out.push_str("</div></li>");
            }
            out.push_str("</ul>");
        }
        "listItem" | "taskItem" => {
            out.push_str("<li>");
            blocks(&n.content, cx, out);
            out.push_str("</li>");
        }
        "codeBlock" => {
            let lang: String = n
                .attr_str("language")
                .unwrap_or("")
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || "+-_.#".contains(*c))
                .take(40)
                .collect();
            let class = if n.attr_bool("wrap") {
                "oad-code is-wrap"
            } else {
                "oad-code"
            };
            if lang.is_empty() {
                out.push_str(&format!("<pre class=\"{class}\"><code>"));
            } else {
                out.push_str(&format!(
                    "<pre class=\"{class}\" data-language=\"{lang}\"><code class=\"language-{lang}\">"
                ));
            }
            out.push_str(&esc(&n.plain_text()));
            out.push_str("</code></pre>");
        }
        "mathBlock" => {
            let tex = n.attr_str("latex").unwrap_or("");
            out.push_str(&format!(
                "<div class=\"oad-math-block\" data-latex=\"{}\">{}</div>",
                esc(tex),
                esc(tex)
            ));
        }
        "horizontalRule" => out.push_str("<hr>"),
        "image" => image(n, cx, out),
        "embed" => embed(n, cx, out),
        "table" => {
            out.push_str("<div class=\"oad-table\"><table><tbody>");
            for row in &n.content {
                out.push_str("<tr>");
                for cell in &row.content {
                    let tag = if cell.ty == "tableHeader" { "th" } else { "td" };
                    out.push_str(&format!("<{tag}>"));
                    blocks(&cell.content, cx, out);
                    out.push_str(&format!("</{tag}>"));
                }
                out.push_str("</tr>");
            }
            out.push_str("</tbody></table></div>");
        }
        "details" => {
            out.push_str(if n.attr_bool("open") {
                "<details class=\"oad-details\" open>"
            } else {
                "<details class=\"oad-details\">"
            });
            out.push_str("<summary>");
            if let Some(s) = n.content.iter().find(|c| c.ty == "detailsSummary") {
                inline(&s.content, cx, out);
            }
            out.push_str("</summary><div class=\"oad-details-content\">");
            if let Some(c) = n.content.iter().find(|c| c.ty == "detailsContent") {
                blocks(&c.content, cx, out);
            }
            out.push_str("</div></details>");
        }
        _ => {
            // 不认识的块：有子块就画子块，有文字就当一段
            if n.content.is_empty() {
                return;
            }
            if n.content
                .iter()
                .all(|c| !c.is_text() && !crate::node::is_inline_atom(&c.ty))
            {
                blocks(&n.content, cx, out);
            } else {
                out.push_str("<p>");
                inline(&n.content, cx, out);
                out.push_str("</p>");
            }
        }
    }
}

fn items(list: &Node, cx: &Cx, out: &mut String) {
    for item in &list.content {
        out.push_str("<li>");
        blocks(&item.content, cx, out);
        out.push_str("</li>");
    }
}

fn image(n: &Node, cx: &Cx, out: &mut String) {
    let alt = n.attr_str("alt").unwrap_or("");
    let caption = n.attr_str("caption").filter(|c| !c.is_empty());
    let width = n.attr_i64("width").filter(|w| (10..=100).contains(w));
    match width {
        Some(w) => out.push_str(&format!(
            "<figure class=\"oad-image has-width\" style=\"width:{w}%\">"
        )),
        None => out.push_str("<figure class=\"oad-image\">"),
    }
    match n.attr_str("src").and_then(|s| cx.url(s, Kind::Image)) {
        Some(src) => {
            out.push_str(&format!("<img src=\"{}\" alt=\"{}\"", esc(&src), esc(alt)));
            if let Some(t) = n.attr_str("title").filter(|t| !t.is_empty()) {
                out.push_str(&format!(" title=\"{}\"", esc(t)));
            }
            out.push_str(" loading=\"lazy\" decoding=\"async\">");
        }
        // 地址不能用：只留替代文字
        None => out.push_str(&format!(
            "<span class=\"oad-image-alt\">{}</span>",
            esc(alt)
        )),
    }
    if let Some(c) = caption {
        out.push_str(&format!("<figcaption>{}</figcaption>", esc(c)));
    }
    out.push_str("</figure>");
}

fn embed(n: &Node, cx: &Cx, out: &mut String) {
    let raw = n.attr_str("url").unwrap_or("");
    let view = if n.attr_str("view") == Some("bookmark") {
        "bookmark"
    } else {
        "embed"
    };
    let Some(url) = cx.url(raw, Kind::Link) else {
        out.push_str(&format!(
            "<div class=\"oad-embed\" data-type=\"embed\" data-view=\"{view}\"><span>{}</span></div>",
            esc(raw)
        ));
        return;
    };
    let u = esc(&url);
    // 上传的文件：文件名从地址里取
    if let Some(name) = file_name(raw) {
        out.push_str(&format!(
            "<div class=\"oad-embed oad-file\" data-type=\"file\" data-url=\"{u}\" data-view=\"{view}\"><a class=\"oad-file-link\" href=\"{u}\"{REL}>{}</a></div>",
            esc(&name)
        ));
        return;
    }
    out.push_str(&format!(
        "<div class=\"oad-embed\" data-type=\"embed\" data-url=\"{u}\" data-view=\"{view}\"><a class=\"oad-bookmark\" href=\"{u}\"{REL}>{}</a></div>",
        esc(raw)
    ));
}

/// `/api/v1/files/<id>/<文件名>`（相对的或带域名的）→ 解码后的文件名。
fn file_name(url: &str) -> Option<String> {
    let path = match url.find("://") {
        Some(i) => &url[i + 3..][url[i + 3..].find('/')?..],
        None => url,
    };
    let rest = path.strip_prefix("/api/v1/files/")?;
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let (id, name) = rest.split_once('/').unwrap_or((rest, ""));
    let name = percent_decode(name);
    Some(if name.is_empty() {
        id.to_string()
    } else {
        name
    })
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h << 4 | l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

// ---- 行内 ----

/// 一个 mark 的开头和结尾标签；不认识的 / 链接地址不能用的返回 None（这个 mark 不画）。
fn mark_tags(m: &Mark, cx: &Cx) -> Option<(String, &'static str)> {
    let (open, close) = match m.ty.as_str() {
        "bold" => ("<strong>".into(), "</strong>"),
        "italic" => ("<em>".into(), "</em>"),
        "strike" => ("<s>".into(), "</s>"),
        "underline" => ("<u>".into(), "</u>"),
        "highlight" => ("<mark>".into(), "</mark>"),
        "code" => ("<code>".into(), "</code>"),
        "link" => {
            let href = cx.url(m.attr_str("href")?, Kind::Link)?;
            let mut s = format!("<a href=\"{}\"", esc(&href));
            if let Some(t) = m.attr_str("title").filter(|t| !t.is_empty()) {
                s.push_str(&format!(" title=\"{}\"", esc(t)));
            }
            if !href.starts_with('#') {
                s.push_str(REL);
            }
            s.push('>');
            (s, "</a>")
        }
        _ => return None,
    };
    Some((open, close))
}

fn inline(nodes: &[Node], cx: &Cx, out: &mut String) {
    // 像 ProseMirror 的 DOMSerializer：相邻的节点共用开着的 mark，只关、开不一样的那部分
    let mut stack: Vec<(&Mark, &'static str)> = Vec::new();
    for n in nodes {
        let mut marks: Vec<&Mark> = n
            .marks
            .iter()
            // 行内提及自己就是链接，不再套 <a>
            .filter(|m| !(n.ty == "linkMention" && m.ty == "link"))
            .collect();
        marks.sort_by_key(|m| mark_rank(&m.ty));
        let mut tags: Vec<(&Mark, String, &'static str)> = marks
            .into_iter()
            .filter_map(|m| mark_tags(m, cx).map(|(o, c)| (m, o, c)))
            .collect();
        let keep = stack
            .iter()
            .zip(&tags)
            .take_while(|((a, _), (b, _, _))| a == b)
            .count();
        while stack.len() > keep {
            out.push_str(stack.pop().unwrap().1);
        }
        for (m, open, close) in tags.drain(keep..) {
            out.push_str(&open);
            stack.push((m, close));
        }
        inline_node(n, cx, out);
    }
    while let Some((_, close)) = stack.pop() {
        out.push_str(close);
    }
}

fn inline_node(n: &Node, cx: &Cx, out: &mut String) {
    match n.ty.as_str() {
        "text" => out.push_str(&esc(n.text.as_deref().unwrap_or(""))),
        "hardBreak" => out.push_str("<br>"),
        "mathInline" => {
            let tex = n.attr_str("latex").unwrap_or("");
            out.push_str(&format!(
                "<span class=\"oad-math-inline\" data-latex=\"{}\">{}</span>",
                esc(tex),
                esc(tex)
            ));
        }
        "linkMention" => {
            let raw = n.attr_str("url").unwrap_or("");
            match cx.url(raw, Kind::Link) {
                Some(u) => {
                    let u = esc(&u);
                    out.push_str(&format!(
                        "<a class=\"oad-link-mention\" data-url=\"{u}\" href=\"{u}\"{REL}>{}</a>",
                        esc(raw)
                    ));
                }
                None => out.push_str(&format!(
                    "<span class=\"oad-link-mention\">{}</span>",
                    esc(raw)
                )),
            }
        }
        // 不认识的行内节点：只留文字
        _ => out.push_str(&esc(&n.plain_text())),
    }
}

// ---- 地址、转义 ----

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Link,
    Image,
}

/// 能放进 href / src 的地址；不能用返回 None。根相对的地址前面拼上 base。
fn safe_url(raw: &str, kind: Kind, base: Option<&str>) -> Option<String> {
    // 浏览器会忽略前后的空白、控制字符，以及中间的换行和制表符（`java\tscript:` 也算 javascript:），这里照着做
    let u: String = raw
        .trim_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    if u.is_empty() || u.chars().any(|c| c.is_control()) {
        return None;
    }
    // 协议：第一个 `:` 在 `/ ? #` 之前才算
    let end = u.find([':', '/', '?', '#']);
    if let Some(i) = end
        && u.as_bytes()[i] == b':'
    {
        let scheme = u[..i].to_ascii_lowercase();
        let ok = match kind {
            Kind::Link => matches!(scheme.as_str(), "http" | "https" | "mailto" | "tel"),
            Kind::Image => matches!(scheme.as_str(), "http" | "https"),
        };
        return ok.then_some(u);
    }
    // 根相对（不是 `//host`、`/\host` 这种协议相对的）：拼上对外地址
    if let Some(b) = base
        && u.starts_with('/')
        && !u[1..].starts_with(['/', '\\'])
    {
        return Some(format!("{b}{u}"));
    }
    Some(u)
}

/// 文字和属性值都用这个：`& < > " '` 全转。
fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            '\0' => o.push('\u{FFFD}'),
            c => o.push(c),
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Node, parse};

    fn md(s: &str) -> String {
        to_html(&parse(s), &HtmlOptions::default())
    }
    fn json(v: serde_json::Value) -> String {
        to_html(&serde_json::from_value(v).unwrap(), &HtmlOptions::default())
    }
    const TAGS: &[&str] = &[
        "div",
        "p",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "blockquote",
        "span",
        "ul",
        "ol",
        "li",
        "input",
        "pre",
        "code",
        "hr",
        "figure",
        "img",
        "figcaption",
        "a",
        "table",
        "tbody",
        "tr",
        "th",
        "td",
        "details",
        "summary",
        "strong",
        "em",
        "s",
        "u",
        "mark",
        "br",
    ];
    const ATTRS: &[&str] = &[
        "class",
        "data-kind",
        "data-checked",
        "data-language",
        "data-latex",
        "data-type",
        "data-url",
        "data-view",
        "aria-hidden",
        "type",
        "disabled",
        "checked",
        "start",
        "style",
        "src",
        "alt",
        "title",
        "loading",
        "decoding",
        "href",
        "target",
        "rel",
        "open",
    ];

    /// 按白名单检查输出：标签、属性都在名单里，属性值都带引号，链接和图片地址不是危险的协议。
    fn assert_safe(html: &str) {
        let b = html.as_bytes();
        let mut i = 0;
        while let Some(off) = html[i..].find('<') {
            i += off + 1;
            let close = b[i] == b'/';
            if close {
                i += 1;
            }
            let name: String = html[i..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            assert!(
                TAGS.contains(&name.as_str()),
                "标签 <{name}> 不在白名单里：{html}"
            );
            i += name.len();
            // 属性：name 或 name="value"
            loop {
                while b[i] == b' ' {
                    i += 1;
                }
                if b[i] == b'>' {
                    i += 1;
                    break;
                }
                assert!(!close, "结束标签里有属性：{html}");
                let an: String = html[i..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                    .collect();
                assert!(
                    ATTRS.contains(&an.as_str()),
                    "属性 {an} 不在白名单里：{html}"
                );
                i += an.len();
                if b[i] == b'=' {
                    assert_eq!(b[i + 1], b'"', "属性值没带引号：{html}");
                    let end = html[i + 2..].find('"').unwrap() + i + 2;
                    let v = &html[i + 2..end];
                    assert!(!v.contains(['<', '>', '\'']), "属性值没转义：{v}");
                    if an == "href" || an == "src" {
                        let l = v.trim().to_ascii_lowercase();
                        for bad in ["javascript:", "vbscript:", "data:", "file:"] {
                            assert!(!l.starts_with(bad), "{an}={v}：{html}");
                        }
                    }
                    if an == "style" {
                        assert!(
                            v.starts_with("width:")
                                && v.ends_with('%')
                                && v[6..v.len() - 1].parse::<u8>().is_ok(),
                            "style={v}"
                        );
                    }
                    i = end + 1;
                }
            }
        }
    }

    #[test]
    fn basic_blocks() {
        assert_eq!(
            md("# 标题\n\n第一段 **加粗** 和 *斜体*"),
            "<div class=\"oad-content\"><h1>标题</h1><p>第一段 <strong>加粗</strong> 和 <em>斜体</em></p></div>"
        );
        let h = md("- 一\n- 二\n\n3. 三\n4. 四\n\n- [x] 完成\n- [ ] 没完成");
        assert!(
            h.contains("<ul><li><p>一</p></li><li><p>二</p></li></ul>"),
            "{h}"
        );
        assert!(h.contains("<ol start=\"3\">"), "{h}");
        assert!(h.contains("<li class=\"oad-task\" data-checked=\"true\"><input type=\"checkbox\" disabled checked><div><p>完成</p></div></li>"), "{h}");
        let h = md("> [!WARNING] 小心\n> 内容");
        assert!(
            h.contains("<div class=\"oad-callout\" data-kind=\"warning\">"),
            "{h}"
        );
        assert!(
            h.contains("<p class=\"oad-callout-title\">小心</p><p>内容</p>"),
            "{h}"
        );
        let h = md("```rust\nfn main() { a < b }\n```");
        assert!(h.contains("<pre class=\"oad-code\" data-language=\"rust\"><code class=\"language-rust\">fn main() { a &lt; b }</code></pre>"), "{h}");
        let h = md("| a | b |\n| --- | --- |\n| 1 | 2 |");
        assert!(
            h.contains("<div class=\"oad-table\"><table><tbody><tr><th><p>a</p></th>"),
            "{h}"
        );
        let h = md("<details>\n<summary>折叠</summary>\n\n里面\n\n</details>");
        assert!(h.contains("<details class=\"oad-details\"><summary>折叠</summary><div class=\"oad-details-content\"><p>里面</p></div></details>"), "{h}");
        assert!(md("---").contains("<hr>"));
        assert!(md("第一行  \n第二行").contains("第一行<br>第二行"));
    }

    #[test]
    fn marks_nest_and_share() {
        // 相邻的文字共用外层的链接
        let h = md("[**粗**和普通](https://a.com)");
        assert_eq!(
            h,
            "<div class=\"oad-content\"><p><a href=\"https://a.com\" target=\"_blank\" rel=\"noopener noreferrer nofollow\"><strong>粗</strong>和普通</a></p></div>"
        );
        let h = md("==高亮== ~~删除~~ <u>下划线</u> `代码`");
        assert!(
            h.contains("<mark>高亮</mark> <s>删除</s> <u>下划线</u> <code>代码</code>"),
            "{h}"
        );
    }

    #[test]
    fn placeholders_keep_text() {
        let h = md("$$\nE = mc^2\n$$\n\n行内 $a<b$ 公式");
        assert!(
            h.contains("<div class=\"oad-math-block\" data-latex=\"E = mc^2\">E = mc^2</div>"),
            "{h}"
        );
        assert!(
            h.contains("<span class=\"oad-math-inline\" data-latex=\"a&lt;b\">a&lt;b</span>"),
            "{h}"
        );
        let h = md("https://www.youtube.com/watch?v=abc");
        assert!(h.contains("<div class=\"oad-embed\" data-type=\"embed\" data-url=\"https://www.youtube.com/watch?v=abc\" data-view=\"embed\"><a class=\"oad-bookmark\" href=\"https://www.youtube.com/watch?v=abc\""), "{h}");
        let h = md("看 <https://github.com/a/b> 这个");
        assert!(h.contains("<a class=\"oad-link-mention\" data-url=\"https://github.com/a/b\" href=\"https://github.com/a/b\""), "{h}");
    }

    #[test]
    fn files_and_base_url() {
        let mut doc = parse("![图](/api/v1/files/abc123/a.png \"标题\")\n\n[页](/p/xyz) [锚](#a)");
        doc.content.insert(
            1,
            Node::new("embed").with("url", "/api/v1/files/def456/%E6%8A%A5%E5%91%8A.pdf"),
        );
        let rel = to_html(&doc, &HtmlOptions::default());
        assert!(rel.contains("<img src=\"/api/v1/files/abc123/a.png\" alt=\"图\" title=\"标题\" loading=\"lazy\" decoding=\"async\">"), "{rel}");
        assert!(
            rel.contains(
                "data-type=\"file\" data-url=\"/api/v1/files/def456/%E6%8A%A5%E5%91%8A.pdf\""
            ),
            "{rel}"
        );
        assert!(rel.contains(">报告.pdf</a>"), "{rel}");
        let abs = to_html(
            &doc,
            &HtmlOptions {
                base_url: Some("https://docs.example.com/".into()),
                ..Default::default()
            },
        );
        assert!(
            abs.contains("src=\"https://docs.example.com/api/v1/files/abc123/a.png\""),
            "{abs}"
        );
        assert!(
            abs.contains("href=\"https://docs.example.com/api/v1/files/def456/"),
            "{abs}"
        );
        assert!(
            abs.contains("href=\"https://docs.example.com/p/xyz\""),
            "{abs}"
        );
        // 锚点不拼、不开新窗口
        assert!(abs.contains("<a href=\"#a\">锚</a>"), "{abs}");
        // 不像地址的 base 不用
        let bad = to_html(
            &doc,
            &HtmlOptions {
                base_url: Some("javascript:alert(1)//".into()),
                ..Default::default()
            },
        );
        assert_safe(&bad);
        assert!(bad.contains("src=\"/api/v1/files/abc123/a.png\""), "{bad}");
        // 签名：本站的文件带上（在 base 之前加），别的地址不动
        let signed = to_html(
            &doc,
            &HtmlOptions {
                base_url: Some("https://docs.example.com".into()),
                signed: [
                    ("abc123".to_string(), "exp=9&sig=s1".to_string()),
                    ("def456".to_string(), "exp=9&sig=s2".to_string()),
                ]
                .into(),
            },
        );
        assert!(
            signed.contains(
                "src=\"https://docs.example.com/api/v1/files/abc123/a.png?exp=9&amp;sig=s1\""
            ),
            "{signed}"
        );
        assert!(
            signed.contains("data-url=\"https://docs.example.com/api/v1/files/def456/%E6%8A%A5%E5%91%8A.pdf?exp=9&amp;sig=s2\""),
            "{signed}"
        );
        assert!(
            signed.contains("href=\"https://docs.example.com/p/xyz\""),
            "{signed}"
        );
        assert_eq!(with_query("/a?w=1#x", "exp=1"), "/a?w=1&exp=1#x");
        assert_eq!(own_asset("/api/v1/media/ff00?w=3"), Some("ff00"));
        assert_eq!(own_asset("//evil/api/v1/files/abc"), None);
    }

    #[test]
    fn image_width_caption() {
        let h = json(serde_json::json!({"type":"doc","content":[
            {"type":"image","attrs":{"src":"https://i.com/a.png","alt":"a","width":50,"caption":"图注 <b>"}},
            {"type":"image","attrs":{"src":"https://i.com/b.png","width":"100%;background:url(x)"}},
            {"type":"image","attrs":{"src":"https://i.com/c.png","width":5000}}
        ]}));
        assert!(h.contains("<figure class=\"oad-image has-width\" style=\"width:50%\"><img src=\"https://i.com/a.png\" alt=\"a\" loading=\"lazy\" decoding=\"async\"><figcaption>图注 &lt;b&gt;</figcaption></figure>"), "{h}");
        assert_eq!(h.matches("style=").count(), 1, "{h}");
    }

    #[test]
    fn xss_script_and_raw_html() {
        for s in [
            "<script>alert(1)</script>",
            "前面 <script>alert(1)</script> 后面",
            "<img src=x onerror=alert(1)>",
            "<iframe src=\"https://evil.com\"></iframe>",
            "<a href=\"javascript:alert(1)\">点</a>",
            "<div onclick=\"alert(1)\">x</div>",
            "<details>\n<summary><img src=x onerror=alert(1)></summary>\n\n里面\n\n</details>",
            "| <script>x</script> |\n| --- |\n| <img src=x onerror=alert(1)> |",
        ] {
            let h = md(s);
            assert_safe(&h);
        }
        // 原始 HTML 当文字显示出来
        assert!(md("<script>alert(1)</script>").contains("&lt;script&gt;"));
    }

    #[test]
    fn xss_javascript_urls() {
        for s in [
            "[点我](javascript:alert(1))",
            "[点我](JavaScript:alert(1))",
            "[点我](javascript&#58;alert(1))",
            "[点我](<java\tscript:alert(1)>)",
            "[点我](%20javascript:alert(1))",
            "[点我](vbscript:msgbox(1))",
            "[点我](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==)",
            "![图](javascript:alert(1))",
            "![图](data:image/svg+xml,<svg onload=alert(1)>)",
            "<javascript:alert(1)>",
            "javascript:alert(1)",
        ] {
            let h = md(s);
            assert_safe(&h);
            assert!(!h.contains("href=") || !h.contains("点我"), "{s:?} → {h}");
        }
        // 文字还在
        assert!(md("[点我](javascript:alert(1))").contains("点我"));
        // 正常的协议照常
        assert!(md("[信](mailto:a@b.com)").contains("href=\"mailto:a@b.com\""));
        assert!(md("[页](./a/b)").contains("href=\"./a/b\""));
    }

    #[test]
    fn xss_attribute_injection() {
        // 文档 JSON 是别人（Agent、接口）写的，属性值什么都可能有
        let h = json(serde_json::json!({"type":"doc","content":[
            {"type":"paragraph","content":[
                {"type":"text","text":"链接","marks":[{"type":"link","attrs":{"href":"https://a.com/\" onmouseover=\"alert(1)","title":"t\" onfocus=\"alert(1)"}}]},
                {"type":"text","text":"坏","marks":[{"type":"link","attrs":{"href":" \u{1}javascript:alert(1)"}}]},
                {"type":"text","text":"\"><script>alert(1)</script>"},
                {"type":"mathInline","attrs":{"latex":"\"><img src=x onerror=alert(1)>"}},
                {"type":"linkMention","attrs":{"url":"javascript:alert(1)"}},
                {"type":"text","text":"x","marks":[{"type":"evil","attrs":{"onclick":"alert(1)"}}]}
            ]},
            {"type":"image","attrs":{"src":"https://i.com/a.png\" onerror=\"alert(1)","alt":"\" onload=\"alert(1)","title":"'><script>"}},
            {"type":"codeBlock","attrs":{"language":"js\"><script>alert(1)</script>"},"content":[{"type":"text","text":"</code></pre><script>alert(1)</script>"}]},
            {"type":"callout","attrs":{"kind":"note\" onclick=\"alert(1)","title":"\"><script>"},"content":[{"type":"paragraph"}]},
            {"type":"embed","attrs":{"url":"javascript:alert(1)","view":"\" onclick=\"x"}},
            {"type":"embed","attrs":{"url":"https://a.com/\"><script>alert(1)</script>"}},
            {"type":"mathBlock","attrs":{"latex":"</div><script>alert(1)</script>"}},
            {"type":"heading","attrs":{"level":"1><script>"},"content":[{"type":"text","text":"h"}]},
            {"type":"orderedList","attrs":{"start":"1\" onclick=\"x"},"content":[{"type":"listItem","content":[{"type":"paragraph"}]}]},
            {"type":"script","content":[{"type":"text","text":"alert(1)"}]}
        ]}));
        assert_safe(&h);
        assert!(
            h.contains("href=\"https://a.com/&quot; onmouseover=&quot;alert(1)\""),
            "{h}"
        );
        assert!(h.contains("data-kind=\"note\""), "{h}");
        assert!(h.contains("data-language=\"jsscriptalert1script\""), "{h}");
        assert!(h.contains("<h1>h</h1>"), "{h}");
        assert!(h.contains("<p>alert(1)</p>"), "{h}");
    }

    #[test]
    fn all_nodes_fixture() {
        // 浏览器那边的全节点样例：每种节点都画得出来，输出都在白名单里
        let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/editor/test/fixtures/all-nodes.json");
        let Ok(j) = std::fs::read_to_string(fx) else {
            return;
        };
        let doc: Node = serde_json::from_str(&j).unwrap();
        let h = to_html(&doc, &HtmlOptions::default());
        assert_safe(&h);
        for tag in [
            "<h1>",
            "<ul class=\"oad-tasks\">",
            "<ol",
            "<blockquote>",
            "oad-callout",
            "<pre class=\"oad-code",
            "oad-math-block",
            "<hr>",
            "<figure",
            "oad-embed",
            "<table>",
            "<details",
            "oad-link-mention",
            "oad-math-inline",
            "<br>",
        ] {
            assert!(h.contains(tag), "没有 {tag}：{h}");
        }
    }

    #[test]
    fn file_name_parsing() {
        assert_eq!(
            file_name("/api/v1/files/abc/a%20b.pdf").as_deref(),
            Some("a b.pdf")
        );
        assert_eq!(
            file_name("https://x.com/api/v1/files/abc/r.pdf?dl=1").as_deref(),
            Some("r.pdf")
        );
        assert_eq!(file_name("/api/v1/files/abc").as_deref(), Some("abc"));
        assert_eq!(file_name("https://x.com/?/api/v1/files/abc"), None);
        assert_eq!(file_name("/p/abc"), None);
        assert_eq!(percent_decode("%E4%B8%AD%zz%"), "中%zz%");
    }
}
