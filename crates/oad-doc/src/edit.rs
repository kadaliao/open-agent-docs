//! 按块改：把一段子节点“对齐”成新的样子，只动变了的地方。
//!
//! 对齐分三步：
//! 1. 新节点沿用旧编号：先按“类型 + 文字”找完全一样的块，剩下的再按类型顺序配；没配上的分配新编号。
//!    Agent 给的 Markdown 没有编号，这一步让“改了列表第三项”只改第三项，而不是整个列表重建。
//! 2. 按“类型#编号”做序列 diff：一样的就地对齐（递归），多的插、少的删。
//! 3. 文字按字符 diff，只插删变了的字，再补格式。人同时在同一段里打字，两边的字都留得住。
use crate::Node;
use crate::convert::{self, Child};
use crate::ids::{assign_descendants, assign_shallow};
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use yrs::types::Attrs;
use yrs::{
    Any, Out, ReadTxn, Text, TransactionMut, Xml, XmlElementRef, XmlFragment, XmlFragmentRef,
    XmlOut, XmlTextRef,
};

#[derive(Clone)]
pub enum Parent {
    Root(XmlFragmentRef),
    El(XmlElementRef),
}

impl Parent {
    fn get<T: ReadTxn>(&self, txn: &T, i: u32) -> Option<XmlOut> {
        match self {
            Parent::Root(f) => f.get(txn, i),
            Parent::El(e) => e.get(txn, i),
        }
    }
    /// [start, start+len) 的子节点，一次走完（按下标逐个 get 每次都从头数，大文档会变成 O(n²)）。
    fn slice<T: ReadTxn>(&self, txn: &T, start: u32, len: u32) -> Vec<XmlOut> {
        let it = match self {
            Parent::Root(f) => f.children(txn),
            Parent::El(e) => e.children(txn),
        };
        it.skip(start as usize).take(len as usize).collect()
    }
    pub fn element<T: ReadTxn>(&self, txn: &T, i: u32) -> Option<XmlElementRef> {
        match self.get(txn, i) {
            Some(XmlOut::Element(el)) => Some(el),
            _ => None,
        }
    }
    pub fn remove(&self, txn: &mut TransactionMut, i: u32, len: u32) {
        match self {
            Parent::Root(f) => f.remove_range(txn, i, len),
            Parent::El(e) => e.remove_range(txn, i, len),
        }
    }
    pub fn write(&self, txn: &mut TransactionMut, at: u32, nodes: &[Node]) {
        match self {
            Parent::Root(f) => convert::write_children(txn, f, at, nodes),
            Parent::El(e) => convert::write_children(txn, e, at, nodes),
        }
    }
    fn write_child(&self, txn: &mut TransactionMut, at: u32, ch: &Child) {
        match self {
            Parent::Root(f) => convert::write_child(txn, f, at, ch),
            Parent::El(e) => convert::write_child(txn, e, at, ch),
        }
    }
}

pub fn id_of<T: ReadTxn>(txn: &T, el: &XmlElementRef) -> Option<String> {
    match el.get_attribute(txn, "id") {
        Some(Out::Any(Any::String(s))) => Some(s.to_string()),
        _ => None,
    }
}

/// 找编号为 id 的元素：(父节点, 下标, 元素)。
pub fn find<T: ReadTxn>(
    txn: &T,
    root: &XmlFragmentRef,
    id: &str,
) -> Option<(Parent, u32, XmlElementRef)> {
    fn walk<T: ReadTxn, F: XmlFragment>(
        txn: &T,
        p: &F,
        me: &Parent,
        id: &str,
    ) -> Option<(Parent, u32, XmlElementRef)> {
        for (i, c) in p.children(txn).enumerate() {
            if let XmlOut::Element(el) = c {
                if id_of(txn, &el).as_deref() == Some(id) {
                    return Some((me.clone(), i as u32, el));
                }
                let child = Parent::El(el.clone());
                if let Some(hit) = walk(txn, &el, &child, id) {
                    return Some(hit);
                }
            }
        }
        None
    }
    // 先在顶层找（Agent 改的多是顶层块），找不到再往下层找
    let me = Parent::Root(root.clone());
    for (i, c) in root.children(txn).enumerate() {
        if let XmlOut::Element(el) = c
            && id_of(txn, &el).as_deref() == Some(id)
        {
            return Some((me, i as u32, el));
        }
    }
    walk(txn, root, &me, id)
}

/// 旧的一个子项（读出来的样子，用来对齐）。
struct Old {
    key: String, // 类型#编号；文字是 #text
    ty: String,
    id: Option<String>,
    content_hash: u64,
    /// 元素读出来的样子（文字为 None）。
    node: Option<Node>,
    item: XmlOut,
}

/// 新节点和旧的完全一样（编号除外的子孙也一样）就不用动。新节点的子孙自带编号时要连编号一起比。
fn unchanged(old: &Node, new: &Node) -> bool {
    fn has_ids(n: &Node) -> bool {
        n.content
            .iter()
            .any(|c| c.attrs.contains_key("id") || has_ids(c))
    }
    // 属性：新的都和旧的一样；旧的多出来的只能是展示属性（会保留）
    let attrs_same = new.attrs.iter().all(|(k, v)| old.attrs.get(k) == Some(v))
        && old
            .attrs
            .keys()
            .all(|k| new.attrs.contains_key(k) || oad_markdown::node::is_presentation(&old.ty, k));
    if has_ids(new) {
        attrs_same && old.content == new.content
    } else {
        attrs_same && old.without_ids().content == new.content
    }
}

fn hash_node(n: &Node) -> u64 {
    let mut h = DefaultHasher::new();
    n.ty.hash(&mut h);
    n.plain_text().hash(&mut h);
    h.finish()
}

fn key(ty: &str, id: Option<&str>) -> String {
    format!("{ty}#{}", id.unwrap_or(""))
}

/// parent 的 [start, start+old_len) 对齐成 new。
pub fn reconcile_range(
    txn: &mut TransactionMut,
    parent: &Parent,
    start: u32,
    old_len: u32,
    new: &[Node],
    existing: &mut HashSet<String>,
) {
    // 读旧的
    let old: Vec<Old> = parent
        .slice(txn, start, old_len)
        .into_iter()
        .map(|item| match &item {
            XmlOut::Element(el) => {
                let n = convert::read_element(txn, el);
                let id = n.attr_str("id").map(str::to_string);
                Old {
                    key: key(&n.ty, id.as_deref()),
                    ty: n.ty.clone(),
                    id,
                    content_hash: hash_node(&n),
                    node: Some(n),
                    item,
                }
            }
            _ => Old {
                key: "#text".into(),
                ty: "#text".into(),
                id: None,
                content_hash: 0,
                node: None,
                item,
            },
        })
        .collect();
    // 1. 沿用编号（只对没有编号的新节点；有编号的照用）
    let mut new: Vec<Node> = new.to_vec();
    adopt_ids(&old, &mut new);
    // 被这次替换掉的旧编号可以重新用
    for o in &old {
        if let Some(id) = &o.id {
            existing.remove(id);
        }
    }
    // 这一层先分配；子孙等递归对齐时再认领旧编号，只有整个新插入的节点才直接给子孙分配
    let old_ids: HashSet<&str> = old.iter().filter_map(|o| o.id.as_deref()).collect();
    for n in &mut new {
        let adopted = n.attr_str("id").is_some_and(|id| old_ids.contains(id));
        assign_shallow(n, existing);
        if !adopted {
            assign_descendants(n, existing);
        }
    }
    // 2. 按 key 对齐
    let groups = convert::group(&new);
    let new_keys: Vec<String> = groups
        .iter()
        .map(|g| match g {
            Child::Text(_) => "#text".into(),
            Child::Node(n) => key(&n.ty, n.attr_str("id")),
        })
        .collect();
    let old_keys: Vec<String> = old.iter().map(|o| o.key.clone()).collect();
    for op in capture_diff_slices(Algorithm::Myers, &old_keys, &new_keys) {
        // 前面的都已经对好了，位置就是 start + new_index
        match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for k in 0..len {
                    let o = &old[old_index + k];
                    match (&o.item, &groups[new_index + k]) {
                        (XmlOut::Element(el), Child::Node(n)) => {
                            if !o.node.as_ref().is_some_and(|on| unchanged(on, n)) {
                                reconcile_el(txn, el, n, existing)
                            }
                        }
                        (XmlOut::Text(t), Child::Text(runs)) => reconcile_text(txn, t, runs),
                        _ => unreachable!("key 对上了类型却不一样"),
                    }
                }
            }
            DiffOp::Delete {
                new_index, old_len, ..
            } => parent.remove(txn, start + new_index as u32, old_len as u32),
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                // 倒着插在同一个位置：每次只从头数到这个位置一次
                let at = start + new_index as u32;
                for k in (0..new_len).rev() {
                    parent.write_child(txn, at, &groups[new_index + k]);
                }
            }
            DiffOp::Replace {
                new_index,
                old_len,
                new_len,
                ..
            } => {
                let at = start + new_index as u32;
                parent.remove(txn, at, old_len as u32);
                for k in (0..new_len).rev() {
                    parent.write_child(txn, at, &groups[new_index + k]);
                }
            }
        }
    }
}

/// 新节点里没有编号的，从旧节点里认领：先认内容完全一样的，再按类型顺序认。
fn adopt_ids(old: &[Old], new: &mut [Node]) {
    let taken: HashSet<String> = new
        .iter()
        .filter_map(|n| n.attr_str("id").map(str::to_string))
        .collect();
    let free: Vec<usize> = (0..old.len())
        .filter(|&i| old[i].id.as_ref().is_some_and(|id| !taken.contains(id)))
        .collect();
    let want: Vec<usize> = (0..new.len())
        .filter(|&i| !new[i].is_text() && new[i].attr_str("id").is_none())
        .collect();
    if free.is_empty() || want.is_empty() {
        return;
    }
    let mut claimed_old = vec![false; old.len()];
    let assign = |ni: usize, oi: usize, new: &mut [Node], claimed: &mut Vec<bool>| {
        claimed[oi] = true;
        new[ni]
            .attrs
            .insert("id".into(), old[oi].id.clone().unwrap().into());
    };
    // 第一轮：类型 + 文字一样
    let ok: Vec<(String, u64)> = free
        .iter()
        .map(|&i| (old[i].ty.clone(), old[i].content_hash))
        .collect();
    let nk: Vec<(String, u64)> = want
        .iter()
        .map(|&i| (new[i].ty.clone(), hash_node(&new[i])))
        .collect();
    let mut matched_new = vec![false; want.len()];
    for op in capture_diff_slices(Algorithm::Myers, &ok, &nk) {
        if let DiffOp::Equal {
            old_index,
            new_index,
            len,
        } = op
        {
            for k in 0..len {
                assign(
                    want[new_index + k],
                    free[old_index + k],
                    new,
                    &mut claimed_old,
                );
                matched_new[new_index + k] = true;
            }
        }
    }
    // 第二轮：剩下的按类型顺序
    let free2: Vec<usize> = free.iter().copied().filter(|&i| !claimed_old[i]).collect();
    let want2: Vec<usize> = want
        .iter()
        .enumerate()
        .filter(|(k, _)| !matched_new[*k])
        .map(|(_, &i)| i)
        .collect();
    let ot: Vec<&str> = free2.iter().map(|&i| old[i].ty.as_str()).collect();
    let nt: Vec<String> = want2.iter().map(|&i| new[i].ty.clone()).collect();
    let nt: Vec<&str> = nt.iter().map(String::as_str).collect();
    for op in capture_diff_slices(Algorithm::Myers, &ot, &nt) {
        if let DiffOp::Equal {
            old_index,
            new_index,
            len,
        } = op
        {
            for k in 0..len {
                assign(
                    want2[new_index + k],
                    free2[old_index + k],
                    new,
                    &mut claimed_old,
                );
            }
        }
    }
}

fn reconcile_el(
    txn: &mut TransactionMut,
    el: &XmlElementRef,
    node: &Node,
    existing: &mut HashSet<String>,
) {
    let cur = convert::attrs_of(txn, el);
    for (k, v) in &node.attrs {
        if v.is_null() {
            if cur.contains_key(k) {
                el.remove_attribute(txn, k);
            }
        } else if cur.get(k) != Some(v) {
            el.insert_attribute(txn, k.as_str(), convert::j2a(v));
        }
    }
    // 新节点里没写的展示属性（从 Markdown 来的）保留旧的；写成 null 的才清掉
    for k in cur.keys() {
        if !node.attrs.contains_key(k) && !oad_markdown::node::is_presentation(&node.ty, k) {
            el.remove_attribute(txn, k);
        }
    }
    let len = el.len(txn);
    reconcile_range(
        txn,
        &Parent::El(el.clone()),
        0,
        len,
        &node.content,
        existing,
    );
}

fn u16len(chars: &[char]) -> u32 {
    chars.iter().map(|c| c.len_utf16() as u32).sum()
}

fn per_char(runs: &[&Node]) -> (Vec<char>, Vec<Vec<oad_markdown::Mark>>) {
    let mut chars = Vec::new();
    let mut marks = Vec::new();
    for r in runs {
        for c in r.text.as_deref().unwrap_or("").chars() {
            chars.push(c);
            marks.push(r.marks.clone());
        }
    }
    (chars, marks)
}

fn reconcile_text(txn: &mut TransactionMut, t: &XmlTextRef, want: &[&Node]) {
    let cur = convert::read_text(txn, t);
    let cur_refs: Vec<&Node> = cur.iter().collect();
    let (old_chars, _) = per_char(&cur_refs);
    let (new_chars, want_marks) = per_char(want);
    let at = |i: usize| u16len(&new_chars[..i]);
    for op in capture_diff_slices(Algorithm::Myers, &old_chars, &new_chars) {
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete {
                old_index,
                new_index,
                old_len,
            } => t.remove_range(
                txn,
                at(new_index),
                u16len(&old_chars[old_index..old_index + old_len]),
            ),
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                let s: String = new_chars[new_index..new_index + new_len].iter().collect();
                t.insert(txn, at(new_index), &s);
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                t.remove_range(
                    txn,
                    at(new_index),
                    u16len(&old_chars[old_index..old_index + old_len]),
                );
                let s: String = new_chars[new_index..new_index + new_len].iter().collect();
                t.insert(txn, at(new_index), &s);
            }
        }
    }
    // 补格式：逐个 mark 找出要改的区间
    let cur = convert::read_text(txn, t);
    let cur_refs: Vec<&Node> = cur.iter().collect();
    let (_, cur_marks) = per_char(&cur_refs);
    let get = |ms: &Vec<oad_markdown::Mark>, ty: &str| ms.iter().find(|m| m.ty == ty).cloned();
    let mut types: Vec<String> = cur_marks
        .iter()
        .chain(want_marks.iter())
        .flatten()
        .map(|m| m.ty.clone())
        .collect();
    types.sort();
    types.dedup();
    for ty in types {
        let mut i = 0;
        while i < new_chars.len() {
            let (c, w) = (get(&cur_marks[i], &ty), get(&want_marks[i], &ty));
            if c == w {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < new_chars.len()
                && get(&want_marks[j], &ty) == w
                && get(&cur_marks[j], &ty) != w
            {
                j += 1;
            }
            let mut attrs = Attrs::new();
            attrs.insert(
                Arc::from(ty.as_str()),
                w.as_ref().map(convert::mark_attrs).unwrap_or(Any::Null),
            );
            t.format(txn, at(i), at(j) - at(i), attrs);
            i = j;
        }
    }
}
