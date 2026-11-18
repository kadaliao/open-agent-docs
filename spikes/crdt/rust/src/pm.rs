//! ProseMirror JSON 的小工具：把子节点分成“一串文字”和“节点”两类（和两个绑定的 normalizeNodeContent 一致），
//! 以及按字符比较两串带格式文字。
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub type Marks = BTreeMap<String, Value>; // mark 名 → attrs（对象）

#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub marks: Marks,
}

pub enum Child<'a> {
    Text(Vec<Run>),
    Node(&'a Value),
}

impl Child<'_> {
    pub fn key(&self) -> String {
        match self {
            Child::Text(_) => "#text".into(),
            Child::Node(n) => node_key(n),
        }
    }
}

pub fn node_key(n: &Value) -> String {
    format!("{}#{}", n["type"].as_str().unwrap_or(""), n["attrs"]["id"].as_str().unwrap_or(""))
}

pub fn marks_of(text_node: &Value) -> Marks {
    let mut m = Marks::new();
    if let Some(arr) = text_node.get("marks").and_then(|x| x.as_array()) {
        for mk in arr {
            let attrs = mk.get("attrs").cloned().unwrap_or_else(|| Value::Object(Map::new()));
            m.insert(mk["type"].as_str().unwrap().to_string(), attrs);
        }
    }
    m
}

pub fn group(node: &Value) -> Vec<Child<'_>> {
    let mut out: Vec<Child> = Vec::new();
    let Some(content) = node.get("content").and_then(|c| c.as_array()) else { return out };
    for c in content {
        if c["type"] == "text" {
            let run = Run { text: c["text"].as_str().unwrap().to_string(), marks: marks_of(c) };
            if let Some(Child::Text(runs)) = out.last_mut() {
                runs.push(run);
            } else {
                out.push(Child::Text(vec![run]));
            }
        } else {
            out.push(Child::Node(c));
        }
    }
    out
}

/// 每个字符一份 marks。
pub fn per_char(runs: &[Run]) -> (Vec<char>, Vec<Marks>) {
    let mut chars = Vec::new();
    let mut marks = Vec::new();
    for r in runs {
        for ch in r.text.chars() {
            chars.push(ch);
            marks.push(r.marks.clone());
        }
    }
    (chars, marks)
}

/// 文字已经一样以后，哪些区间的哪个 mark 要改：(start, end, key, Some(新值) / None=去掉)。区间按字符计。
pub fn mark_patches(cur: &[Marks], want: &[Marks]) -> Vec<(usize, usize, String, Option<Value>)> {
    assert_eq!(cur.len(), want.len());
    let mut keys: Vec<&String> = cur.iter().chain(want.iter()).flat_map(|m| m.keys()).collect();
    keys.sort();
    keys.dedup();
    let mut out = Vec::new();
    for key in keys {
        let mut i = 0;
        while i < cur.len() {
            let (c, w) = (cur[i].get(key), want[i].get(key));
            if c == w {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < cur.len() && want[j].get(key) == w && cur[j].get(key) != w {
                j += 1;
            }
            out.push((i, j, key.clone(), w.cloned()));
            i = j;
        }
    }
    out
}

/// 把 runs 拼回 PM 的 text 节点（attrs 为空的 mark 不写 attrs，和 PM 的 toJSON 一样）。
pub fn runs_to_json(runs: &[Run]) -> Vec<Value> {
    runs.iter()
        .filter(|r| !r.text.is_empty())
        .map(|r| {
            let mut o = Map::new();
            o.insert("type".into(), "text".into());
            if !r.marks.is_empty() {
                let marks: Vec<Value> = r
                    .marks
                    .iter()
                    .map(|(k, v)| {
                        let mut m = Map::new();
                        m.insert("type".into(), k.clone().into());
                        if v.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
                            m.insert("attrs".into(), v.clone());
                        }
                        Value::Object(m)
                    })
                    .collect();
                o.insert("marks".into(), marks.into());
            }
            o.insert("text".into(), r.text.clone().into());
            Value::Object(o)
        })
        .collect()
}
