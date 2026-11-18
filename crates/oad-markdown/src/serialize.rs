//! 文档结构 → Markdown。输出要满足：parse(serialize(doc)) == doc（id 除外），给 Agent 读起来也干净。
//!
//! 难点在行内：
//! - mark 的开头不能紧跟空白、结尾不能紧挨空白（`** 加粗**` 不算加粗），所以空白挪到 mark 外面；
//! - 按 CommonMark 的左右侧规则算不成立的位置（比如 `**foo.**bar`），改用 HTML 标签（`<strong>`），parse.rs 认这些标签；
//! - 只在会被误认的地方转义，比如 `_` 只在词边上转义，`~` `$` 只在一段里出现两个以上时转义。
use crate::node::{Mark, Node, sort_marks};

pub fn serialize(doc: &Node) -> String {
    let mut s = blocks(&doc.content, Ctx::default());
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

/// 只输出一个块（给 Agent 按段读的时候用）。
pub fn serialize_block(n: &Node) -> String {
    block(n, Ctx::default())
}

#[derive(Clone, Copy, Default)]
struct Ctx {
    /// 换行要写成 <br>（表格、标题、折叠块标题里）
    inline_br: bool,
    table: bool,
}

fn blocks(nodes: &[Node], cx: Ctx) -> String {
    let mut out = String::new();
    let mut prev: Option<&Node> = None;
    let mut alt = false; // 相邻的同类列表换一种记号，不然会被合成一个列表
    for n in nodes {
        // 无序和待办都用 - / *，有序用 . / )：同一族的列表挨着就换记号
        let family = |t: &str| match t {
            "bulletList" | "taskList" => 1,
            "orderedList" => 2,
            _ => 0,
        };
        let same_list = prev.is_some_and(|p| family(&p.ty) != 0 && family(&p.ty) == family(&n.ty));
        alt = if same_list { !alt } else { false };
        let s = if is_list(&n.ty) {
            list(n, cx, alt)
        } else {
            block(n, cx)
        };
        if let Some(p) = prev {
            out.push_str(if p.ty == "paragraph" && is_list(&n.ty) && cx.table {
                "\n"
            } else {
                "\n\n"
            });
        }
        out.push_str(&s);
        prev = Some(n);
    }
    out
}

fn is_list(t: &str) -> bool {
    matches!(t, "bulletList" | "orderedList" | "taskList")
}

fn prefix_lines(s: &str, first: &str, rest: &str, empty: &str) -> String {
    s.split('\n')
        .enumerate()
        .map(|(i, l)| {
            if l.is_empty() {
                if i == 0 {
                    first.trim_end().to_string()
                } else {
                    empty.to_string()
                }
            } else if i == 0 {
                format!("{first}{l}")
            } else {
                format!("{rest}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn block(n: &Node, cx: Ctx) -> String {
    match n.ty.as_str() {
        "paragraph" => {
            if n.content.is_empty() {
                return "<br>".into();
            }
            let s = inline(&n.content, cx);
            // 整段只是一个网址的文字会被当成链接卡片，包上尖括号
            if crate::is_bare_url(&s) {
                format!("<{s}>")
            } else {
                s
            }
        }
        "heading" => {
            let level = n.attr_i64("level").unwrap_or(1).clamp(1, 6) as usize;
            let mut s = inline(
                &n.content,
                Ctx {
                    inline_br: true,
                    ..cx
                },
            );
            if s.ends_with('#') {
                s.insert(s.len() - 1, '\\');
            }
            format!("{} {}", "#".repeat(level), s)
                .trim_end()
                .to_string()
        }
        "blockquote" => prefix_lines(&blocks(&n.content, cx), "> ", "> ", ">"),
        "callout" => {
            let kind = n.attr_str("kind").unwrap_or("note").to_ascii_uppercase();
            let head = match n.attr_str("title") {
                Some(t) if !t.is_empty() => format!("[!{kind}] {t}"),
                _ => format!("[!{kind}]"),
            };
            let body = blocks(&n.content, cx);
            prefix_lines(&format!("{head}\n{body}"), "> ", "> ", ">")
        }
        "bulletList" | "orderedList" | "taskList" => list(n, cx, false),
        "codeBlock" => {
            let text = n.plain_text();
            let longest = longest_run(&text, '`');
            let fence = "`".repeat(longest.max(2) + 1);
            format!(
                "{fence}{}\n{text}\n{fence}",
                n.attr_str("language").unwrap_or("")
            )
        }
        "mathBlock" => format!(
            "$$\n{}\n$$",
            n.attr_str("latex").unwrap_or("").trim_matches('\n')
        ),
        "horizontalRule" => "---".into(),
        "image" => image(n),
        "embed" => {
            let url = n.attr_str("url").unwrap_or("");
            if crate::is_bare_url(url) {
                url.to_string()
            } else {
                format!("<{url}>")
            }
        }
        "table" => table(n),
        "details" => {
            let summary = n
                .content
                .iter()
                .find(|c| c.ty == "detailsSummary")
                .map(|s| s.plain_text())
                .unwrap_or_default();
            let body = n
                .content
                .iter()
                .find(|c| c.ty == "detailsContent")
                .map(|c| blocks(&c.content, cx))
                .unwrap_or_default();
            let open = if n.attr_bool("open") { " open" } else { "" };
            format!(
                "<details{open}>\n<summary>{}</summary>\n\n{body}\n\n</details>",
                html_escape(&summary.replace('\n', " "))
            )
        }
        _ => {
            // 不认识的块：有子块就输出子块，否则输出文字
            if n.content
                .iter()
                .any(|c| !c.is_text() && c.ty != "hardBreak")
                && n.content.iter().all(|c| !c.is_text())
            {
                blocks(&n.content, cx)
            } else {
                inline(&n.content, cx)
            }
        }
    }
}

fn list(n: &Node, cx: Ctx, alt: bool) -> String {
    let start = n.attr_i64("start").unwrap_or(1);
    let mut items = Vec::new();
    let mut loose = false;
    for (i, item) in n.content.iter().enumerate() {
        let marker = match n.ty.as_str() {
            "orderedList" => format!("{}{} ", start + i as i64, if alt { ")" } else { "." }),
            _ => format!("{} ", if alt { "*" } else { "-" }),
        };
        let check = if n.ty == "taskList" {
            if item.attr_bool("checked") {
                "[x] "
            } else {
                "[ ] "
            }
        } else {
            ""
        };
        // 列表项里：段落后面紧跟子列表时不空行（紧凑），别的块之间空一行
        let mut body = String::new();
        for (k, c) in item.content.iter().enumerate() {
            if k > 0 {
                let tight = item.content[k - 1].ty == "paragraph" && is_list(&c.ty);
                body.push_str(if tight { "\n" } else { "\n\n" });
                if !tight {
                    loose = true;
                }
            }
            body.push_str(&if is_list(&c.ty) {
                list(c, cx, false)
            } else {
                block(c, cx)
            });
        }
        if body == "<br>" {
            body.clear();
        }
        let indent = " ".repeat(marker.len());
        items.push(prefix_lines(
            &format!("{check}{body}"),
            &marker,
            &indent,
            "",
        ));
    }
    items.join(if loose { "\n\n" } else { "\n" })
}

fn image(n: &Node) -> String {
    let alt = n
        .attr_str("alt")
        .unwrap_or("")
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    let src = n.attr_str("src").unwrap_or("");
    let src = if src
        .chars()
        .any(|c| c.is_whitespace() || c == '(' || c == ')' || c == '<' || c == '>')
    {
        format!("<{}>", src.replace('<', "%3C").replace('>', "%3E"))
    } else {
        src.to_string()
    };
    match n.attr_str("title") {
        Some(t) if !t.is_empty() => format!("![{alt}]({src} \"{}\")", t.replace('"', "\\\"")),
        _ => format!("![{alt}]({src})"),
    }
}

fn table(n: &Node) -> String {
    let rows: Vec<Vec<String>> = n
        .content
        .iter()
        .map(|r| {
            r.content
                .iter()
                .map(|cell| {
                    let parts: Vec<String> =
                        cell.content
                            .iter()
                            .map(|p| {
                                if p.content.iter().all(|c| {
                                    c.is_text() || c.ty == "hardBreak" || c.ty == "mathInline"
                                }) {
                                    inline(
                                        &p.content,
                                        Ctx {
                                            inline_br: true,
                                            table: true,
                                        },
                                    )
                                } else {
                                    escape_text(
                                        &p.plain_text().replace('\n', " "),
                                        &mut Esc {
                                            table: true,
                                            ..Default::default()
                                        },
                                    )
                                }
                            })
                            .collect();
                    parts.join("<br>")
                })
                .collect()
        })
        .collect();
    if rows.is_empty() {
        return String::new();
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let line = |r: &Vec<String>| {
        let mut cells: Vec<String> = r.clone();
        cells.resize(cols, String::new());
        format!("| {} |", cells.join(" | "))
    };
    let mut out = vec![line(&rows[0]), format!("|{}", " --- |".repeat(cols))];
    out.extend(rows[1..].iter().map(line));
    out.join("\n")
}

fn longest_run(s: &str, ch: char) -> usize {
    let (mut best, mut cur) = (0, 0);
    for c in s.chars() {
        if c == ch {
            cur += 1;
            best = best.max(cur);
        } else {
            cur = 0;
        }
    }
    best
}

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ---------------- 行内 ----------------

enum Kind {
    Text(String),
    Code(String),
    Math(String),
    /// 行内提及：`<网址>`
    Mention(String),
    Break,
}

struct Piece {
    kind: Kind,
    marks: Vec<Mark>,
}

enum Tok {
    Open(usize),
    Close(usize),
    Piece(usize),
}

fn is_ws(c: Option<char>) -> bool {
    c.is_none_or(char::is_whitespace)
}

fn is_punct(c: char) -> bool {
    c.is_ascii_punctuation() || (!c.is_alphanumeric() && !c.is_whitespace())
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x2E80..=0x2FFF | 0x3000..=0x303F | 0x3040..=0x30FF | 0x3100..=0x31FF |
        0x3200..=0x4DBF | 0x4E00..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF |
        0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
}

/// `*`、`~~`、`==` 这类分隔符放在 prev 和 next 之间，能不能当开头（open=true）/ 结尾用。
fn flanking_ok(prev: Option<char>, next: Option<char>, open: bool) -> bool {
    let (inner, outer) = if open { (next, prev) } else { (prev, next) };
    if is_ws(inner) {
        return false;
    }
    let inner = inner.unwrap();
    if inner.is_alphanumeric() && !is_punct(inner) {
        return true;
    }
    // inner 是标点：外侧得是空白、标点或开头结尾
    if is_ws(outer) || outer.is_some_and(is_punct) {
        return true;
    }
    // 中日韩友好规则：挨着的是中日韩字符也行
    is_cjk(inner) || outer.is_some_and(is_cjk)
}

fn md_delims(ty: &str) -> Option<(&'static str, &'static str)> {
    match ty {
        "bold" => Some(("**", "**")),
        "italic" => Some(("*", "*")),
        "strike" => Some(("~~", "~~")),
        "highlight" => Some(("==", "==")),
        _ => None,
    }
}

fn html_tag(ty: &str) -> &'static str {
    match ty {
        "bold" => "strong",
        "italic" => "em",
        "strike" => "s",
        "highlight" => "mark",
        _ => "u",
    }
}

#[derive(Default)]
struct Esc {
    table: bool,
    multi_dollar: bool,
    double_eq: bool,
    line_start: bool,
    prev: Option<char>,
}

/// 行内输出。用了 `*` `~~` `==` 这类记号时，马上解析回来核对一遍；对不上（强调的配对规则太复杂，
/// 特别是中日韩标点挨着记号的时候），就把这些格式全换成 HTML 标签再输出一次。
fn inline(nodes: &[Node], cx: Ctx) -> String {
    let (s, expected, used_md) = inline_with(nodes, cx, false);
    if !used_md {
        return s;
    }
    let back = crate::parse::parse(&s);
    if back.content.len() == 1
        && back.content[0].ty == "paragraph"
        && back.content[0].content == expected
    {
        return s;
    }
    inline_with(nodes, cx, true).0
}

fn inline_with(nodes: &[Node], cx: Ctx, all_html: bool) -> (String, Vec<Node>, bool) {
    // 1. 切成片：每段文字拆成 前导空白 / 正文 / 末尾空白
    let mut pieces: Vec<Piece> = Vec::new();
    let mut ws_flags: Vec<i8> = Vec::new(); // -1 前导空白，1 末尾空白，0 别的
    for n in nodes {
        let mut marks: Vec<Mark> = n.marks.iter().filter(|m| m.ty != "code").cloned().collect();
        sort_marks(&mut marks);
        match n.ty.as_str() {
            "text" => {
                let t = n.text.as_deref().unwrap_or("");
                if n.marks.iter().any(|m| m.ty == "code") {
                    pieces.push(Piece {
                        kind: Kind::Code(t.to_string()),
                        marks,
                    });
                    ws_flags.push(0);
                    continue;
                }
                for (i, line) in t.split('\n').enumerate() {
                    if i > 0 {
                        pieces.push(Piece {
                            kind: Kind::Break,
                            marks: marks.clone(),
                        });
                        ws_flags.push(0);
                    }
                    let core = line.trim();
                    if core.is_empty() {
                        if !line.is_empty() {
                            pieces.push(Piece {
                                kind: Kind::Text(line.to_string()),
                                marks: marks.clone(),
                            });
                            ws_flags.push(-1);
                        }
                        continue;
                    }
                    let lead = &line[..line.len() - line.trim_start().len()];
                    let trail = &line[line.trim_end().len()..];
                    if !lead.is_empty() {
                        pieces.push(Piece {
                            kind: Kind::Text(lead.to_string()),
                            marks: marks.clone(),
                        });
                        ws_flags.push(-1);
                    }
                    pieces.push(Piece {
                        kind: Kind::Text(core.to_string()),
                        marks: marks.clone(),
                    });
                    ws_flags.push(0);
                    if !trail.is_empty() {
                        pieces.push(Piece {
                            kind: Kind::Text(trail.to_string()),
                            marks: marks.clone(),
                        });
                        ws_flags.push(1);
                    }
                }
            }
            "hardBreak" => {
                pieces.push(Piece {
                    kind: Kind::Break,
                    marks,
                });
                ws_flags.push(0);
            }
            "mathInline" => {
                pieces.push(Piece {
                    kind: Kind::Math(n.attr_str("latex").unwrap_or("").trim().to_string()),
                    marks,
                });
                ws_flags.push(0);
            }
            "linkMention" => {
                let url = n.attr_str("url").unwrap_or("").to_string();
                // 提及不带格式（`<网址>` 外面套 ** 之类解析回来会是链接）；网址写不成 <…> 的退成普通文字
                let kind = if mention_ok(&url) {
                    Kind::Mention(url)
                } else {
                    Kind::Text(url)
                };
                pieces.push(Piece {
                    kind,
                    marks: Vec::new(),
                });
                ws_flags.push(0);
            }
            _ => {
                pieces.push(Piece {
                    kind: Kind::Text(n.plain_text()),
                    marks,
                });
                ws_flags.push(0);
            }
        }
    }
    // 2. 空白上不开新 mark：前导空白只保留和前一片共有的，末尾空白只保留和后一片共有的；
    //    换行（不是 <br> 的时候）也不跨 mark，避免下一行开头被当成块。
    let orig: Vec<Vec<Mark>> = pieces.iter().map(|p| p.marks.clone()).collect();
    let mut eff: Vec<Vec<Mark>> = Vec::with_capacity(pieces.len());
    for i in 0..pieces.len() {
        let m = &orig[i];
        let e = match ws_flags[i] {
            -1 => {
                let prev = if i > 0 { eff[i - 1].clone() } else { vec![] };
                m.iter().filter(|x| prev.contains(x)).cloned().collect()
            }
            1 => {
                let next = orig.get(i + 1).cloned().unwrap_or_default();
                m.iter().filter(|x| next.contains(x)).cloned().collect()
            }
            _ => {
                if matches!(pieces[i].kind, Kind::Break) && !cx.inline_br {
                    vec![]
                } else {
                    m.clone()
                }
            }
        };
        eff.push(e);
    }
    // 3. 按栈生成 开/关/内容 的序列
    let mut toks: Vec<Tok> = Vec::new();
    let mut stack: Vec<(Mark, usize)> = Vec::new(); // (mark, open 的编号)
    let mut opens: Vec<Mark> = Vec::new();
    let mut close_of: Vec<usize> = Vec::new(); // open 编号 → close 在 toks 里的位置
    let mut open_at: Vec<usize> = Vec::new(); // open 编号 → open 在 toks 里的位置
    for (i, e) in eff.iter().enumerate() {
        let k = stack
            .iter()
            .position(|(m, _)| !e.contains(m))
            .unwrap_or(stack.len());
        while stack.len() > k {
            let (_, id) = stack.pop().unwrap();
            close_of[id] = toks.len();
            toks.push(Tok::Close(id));
        }
        for m in e {
            if !stack.iter().any(|(x, _)| x == m) {
                let id = opens.len();
                opens.push(m.clone());
                close_of.push(usize::MAX);
                open_at.push(toks.len());
                toks.push(Tok::Open(id));
                stack.push((m.clone(), id));
            }
        }
        toks.push(Tok::Piece(i));
    }
    while let Some((_, id)) = stack.pop() {
        close_of[id] = toks.len();
        toks.push(Tok::Close(id));
    }
    // 4. 每个 mark 用 Markdown 记号还是 HTML 标签：看开头、结尾两侧的字符
    let first_char = |p: &Piece| -> Option<char> {
        match &p.kind {
            Kind::Text(t) => t.chars().next(),
            Kind::Code(_) => Some('`'),
            Kind::Math(_) => Some('$'),
            Kind::Mention(_) => Some('<'),
            Kind::Break => Some(' '),
        }
    };
    let last_char = |p: &Piece| -> Option<char> {
        match &p.kind {
            Kind::Text(t) => t.chars().last(),
            Kind::Code(_) => Some('`'),
            Kind::Math(_) => Some('$'),
            Kind::Mention(_) => Some('>'),
            Kind::Break => Some(' '),
        }
    };
    let char_before = |pos: usize| -> Option<char> {
        toks[..pos]
            .iter()
            .rev()
            .find_map(|t| {
                if let Tok::Piece(i) = t {
                    Some(last_char(&pieces[*i]))
                } else {
                    None
                }
            })
            .flatten()
    };
    let char_after = |pos: usize| -> Option<char> {
        toks[pos + 1..]
            .iter()
            .find_map(|t| {
                if let Tok::Piece(i) = t {
                    Some(first_char(&pieces[*i]))
                } else {
                    None
                }
            })
            .flatten()
    };
    let use_html: Vec<bool> = (0..opens.len())
        .map(|id| {
            let ty = opens[id].ty.as_str();
            if md_delims(ty).is_none() {
                return ty != "link";
            }
            let (o, c) = (open_at[id], close_of[id]);
            all_html
                || !(flanking_ok(char_before(o), char_after(o), true)
                    && flanking_ok(char_before(c), char_after(c), false))
        })
        .collect();
    // 关掉一个 * 紧接着又开一个 *（`**a***b*`）会连成一串，解析时配对就乱了：后开的那个改用 HTML 标签
    let mut use_html = use_html;
    let delim_char = |ty: &str| md_delims(ty).map(|d| d.0.chars().next().unwrap());
    for w in 1..toks.len() {
        if let (Tok::Close(c), Tok::Open(o)) = (&toks[w - 1], &toks[w]) {
            let (cc, oc) = (delim_char(&opens[*c].ty), delim_char(&opens[*o].ty));
            if cc.is_some() && cc == oc && !use_html[*c] {
                use_html[*o] = true;
            }
        }
    }
    // 5. 输出
    let plain: String = pieces
        .iter()
        .filter_map(|p| {
            if let Kind::Text(t) = &p.kind {
                Some(t.as_str())
            } else {
                None
            }
        })
        .collect();
    let mut esc = Esc {
        table: cx.table,
        // 行内代码里的 $ 也会和外面的 $ 配成公式
        multi_dollar: plain.matches('$').count()
            + pieces
                .iter()
                .map(|p| {
                    if let Kind::Code(c) = &p.kind {
                        c.matches('$').count()
                    } else {
                        0
                    }
                })
                .sum::<usize>()
            >= 2
            || pieces.iter().any(|p| matches!(p.kind, Kind::Math(_))),
        double_eq: plain.contains("==") || has_mark(&pieces, "highlight"),
        line_start: true,
        prev: None,
    };
    let mut out = String::new();
    for t in &toks {
        match t {
            Tok::Open(id) => {
                let m = &opens[*id];
                if m.ty == "link" {
                    out.push('[');
                } else if use_html[*id] {
                    out.push_str(&format!("<{}>", html_tag(&m.ty)));
                } else {
                    out.push_str(md_delims(&m.ty).unwrap().0);
                }
                esc.line_start = false;
            }
            Tok::Close(id) => {
                let m = &opens[*id];
                if m.ty == "link" {
                    out.push_str(&link_tail(m));
                } else if use_html[*id] {
                    out.push_str(&format!("</{}>", html_tag(&m.ty)));
                } else {
                    out.push_str(md_delims(&m.ty).unwrap().1);
                }
            }
            Tok::Piece(i) => match &pieces[*i].kind {
                Kind::Text(s) => {
                    let s = if esc.line_start { s.trim_start() } else { s };
                    out.push_str(&escape_text(s, &mut esc));
                }
                Kind::Code(s) => {
                    out.push_str(&code_span(s, cx.table));
                    esc.line_start = false;
                }
                Kind::Math(s) => {
                    out.push('$');
                    out.push_str(s);
                    out.push('$');
                    esc.line_start = false;
                }
                Kind::Mention(u) => {
                    out.push('<');
                    out.push_str(u);
                    out.push('>');
                    esc.line_start = false;
                }
                Kind::Break => {
                    if cx.inline_br {
                        out.push_str("<br>");
                    } else {
                        // 行尾空白会被吃掉
                        while out.ends_with(' ') || out.ends_with('\t') {
                            out.pop();
                        }
                        out.push('\n');
                        esc.line_start = true;
                    }
                }
            },
        }
    }
    // 解析回来应该得到的行内节点（空白挪动、行首行尾空白去掉以后）
    let mut expected: Vec<Node> = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        let mut m = eff[i].clone();
        match &p.kind {
            Kind::Text(t) => expected.push(Node::text(t.clone(), m)),
            Kind::Code(t) => {
                m.retain(|x| x.ty == "link");
                m.push(Mark::new("code"));
                expected.push(Node::text(t.clone(), m));
            }
            Kind::Math(t) => expected.push(Node::new("mathInline").with("latex", t.clone())),
            Kind::Mention(u) => expected.push(Node::new("linkMention").with("url", u.clone())),
            Kind::Break => expected.push(Node::new("hardBreak")),
        }
    }
    let expected = tidy_lines(expected);
    let used_md = opens
        .iter()
        .enumerate()
        .any(|(i, m)| md_delims(&m.ty).is_some() && !use_html[i]);
    (out.trim_end().to_string(), expected, used_md)
}

/// 行首、行尾（换行两边、开头结尾）的空白去掉，相邻同格式的文字合并。
pub(crate) fn tidy_lines(nodes: Vec<Node>) -> Vec<Node> {
    let mut v = crate::node::merge_texts(nodes);
    let n = v.len();
    for i in 0..n {
        let start = i == 0 || v[i - 1].ty == "hardBreak";
        let end = i + 1 == n || v[i + 1].ty == "hardBreak";
        let code = v[i].marks.iter().any(|m| m.ty == "code");
        if let (Some(t), false) = (v[i].text.as_mut(), code) {
            if start {
                *t = t.trim_start().to_string();
            }
            if end {
                *t = t.trim_end().to_string();
            }
        }
    }
    crate::node::merge_texts(v)
}

fn has_mark(pieces: &[Piece], ty: &str) -> bool {
    pieces.iter().any(|p| p.marks.iter().any(|m| m.ty == ty))
}

/// 能写成 `<网址>` 的提及：http(s) 开头，没有空白和尖括号。
fn mention_ok(url: &str) -> bool {
    crate::is_bare_url(url) && !url.contains(['<', '>'])
}

fn link_tail(m: &Mark) -> String {
    let href = m.attr_str("href").unwrap_or("");
    let href = if href.is_empty()
        || href
            .chars()
            .any(|c| c.is_whitespace() || c == '(' || c == ')' || c == '<' || c == '>')
    {
        format!("<{}>", href.replace('<', "%3C").replace('>', "%3E"))
    } else {
        href.to_string()
    };
    match m.attr_str("title") {
        Some(t) if !t.is_empty() => format!("]({href} \"{}\")", t.replace('"', "\\\"")),
        _ => format!("]({href})"),
    }
}

fn code_span(s: &str, table: bool) -> String {
    let n = longest_run(s, '`') + 1;
    let fence = "`".repeat(n);
    let s = if table {
        s.replace('|', "\\|")
    } else {
        s.to_string()
    };
    let pad = s.starts_with('`')
        || s.ends_with('`')
        || (s.starts_with(' ') && s.ends_with(' ') && !s.trim().is_empty());
    if pad {
        format!("{fence} {s} {fence}")
    } else {
        format!("{fence}{s}{fence}")
    }
}

fn escape_text(s: &str, e: &mut Esc) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        let prev = if i > 0 { Some(chars[i - 1]) } else { e.prev };
        let next = chars.get(i + 1).copied();
        let at_start = e.line_start && i == 0;
        let esc = match c {
            '\\' | '*' | '`' | '[' | ']' => true,
            '_' => {
                !(prev.is_some_and(char::is_alphanumeric)
                    && next.is_some_and(char::is_alphanumeric))
            }
            '<' => next.is_some_and(|n| n.is_ascii_alphabetic() || matches!(n, '/' | '!' | '?')),
            '>' | '#' | '+' | '-' | '=' | '|' | ':' if at_start => {
                let line = &chars[i..];
                match c {
                    // 只有 | - : 空格的一行会被当成表格的分隔行，把上一行变成表头
                    '|' => true,
                    ':' => line.iter().all(|&x| matches!(x, ':' | '-' | '|' | ' ')),
                    '>' => true,
                    '#' => {
                        line.iter().take_while(|&&x| x == '#').count() <= 6
                            && line
                                .iter()
                                .find(|&&x| x != '#')
                                .is_none_or(|x| x.is_whitespace())
                    }
                    '+' | '-' => {
                        next.is_none_or(char::is_whitespace)
                            || line.iter().all(|&x| x == c || x == ' ')
                    }
                    _ => line.iter().all(|&x| x == '=' || x == ' '),
                }
            }
            // ~ = $ 挨着别的格式记号（在片的两头）或者连着同样的字时，会被当成删除线 / 高亮 / 公式的记号
            // GFM 的删除线单个 ~ 也算，一段里两个 ~ 就可能配成一对：一律转义
            '~' => true,
            '=' | '$' => {
                let edge = i == 0 || i + 1 == chars.len() || prev == Some(c) || next == Some(c);
                edge || (c == '=' && e.double_eq) || (c == '$' && e.multi_dollar)
            }
            '|' => e.table,
            // 不是链接的网址、邮箱别被自动识别成链接
            ':' => {
                let before: String = chars[..i]
                    .iter()
                    .rev()
                    .take(5)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                let low = before.to_ascii_lowercase();
                (low.ends_with("http") || low.ends_with("https"))
                    && chars.get(i + 1) == Some(&'/')
                    && chars.get(i + 2) == Some(&'/')
            }
            '.' => {
                let before: String = chars[..i]
                    .iter()
                    .rev()
                    .take(3)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                before.eq_ignore_ascii_case("www") && (i < 4 || !chars[i - 4].is_alphanumeric())
            }
            '@' => {
                prev.is_some_and(|p| p.is_alphanumeric() || matches!(p, '.' | '_' | '-' | '+'))
                    && next.is_some_and(char::is_alphanumeric)
            }
            '&' => {
                let rest: String = chars[i + 1..].iter().take(32).collect();
                rest.find(';').is_some_and(|p| {
                    p > 0
                        && rest[..p]
                            .chars()
                            .all(|x| x.is_ascii_alphanumeric() || x == '#')
                })
            }
            _ => false,
        };
        // 行首的 “1. ” “1) ” 会被当成有序列表
        if at_start && c.is_ascii_digit() {
            let digits = chars[i..].iter().take_while(|x| x.is_ascii_digit()).count();
            if let Some(&d) = chars.get(i + digits)
                && digits <= 9
                && (d == '.' || d == ')')
                && chars.get(i + digits + 1).is_none_or(|x| x.is_whitespace())
            {
                out.extend(&chars[i..i + digits]);
                out.push('\\');
                // 剩下的字符照常处理：把数字跳过去
                let rest: String = chars[i + digits..].iter().collect();
                e.line_start = false;
                e.prev = Some(chars[i + digits - 1]);
                out.push_str(&escape_text(&rest, e));
                return out;
            }
        }
        if esc {
            out.push('\\');
        }
        out.push(c);
    }
    if let Some(&l) = chars.last() {
        e.prev = Some(l);
        e.line_start = false;
    }
    out
}
