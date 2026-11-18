//! Yrs 版：结构和 y-prosemirror（Tiptap Collaboration）一样。根是 XmlFragment "default"，里面是文档的子节点：
//! 节点是 XmlElement（标签 = 节点类型，属性 = attrs），一串文字是 XmlText（格式：mark 名 → attrs）。
//! 位置按 UTF-16 计（和 JS 端一致）。
use crate::pm::{self, Child, Run};
use similar::{capture_diff_slices, Algorithm, DiffOp};
use std::collections::HashMap;
use std::sync::Arc;
use yrs::types::text::YChange;
use yrs::types::Attrs;
use yrs::{
    Any, ClientID, Doc, Number, OffsetKind, Options, Out, ReadTxn, Text, Transact, TransactionMut, Xml, XmlElementPrelim, XmlElementRef,
    XmlFragment, XmlFragmentRef, XmlOut, XmlTextPrelim, XmlTextRef,
};
use serde_json::{Map, Value};

pub const FIELD: &str = "default";

pub fn new_doc(client: u64, skip_gc: bool) -> Doc {
    Doc::with_options(Options { client_id: ClientID::new(client), offset_kind: OffsetKind::Utf16, skip_gc, ..Default::default() })
}

pub fn j2a(v: &Value) -> Any {
    match v {
        Value::Null => Any::Null,
        Value::Bool(b) => Any::Bool(*b),
        Value::Number(n) => Any::Number(n.as_i64().map(Number::Int).unwrap_or_else(|| Number::Float(n.as_f64().unwrap()))),
        Value::String(s) => Any::String(s.as_str().into()),
        Value::Array(a) => Any::Array(a.iter().map(j2a).collect::<Vec<_>>().into()),
        Value::Object(o) => Any::Map(Arc::new(o.iter().map(|(k, v)| (k.clone(), j2a(v))).collect::<HashMap<_, _>>())),
    }
}

pub fn a2j(v: &Any) -> Value {
    match v {
        Any::Null | Any::Undefined => Value::Null,
        Any::Bool(b) => (*b).into(),
        Any::Number(Number::Int(i)) => (*i).into(),
        Any::Number(Number::Float(f)) if f.fract() == 0.0 && f.abs() < 9e15 => (*f as i64).into(),
        Any::Number(Number::Float(f)) => serde_json::Number::from_f64(*f).map(Value::Number).unwrap_or(Value::Null),
        Any::String(s) => Value::String(s.to_string()),
        Any::Array(a) => Value::Array(a.iter().map(a2j).collect()),
        Any::Map(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), a2j(v))).collect()),
        Any::Buffer(_) => Value::Null,
    }
}

fn out2j(o: &Out) -> Value {
    match o {
        Out::Any(a) => a2j(a),
        _ => Value::Null,
    }
}

fn u16len(chars: &[char]) -> u32 {
    chars.iter().map(|c| c.len_utf16() as u32).sum()
}

pub fn root(doc: &Doc) -> XmlFragmentRef {
    doc.get_or_insert_xml_fragment(FIELD)
}

pub fn build(doc: &Doc, pm_doc: &Value) {
    let frag = root(doc);
    let mut txn = doc.transact_mut();
    for (i, ch) in pm::group(pm_doc).iter().enumerate() {
        insert_child(&mut txn, &frag, i as u32, ch);
    }
}

fn insert_child<F: XmlFragment>(txn: &mut TransactionMut, parent: &F, pos: u32, ch: &Child) {
    match ch {
        Child::Node(n) => {
            let el = parent.insert(txn, pos, XmlElementPrelim::empty(n["type"].as_str().unwrap()));
            fill_el(txn, &el, n);
        }
        Child::Text(runs) => {
            let t = parent.insert(txn, pos, XmlTextPrelim::new(""));
            let all: String = runs.iter().map(|r| r.text.as_str()).collect();
            t.insert(txn, 0, &all);
            let mut off = 0u32;
            for r in runs {
                let len = r.text.encode_utf16().count() as u32;
                if !r.marks.is_empty() {
                    let attrs: Attrs = r.marks.iter().map(|(k, v)| (Arc::from(k.as_str()), j2a(v))).collect();
                    t.format(txn, off, len, attrs);
                }
                off += len;
            }
        }
    }
}

fn fill_el(txn: &mut TransactionMut, el: &XmlElementRef, node: &Value) {
    if let Some(a) = node.get("attrs").and_then(|a| a.as_object()) {
        for (k, v) in a {
            if !v.is_null() {
                el.insert_attribute(txn, k.as_str(), j2a(v));
            }
        }
    }
    for (i, ch) in pm::group(node).iter().enumerate() {
        insert_child(txn, el, i as u32, ch);
    }
}

// ---------- 读 ----------

pub fn read(doc: &Doc) -> Value {
    let frag = root(doc);
    let txn = doc.transact();
    let mut o = Map::new();
    o.insert("type".into(), "doc".into());
    o.insert("content".into(), read_children(&txn, &frag).into());
    Value::Object(o)
}

fn attrs_of<T: ReadTxn>(txn: &T, el: &XmlElementRef) -> Map<String, Value> {
    el.attributes(txn).map(|(k, v)| (k.to_string(), out2j(&v))).filter(|(_, v)| !v.is_null()).collect()
}

fn runs_of<T: ReadTxn>(txn: &T, t: &XmlTextRef) -> Vec<Run> {
    t.diff(txn, YChange::identity)
        .into_iter()
        .map(|d| Run {
            text: match &d.insert {
                Out::Any(Any::String(s)) => s.to_string(),
                _ => String::new(),
            },
            marks: d
                .attributes
                .map(|a| a.iter().filter(|(_, v)| !matches!(v, Any::Null)).map(|(k, v)| (k.to_string(), a2j(v))).collect())
                .unwrap_or_default(),
        })
        .collect()
}

fn read_children<T: ReadTxn, F: XmlFragment>(txn: &T, parent: &F) -> Vec<Value> {
    let mut out = Vec::new();
    for c in parent.children(txn) {
        match c {
            XmlOut::Element(el) => {
                let mut o = Map::new();
                o.insert("type".into(), el.tag().to_string().into());
                let attrs = attrs_of(txn, &el);
                if !attrs.is_empty() {
                    o.insert("attrs".into(), Value::Object(attrs));
                }
                let content = read_children(txn, &el);
                if !content.is_empty() {
                    o.insert("content".into(), content.into());
                }
                out.push(Value::Object(o));
            }
            XmlOut::Text(t) => out.extend(pm::runs_to_json(&runs_of(txn, &t))),
            XmlOut::Fragment(_) => {}
        }
    }
    out
}

// ---------- 按块编号改 ----------

enum Parent {
    Root(XmlFragmentRef),
    El(XmlElementRef),
}

fn find<T: ReadTxn>(txn: &T, frag: XmlFragmentRef, id: &str) -> Option<(Parent, u32)> {
    fn walk<T: ReadTxn, F: XmlFragment>(txn: &T, p: &F, id: &str) -> Option<(Option<XmlElementRef>, u32)> {
        for (i, c) in p.children(txn).enumerate() {
            if let XmlOut::Element(el) = c {
                if matches!(el.get_attribute(txn, "id"), Some(Out::Any(Any::String(s))) if &*s == id) {
                    return Some((None, i as u32));
                }
                if let Some((parent, idx)) = walk(txn, &el, id) {
                    return Some((Some(parent.unwrap_or(el)), idx));
                }
            }
        }
        None
    }
    let (p, i) = walk(txn, &frag, id)?;
    Some((p.map(Parent::El).unwrap_or(Parent::Root(frag)), i))
}

fn key_at<T: ReadTxn, F: XmlFragment>(txn: &T, p: &F, i: u32) -> String {
    match p.get(txn, i) {
        Some(XmlOut::Element(el)) => format!(
            "{}#{}",
            el.tag(),
            match el.get_attribute(txn, "id") {
                Some(Out::Any(Any::String(s))) => s.to_string(),
                _ => String::new(),
            }
        ),
        _ => "#text".into(),
    }
}

pub fn replace_block(doc: &Doc, id: &str, nodes: &[Value]) -> bool {
    // 先拿根，再开写事务（事务开着时 get_or_insert_xml_fragment 会等锁，卡死）
    let frag = root(doc);
    let mut txn = doc.transact_mut();
    let Some((parent, idx)) = find(&txn, frag, id) else { return false };
    let new: Vec<Child> = nodes.iter().map(Child::Node).collect();
    match parent {
        Parent::Root(f) => {
            let k = vec![key_at(&txn, &f, idx)];
            reconcile_range(&mut txn, &f, idx, &k, &new)
        }
        Parent::El(e) => {
            let k = vec![key_at(&txn, &e, idx)];
            reconcile_range(&mut txn, &e, idx, &k, &new)
        }
    }
    true
}

pub fn insert_after(doc: &Doc, id: &str, nodes: &[Value]) -> bool {
    // 先拿根，再开写事务（事务开着时 get_or_insert_xml_fragment 会等锁，卡死）
    let frag = root(doc);
    let mut txn = doc.transact_mut();
    let Some((parent, idx)) = find(&txn, frag, id) else { return false };
    for (k, n) in nodes.iter().enumerate() {
        let pos = idx + 1 + k as u32;
        match &parent {
            Parent::Root(f) => insert_child(&mut txn, f, pos, &Child::Node(n)),
            Parent::El(e) => insert_child(&mut txn, e, pos, &Child::Node(n)),
        }
    }
    true
}

pub fn delete_block(doc: &Doc, id: &str) -> bool {
    // 先拿根，再开写事务（事务开着时 get_or_insert_xml_fragment 会等锁，卡死）
    let frag = root(doc);
    let mut txn = doc.transact_mut();
    let Some((parent, idx)) = find(&txn, frag, id) else { return false };
    match parent {
        Parent::Root(f) => f.remove_range(&mut txn, idx, 1),
        Parent::El(e) => e.remove_range(&mut txn, idx, 1),
    }
    true
}

fn reconcile_range<F: XmlFragment>(txn: &mut TransactionMut, p: &F, start: u32, old_keys: &[String], new: &[Child]) {
    let new_keys: Vec<String> = new.iter().map(|c| c.key()).collect();
    for op in capture_diff_slices(Algorithm::Myers, old_keys, &new_keys) {
        match op {
            DiffOp::Equal { new_index, len, .. } => {
                for k in 0..len {
                    let pos = start + (new_index + k) as u32;
                    match (p.get(txn, pos), &new[new_index + k]) {
                        (Some(XmlOut::Element(el)), Child::Node(n)) => reconcile_el(txn, &el, n),
                        (Some(XmlOut::Text(t)), Child::Text(runs)) => reconcile_text(txn, &t, runs),
                        _ => unreachable!(),
                    }
                }
            }
            DiffOp::Delete { new_index, old_len, .. } => p.remove_range(txn, start + new_index as u32, old_len as u32),
            DiffOp::Insert { new_index, new_len, .. } => {
                for k in 0..new_len {
                    insert_child(txn, p, start + (new_index + k) as u32, &new[new_index + k]);
                }
            }
            DiffOp::Replace { new_index, old_len, new_len, .. } => {
                p.remove_range(txn, start + new_index as u32, old_len as u32);
                for k in 0..new_len {
                    insert_child(txn, p, start + (new_index + k) as u32, &new[new_index + k]);
                }
            }
        }
    }
}

fn reconcile_el(txn: &mut TransactionMut, el: &XmlElementRef, node: &Value) {
    let cur = attrs_of(txn, el);
    let want = node.get("attrs").and_then(|a| a.as_object()).cloned().unwrap_or_default();
    for (k, v) in &want {
        if v.is_null() {
            if cur.contains_key(k) {
                el.remove_attribute(txn, k);
            }
        } else if cur.get(k) != Some(v) {
            el.insert_attribute(txn, k.as_str(), j2a(v));
        }
    }
    for k in cur.keys() {
        if !want.contains_key(k) {
            el.remove_attribute(txn, k);
        }
    }
    let n = el.len(txn);
    let old_keys: Vec<String> = (0..n).map(|i| key_at(txn, el, i)).collect();
    reconcile_range(txn, el, 0, &old_keys, &pm::group(node));
}

fn reconcile_text(txn: &mut TransactionMut, t: &XmlTextRef, want: &[Run]) {
    let (old_chars, _) = pm::per_char(&runs_of(txn, t));
    let (new_chars, want_marks) = pm::per_char(want);
    for op in capture_diff_slices(Algorithm::Myers, &old_chars, &new_chars) {
        let at = |i: usize| u16len(&new_chars[..i]);
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete { old_index, new_index, old_len } => {
                t.remove_range(txn, at(new_index), u16len(&old_chars[old_index..old_index + old_len]))
            }
            DiffOp::Insert { new_index, new_len, .. } => {
                let s: String = new_chars[new_index..new_index + new_len].iter().collect();
                t.insert(txn, at(new_index), &s);
            }
            DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                t.remove_range(txn, at(new_index), u16len(&old_chars[old_index..old_index + old_len]));
                let s: String = new_chars[new_index..new_index + new_len].iter().collect();
                t.insert(txn, at(new_index), &s);
            }
        }
    }
    let (_, cur_marks) = pm::per_char(&runs_of(txn, t));
    for (a, b, key, val) in pm::mark_patches(&cur_marks, &want_marks) {
        let (s, e) = (u16len(&new_chars[..a]), u16len(&new_chars[..b]));
        let mut attrs = Attrs::new();
        attrs.insert(Arc::from(key.as_str()), val.as_ref().map(j2a).unwrap_or(Any::Null));
        t.format(txn, s, e - s, attrs);
    }
}
