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

#[test]
fn crafted_version_is_rejected() {
    use yrs::updates::encoder::Encode;
    use yrs::{ClientID, IdSet, ReadTxn, Snapshot, StateVector, Transact};
    let d = doc_with_ids("第一段\n");
    let p = PageDoc::from_node(42, &d);
    let snap = |sv: Vec<(u64, u32)>, ds: Vec<(u64, std::ops::Range<u32>)>| {
        Snapshot::new(
            StateVector::from_iter(sv.into_iter().map(|(c, k)| (ClientID::new(c), k))),
            IdSet::from_iter(ds.into_iter().map(|(c, r)| (ClientID::new(c), [r]))),
        )
        .encode_v1()
    };
    let clock = p.ydoc().transact().state_vector().get(&ClientID::new(42));
    // 时钟 0（以前会让 Yrs 溢出 panic）、超出本地时钟、不认识的 client、删除集越界
    assert!(p.fork_at(&snap(vec![(42, 0)], vec![]), 7).is_err());
    assert!(p.fork_at(&snap(vec![(42, clock + 1)], vec![]), 7).is_err());
    assert!(p.fork_at(&snap(vec![(43, 1)], vec![]), 7).is_err());
    assert!(
        p.fork_at(&snap(vec![(42, clock)], vec![(42, 0..clock + 5)]), 7)
            .is_err()
    );
    assert!(
        p.fork_at(&snap(vec![(42, clock)], vec![(44, 0..1)]), 7)
            .is_err()
    );
    assert!(p.fork_at(b"\xff\xff\xff", 7).is_err());
    // 正常的版本照样能用
    assert!(p.fork_at(&p.version(), 7).is_ok());
    assert!(p.fork_at(&snap(vec![(42, clock)], vec![]), 7).is_ok());
}

#[test]
fn block_authors_and_insert_before() {
    let d = doc_with_ids("第一段\n\n第二段\n");
    let server = PageDoc::from_node(100, &d);
    let id = d.content[0].attr_str("id").unwrap().to_string();
    // 另一个人在第一段里打了更多的字
    let other = PageDoc::from_update(200, &server.encode_state()).unwrap();
    let sv = other.state_vector();
    other
        .replace_block(
            &id,
            &parse("第一段，后面是另一个人写的很长的一句话").content,
        )
        .unwrap();
    server
        .apply_update(&other.encode_diff(&sv).unwrap())
        .unwrap();
    let a = server.block_authors(&id).unwrap();
    assert_eq!(a.get(&100), Some(&3));
    assert!(a[&200] > 3, "{a:?}");
    assert!(server.block_authors("nope").is_none());
    assert_eq!(server.clock(200), other.clock(200));
    assert_eq!(
        oad_doc::version_clock(&server.version(), 200),
        Some(server.clock(200))
    );
    // 插到前面
    let second = server.top_ids()[1].clone();
    let ids = server
        .insert_before(&second, &parse("插在第二段前面").content)
        .unwrap();
    assert_eq!(server.top_ids()[1], ids[0]);
    assert_eq!(server.to_node().content[2].plain_text(), "第二段");
}

#[test]
fn presentation_attrs_survive_markdown_rewrite() {
    // 网页设了图注、宽度、书签样式、自动换行；Agent 用 Markdown 改写，这些不该丢
    let mut d = doc_with_ids(
        "![猫](https://a.com/cat.png)\n\nhttps://www.youtube.com/watch?v=1\n\n```rust\nfn a() {}\n```\n\n一段\n",
    );
    d.content[0].attrs.insert("caption".into(), "一只猫".into());
    d.content[0].attrs.insert("width".into(), 60.into());
    d.content[1].attrs.insert("view".into(), "bookmark".into());
    d.content[2].attrs.insert("wrap".into(), true.into());
    let p = PageDoc::from_node(1, &d);
    p.set_content(&parse("![猫](https://a.com/cat.png \"标题\")\n\nhttps://www.youtube.com/watch?v=1\n\n```rust\nfn b() {}\n```\n\n一段改过\n"));
    let n = p.to_node();
    assert_eq!(n.content[0].attr_str("caption"), Some("一只猫"));
    assert_eq!(n.content[0].attr_i64("width"), Some(60));
    assert_eq!(
        n.content[0].attr_str("title"),
        Some("标题"),
        "Markdown 里有的照常改"
    );
    assert_eq!(n.content[1].attr_str("view"), Some("bookmark"));
    assert!(n.content[2].attr_bool("wrap"));
    assert!(n.content[2].plain_text().contains("fn b()"));
    // 按块替换也一样
    let id = n.content[0].attr_str("id").unwrap().to_string();
    p.replace_block(&id, &parse("![猫](https://a.com/cat2.png)").content)
        .unwrap();
    let img = p.block(&id).unwrap();
    assert_eq!(img.attr_str("src"), Some("https://a.com/cat2.png"));
    assert_eq!(img.attr_str("caption"), Some("一只猫"));
    // 明确写 null 才清掉
    let mut cleared = img.clone();
    cleared
        .attrs
        .insert("caption".into(), serde_json::Value::Null);
    p.replace_block(&id, &[cleared]).unwrap();
    assert_eq!(p.block(&id).unwrap().attr_str("caption"), None);
}

#[test]
fn link_mention_is_inline() {
    let d = doc_with_ids("看 <https://github.com/a/b> 这个\n");
    assert!(
        d.content[0].content[1].attr_str("id").is_none(),
        "行内提及没有块编号"
    );
    let p = PageDoc::from_node(1, &d);
    assert_eq!(p.to_node(), d);
}

#[test]
fn writes_as_another_client() {
    let d = doc_with_ids("第一段\n\n第二段\n");
    let p = PageDoc::from_node(1 << 40, &d);
    let id = d.content[0].attr_str("id").unwrap().to_string();
    let agent = (1u64 << 40) + 7;
    p.as_client(agent, |p| {
        p.replace_block(&id, &parse("Agent 改写了第一段的全部文字").content)
            .unwrap()
    });
    let a = p.block_authors(&id).unwrap();
    // 没变的“第一段”三个字还是原来的作者，新写的字都记在 agent 名下
    assert_eq!(a.get(&(1 << 40)), Some(&3), "{a:?}");
    assert!(a[&agent] > 10, "{a:?}");
    // 之后又是自己写
    let second = d.content[1].attr_str("id").unwrap().to_string();
    p.replace_block(&second, &parse("服务器自己改的第二段").content)
        .unwrap();
    let b = p.block_authors(&second).unwrap();
    assert!(
        b.contains_key(&(1 << 40)) && !b.contains_key(&agent),
        "{b:?}"
    );
    // 合并到别的副本里，作者不变
    let q = PageDoc::from_update(5, &p.encode_state()).unwrap();
    assert_eq!(q.block_authors(&id).unwrap(), a);
    assert_eq!(q.to_node(), p.to_node());
}

#[test]
fn compact_state_for_new_clients() {
    let d = doc_with_ids(MD);
    let p = PageDoc::from_node(1, &d);
    // 没删过：和整份一样
    assert_eq!(p.encode_state_compact().unwrap(), p.encode_state());
    // 改几遍（换掉的字、删掉的块都是墓碑）
    for i in 0..3 {
        let mut n = p.to_node();
        let mut fresh = parse(&format!("第 {i} 版改过的段落 😀\n\n另一段 {i}\n")).content;
        n.content.truncate(4);
        n.content.append(&mut fresh);
        assign_ids(&mut n, &mut HashSet::new());
        p.set_content(&n);
    }
    let full = p.encode_state();
    let compact = p.encode_state_compact().unwrap();
    assert!(
        compact.len() < full.len(),
        "{} {}",
        compact.len(),
        full.len()
    );
    // 新客户端拿回收过的那份：内容、state vector 一样
    let c = PageDoc::from_update(7, &compact).unwrap();
    assert_eq!(c.to_node(), p.to_node());
    assert_eq!(c.state_vector(), p.state_vector());
    // 之后两边的增量照常合得上
    let sv = p.state_vector();
    let mut n = p.to_node();
    n.content
        .insert(1, parse("服务器后来加的 😀\n").content.remove(0));
    assign_ids(&mut n, &mut HashSet::new());
    p.set_content(&n);
    c.apply_update(&p.encode_diff(&sv).unwrap()).unwrap();
    assert_eq!(c.to_node(), p.to_node());
    let sv = c.state_vector();
    let mut n = c.to_node();
    n.content.remove(0);
    c.set_content(&n);
    p.apply_update(&c.encode_diff(&sv).unwrap()).unwrap();
    assert_eq!(c.to_node(), p.to_node());
}
