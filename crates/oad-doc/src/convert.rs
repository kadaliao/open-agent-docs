//! Node ↔ Yrs。和 y-prosemirror 的约定一致：连着的 text 节点合成一个 XmlText，mark 是文字格式（名 → attrs 对象），
//! 属性值为 null 的不写。
use crate::Node;
use oad_markdown::Mark;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use yrs::types::Attrs;
use yrs::types::text::YChange;
use yrs::{
    Any, Number, Out, ReadTxn, Text, TransactionMut, Xml, XmlElementPrelim, XmlElementRef,
    XmlFragment, XmlOut, XmlTextPrelim, XmlTextRef,
};

pub fn j2a(v: &Value) -> Any {
    match v {
        Value::Null => Any::Null,
        Value::Bool(b) => Any::Bool(*b),
        Value::Number(n) => Any::Number(
            n.as_i64()
                .map(Number::Int)
                .unwrap_or_else(|| Number::Float(n.as_f64().unwrap_or(0.0))),
        ),
        Value::String(s) => Any::String(s.as_str().into()),
        Value::Array(a) => Any::Array(a.iter().map(j2a).collect::<Vec<_>>().into()),
        Value::Object(o) => Any::Map(Arc::new(
            o.iter()
                .map(|(k, v)| (k.clone(), j2a(v)))
                .collect::<HashMap<_, _>>(),
        )),
    }
}

pub fn a2j(v: &Any) -> Value {
    match v {
        Any::Null | Any::Undefined => Value::Null,
        Any::Bool(b) => (*b).into(),
        Any::Number(Number::Int(i)) => (*i).into(),
        Any::Number(Number::Float(f)) if f.fract() == 0.0 && f.abs() < 9e15 => (*f as i64).into(),
        Any::Number(Number::Float(f)) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Any::String(s) => Value::String(s.to_string()),
        Any::Array(a) => Value::Array(a.iter().map(a2j).collect()),
        Any::Map(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), a2j(v))).collect()),
        Any::Buffer(_) => Value::Null,
    }
}

/// 子节点分组：连着的行内节点（text）合成一段文字，别的单独。
pub enum Child<'a> {
    Text(Vec<&'a Node>),
    Node(&'a Node),
}

pub fn group(nodes: &[Node]) -> Vec<Child<'_>> {
    let mut out: Vec<Child> = Vec::new();
    for n in nodes {
        if n.is_text() {
            if let Some(Child::Text(v)) = out.last_mut() {
                v.push(n);
            } else {
                out.push(Child::Text(vec![n]));
            }
        } else {
            out.push(Child::Node(n));
        }
    }
    out
}

pub fn mark_attrs(m: &Mark) -> Any {
    j2a(&Value::Object(m.attrs.clone()))
}

/// 在 parent 的 at 位置写入一串子节点。倒着往同一个位置插：Yrs 按下标插入每次都要从头数，正着一个个往后插大文档会变成 O(n²)。
pub fn write_children<F: XmlFragment>(
    txn: &mut TransactionMut,
    parent: &F,
    at: u32,
    nodes: &[Node],
) {
    for g in group(nodes).iter().rev() {
        write_child(txn, parent, at, g);
    }
}

pub fn write_child<F: XmlFragment>(txn: &mut TransactionMut, parent: &F, pos: u32, ch: &Child) {
    match ch {
        Child::Node(n) => {
            let el = parent.insert(txn, pos, XmlElementPrelim::empty(n.ty.as_str()));
            for (k, v) in &n.attrs {
                if !v.is_null() {
                    el.insert_attribute(txn, k.as_str(), j2a(v));
                }
            }
            write_children(txn, &el, 0, &n.content);
        }
        Child::Text(runs) => {
            let t = parent.insert(txn, pos, XmlTextPrelim::new(""));
            let all: String = runs
                .iter()
                .map(|r| r.text.as_deref().unwrap_or(""))
                .collect();
            t.insert(txn, 0, &all);
            let mut off = 0u32;
            for r in runs {
                let len = r.text.as_deref().unwrap_or("").encode_utf16().count() as u32;
                if !r.marks.is_empty() {
                    let attrs: Attrs = r
                        .marks
                        .iter()
                        .map(|m| (Arc::from(m.ty.as_str()), mark_attrs(m)))
                        .collect();
                    t.format(txn, off, len, attrs);
                }
                off += len;
            }
        }
    }
}

pub fn attrs_of<T: ReadTxn>(txn: &T, el: &XmlElementRef) -> Map<String, Value> {
    let mut m: Map<String, Value> = el
        .attributes(txn)
        .filter_map(|(k, v)| match v {
            Out::Any(a) => Some((k.to_string(), a2j(&a))),
            _ => None,
        })
        .filter(|(_, v)| !v.is_null())
        .collect();
    m.sort_keys();
    m
}

/// 一段文字读成 text 节点。
pub fn read_text<T: ReadTxn>(txn: &T, t: &XmlTextRef) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::new();
    for d in t.diff(txn, YChange::identity) {
        let Out::Any(Any::String(s)) = &d.insert else {
            continue;
        };
        if s.is_empty() {
            continue;
        }
        let mut marks: Vec<Mark> = d
            .attributes
            .map(|a| {
                a.iter()
                    .filter(|(_, v)| !matches!(v, Any::Null | Any::Undefined))
                    .map(|(k, v)| {
                        let attrs = match a2j(v) {
                            Value::Object(o) => {
                                o.into_iter().filter(|(_, x)| !x.is_null()).collect()
                            }
                            _ => Map::new(),
                        };
                        Mark {
                            ty: k.to_string(),
                            attrs,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        oad_markdown::node::sort_marks(&mut marks);
        out.push(Node::text(s.to_string(), marks));
    }
    oad_markdown::node::merge_texts(out)
}

pub fn read_element<T: ReadTxn>(txn: &T, el: &XmlElementRef) -> Node {
    let mut n = Node::new(el.tag().as_ref());
    n.attrs = attrs_of(txn, el);
    n.content = read_children(txn, el);
    n
}

pub fn read_children<T: ReadTxn, F: XmlFragment>(txn: &T, parent: &F) -> Vec<Node> {
    let mut out = Vec::new();
    for c in parent.children(txn) {
        match c {
            XmlOut::Element(el) => out.push(read_element(txn, &el)),
            XmlOut::Text(t) => out.extend(read_text(txn, &t)),
            XmlOut::Fragment(_) => {}
        }
    }
    out
}
