//! WASM 版（packages/markdown-wasm）和这里的结果一样：同一组 Markdown 在 Node 里用 WASM 解析、输出，和 Rust 的比。
//! 要先编好 WASM（`sh packages/markdown-wasm/build.sh`）、有 node；没有就跳过。
use oad_markdown::{Node, parse, serialize};
use std::path::PathBuf;
use std::process::Command;

const SNIPPETS: &[&str] = &[
    "# 标题\n",
    "第一段，有 **加粗**、*斜体*、`代码` 和 [链接](https://a.com)。",
    "- 一\n- 二\n  - 嵌套\n",
    "1. 一\n2. 二\n",
    "- [ ] 待办\n- [x] 完成\n",
    "> [!TIP] 提示\n> 内容\n",
    "> 引用\n",
    "```rust\nfn main() {}\n```\n",
    "$$\nE = mc^2\n$$\n",
    "https://www.youtube.com/watch?v=abc\n",
    "看 <https://github.com/a/b> 这个，还有 https://x.com/a 链接",
    "![图](https://img.com/a.png \"标题\")\n",
    "| a | b |\n| --- | --- |\n| 1 | `x\\|y` |\n",
    "<details>\n<summary>折叠</summary>\n\n里面\n\n</details>\n",
    "---\n",
    "中文**挨着**加粗，==高亮==，~~删除~~，<u>下划线</u>",
    "行内公式 $a^2$ 和 😀 表情\n第二行",
    "\\*不是斜体\\* 和 1\\. 不是列表",
    "",
    "   ",
    "<script>alert(1)</script>",
];

fn corpus() -> Vec<String> {
    let mut v: Vec<String> = SNIPPETS.iter().map(|s| s.to_string()).collect();
    // 两两拼起来
    for (i, a) in SNIPPETS.iter().enumerate() {
        for b in SNIPPETS.iter().skip(i % 5).step_by(4) {
            v.push(format!("{a}\n\n{b}"));
        }
    }
    // 浏览器那边的全节点样例
    let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/editor/test/fixtures/all-nodes.json");
    if let Ok(j) = std::fs::read_to_string(fx) {
        let n: Node = serde_json::from_str(&j).unwrap();
        v.push(serialize(&n));
    }
    v
}

#[test]
fn wasm_matches_rust() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/markdown-wasm");
    if !root.join("dist/oad_markdown_bg.wasm").exists() {
        eprintln!("没有编好的 WASM（sh packages/markdown-wasm/build.sh），跳过");
        return;
    }
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("没有 node，跳过");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (input, output) = (dir.path().join("in.json"), dir.path().join("out.json"));
    let cases = corpus();
    std::fs::write(&input, serde_json::to_string(&cases).unwrap()).unwrap();
    let st = Command::new("node")
        .arg(root.join("test/run.mjs"))
        .arg(&input)
        .arg(&output)
        .status()
        .unwrap();
    assert!(st.success(), "node 跑失败了");
    let got: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&output).unwrap()).unwrap();
    assert_eq!(got.len(), cases.len());
    for (md, g) in cases.iter().zip(&got) {
        let doc = parse(md);
        assert_eq!(
            g["doc"],
            serde_json::to_value(&doc).unwrap(),
            "解析不一样：{md:?}"
        );
        assert_eq!(g["md"], serialize(&doc), "输出不一样：{md:?}");
    }
    eprintln!("{} 段 Markdown，WASM 和 Rust 一样", cases.len());
}
