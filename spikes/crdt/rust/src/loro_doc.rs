//! Loro 版：结构和 loro-prosemirror 一样。根是 map "doc"：{nodeName, attributes: Map, children: List}，
//! children 里是节点（同样结构的 Map）或一串文字（Text，mark 是 text style：mark 名 → attrs）。
use crate::pm::{self, Child, Run};
use loro::{
    Container, ExpandType, LoroDoc, LoroList, LoroMap, LoroText, LoroValue, StyleConfig, StyleConfigMap, TextDelta,
    ValueOrContainer,
};
use serde_json::{Map, Value};
use similar::{capture_diff_slices, Algorithm, DiffOp};
use std::collections::HashMap;

pub fn new_doc(peer: u64, styles: &HashMap<String, String>) -> LoroDoc {
    let d = LoroDoc::new();
    d.set_peer_id(peer).unwrap();
    let mut m = StyleConfigMap::new();
    for (k, v) in styles {
        let expand = match v.as_str() {
            "after" => ExpandType::After,
            "before" => ExpandType::Before,
            "both" => ExpandType::Both,
            _ => ExpandType::None,
        };
        m.insert(k.as_str().into(), StyleConfig { expand });
    }
    d.config_text_style(m);
    d
}

pub fn j2l(v: &Value) -> LoroValue {
    match v {
        Value::Null => LoroValue::Null,
        Value::Bool(b) => LoroValue::Bool(*b),
        Value::Number(n) => n.as_i64().map(LoroValue::I64).unwrap_or_else(|| LoroValue::Double(n.as_f64().unwrap())),
        Value::String(s) => LoroValue::from(s.as_str()),
        Value::Array(a) => LoroValue::from(a.iter().map(j2l).collect::<Vec<_>>()),
        Value::Object(o) => {
            let m: HashMap<String, LoroValue> = o.iter().map(|(k, v)| (k.clone(), j2l(v))).collect();
            LoroValue::from(m)
        }
    }
}

pub fn l2j(v: &LoroValue) -> Value {
    match v {
        LoroValue::Null => Value::Null,
        LoroValue::Bool(b) => (*b).into(),
        LoroValue::I64(i) => (*i).into(),
        LoroValue::Double(f) => serde_json::Number::from_f64(*f).map(Value::Number).unwrap_or(Value::Null),
        LoroValue::String(s) => Value::String(s.to_string()),
        LoroValue::List(l) => Value::Array(l.iter().map(l2j).collect()),
        LoroValue::Map(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), l2j(v))).collect()),
        LoroValue::Binary(_) | LoroValue::Container(_) => Value::Null,
    }
}

fn root(doc: &LoroDoc) -> LoroMap {
    doc.get_map("doc")
}

pub fn build(doc: &LoroDoc, pm_doc: &Value) {
    fill_map(&root(doc), pm_doc);
    doc.commit();
}

fn fill_map(m: &LoroMap, node: &Value) {
    m.insert("nodeName", node["type"].as_str().unwrap()).unwrap();
    let attrs = m.insert_container("attributes", LoroMap::new()).unwrap();
    if let Some(a) = node.get("attrs").and_then(|a| a.as_object()) {
        for (k, v) in a {
            if !v.is_null() {
                attrs.insert(k, j2l(v)).unwrap();
            }
        }
    }
    let children = m.insert_container("children", LoroList::new()).unwrap();
    for (i, ch) in pm::group(node).iter().enumerate() {
        insert_child(&children, i, ch);
    }
}

fn insert_child(list: &LoroList, pos: usize, ch: &Child) {
    match ch {
        Child::Node(n) => {
            let m = list.insert_container(pos, LoroMap::new()).unwrap();
            fill_map(&m, n);
        }
        Child::Text(runs) => {
            let t = list.insert_container(pos, LoroText::new()).unwrap();
            let all: String = runs.iter().map(|r| r.text.as_str()).collect();
            t.insert(0, &all).unwrap();
            let mut off = 0;
            for r in runs {
                let len = r.text.chars().count();
                for (k, v) in &r.marks {
                    t.mark(off..off + len, k, j2l(v)).unwrap();
                }
                off += len;
            }
        }
    }
}

// ---------- 读 ----------

pub fn read(doc: &LoroDoc) -> Value {
    read_map(&root(doc))
}

fn attrs_of(m: &LoroMap) -> Map<String, Value> {
    match m.get("attributes") {
        Some(ValueOrContainer::Container(Container::Map(a))) => match l2j(&a.get_deep_value()) {
            Value::Object(o) => o,
            _ => Map::new(),
        },
        _ => Map::new(),
    }
}

fn children_of(m: &LoroMap) -> Option<LoroList> {
    match m.get("children") {
        Some(ValueOrContainer::Container(Container::List(l))) => Some(l),
        _ => None,
    }
}

fn name_of(m: &LoroMap) -> String {
    match m.get("nodeName") {
        Some(ValueOrContainer::Value(v)) => v.as_string().map(|s| s.to_string()).unwrap_or_default(),
        _ => String::new(),
    }
}

fn runs_of(t: &LoroText) -> Vec<Run> {
    t.to_delta()
        .into_iter()
        .filter_map(|d| match d {
            TextDelta::Insert { insert, attributes } => Some(Run {
                text: insert,
                marks: attributes
                    .unwrap_or_default()
                    .iter()
                    .filter(|(_, v)| !v.is_null())
                    .map(|(k, v)| (k.clone(), l2j(v)))
                    .collect(),
            }),
            _ => None,
        })
        .collect()
}

fn read_map(m: &LoroMap) -> Value {
    let mut o = Map::new();
    o.insert("type".into(), name_of(m).into());
    let attrs = attrs_of(m);
    if !attrs.is_empty() {
        o.insert("attrs".into(), Value::Object(attrs));
    }
    let mut content = Vec::new();
    if let Some(list) = children_of(m) {
        for i in 0..list.len() {
            match list.get(i) {
                Some(ValueOrContainer::Container(Container::Map(c))) => content.push(read_map(&c)),
                Some(ValueOrContainer::Container(Container::Text(t))) => content.extend(pm::runs_to_json(&runs_of(&t))),
                _ => {}
            }
        }
    }
    if !content.is_empty() {
        o.insert("content".into(), content.into());
    }
    Value::Object(o)
}

// ---------- 按块编号改 ----------

/// 找编号为 id 的节点：(所在的 children 列表, 下标, 节点)。
pub fn find(doc: &LoroDoc, id: &str) -> Option<(LoroList, usize, LoroMap)> {
    fn walk(m: &LoroMap, id: &str) -> Option<(LoroList, usize, LoroMap)> {
        let list = children_of(m)?;
        for i in 0..list.len() {
            if let Some(ValueOrContainer::Container(Container::Map(c))) = list.get(i) {
                if attrs_of(&c).get("id").and_then(|v| v.as_str()) == Some(id) {
                    return Some((list, i, c));
                }
                if let Some(hit) = walk(&c, id) {
                    return Some(hit);
                }
            }
        }
        None
    }
    walk(&root(doc), id)
}

/// 把编号为 id 的块换成 nodes（可以是多个）。同类型、同编号的节点就地对比着改（只改变了的字），别的整个换。
pub fn replace_block(doc: &LoroDoc, id: &str, nodes: &[Value]) -> bool {
    let Some((list, idx, _)) = find(doc, id) else { return false };
    // 把这一个旧块和新的几个块当成一段子列表来对齐
    let old_keys = vec![key_at(&list, idx)];
    let new_children: Vec<Child> = nodes.iter().map(Child::Node).collect();
    reconcile_range(&list, idx, &old_keys, &new_children);
    doc.commit();
    true
}

/// 在编号为 id 的块后面插入 nodes。
pub fn insert_after(doc: &LoroDoc, id: &str, nodes: &[Value]) -> bool {
    let Some((list, idx, _)) = find(doc, id) else { return false };
    for (k, n) in nodes.iter().enumerate() {
        insert_child(&list, idx + 1 + k, &Child::Node(n));
    }
    doc.commit();
    true
}

pub fn delete_block(doc: &LoroDoc, id: &str) -> bool {
    let Some((list, idx, _)) = find(doc, id) else { return false };
    list.delete(idx, 1).unwrap();
    doc.commit();
    true
}

fn key_at(list: &LoroList, i: usize) -> String {
    match list.get(i) {
        Some(ValueOrContainer::Container(Container::Map(c))) => {
            format!("{}#{}", name_of(&c), attrs_of(&c).get("id").and_then(|v| v.as_str()).unwrap_or(""))
        }
        _ => "#text".into(),
    }
}

/// list[start .. start+old_keys.len()] 对齐成 new。
fn reconcile_range(list: &LoroList, start: usize, old_keys: &[String], new: &[Child]) {
    let new_keys: Vec<String> = new.iter().map(|c| c.key()).collect();
    for op in capture_diff_slices(Algorithm::Myers, old_keys, &new_keys) {
        // 前面的都已经对好了，所以位置就是 start + new_index
        match op {
            DiffOp::Equal { new_index, len, .. } => {
                for k in 0..len {
                    let pos = start + new_index + k;
                    reconcile_child(list, pos, &new[new_index + k]);
                }
            }
            DiffOp::Delete { new_index, old_len, .. } => list.delete(start + new_index, old_len).unwrap(),
            DiffOp::Insert { new_index, new_len, .. } => {
                for k in 0..new_len {
                    insert_child(list, start + new_index + k, &new[new_index + k]);
                }
            }
            DiffOp::Replace { new_index, old_len, new_len, .. } => {
                list.delete(start + new_index, old_len).unwrap();
                for k in 0..new_len {
                    insert_child(list, start + new_index + k, &new[new_index + k]);
                }
            }
        }
    }
}

fn reconcile_child(list: &LoroList, pos: usize, want: &Child) {
    match (list.get(pos), want) {
        (Some(ValueOrContainer::Container(Container::Map(m))), Child::Node(n)) => reconcile_map(&m, n),
        (Some(ValueOrContainer::Container(Container::Text(t))), Child::Text(runs)) => reconcile_text(&t, runs),
        _ => unreachable!(),
    }
}

fn reconcile_map(m: &LoroMap, node: &Value) {
    let attrs = match m.get("attributes") {
        Some(ValueOrContainer::Container(Container::Map(a))) => a,
        _ => m.insert_container("attributes", LoroMap::new()).unwrap(),
    };
    let cur = attrs_of(m);
    let want = node.get("attrs").and_then(|a| a.as_object()).cloned().unwrap_or_default();
    for (k, v) in &want {
        if v.is_null() {
            if cur.contains_key(k) {
                attrs.delete(k).unwrap();
            }
        } else if cur.get(k) != Some(v) {
            attrs.insert(k, j2l(v)).unwrap();
        }
    }
    for k in cur.keys() {
        if !want.contains_key(k) {
            attrs.delete(k).unwrap();
        }
    }
    let list = children_of(m).unwrap_or_else(|| m.insert_container("children", LoroList::new()).unwrap());
    let old_keys: Vec<String> = (0..list.len()).map(|i| key_at(&list, i)).collect();
    reconcile_range(&list, 0, &old_keys, &pm::group(node));
}

/// 文字只改变了的字：先按字符 diff 改文字，再补 mark。位置按 Unicode 字符计（Loro 的 Rust 接口就是这样）。
fn reconcile_text(t: &LoroText, want: &[Run]) {
    let (old_chars, _) = pm::per_char(&runs_of(t));
    let (new_chars, want_marks) = pm::per_char(want);
    for op in capture_diff_slices(Algorithm::Myers, &old_chars, &new_chars) {
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete { new_index, old_len, .. } => t.delete(new_index, old_len).unwrap(),
            DiffOp::Insert { new_index, new_len, .. } => {
                let s: String = new_chars[new_index..new_index + new_len].iter().collect();
                t.insert(new_index, &s).unwrap();
            }
            DiffOp::Replace { new_index, old_len, new_len, .. } => {
                t.delete(new_index, old_len).unwrap();
                let s: String = new_chars[new_index..new_index + new_len].iter().collect();
                t.insert(new_index, &s).unwrap();
            }
        }
    }
    let (_, cur_marks) = pm::per_char(&runs_of(t));
    for (a, b, key, val) in pm::mark_patches(&cur_marks, &want_marks) {
        match val {
            Some(v) => t.mark(a..b, &key, j2l(&v)).unwrap(),
            None => t.unmark(a..b, &key).unwrap(),
        }
    }
}
