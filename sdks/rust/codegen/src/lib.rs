//! 生成器：读 `docs/openapi.json`（接口、模型）和 `sdks/operations.json`（每个接口在 SDK 里叫什么），
//! 写 `open-agent-docs/src/generated/` 下的三个文件：
//!
//! - `types.rs`：模型（`oad::types::*`）和工作区事件的枚举；
//! - `resources.rs`：`client.pages().versions().list(id)` 这样的资源访问器，每个接口一个请求构造器；
//! - `mod.rs`。
//!
//! 生成完跑一遍 rustfmt。服务器加了接口、改了模型，重新跑 `cargo run -p codegen` 就行；
//! `cargo test -p codegen` 检查提交进仓库的生成结果是最新的。
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const HEADER: &str = "// 生成的，不要手改：改 docs/openapi.json 或 sdks/operations.json，再跑 `cargo run -p codegen`（在 sdks/rust 下）。\n";

/// 仓库里的几个位置（相对 codegen 自己的目录算）。
pub struct Paths {
    pub openapi: PathBuf,
    pub operations: PathBuf,
    pub out_dir: PathBuf,
}

impl Paths {
    pub fn default_paths() -> Paths {
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Paths {
            openapi: here.join("../../../docs/openapi.json"),
            operations: here.join("../../operations.json"),
            out_dir: here.join("../open-agent-docs/src/generated"),
        }
    }
}

/// 生成并写文件；`check` 时只比较，返回过时的文件名。
pub fn run(paths: &Paths, check: bool) -> Result<Vec<String>, String> {
    let read = |p: &PathBuf| -> Result<Value, String> {
        let s = std::fs::read_to_string(p).map_err(|e| format!("读 {}：{e}", p.display()))?;
        serde_json::from_str(&s).map_err(|e| format!("解析 {}：{e}", p.display()))
    };
    let spec = read(&paths.openapi)?;
    let ops = read(&paths.operations)?;
    let files = generate(&spec, &ops)?;
    let mut stale = Vec::new();
    for (name, src) in files {
        let src = rustfmt(&src).map_err(|e| format!("rustfmt {name}：{e}"))?;
        let path = paths.out_dir.join(&name);
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        if old != src {
            stale.push(name.clone());
            if !check {
                std::fs::create_dir_all(&paths.out_dir).map_err(|e| e.to_string())?;
                std::fs::write(&path, src).map_err(|e| format!("写 {}：{e}", path.display()))?;
            }
        }
    }
    Ok(stale)
}

/// 用 rustfmt 排版（从标准输入读，标准输出写）。
pub fn rustfmt(src: &str) -> Result<String, String> {
    let mut child = Command::new(std::env::var("RUSTFMT").unwrap_or_else(|_| "rustfmt".into()))
        .args(["--edition", "2024"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("跑不了 rustfmt：{e}（rustup component add rustfmt）"))?;
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(src.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

/// 生成三个文件的内容（还没排版）。
pub fn generate(spec: &Value, ops: &Value) -> Result<Vec<(String, String)>, String> {
    let mut g = Gen::new(spec)?;
    g.models()?;
    g.events()?;
    let resources = g.resources(ops)?;
    let types = g.finish_types();
    let module = format!(
        "{HEADER}//! 从 `docs/openapi.json` 生成的模型和接口。\n\npub mod resources;\npub mod types;\n"
    );
    Ok(vec![
        ("mod.rs".into(), module),
        ("types.rs".into(), types),
        ("resources.rs".into(), resources),
    ])
}

// ---------- 名字 ----------

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
];

fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in s.chars() {
        if !c.is_ascii_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if c.is_ascii_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        cur.push(c.to_ascii_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn pascal(s: &str) -> String {
    words(s)
        .iter()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn snake(s: &str) -> String {
    words(s).join("_")
}

fn ident(s: &str) -> String {
    let s = snake(s);
    let s = if s.is_empty() { "value".to_string() } else { s };
    let s = if s.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{s}")
    } else {
        s
    };
    if KEYWORDS.contains(&s.as_str()) {
        if matches!(s.as_str(), "self" | "Self" | "super" | "crate") {
            format!("{s}_")
        } else {
            format!("r#{s}")
        }
    } else {
        s
    }
}

/// 文档注释：每行前面加 `///`，空的不出。
fn doc(out: &mut String, indent: &str, text: Option<&str>) {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            let _ = writeln!(out, "{indent}///");
        } else {
            let _ = writeln!(out, "{indent}/// {line}");
        }
    }
}

fn rust_str(s: &str) -> String {
    format!("{s:?}")
}

// ---------- 生成 ----------

struct Gen<'a> {
    spec: &'a Value,
    schemas: &'a Map<String, Value>,
    /// 只在请求里出现的模型：可空可省的字段用三态的 Nullable
    request_only: HashSet<String>,
    /// 已经用掉的类型名
    names: HashSet<String>,
    /// types.rs 的正文（按出现的顺序）
    types: String,
}

fn refname(r: &str) -> &str {
    r.rsplit('/').next().unwrap_or(r)
}

/// 去掉 `{type: null}` 的分支 / `type: [x, "null"]` 里的 null：返回（剩下的 schema，是否可空）
fn strip_null(s: &Value) -> (Value, bool) {
    if let Some(one) = s.get("oneOf").and_then(Value::as_array) {
        let rest: Vec<&Value> = one
            .iter()
            .filter(|v| v.get("type").and_then(Value::as_str) != Some("null"))
            .collect();
        let nullable = rest.len() != one.len();
        if rest.len() == 1 {
            let (inner, n2) = strip_null(rest[0]);
            return (inner, nullable || n2);
        }
        let mut c = s.clone();
        c["oneOf"] = Value::Array(rest.into_iter().cloned().collect());
        return (c, nullable);
    }
    if let Some(ts) = s.get("type").and_then(Value::as_array) {
        let rest: Vec<&Value> = ts.iter().filter(|t| t.as_str() != Some("null")).collect();
        let nullable = rest.len() != ts.len();
        let mut c = s.clone();
        c["type"] = if rest.len() == 1 {
            rest[0].clone()
        } else {
            Value::Array(rest.into_iter().cloned().collect())
        };
        return (c, nullable);
    }
    (s.clone(), false)
}

fn collect_refs(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            if let Some(r) = m.get("$ref").and_then(Value::as_str) {
                out.insert(refname(r).to_string());
            }
            for x in m.values() {
                collect_refs(x, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_refs(x, out)),
        _ => {}
    }
}

fn integer_type(s: &Value) -> &'static str {
    let unsigned = s
        .get("minimum")
        .and_then(Value::as_f64)
        .is_some_and(|m| m >= 0.0);
    match (s.get("format").and_then(Value::as_str), unsigned) {
        (Some("int32"), true) => "u32",
        (Some("int32"), false) => "i32",
        (_, true) => "u64",
        (_, false) => "i64",
    }
}

impl<'a> Gen<'a> {
    fn new(spec: &'a Value) -> Result<Self, String> {
        let schemas = spec["components"]["schemas"]
            .as_object()
            .ok_or("openapi.json 没有 components.schemas")?;
        let g = Gen {
            spec,
            schemas,
            request_only: HashSet::new(),
            names: schemas.keys().cloned().collect(),
            types: String::new(),
        };
        let mut g = g;
        g.request_only = g.request_only_models();
        Ok(g)
    }

    /// 从请求体能走到、从响应（和事件）走不到的模型。
    fn request_only_models(&self) -> HashSet<String> {
        let mut req = BTreeSet::new();
        let mut res = BTreeSet::new();
        for item in self.spec["paths"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.values())
        {
            for op in item.as_object().into_iter().flat_map(|m| m.values()) {
                collect_refs(&op["requestBody"], &mut req);
                for (code, r) in op["responses"].as_object().into_iter().flatten() {
                    if code.starts_with('2') {
                        collect_refs(r, &mut res);
                    }
                }
            }
        }
        collect_refs(&self.spec["webhooks"], &mut res);
        let close = |set: &mut BTreeSet<String>| loop {
            let mut more = BTreeSet::new();
            for n in set.iter() {
                if let Some(s) = self.schemas.get(n) {
                    collect_refs(s, &mut more);
                }
            }
            let before = set.len();
            set.extend(more);
            if set.len() == before {
                break;
            }
        };
        close(&mut req);
        close(&mut res);
        req.difference(&res).cloned().collect()
    }

    fn fresh(&mut self, want: &str) -> String {
        let mut name = want.to_string();
        let mut i = 2;
        while self.names.contains(&name) {
            name = format!("{want}{i}");
            i += 1;
        }
        self.names.insert(name.clone());
        name
    }

    /// schema → Rust 类型；需要新类型（内联的对象、枚举、oneOf）时用 `ctx` 起名生成。返回（类型，是否可空）。
    fn ty(&mut self, schema: &Value, ctx: &str, request: bool) -> Result<(String, bool), String> {
        let (s, nullable) = strip_null(schema);
        if let Some(r) = s.get("$ref").and_then(Value::as_str) {
            return Ok((refname(r).to_string(), nullable));
        }
        if s.get("oneOf").is_some() || s.get("anyOf").is_some() {
            let name = self.fresh(ctx);
            self.one_of(&name, &s, request)?;
            return Ok((name, nullable));
        }
        if s.get("allOf").is_some() {
            let name = self.fresh(ctx);
            self.object(&name, &s, request)?;
            return Ok((name, nullable));
        }
        let t = match s.get("type") {
            Some(Value::String(t)) => t.as_str(),
            Some(Value::Array(_)) => return Ok(("serde_json::Value".into(), nullable)),
            _ => return Ok(("serde_json::Value".into(), nullable)),
        };
        let out = match t {
            "string" if s.get("enum").is_some() => {
                let name = self.fresh(ctx);
                self.string_enum(&name, &s);
                name
            }
            "string" => "String".into(),
            "integer" => integer_type(&s).into(),
            "number" => "f64".into(),
            "boolean" => "bool".into(),
            "array" => {
                let item = match s.get("items") {
                    Some(i) => self.ty(i, &format!("{ctx}Item"), request)?.0,
                    None => "serde_json::Value".into(),
                };
                format!("Vec<{item}>")
            }
            "object" => {
                let has_props = s
                    .get("properties")
                    .and_then(Value::as_object)
                    .is_some_and(|p| !p.is_empty());
                if has_props {
                    let name = self.fresh(ctx);
                    self.object(&name, &s, request)?;
                    name
                } else {
                    match s.get("additionalProperties") {
                        Some(Value::Object(ap)) if !ap.is_empty() => {
                            let v = self
                                .ty(&Value::Object(ap.clone()), &format!("{ctx}Value"), request)?
                                .0;
                            format!("std::collections::HashMap<String, {v}>")
                        }
                        Some(Value::Object(_)) | Some(Value::Bool(true)) => {
                            "std::collections::HashMap<String, serde_json::Value>".into()
                        }
                        _ => "serde_json::Value".into(),
                    }
                }
            }
            _ => "serde_json::Value".into(),
        };
        Ok((out, nullable))
    }

    /// 合并 allOf 的各部分：（properties，required）
    fn flatten(&self, s: &Value) -> (Map<String, Value>, BTreeSet<String>) {
        let mut props = Map::new();
        let mut req = BTreeSet::new();
        let add = |part: &Value, props: &mut Map<String, Value>, req: &mut BTreeSet<String>| {
            for (k, v) in part["properties"].as_object().into_iter().flatten() {
                props.insert(k.clone(), v.clone());
            }
            for r in part["required"].as_array().into_iter().flatten() {
                if let Some(r) = r.as_str() {
                    req.insert(r.to_string());
                }
            }
        };
        if let Some(parts) = s.get("allOf").and_then(Value::as_array) {
            for p in parts {
                let p = match p.get("$ref").and_then(Value::as_str) {
                    Some(r) => self.schemas.get(refname(r)).cloned().unwrap_or(Value::Null),
                    None => p.clone(),
                };
                let (pp, rr) = self.flatten(&p);
                for (k, v) in pp {
                    props.insert(k, v);
                }
                req.extend(rr);
            }
        }
        add(s, &mut props, &mut req);
        (props, req)
    }

    fn object(&mut self, name: &str, s: &Value, request: bool) -> Result<(), String> {
        let (props, required) = self.flatten(s);
        let mut fields = String::new();
        for (key, ps) in &props {
            let field = ident(key);
            let (ty, nullable) = self.ty(ps, &format!("{name}{}", pascal(key)), request)?;
            let is_req = required.contains(key);
            let desc = ps.get("description").and_then(Value::as_str).or_else(|| {
                ps.get("oneOf")
                    .and_then(Value::as_array)
                    .and_then(|a| a.iter().find_map(|x| x.get("description")?.as_str()))
            });
            doc(&mut fields, "    ", desc);
            let plain = field.trim_start_matches("r#");
            let mut attrs = Vec::new();
            if plain != key {
                attrs.push(format!("rename = {}", rust_str(key)));
            }
            let fty = if is_req && !nullable {
                ty
            } else if is_req {
                format!("Option<{ty}>")
            } else if request && nullable {
                attrs.push("skip_serializing_if = \"crate::Nullable::is_omitted\"".into());
                format!("crate::Nullable<{ty}>")
            } else {
                attrs.push("skip_serializing_if = \"Option::is_none\"".into());
                format!("Option<{ty}>")
            };
            if !attrs.is_empty() {
                let _ = writeln!(fields, "    #[serde({})]", attrs.join(", "));
            }
            let _ = writeln!(fields, "    pub {field}: {fty},");
        }
        let mut out = String::new();
        doc(&mut out, "", s.get("description").and_then(Value::as_str));
        out.push_str("#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]\n#[serde(default)]\n");
        let _ = writeln!(out, "pub struct {name} {{\n{fields}}}\n");
        self.types.push_str(&out);
        Ok(())
    }

    fn string_enum(&mut self, name: &str, s: &Value) {
        let values: Vec<&str> = s["enum"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut out = String::new();
        doc(&mut out, "", s.get("description").and_then(Value::as_str));
        out.push_str("///\n/// 服务器以后加了新的值时是 `Other`。\n");
        out.push_str("#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]\n#[non_exhaustive]\n");
        let _ = writeln!(out, "pub enum {name} {{");
        let variants: Vec<String> = values.iter().map(|v| pascal(v)).collect();
        for (i, (v, raw)) in variants.iter().zip(&values).enumerate() {
            let default = if i == 0 { "    #[default]\n" } else { "" };
            let _ = writeln!(out, "    /// `{raw}`\n{default}    {v},");
        }
        if variants.is_empty() {
            out.push_str("    #[default]\n    Unknown,\n");
        }
        out.push_str("    /// 这个版本的 SDK 还不认识的值\n    Other(String),\n}\n\n");
        let _ = writeln!(
            out,
            "impl {name} {{\n    /// 线上的写法\n    pub fn as_str(&self) -> &str {{\n        match self {{"
        );
        for (v, raw) in variants.iter().zip(&values) {
            let _ = writeln!(out, "            {name}::{v} => {},", rust_str(raw));
        }
        if variants.is_empty() {
            let _ = writeln!(out, "            {name}::Unknown => \"\",");
        }
        let _ = writeln!(
            out,
            "            {name}::Other(s) => s,\n        }}\n    }}\n}}\n"
        );
        let _ = writeln!(
            out,
            "impl From<&str> for {name} {{\n    fn from(s: &str) -> Self {{\n        match s {{"
        );
        for (v, raw) in variants.iter().zip(&values) {
            let _ = writeln!(out, "            {} => {name}::{v},", rust_str(raw));
        }
        let _ = writeln!(
            out,
            "            other => {name}::Other(other.to_string()),\n        }}\n    }}\n}}\n"
        );
        let _ = writeln!(
            out,
            "impl std::fmt::Display for {name} {{\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{\n        f.write_str(self.as_str())\n    }}\n}}\n"
        );
        let _ = writeln!(
            out,
            "impl Serialize for {name} {{\n    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {{\n        s.serialize_str(self.as_str())\n    }}\n}}\n"
        );
        let _ = writeln!(
            out,
            "impl<'de> Deserialize<'de> for {name} {{\n    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {{\n        let s = String::deserialize(d)?;\n        Ok({name}::from(s.as_str()))\n    }}\n}}\n"
        );
        self.types.push_str(&out);
    }

    fn one_of(&mut self, name: &str, s: &Value, request: bool) -> Result<(), String> {
        let alts = s
            .get("oneOf")
            .or_else(|| s.get("anyOf"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut variants: Vec<(String, String)> = Vec::new();
        for a in &alts {
            let vname = if let Some(r) = a.get("$ref").and_then(Value::as_str) {
                refname(r).to_string()
            } else {
                match a.get("type").and_then(Value::as_str) {
                    Some("string") => "String".into(),
                    Some("integer") => "Integer".into(),
                    Some("number") => "Number".into(),
                    Some("boolean") => "Bool".into(),
                    Some("array") => "List".into(),
                    Some("object") => "Object".into(),
                    _ => "Value".into(),
                }
            };
            let mut vname = vname;
            while variants.iter().any(|(n, _)| *n == vname) {
                vname.push('2');
            }
            let (ty, _) = self.ty(a, &format!("{name}{vname}"), request)?;
            variants.push((vname, ty));
        }
        let mut out = String::new();
        doc(&mut out, "", s.get("description").and_then(Value::as_str));
        out.push_str(
            "#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]\n#[serde(untagged)]\n",
        );
        let _ = writeln!(out, "pub enum {name} {{");
        for (v, t) in &variants {
            let _ = writeln!(out, "    {v}({t}),");
        }
        out.push_str("}\n\n");
        if let Some((v, _)) = variants.first() {
            let _ = writeln!(
                out,
                "impl Default for {name} {{\n    fn default() -> Self {{\n        {name}::{v}(Default::default())\n    }}\n}}\n"
            );
        }
        for (v, t) in &variants {
            // 同一个类型只出现一次时才能有 From
            if variants.iter().filter(|(_, t2)| t2 == t).count() == 1 {
                let _ = writeln!(
                    out,
                    "impl From<{t}> for {name} {{\n    fn from(v: {t}) -> Self {{\n        {name}::{v}(v)\n    }}\n}}\n"
                );
            }
        }
        self.types.push_str(&out);
        Ok(())
    }

    /// components.schemas 里的每一个
    fn models(&mut self) -> Result<(), String> {
        let schemas = self.schemas.clone();
        for (name, s) in &schemas {
            let request = self.request_only.contains(name);
            let (inner, nullable) = strip_null(s);
            let is_object = inner.get("allOf").is_some()
                || inner
                    .get("properties")
                    .and_then(Value::as_object)
                    .is_some_and(|p| !p.is_empty());
            if is_object {
                self.object(name, &inner, request)?;
            } else if inner.get("enum").is_some()
                && inner.get("type").and_then(Value::as_str) == Some("string")
            {
                self.string_enum(name, &inner);
            } else if inner.get("oneOf").is_some() || inner.get("anyOf").is_some() {
                self.one_of(name, &inner, request)?;
            } else {
                let (ty, _) = self.ty(&inner, &format!("{name}Inner"), request)?;
                let ty = if nullable {
                    format!("Option<{ty}>")
                } else {
                    ty
                };
                let mut out = String::new();
                doc(&mut out, "", s.get("description").and_then(Value::as_str));
                let _ = writeln!(out, "pub type {name} = {ty};\n");
                self.types.push_str(&out);
            }
        }
        Ok(())
    }

    /// 工作区事件：openapi 的 webhooks（事件名 → 模型）
    fn events(&mut self) -> Result<(), String> {
        let hooks = self.spec["webhooks"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let mut variants = Vec::new();
        for (ev, item) in &hooks {
            // agent.* 是发给 Agent 的 webhook（另有 verify_webhook），不在工作区事件流里
            if ev.starts_with("agent.") {
                continue;
            }
            let Some(post) = item.get("post") else {
                continue;
            };
            let schema = &post["requestBody"]["content"]["application/json"]["schema"];
            let v = pascal(ev);
            let (ty, _) = self.ty(schema, &format!("{v}Event"), false)?;
            let summary = post.get("summary").and_then(Value::as_str).unwrap_or("");
            variants.push((ev.clone(), v, ty, summary.to_string()));
        }
        let mut out = String::new();
        out.push_str(
            "/// 工作区事件流（`client.workspaces().events(ws)`）里一条事件的数据，按事件名解析好。\n\
             ///\n/// 不认识的事件名、或者数据和模型对不上时是 `Other`（原始 JSON）。\n\
             #[derive(Clone, Debug, PartialEq)]\n#[non_exhaustive]\npub enum WorkspaceEvent {\n",
        );
        for (ev, v, ty, summary) in &variants {
            let _ = writeln!(out, "    /// `{ev}`：{summary}\n    {v}({ty}),");
        }
        out.push_str("    /// 别的事件（原始 JSON）\n    Other(serde_json::Value),\n}\n\n");
        out.push_str("impl WorkspaceEvent {\n    /// 按事件名解析 `data`。\n    pub fn parse(name: &str, data: serde_json::Value) -> WorkspaceEvent {\n        fn conv<T: serde::de::DeserializeOwned>(v: &serde_json::Value) -> Option<T> {\n            T::deserialize(v).ok()\n        }\n        let parsed = match name {\n");
        for (ev, v, _, _) in &variants {
            let _ = writeln!(
                out,
                "            {} => conv(&data).map(WorkspaceEvent::{v}),",
                rust_str(ev)
            );
        }
        out.push_str("            _ => None,\n        };\n        parsed.unwrap_or(WorkspaceEvent::Other(data))\n    }\n}\n\n");
        self.types.push_str(&out);
        Ok(())
    }

    fn finish_types(&mut self) -> String {
        format!(
            "{HEADER}//! 模型：`docs/openapi.json` 的 `components.schemas`。\n//!\n\
             //! 只在请求里用的模型，可空可省的字段是 [`Nullable`](crate::Nullable)（不发 / 发 null / 发值）；\n\
             //! 其余的可省字段是 `Option`（`None` 不发）。\n\
             #![allow(rustdoc::broken_intra_doc_links, rustdoc::invalid_html_tags, rustdoc::bare_urls, clippy::large_enum_variant, clippy::doc_lazy_continuation, clippy::doc_overindented_list_items)]\n\n\
             use serde::{{Deserialize, Serialize}};\n\n{}",
            self.types
        )
    }

    fn resources(&mut self, table: &Value) -> Result<String, String> {
        let entries = table["operations"]
            .as_object()
            .ok_or("operations.json 没有 operations")?;
        // operationId → (path, method, op)
        let mut by_id = BTreeMap::new();
        for (path, item) in self.spec["paths"].as_object().into_iter().flatten() {
            for (method, op) in item.as_object().into_iter().flatten() {
                if let Some(id) = op.get("operationId").and_then(Value::as_str) {
                    by_id.insert(id.to_string(), (path.clone(), method.clone(), op.clone()));
                }
            }
        }
        // 资源树：资源路径（["pages","versions"]）→ 方法
        let mut tree: BTreeMap<Vec<String>, Vec<Op>> = BTreeMap::new();
        for (id, e) in entries {
            let Some((path, method, op)) = by_id.get(id) else {
                return Err(format!("operations.json 的 {id} 在 openapi.json 里没有"));
            };
            let name = e["name"].as_str().ok_or(format!("{id} 没有 name"))?;
            let segs: Vec<String> = name.split('.').map(String::from).collect();
            let (res, m) = segs.split_at(segs.len() - 1);
            tree.entry(res.to_vec()).or_default().push(Op {
                id: id.clone(),
                name: name.to_string(),
                method_name: m[0].clone(),
                http: method.to_uppercase(),
                path: path.clone(),
                op: op.clone(),
                hints: e.clone(),
            });
        }
        // 父资源都要有（pages.versions → pages）
        let keys: Vec<Vec<String>> = tree.keys().cloned().collect();
        for k in keys {
            for i in 0..k.len() {
                tree.entry(k[..i].to_vec()).or_default();
            }
        }

        let mut out = String::new();
        out.push_str(HEADER);
        out.push_str(
            "//! 资源和接口：`client.pages().versions().list(id)`。每个接口一个请求构造器：\n\
             //! 位置参数是路径参数（和必填的查询参数）、请求体；可选的查询参数用链式方法设；`.await` 发出去。\n\
             #![allow(rustdoc::broken_intra_doc_links, rustdoc::invalid_html_tags, rustdoc::bare_urls, clippy::doc_lazy_continuation)]\n\n\
             use crate::Client;\nuse crate::error::Result;\nuse crate::request::{Spec, enc};\nuse crate::types::*;\n\
             use futures_util::future::BoxFuture;\nuse std::future::IntoFuture;\n\n",
        );
        for (res, ops) in &tree {
            let api = if res.is_empty() {
                "Client".to_string()
            } else {
                format!("{}Api", res.iter().map(|s| pascal(s)).collect::<String>())
            };
            if !res.is_empty() {
                let dotted = res.join(".");
                let _ = writeln!(
                    out,
                    "/// `{dotted}.*` 的接口。\n#[derive(Clone, Debug)]\npub struct {api} {{\n    pub(crate) client: Client,\n}}\n"
                );
            }
            let mut methods = String::new();
            // 子资源
            for child in tree
                .keys()
                .filter(|k| k.len() == res.len() + 1 && k.starts_with(res))
            {
                let cname = format!("{}Api", child.iter().map(|s| pascal(s)).collect::<String>());
                let field = ident(child.last().expect("子资源"));
                let me = if res.is_empty() {
                    "self.clone()"
                } else {
                    "self.client.clone()"
                };
                let _ = writeln!(
                    methods,
                    "    /// `{}.*` 的接口\n    pub fn {field}(&self) -> {cname} {{\n        {cname} {{ client: {me} }}\n    }}\n",
                    child.join(".")
                );
            }
            for op in ops {
                let (m, defs) = self.operation(op, res.is_empty())?;
                methods.push_str(&m);
                out.push_str(&defs);
            }
            let _ = writeln!(out, "impl {api} {{\n{methods}}}\n");
        }
        Ok(out)
    }

    /// 一个接口：资源上的方法 + 请求构造器的定义
    fn operation(&mut self, o: &Op, on_client: bool) -> Result<(String, String), String> {
        let req = format!("{}Request", pascal(&o.name.replace('.', "_")));
        let params: Vec<Value> = o.op["parameters"].as_array().cloned().unwrap_or_default();
        let hints = &o.hints;
        let sse = hints.get("sse").and_then(Value::as_str);
        let upload = hints.get("upload").and_then(Value::as_str);
        let download = hints
            .get("download")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let unwrap = hints.get("unwrap").and_then(Value::as_str);
        let paginate = hints.get("paginate").and_then(Value::as_str);
        let null_on_404 = hints
            .get("null_on_404")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // 路径参数：按路径里出现的顺序
        let mut path_params = Vec::new();
        let mut rest = o.path.as_str();
        while let Some(i) = rest.find('{') {
            let j = rest[i..].find('}').ok_or("路径的 { 没有配对")? + i;
            path_params.push(rest[i + 1..j].to_string());
            rest = &rest[j + 1..];
        }
        let find = |name: &str, loc: &str| {
            params
                .iter()
                .find(|p| p["name"] == name && p["in"] == loc)
                .cloned()
                .unwrap_or(Value::Null)
        };

        // 构造器的字段：（字段名，类型，参数）
        struct Field {
            name: String,
            ty: String,
            /// 设的时候用 impl Into<String>
            string: bool,
            wire: String,
            loc: &'static str,
            positional: bool,
            desc: Option<String>,
        }
        let mut fields: Vec<Field> = Vec::new();
        for pp in &path_params {
            let p = find(pp, "path");
            let (ty, _) = strip_null(&p["schema"]);
            let t = match ty.get("type").and_then(Value::as_str) {
                Some("integer") => integer_type(&ty).to_string(),
                _ => "String".to_string(),
            };
            fields.push(Field {
                name: ident(pp),
                string: t == "String",
                ty: t,
                wire: pp.clone(),
                loc: "path",
                positional: true,
                desc: p
                    .get("description")
                    .and_then(Value::as_str)
                    .map(String::from),
            });
        }
        for p in &params {
            let loc = match p["in"].as_str() {
                Some("query") => "query",
                Some("header") => "header",
                _ => continue,
            };
            let wire = p["name"].as_str().unwrap_or_default().to_string();
            if loc == "header"
                && (sse.is_some() && wire.eq_ignore_ascii_case("last-event-id")
                    || upload.is_some() && wire.eq_ignore_ascii_case("x-filename"))
            {
                continue;
            }
            let (s, _) = strip_null(&p["schema"]);
            let t = match s.get("type").and_then(Value::as_str) {
                Some("integer") => integer_type(&s).to_string(),
                Some("number") => "f64".to_string(),
                Some("boolean") => "bool".to_string(),
                _ => "String".to_string(),
            };
            fields.push(Field {
                name: ident(&wire),
                string: t == "String",
                ty: t,
                wire,
                loc,
                positional: p["required"].as_bool().unwrap_or(false),
                desc: p
                    .get("description")
                    .and_then(Value::as_str)
                    .map(String::from),
            });
        }

        // 请求体
        let mut body_ty: Option<String> = None;
        if upload.is_none() {
            if let Some(content) = o.op["requestBody"]["content"].as_object() {
                if let Some(j) = content.get("application/json") {
                    let schema = j.get("schema").cloned().unwrap_or(Value::Null);
                    let (t, _) = self.ty(
                        &schema,
                        &format!("{}Body", pascal(&o.name.replace('.', "_"))),
                        true,
                    )?;
                    body_ty = Some(t);
                }
            }
        }

        // 成功的响应
        let responses = o.op["responses"].as_object().cloned().unwrap_or_default();
        let ok: Vec<(&String, &Value)> = responses
            .iter()
            .filter(|(k, _)| k.starts_with('2'))
            .collect();
        let json_schema = ok
            .iter()
            .find_map(|(_, r)| r["content"]["application/json"].get("schema").cloned());
        let resp_model = json_schema
            .as_ref()
            .and_then(|s| s.get("$ref"))
            .and_then(Value::as_str)
            .map(|r| refname(r).to_string());

        // 返回类型和怎么发
        enum Kind {
            Empty,
            Bytes,
            Json(String),
            Optional(String),
            Unwrap {
                model: String,
                field: String,
                ty: String,
            },
            Events(String),
        }
        let kind = if let Some(s) = sse {
            Kind::Events(match s {
                "workspace" => "WorkspaceEvent".into(),
                _ => ok
                    .iter()
                    .find_map(|(_, r)| r["content"]["text/event-stream"]["schema"]["$ref"].as_str())
                    .map(|r| refname(r).to_string())
                    .ok_or(format!("{}：sse 的响应没有模型", o.id))?,
            })
        } else if download {
            Kind::Bytes
        } else if let Some(f) = unwrap {
            let model = resp_model
                .clone()
                .ok_or(format!("{}：unwrap 的响应没有模型", o.id))?;
            let ms = self.schemas.get(&model).cloned().unwrap_or(Value::Null);
            let (props, _) = self.flatten(&ms);
            let fs = props.get(f).ok_or(format!("{}：响应里没有 {f}", o.id))?;
            let (ty, _) = self.ty(fs, &format!("{model}{}", pascal(f)), false)?;
            Kind::Unwrap {
                model,
                field: ident(f),
                ty,
            }
        } else if let Some(s) = &json_schema {
            let (t, _) = self.ty(
                s,
                &format!("{}Response", pascal(&o.name.replace('.', "_"))),
                false,
            )?;
            if null_on_404 {
                Kind::Optional(t)
            } else {
                Kind::Json(t)
            }
        } else if ok.iter().all(|(_, r)| r.get("content").is_none()) {
            Kind::Empty
        } else {
            Kind::Bytes
        };
        let output = match &kind {
            Kind::Empty => "()".to_string(),
            Kind::Bytes => "bytes::Bytes".to_string(),
            Kind::Json(t) => t.clone(),
            Kind::Optional(t) => format!("Option<{t}>"),
            Kind::Unwrap { ty, .. } => ty.clone(),
            Kind::Events(t) => format!("crate::events::EventStream<{t}>"),
        };

        // ---- 文档 ----
        let mut opdoc = String::new();
        if let Some(d) = o.op.get("description").and_then(Value::as_str) {
            opdoc.push_str(d.trim());
            opdoc.push_str("\n\n");
        }
        let _ = write!(opdoc, "`{} /api/v1{}`", o.http, o.path);
        match &kind {
            Kind::Unwrap { field, .. } => {
                let _ = write!(opdoc, "；返回响应里的 `{}`", field.trim_start_matches("r#"));
            }
            Kind::Optional(_) => opdoc.push_str("；404 时返回 `None`"),
            Kind::Events(_) => {
                opdoc.push_str("；`.await` 连上以后得到事件流，断了自动重连（带 `Last-Event-ID`）")
            }
            _ => {}
        }
        if paginate.is_some() {
            opdoc.push_str("。一页；逐条取完所有页用 `list_all` 或 `.into_stream()`");
        }

        // ---- 构造器 ----
        let mut defs = String::new();
        doc(
            &mut defs,
            "",
            Some(&format!("`{}` 的请求：{}", o.name, opdoc)),
        );
        let _ = writeln!(
            defs,
            "#[derive(Clone, Debug)]\n#[must_use = \"请求要 .await 才会发出去\"]\npub struct {req} {{\n    client: Client,"
        );
        for f in &fields {
            if f.positional {
                let _ = writeln!(defs, "    {}: {},", f.name, f.ty);
            } else {
                let _ = writeln!(defs, "    {}: Option<{}>,", f.name, f.ty);
            }
        }
        if let Some(b) = &body_ty {
            let _ = writeln!(defs, "    body: {b},");
        }
        match upload {
            Some("file") => defs.push_str("    filename: String,\n    data: bytes::Bytes,\n"),
            Some(_) => defs.push_str("    data: bytes::Bytes,\n"),
            None => {}
        }
        defs.push_str("}\n\n");

        // 设可选参数
        let mut setters = String::new();
        for f in fields.iter().filter(|f| !f.positional) {
            let desc = f.desc.clone().unwrap_or_else(|| format!("`{}`", f.wire));
            doc(
                &mut setters,
                "    ",
                Some(&format!(
                    "{desc}（{}参数 `{}`）",
                    if f.loc == "query" {
                        "查询"
                    } else {
                        "请求头"
                    },
                    f.wire
                )),
            );
            if f.string {
                let _ = writeln!(
                    setters,
                    "    pub fn {n}(mut self, v: impl Into<String>) -> Self {{\n        self.{n} = Some(v.into());\n        self\n    }}\n",
                    n = f.name
                );
            } else {
                let _ = writeln!(
                    setters,
                    "    pub fn {n}(mut self, v: {t}) -> Self {{\n        self.{n} = Some(v);\n        self\n    }}\n",
                    n = f.name,
                    t = f.ty
                );
            }
        }

        // 拼请求
        let mut path_fmt = String::new();
        let mut path_args = Vec::new();
        let mut rest = o.path.as_str();
        while let Some(i) = rest.find('{') {
            let j = rest[i..].find('}').expect("配对") + i;
            path_fmt.push_str(&rest[..i]);
            path_fmt.push_str("{}");
            let f = fields
                .iter()
                .find(|f| f.loc == "path" && f.wire == rest[i + 1..j])
                .expect("路径参数");
            path_args.push(if f.string {
                format!("enc(&self.{})", f.name)
            } else {
                format!("self.{}", f.name)
            });
            rest = &rest[j + 1..];
        }
        path_fmt.push_str(rest);
        let path_expr = if path_args.is_empty() {
            format!("{}.to_string()", rust_str(&path_fmt))
        } else {
            format!("format!({}, {})", rust_str(&path_fmt), path_args.join(", "))
        };
        let mut spec_fn = String::new();
        let resume = sse.is_some();
        let _ = writeln!(
            spec_fn,
            "    fn spec(&self{}) -> Result<Spec> {{\n        #[allow(unused_mut)]\n        let mut spec = Spec::new(reqwest::Method::{}, {path_expr});",
            if resume {
                ", last_event_id: Option<&str>"
            } else {
                ""
            },
            o.http
        );
        if resume {
            spec_fn.push_str("        if let Some(id) = last_event_id {\n            spec.header(\"last-event-id\", id);\n        }\n");
        }
        for f in fields.iter().filter(|f| f.loc != "path") {
            let call = if f.loc == "query" { "query" } else { "header" };
            // 事件流重连时 after / cursor 换成最后收到的编号
            let resumable = resume && (f.wire == "after" || f.wire == "cursor");
            let value = if f.positional {
                format!("self.{}.to_string()", f.name)
            } else {
                String::new()
            };
            if f.positional {
                let _ = writeln!(
                    spec_fn,
                    "        spec.{call}({}, {value});",
                    rust_str(&f.wire)
                );
            } else if resumable && f.wire == "after" {
                let _ = writeln!(
                    spec_fn,
                    "        match last_event_id {{\n            Some(id) => spec.query(\"after\", id.to_string()),\n            None => {{\n                if let Some(v) = &self.{n} {{\n                    spec.query(\"after\", v.to_string());\n                }}\n            }}\n        }}",
                    n = f.name
                );
            } else if resumable {
                let _ = writeln!(
                    spec_fn,
                    "        if let (None, Some(v)) = (last_event_id, &self.{n}) {{\n            spec.{call}({w}, v.to_string());\n        }}",
                    n = f.name,
                    w = rust_str(&f.wire)
                );
            } else {
                let _ = writeln!(
                    spec_fn,
                    "        if let Some(v) = &self.{n} {{\n            spec.{call}({w}, v.to_string());\n        }}",
                    n = f.name,
                    w = rust_str(&f.wire)
                );
            }
        }
        if body_ty.is_some() {
            spec_fn.push_str("        spec.json(&self.body)?;\n");
        }
        match upload {
            Some("file") => spec_fn.push_str(
                "        spec.header(\"x-filename\", enc(&self.filename));\n        spec.raw(\"application/octet-stream\", self.data.clone());\n",
            ),
            Some(_) => spec_fn.push_str("        spec.raw(\"text/csv\", self.data.clone());\n"),
            None => {}
        }
        spec_fn.push_str("        Ok(spec)\n    }\n");

        let send = match &kind {
            Kind::Empty => "client.send_empty(spec?).await".to_string(),
            Kind::Bytes => "client.send_bytes(spec?).await".to_string(),
            Kind::Json(_) => "client.send_json(spec?).await".to_string(),
            Kind::Optional(_) => "client.send_optional(spec?).await".to_string(),
            Kind::Unwrap { model, field, .. } => {
                format!(
                    "let r: {model} = client.send_json(spec?).await?;\n            Ok(r.{field})"
                )
            }
            Kind::Events(_) => String::new(),
        };
        let mut into_stream = String::new();
        if let Some(pf) = paginate {
            let model = resp_model
                .clone()
                .ok_or(format!("{}：paginate 的响应没有模型", o.id))?;
            let ms = self.schemas.get(&model).cloned().unwrap_or(Value::Null);
            let (props, _) = self.flatten(&ms);
            let fs = props.get(pf).ok_or(format!("{}：响应里没有 {pf}", o.id))?;
            let (ty, _) = self.ty(fs, &format!("{model}{}", pascal(pf)), false)?;
            let item = ty
                .strip_prefix("Vec<")
                .and_then(|t| t.strip_suffix('>'))
                .ok_or(format!("{}：{pf} 不是数组", o.id))?
                .to_string();
            let cursor = fields
                .iter()
                .find(|f| f.wire == "cursor")
                .ok_or(format!("{}：没有 cursor 参数", o.id))?;
            into_stream = "    /// 逐条取完所有页（自动带 `cursor`）。\n    pub fn into_stream(self) -> crate::ListAll<Self> {\n        crate::ListAll::new(self)\n    }\n\n".to_string();
            let _ = writeln!(
                defs,
                "impl crate::Paginated for {req} {{\n    type Item = {item};\n\n    fn cursor(&self) -> Option<String> {{\n        self.{c}.clone()\n    }}\n\n    fn with_cursor(&self, cursor: Option<String>) -> Self {{\n        let mut r = self.clone();\n        r.{c} = cursor;\n        r\n    }}\n\n    fn fetch(self) -> crate::pagination::PageFuture<{item}> {{\n        Box::pin(async move {{\n            let p: {model} = self.await?;\n            Ok((p.{pfi}, p.next_cursor))\n        }})\n    }}\n}}\n",
                c = cursor.name,
                pfi = ident(pf)
            );
            // list_all 返回的 ListAll 上也能设参数
            let mut all_setters = String::new();
            for f in fields.iter().filter(|f| !f.positional) {
                let desc = f.desc.clone().unwrap_or_else(|| format!("`{}`", f.wire));
                doc(&mut all_setters, "    ", Some(&desc));
                let (arg, val) = if f.string {
                    ("impl Into<String>".to_string(), "v.into()")
                } else {
                    (f.ty.clone(), "v")
                };
                let _ = writeln!(
                    all_setters,
                    "    pub fn {n}(self, v: {arg}) -> Self {{\n        self.map_request(|r| r.{n}({val}))\n    }}\n",
                    n = f.name
                );
            }
            let _ = writeln!(defs, "impl crate::ListAll<{req}> {{\n{all_setters}}}\n");
        }

        let _ = writeln!(defs, "impl {req} {{\n{setters}{into_stream}{spec_fn}}}\n");
        if let Kind::Events(t) = &kind {
            let _ = writeln!(
                defs,
                "impl IntoFuture for {req} {{\n    type Output = Result<{output}>;\n    type IntoFuture = BoxFuture<'static, Self::Output>;\n\n    fn into_future(self) -> Self::IntoFuture {{\n        Box::pin(async move {{\n            let client = self.client.clone();\n            client\n                .open_events::<{t}>(std::sync::Arc::new(move |last: Option<&str>| self.spec(last)))\n                .await\n        }})\n    }}\n}}\n"
            );
        } else {
            let _ = writeln!(
                defs,
                "impl IntoFuture for {req} {{\n    type Output = Result<{output}>;\n    type IntoFuture = BoxFuture<'static, Self::Output>;\n\n    fn into_future(self) -> Self::IntoFuture {{\n        let spec = self.spec();\n        let client = self.client;\n        Box::pin(async move {{\n            {send}\n        }})\n    }}\n}}\n"
            );
        }

        // ---- 资源上的方法 ----
        let mut args = Vec::new();
        let mut inits = Vec::new();
        for f in &fields {
            if f.positional {
                if f.string {
                    args.push(format!("{}: impl Into<String>", f.name));
                    inits.push(format!("{n}: {n}.into()", n = f.name));
                } else {
                    args.push(format!("{}: {}", f.name, f.ty));
                    inits.push(f.name.clone());
                }
            } else {
                inits.push(format!("{}: None", f.name));
            }
        }
        if let Some(b) = &body_ty {
            args.push(format!("body: {b}"));
            inits.push("body".into());
        }
        match upload {
            Some("file") => {
                args.push("filename: impl Into<String>".into());
                args.push("data: impl Into<bytes::Bytes>".into());
                inits.push("filename: filename.into()".into());
                inits.push("data: data.into()".into());
            }
            Some(_) => {
                args.push("data: impl Into<bytes::Bytes>".into());
                inits.push("data: data.into()".into());
            }
            None => {}
        }
        let me = if on_client {
            "self.clone()"
        } else {
            "self.client.clone()"
        };
        let mut m = String::new();
        doc(&mut m, "    ", Some(&opdoc));
        let _ = writeln!(
            m,
            "    pub fn {name}(&self{sep}{args}) -> {req} {{\n        {req} {{\n            client: {me},\n            {inits}\n        }}\n    }}\n",
            name = ident(&o.method_name),
            sep = if args.is_empty() { "" } else { ", " },
            args = args.join(", "),
            inits = inits.join(",\n            "),
        );
        if paginate.is_some() {
            let call_args: Vec<String> = fields
                .iter()
                .filter(|f| f.positional)
                .map(|f| f.name.clone())
                .chain(body_ty.iter().map(|_| "body".to_string()))
                .collect();
            let all = format!("{}_all", o.method_name);
            let _ = writeln!(
                m,
                "    /// 和 [`{name}`](Self::{name}) 一样，但逐条取完所有页：返回 `Stream<Item = Result<条目>>`，停止条件是没有下一页、这页是空的或游标没变。\n    pub fn {all}(&self{sep}{args}) -> crate::ListAll<{req}> {{\n        self.{name}({cargs}).into_stream()\n    }}\n",
                name = ident(&o.method_name),
                all = ident(&all),
                sep = if args.is_empty() { "" } else { ", " },
                args = args.join(", "),
                cargs = call_args.join(", "),
            );
        }
        Ok((m, defs))
    }
}

struct Op {
    id: String,
    name: String,
    method_name: String,
    http: String,
    path: String,
    op: Value,
    hints: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(pascal("pages.versions_list"), "PagesVersionsList");
        assert_eq!(pascal("page.created"), "PageCreated");
        assert_eq!(snake("Last-Event-ID"), "last_event_id");
        assert_eq!(ident("type"), "r#type");
        assert_eq!(ident("self"), "self_");
        assert_eq!(pascal("multi_select"), "MultiSelect");
    }
}
