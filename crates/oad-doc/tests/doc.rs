use oad_doc::{PageDoc, assign_ids};
use oad_markdown::{Node, parse, serialize};
use std::collections::HashSet;

const MD: &str = r#"# 标题

第一段，有 **加粗**、*斜体*、`代码` 和 [链接](https://a.com)。

- 第一项
- 第二项
  - 嵌套
- 第三项

1. 一
2. 二

- [ ] 待办
- [x] 完成

> [!TIP] 提示
> 提示里的内容

> 引用

```rust
fn main() {}
```

$$
E = mc^2
$$

https://www.youtube.com/watch?v=abc

![图](https://img.com/a.png)

| a | b |
| --- | --- |
| 1 | 2 |

<details>
<summary>折叠</summary>

里面的段落

</details>

---

最后一段 😀 中文
"#;

fn doc_with_ids(md: &str) -> Node {
    let mut d = parse(md);
    assign_ids(&mut d, &mut HashSet::new());
    d
}

fn ids_of(n: &Node) -> Vec<String> {
    let mut v = Vec::new();
    fn walk(n: &Node, v: &mut Vec<String>) {
        if let Some(id) = n.attr_str("id") {
            v.push(id.to_string());
        }
        n.content.iter().for_each(|c| walk(c, v));
    }
    walk(n, &mut v);
    v
}

#[test]
fn node_roundtrip() {
    let d = doc_with_ids(MD);
    let p = PageDoc::from_node(1, &d);
    assert_eq!(p.to_node(), d);
    // 经过增量编码再读一遍
    let q = PageDoc::from_update(2, &p.encode_state()).unwrap();
    assert_eq!(q.to_node(), d);
    assert_eq!(serialize(&q.to_node()), serialize(&d));
}

#[test]
fn replace_keeps_ids() {
    let d = doc_with_ids(MD);
    let p = PageDoc::from_node(1, &d);
    let list = d.content.iter().find(|n| n.ty == "bulletList").unwrap();
    let before = ids_of(list);
    // Agent 用 Markdown 改了第二项的字（Markdown 没有编号）
    let new = parse("- 第一项\n- 第二项改过了\n  - 嵌套\n- 第三项\n").content;
    p.replace_block(list.attr_str("id").unwrap(), &new).unwrap();
    let after_doc = p.to_node();
    let after = after_doc
        .content
        .iter()
        .find(|n| n.ty == "bulletList")
        .unwrap();
    assert_eq!(ids_of(after), before, "列表和每一项的编号都应该沿用");
    assert!(after.plain_text().contains("第二项改过了"));
    // 中间插一项：原来三项的编号还在，新的一项有新编号
    let new = parse("- 第一项\n- 新插的\n- 第二项改过了\n  - 嵌套\n- 第三项\n").content;
    p.replace_block(list.attr_str("id").unwrap(), &new).unwrap();
    let after_doc = p.to_node();
    let after = after_doc
        .content
        .iter()
        .find(|n| n.ty == "bulletList")
        .unwrap();
    let ids = ids_of(after);
    for id in &before {
        assert!(ids.contains(id), "编号 {id} 丢了");
    }
    assert_eq!(ids.len(), before.len() + 2); // 新的项 + 它的段落
    let all: HashSet<_> = ids_of(&after_doc).into_iter().collect();
    assert_eq!(all.len(), ids_of(&after_doc).len(), "编号有重复");
}

#[test]
fn concurrent_same_block() {
    let d = doc_with_ids("第一段原来的文字，这里是中间，这里是结尾。\n\n第二段\n");
    let base = PageDoc::from_node(1, &d);
    let a = PageDoc::from_update(10, &base.encode_state()).unwrap(); // 人
    let b = PageDoc::from_update(20, &base.encode_state()).unwrap(); // Agent
    let id = d.content[0].attr_str("id").unwrap();
    let sv_a = a.state_vector();
    let sv_b = b.state_vector();
    a.replace_block(
        id,
        &parse("第一段原来的文字，这里是中间【人插的字】，这里是结尾。").content,
    )
    .unwrap();
    b.replace_block(
        id,
        &parse("Agent 改了开头的文字，这里是中间，这里是结尾。（补一句）").content,
    )
    .unwrap();
    let ua = a.encode_diff(&sv_a).unwrap();
    let ub = b.encode_diff(&sv_b).unwrap();
    a.apply_update(&ub).unwrap();
    b.apply_update(&ua).unwrap();
    assert_eq!(a.to_node(), b.to_node(), "两边合并后不一样");
    let t = a.to_node().content[0].plain_text();
    assert!(
        t.contains("【人插的字】") && t.starts_with("Agent 改了开头") && t.ends_with("（补一句）"),
        "合并结果：{t}"
    );
    assert_eq!(a.to_node().content[0].attr_str("id"), Some(id));
}

#[test]
fn set_content_and_insert() {
    let d = doc_with_ids(MD);
    let p = PageDoc::from_node(1, &d);
    let before = ids_of(&d);
    // 整页重写，只改了一处
    let md2 = serialize(&d).replace("第一段，有", "第一段（改过），有");
    p.set_content(&parse(&md2));
    let after = p.to_node();
    assert_eq!(ids_of(&after), before, "整页重写只改一处，编号都该沿用");
    assert!(after.plain_text().contains("第一段（改过）"));
    // 插入和删除
    let first = after.content[0].attr_str("id").unwrap().to_string();
    let new_ids = p
        .insert_after(Some(&first), &parse("插在标题后面\n\n又一段").content)
        .unwrap();
    assert_eq!(new_ids.len(), 2);
    assert_eq!(p.to_node().content[1].plain_text(), "插在标题后面");
    p.delete_block(&new_ids[0]).unwrap();
    assert_eq!(p.to_node().content[1].plain_text(), "又一段");
    assert!(p.block(&new_ids[1]).is_some());
    assert!(p.replace_block("nope", &[]).is_err());
}

#[test]
fn big_doc_speed() {
    let mut md = String::new();
    for i in 0..10_000 {
        md.push_str(&format!(
            "第 {i} 段，有 **加粗** 和 [链接](https://a.com/{i})，还有一些中文和 English words.\n\n"
        ));
    }
    let d = doc_with_ids(&md);
    let t = std::time::Instant::now();
    let p = PageDoc::from_node(1, &d);
    let build = t.elapsed();
    let t = std::time::Instant::now();
    let state = p.encode_state();
    let encode = t.elapsed();
    let t = std::time::Instant::now();
    let q = PageDoc::from_update(2, &state).unwrap();
    let load = t.elapsed();
    let t = std::time::Instant::now();
    let back = q.to_node();
    let read = t.elapsed();
    assert_eq!(back, d);
    let ids: Vec<String> = d
        .content
        .iter()
        .step_by(100)
        .map(|n| n.attr_str("id").unwrap().to_string())
        .collect();
    let t = std::time::Instant::now();
    for id in &ids {
        q.replace_block(id, &parse("Agent 改写的一段").content)
            .unwrap();
    }
    let edit = t.elapsed() / ids.len() as u32;
    eprintln!(
        "10000 段：建 {build:?}，编码 {encode:?}（{} KB），加载 {load:?}，读成节点 {read:?}，按段改 {edit:?}/次",
        state.len() / 1024
    );
}

#[test]
fn title_and_replace_ids() {
    let d = doc_with_ids("第一段\n\n第二段\n");
    let p = PageDoc::from_node(oad_doc::server_client_id(), &d);
    assert!(oad_doc::is_server_client(p.client_id()));
    p.set_title("标题 😀");
    let q = PageDoc::from_update(7, &p.encode_state()).unwrap();
    assert_eq!(q.title(), "标题 😀");
    // 两边同时改标题，字都留得住
    let sv = q.state_vector();
    q.set_title("新标题 😀");
    p.set_title("标题 😀！");
    p.apply_update(&q.encode_diff(&sv).unwrap()).unwrap();
    assert_eq!(p.title(), "新标题 😀！");
    // 一块换成两块：第一块沿用编号，返回两个编号
    let id = d.content[0].attr_str("id").unwrap();
    let ids = p
        .replace_block(id, &parse("第一段\n\n插进来的").content)
        .unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], id);
    assert_eq!(p.block(&ids[1]).unwrap().plain_text(), "插进来的");
}

#[test]
fn edit_on_old_version() {
    // Agent 读了一段，人接着打字（已经到了服务器），Agent 按读到的版本改：人打的字留得住
    let d = doc_with_ids("第一段原来的文字，这里是中间，这里是结尾。\n");
    let server = PageDoc::from_node(oad_doc::server_client_id(), &d);
    let id = d.content[0].attr_str("id").unwrap();
    let base = server.version();
    let human = PageDoc::from_update(10, &server.encode_state()).unwrap();
    let sv = human.state_vector();
    human
        .replace_block(
            id,
            &parse("第一段原来的文字，这里是中间【人】，这里是结尾。").content,
        )
        .unwrap();
    server
        .apply_update(&human.encode_diff(&sv).unwrap())
        .unwrap();
    let fork = server.fork_at(&base, oad_doc::server_client_id()).unwrap();
    assert!(!fork.to_node().plain_text().contains("【人】"));
    let fsv = fork.state_vector();
    fork.replace_block(
        id,
        &parse("Agent 改了开头，这里是中间，这里是结尾。").content,
    )
    .unwrap();
    server
        .apply_update(&fork.encode_diff(&fsv).unwrap())
        .unwrap();
    let t = server.to_node().plain_text();
    assert!(
        t.starts_with("Agent 改了开头") && t.contains("【人】"),
        "{t}"
    );
}
