//! 浏览器和服务器看到的是同一棵树：网页那边（packages/editor 的测试）用 y-prosemirror 把一份用到所有节点、属性、mark 的文档
//! 写进 Y.Doc，导出 all-nodes.ydoc.bin 和对应的 ProseMirror JSON；这里读 .bin，结果要和 JSON 一样，
//! 再用 Rust 这边建一份文档、转 Markdown 来回，也要一样。
use oad_doc::{Node, PageDoc};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/editor/test/fixtures")
}

/// 去掉 null 属性（y-prosemirror 不写 null），文字按 marks 合并。
fn normalize(n: &Node) -> Node {
    let mut c = n.clone();
    c.attrs.retain(|_, v| !v.is_null());
    for m in &mut c.marks {
        m.attrs.retain(|_, v| !v.is_null());
    }
    oad_markdown::node::sort_marks(&mut c.marks);
    c.content = oad_markdown::node::merge_texts(c.content.iter().map(normalize).collect());
    c
}

#[test]
fn browser_doc_reads_the_same() {
    let dir = fixtures();
    let (Ok(bin), Ok(json)) = (
        std::fs::read(dir.join("all-nodes.ydoc.bin")),
        std::fs::read_to_string(dir.join("all-nodes.json")),
    ) else {
        eprintln!("没有 fixtures（先跑 packages/editor 的测试），跳过");
        return;
    };
    let want: Node = serde_json::from_str(&json).unwrap();
    let want = normalize(&want);
    let got = normalize(&PageDoc::from_update(1, &bin).unwrap().to_node());
    assert_eq!(
        serde_json::to_string_pretty(&got).unwrap(),
        serde_json::to_string_pretty(&want).unwrap()
    );

    // Rust 自己写一份再读出来，也一样
    let again = normalize(&PageDoc::from_node(2, &want).to_node());
    assert_eq!(again, want);

    // 每种节点、mark 都能转成 Markdown 再转回来（编号不比）
    let md = oad_markdown::serialize(&want);
    let back = normalize(&oad_markdown::parse(&md));
    let md2 = oad_markdown::serialize(&back);
    assert_eq!(md, md2, "Markdown 来回不稳定");
}

#[test]
fn markdown_back_equals_browser_doc() {
    let Ok(json) = std::fs::read_to_string(fixtures().join("all-nodes.json")) else {
        return;
    };
    // 展示属性（图注、宽度、embed.view、codeBlock.wrap）不进 Markdown
    let want = normalize(&serde_json::from_str::<Node>(&json).unwrap())
        .without_ids()
        .without_presentation();
    let back = normalize(&oad_markdown::parse(&oad_markdown::serialize(&want))).without_ids();
    assert_eq!(
        serde_json::to_string_pretty(&back).unwrap(),
        serde_json::to_string_pretty(&want).unwrap()
    );
}
