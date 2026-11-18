//! 块编号：每个块级节点一个 attrs.id，8 位字母数字，在一页里唯一。
use crate::Node;
use std::collections::HashSet;

const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

pub fn new_id() -> String {
    let mut x: u64 = rand::random();
    (0..8)
        .map(|_| {
            let c = ALPHABET[(x % 62) as usize] as char;
            x /= 62;
            c
        })
        .collect()
}

/// 行内节点（text、hardBreak、mathInline、linkMention）和根节点没有编号，别的都有。
pub fn is_block(ty: &str) -> bool {
    !matches!(ty, "doc" | "text") && !oad_markdown::node::is_inline_atom(ty)
}

/// 只给这一个节点（不管子节点）分配编号。
pub fn assign_shallow(n: &mut Node, existing: &mut HashSet<String>) {
    if !is_block(&n.ty) {
        return;
    }
    let ok = n
        .attr_str("id")
        .is_some_and(|id| !id.is_empty() && !existing.contains(id));
    if ok {
        existing.insert(n.attr_str("id").unwrap().to_string());
    } else {
        let mut id = new_id();
        while existing.contains(&id) {
            id = new_id();
        }
        existing.insert(id.clone());
        n.attrs.insert("id".into(), id.into());
    }
}

/// 子孙节点分配编号（自己不动）。
pub fn assign_descendants(n: &mut Node, existing: &mut HashSet<String>) {
    for c in &mut n.content {
        assign_ids(c, existing);
    }
}

/// 给没有编号（或编号重复）的块分配新编号。`existing` 是已经占用的，分配后加进去。
pub fn assign_ids(n: &mut Node, existing: &mut HashSet<String>) {
    assign_shallow(n, existing);
    for c in &mut n.content {
        assign_ids(c, existing);
    }
}

/// 收集一棵树里的编号。
pub fn collect(n: &Node, set: &mut HashSet<String>) {
    if let Some(id) = n.attr_str("id") {
        set.insert(id.to_string());
    }
    n.content.iter().for_each(|c| collect(c, set));
}

/// 复制一份，只保留在 allowed 里的编号，别的去掉。
pub fn keep_only(n: &Node, allowed: &HashSet<String>) -> Node {
    let mut c = n.clone();
    if c.attr_str("id").is_some_and(|id| !allowed.contains(id)) {
        c.attrs.remove("id");
    }
    c.content = n.content.iter().map(|x| keep_only(x, allowed)).collect();
    c
}
