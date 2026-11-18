use oad_markdown::{Mark, Node, parse, serialize};
use serde_json::Value;

/// md → doc → md → doc：两次的 doc 一样，第二次的 md 和第一次一样。
fn stable(md: &str) -> (Node, String) {
    let d1 = parse(md);
    let s1 = serialize(&d1);
    let d2 = parse(&s1);
    assert_eq!(
        d1,
        d2,
        "\n--- 原文 ---\n{md}\n--- 输出 ---\n{s1}\n--- 第一次 ---\n{}\n--- 第二次 ---\n{}",
        serde_json::to_string_pretty(&d1).unwrap(),
        serde_json::to_string_pretty(&d2).unwrap()
    );
    let s2 = serialize(&d2);
    assert_eq!(s1, s2, "输出不稳定");
    (d1, s1)
}

fn first(md: &str) -> Node {
    stable(md).0.content.into_iter().next().unwrap()
}

#[test]
fn blocks_basic() {
    for md in [
        "# 标题\n\n段落 **加粗** *斜体* ~~删除~~ `代码` [链接](https://a.com \"标题\")\n",
        "- a\n- b\n  - c\n  - d\n- e\n",
        "1. one\n2. two\n\n   second para\n3. three\n",
        "5. five\n6. six\n",
        "- [ ] 待办\n- [x] 完成了\n",
        "> 引用\n>\n> 第二段\n",
        "```rust\nfn main() {}\n```\n",
        "````\n```\nnested\n```\n````\n",
        "---\n",
        "| a | b |\n| --- | --- |\n| 1 | 2 |\n| x\\|y | `a\\|b` |\n",
        "![图](https://img.com/a.png \"t\")\n",
        "$$\nE = mc^2\n$$\n",
        "公式 $a^2+b^2$ 在行内\n",
        "第一行\n第二行\n第三行\n",
        "- a\n\n* b\n",
        "1. a\n\n1) b\n",
    ] {
        stable(md);
    }
}

#[test]
fn embed_vs_link() {
    assert_eq!(first("https://x.com/a/status/1").ty, "embed");
    assert_eq!(
        first("https://www.youtube.com/watch?v=abc&t=10").attr_str("url"),
        Some("https://www.youtube.com/watch?v=abc&t=10")
    );
    // <网址> 是行内提及；[文字](网址) 和正文里裸露的网址是普通链接
    let p = first("<https://x.com/a>");
    assert_eq!(p.ty, "paragraph");
    assert_eq!(p.content[0].ty, "linkMention");
    assert_eq!(p.content[0].attr_str("url"), Some("https://x.com/a"));
    let p = first("看 <https://github.com/a/b> 这个");
    assert_eq!(p.content[1].ty, "linkMention");
    assert_eq!(
        serialize(&Node::new("doc").children(vec![p])),
        "看 <https://github.com/a/b> 这个\n"
    );
    assert_eq!(
        first("[https://x.com/a](https://x.com/a)").content[0].marks[0].ty,
        "link"
    );
    assert_eq!(
        first("**<https://x.com/a>**").content[0].marks.len(),
        2,
        "加了格式的还是链接"
    );
    assert_eq!(first("[https://x.com](https://x.com)").ty, "paragraph");
    assert_eq!(
        first("看这个 https://x.com/a 链接").content[1].marks[0].ty,
        "link"
    );
}

#[test]
fn callouts_and_details() {
    let c = first("> [!WARNING] 小心\n> 这里有坑\n");
    assert_eq!(
        (c.ty.as_str(), c.attr_str("kind"), c.attr_str("title")),
        ("callout", Some("warning"), Some("小心"))
    );
    let c = first("> [!NOTE]\n> 说明\n");
    assert_eq!(c.attr_str("kind"), Some("note"));
    let d =
        first("<details>\n<summary>展开看看</summary>\n\n- 里面是列表\n- **加粗**\n\n</details>\n");
    assert_eq!(d.ty, "details");
    assert_eq!(d.content[0].plain_text(), "展开看看");
    assert_eq!(d.content[1].content[0].ty, "bulletList");
    // 没有空行的写法也认
    let d = first("<details><summary>标题</summary>\n正文\n</details>");
    assert_eq!(d.content[1].content[0].plain_text(), "正文");
}

#[test]
fn cjk_emphasis() {
    let p = first("这是**加粗**的文字，还有**「引号」**和*斜体。*后面");
    let bold: Vec<String> = p
        .content
        .iter()
        .filter(|n| n.marks.iter().any(|m| m.ty == "bold"))
        .map(|n| n.text.clone().unwrap())
        .collect();
    assert_eq!(bold, vec!["加粗", "「引号」"]);
}

#[test]
fn html_marks_and_breaks() {
    let p = first("a <u>下划线</u> 和 ==高亮== 和 <mark>标记</mark><br>换行");
    assert!(
        p.content
            .iter()
            .any(|n| n.marks.iter().any(|m| m.ty == "underline"))
    );
    assert_eq!(
        p.content
            .iter()
            .filter(|n| n.marks.iter().any(|m| m.ty == "highlight"))
            .count(),
        2
    );
    assert!(p.content.iter().any(|n| n.ty == "hardBreak"));
    assert_eq!(first("<br>").ty, "paragraph");
    // 不认的 HTML 原样当文字，不渲染
    assert_eq!(
        first("<script>alert(1)</script>").plain_text(),
        "<script>alert(1)</script>"
    );
}

#[test]
fn escapes() {
    for md in [
        "\\# 不是标题",
        "\\- 不是列表",
        "1\\. 不是列表",
        "\\> 不是引用",
        "a \\* b \\_c\\_ snake_case \\[x\\]",
        "价格 $5",
        "价格 \\$5 和 \\$6",
        "约 \\~10 分钟",
        "\\~a\\~ b",
        "a == b ?",
        "&amp; 和 \\&copy;",
        "1 < 2 且 \\<b> 标签",
        "**foo.**bar",
        "*a*b",
        "x**y**z",
    ] {
        stable(md);
    }
    let p = first("价格 $5");
    assert_eq!(p.plain_text(), "价格 $5");
    // CommonMark 不认 **foo.**bar 是加粗；文档里真有这种加粗时输出要换成 <strong>
    assert!(first("**foo.**bar").content[0].marks.is_empty());
    let doc = Node::new("doc").children(vec![Node::new("paragraph").children(vec![
        Node::text("foo.", vec![Mark::new("bold")]),
        Node::text("bar", vec![]),
    ])]);
    let md = serialize(&doc);
    assert_eq!(md, "<strong>foo.</strong>bar\n");
    assert_eq!(parse(&md), doc);
}

// ---------- 随机文档：doc → md → doc 必须一样 ----------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.below(v.len())]
    }
}

const WORDS: &[&str] = &[
    "文档",
    "协作",
    "agent",
    "snake_case",
    "a*b",
    "x_y_",
    "_lead",
    "[x]",
    "a]b",
    "#tag",
    "1.",
    "2)",
    "-",
    "+",
    "==",
    "~",
    "$",
    "<div>",
    "a<b",
    "&amp;",
    "|",
    "`tick`",
    "\\",
    "!",
    "。",
    "，",
    "「引号」",
    "foo.",
    "(括号)",
    "e.g.",
    "https://example.com/x",
    "**",
    "=",
    ">",
    "100%",
    "中文",
    "Ünïcødé",
    "😀",
];

fn rand_inline(r: &mut Rng, table: bool) -> Vec<Node> {
    let mut out = Vec::new();
    let n = 1 + r.below(6);
    for i in 0..n {
        if i > 0 && !table && r.below(8) == 0 {
            out.push(Node::new("hardBreak"));
        }
        let words: Vec<&str> = (0..1 + r.below(4)).map(|_| *r.pick(WORDS)).collect();
        let marked = r.below(3) == 0;
        let mut text = if marked {
            words.join(if r.below(2) == 0 { " " } else { "" })
        } else {
            format!(" {} ", words.join(" "))
        };
        if table {
            // 已知限制：表格里行内代码中“反斜杠紧跟竖线”写不出来（GFM 先按 \| 切单元格）
            text = text.replace("\\|", "\\ |");
        }
        let mut marks = Vec::new();
        if marked {
            match r.below(7) {
                0 => marks.push(Mark::new("bold")),
                1 => marks.push(Mark::new("italic")),
                2 => marks.push(Mark::new("strike")),
                3 => marks.push(Mark::new("code")),
                4 => marks.push(Mark::new("link").with("href", "https://a.com/p")),
                5 => marks.push(Mark::new("highlight")),
                _ => {
                    marks.push(Mark::new("bold"));
                    marks.push(Mark::new("italic"));
                }
            }
        }
        out.push(Node::text(text, marks));
        // 行内提及
        if !table && r.below(10) == 0 {
            let url = *r.pick(&[
                "https://github.com/a/b",
                "https://x.com/u/status/1",
                "http://a.cn/p?q=1&r=2",
            ]);
            out.push(Node::text(" ", vec![]));
            out.push(Node::new("linkMention").with("url", url));
            out.push(Node::text(" ", vec![]));
        }
    }
    out
}

/// 期望值的规整：行首行尾空白去掉、相邻同格式文字合并（Markdown 本来就不保留这些）。
fn tidy(nodes: Vec<Node>) -> Vec<Node> {
    let mut v = oad_markdown::node::merge_texts(nodes);
    let n = v.len();
    for i in 0..n {
        let start = i == 0 || v[i - 1].ty == "hardBreak";
        let end = i + 1 == n || v[i + 1].ty == "hardBreak";
        if let Some(t) = v[i].text.as_mut() {
            if start {
                *t = t.trim_start().to_string();
            }
            if end {
                *t = t.trim_end().to_string();
            }
        }
    }
    oad_markdown::node::merge_texts(v)
}

fn rand_block(r: &mut Rng, depth: usize) -> Node {
    let para = |r: &mut Rng| Node::new("paragraph").children(tidy(rand_inline(r, false)));
    match r.below(if depth > 1 { 3 } else { 9 }) {
        0 | 1 => para(r),
        2 => Node::new("heading")
            .with("level", 1 + r.below(3) as i64)
            .children(tidy(rand_inline(r, true))),
        3 => {
            let ty = *r.pick(&["bulletList", "orderedList", "taskList"]);
            let items = (0..1 + r.below(3))
                .map(|_| {
                    let mut body = vec![para(r)];
                    if r.below(3) == 0 {
                        body.push(rand_block(r, depth + 1));
                    }
                    if ty == "taskList" {
                        Node::new("taskItem")
                            .with("checked", r.below(2) == 0)
                            .children(body)
                    } else {
                        Node::new("listItem").children(body)
                    }
                })
                .collect();
            Node::new(ty).children(items)
        }
        4 => Node::new("blockquote").children(vec![para(r), rand_block(r, depth + 1)]),
        5 => Node::new("codeBlock")
            .with("language", "js")
            .children(vec![Node::text("let a = `x`;\n```\n  indented", vec![])]),
        6 => {
            let row = |r: &mut Rng, h: bool| {
                Node::new("tableRow").children(
                    (0..2)
                        .map(|_| {
                            Node::new(if h { "tableHeader" } else { "tableCell" }).children(vec![
                                Node::new("paragraph").children(tidy(rand_inline(r, true))),
                            ])
                        })
                        .collect(),
                )
            };
            Node::new("table").children(vec![row(r, true), row(r, false)])
        }
        7 => Node::new("callout")
            .with("kind", "tip")
            .children(vec![para(r)]),
        _ => Node::new("embed").with("url", "https://x.com/a/status/123"),
    }
}

#[test]
fn random_docs_roundtrip() {
    let mut r = Rng(0x9E3779B97F4A7C15);
    for case in 0..(std::env::var("OAD_FUZZ")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000))
    {
        let doc =
            Node::new("doc").children((0..1 + r.below(5)).map(|_| rand_block(&mut r, 0)).collect());
        let md = serialize(&doc);
        let back = parse(&md);
        if back != doc {
            let i = doc
                .content
                .iter()
                .zip(back.content.iter())
                .position(|(a, b)| a != b)
                .unwrap_or(0);
            let a = serde_json::to_string(&doc.content[i]).unwrap();
            let b = serde_json::to_string(&back.content.get(i)).unwrap();
            let at = a
                .bytes()
                .zip(b.bytes())
                .position(|(x, y)| x != y)
                .unwrap_or(0);
            let from = a.floor_char_boundary(at.saturating_sub(150));
            panic!(
                "第 {case} 个随机文档来回不一致（第 {i} 块）\n--- md ---\n{}\n--- 原 ---\n…{}\n--- 回 ---\n…{}",
                oad_markdown::serialize_block(&doc.content[i]),
                &a[from..a.floor_char_boundary((at + 200).min(a.len()))],
                &b[b.floor_char_boundary(from)..b.floor_char_boundary((at + 200).min(b.len()))]
            );
        }
        assert_eq!(serialize(&back), md);
    }
}

// ---------- CRDT 实验生成的大文档（Tiptap schema）----------

fn norm_tiptap(v: &Value) -> Value {
    // 去掉 id、null 属性、Tiptap 的默认值，链接只留 href / title
    match v {
        Value::Object(o) => {
            let mut m = serde_json::Map::new();
            for (k, x) in o {
                if k == "attrs" {
                    let ty = o.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    let mut a = serde_json::Map::new();
                    for (ak, av) in x.as_object().unwrap() {
                        if av.is_null()
                            || ak == "id"
                            || (ty == "orderedList" && ak == "start" && av == 1)
                        {
                            continue;
                        }
                        if ty == "link" && ak != "href" && ak != "title" {
                            continue;
                        }
                        a.insert(ak.clone(), av.clone());
                    }
                    if !a.is_empty() {
                        m.insert(k.clone(), Value::Object(a));
                    }
                } else {
                    m.insert(k.clone(), norm_tiptap(x));
                }
            }
            Value::Object(m)
        }
        Value::Array(a) => Value::Array(a.iter().map(norm_tiptap).collect()),
        _ => v.clone(),
    }
}

#[test]
fn big_generated_doc() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../spikes/crdt/out/base10k.json"
    );
    let Ok(raw) = std::fs::read(path) else {
        eprintln!("没有 {path}，跳过");
        return;
    };
    let doc: Node =
        serde_json::from_value(norm_tiptap(&serde_json::from_slice(&raw).unwrap())).unwrap();
    let doc = Node::new("doc").children(doc.content.into_iter().map(fix_inline).collect());
    let t = std::time::Instant::now();
    let md = serialize(&doc);
    let ser_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = std::time::Instant::now();
    let back = parse(&md);
    let parse_ms = t.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "10000 块：输出 Markdown {ser_ms:.1}ms（{} KB），解析 {parse_ms:.1}ms",
        md.len() / 1024
    );
    assert_eq!(doc.content.len(), back.content.len());
    for (i, (a, b)) in doc.content.iter().zip(back.content.iter()).enumerate() {
        assert_eq!(a.ty, b.ty, "第 {i} 块类型不一样");
        assert_eq!(
            a.plain_text(),
            b.plain_text(),
            "第 {i} 块文字不一样\n{}",
            oad_markdown::serialize_block(a)
        );
    }
    assert_eq!(parse(&serialize(&back)), back, "第二遍不稳定");
}

/// 生成器的文字前后带空格，按 Markdown 的规矩规整一下
fn fix_inline(mut n: Node) -> Node {
    if n.content.iter().any(|c| c.is_text()) && n.ty != "codeBlock" {
        n.content = tidy(n.content);
    } else {
        n.content = n.content.into_iter().map(fix_inline).collect();
    }
    n
}
