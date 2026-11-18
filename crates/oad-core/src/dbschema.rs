//! 改表结构（加列、改名、改类型、删列、选项、公式、配置、挪位置）和撤销。
//! 一批改动要么都做、要么都不做；记一条 db_schema_ops（前后的表结构、视图、因此改了的格子），撤销时放回去。
use crate::db::short_id;
use crate::dbengine::{
    self as eng, Cached, fill_options, fresh_key, parse_schema, person_ref, to_value,
};
use crate::model::*;
use crate::{Core, Error, Result};
use oad_db::schema::{Prop, PropType, RelationConf, RollupConf, Schema, SelectOption};
use oad_db::{Agg, Change};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};

/// 一个属性最多几个，名字最长几个字。
pub const MAX_PROPS: usize = 200;
const NAME_LEN: usize = 100;

fn bad(msg: impl Into<String>) -> Error {
    Error::BadRequest(msg.into())
}

/// 这批改动顺带改了的格子：(行, key) → (原来的, 现在的)。
#[derive(Default)]
struct Cells(BTreeMap<(String, String), (Option<Value>, Option<Value>)>);

impl Cells {
    fn set(&mut self, eng: &mut oad_db::Engine, row: &str, key: &str, new: Option<Value>) {
        let cur = eng.row(row).and_then(|r| r.props.get(key).cloned());
        if cur == new {
            return;
        }
        let e = self
            .0
            .entry((row.into(), key.into()))
            .or_insert((cur, None));
        e.1 = new.clone();
        // 引擎里也改掉，同一批后面的改动看到的是改过的
        let mut m = Map::new();
        m.insert(key.into(), new.unwrap_or(Value::Null));
        eng.apply(vec![Change {
            row_id: row.into(),
            props: Some(m),
            ..Default::default()
        }]);
    }

    fn json(&self, new: bool) -> Value {
        Value::Array(
            self.0
                .iter()
                .filter(|(_, (a, b))| a != b)
                .map(|((r, k), (a, b))| json!([r, k, if new { b } else { a }]))
                .collect(),
        )
    }

    fn writes(&self) -> Vec<CellSet> {
        self.0
            .iter()
            .filter(|(_, (a, b))| a != b)
            .map(|((r, k), (_, b))| CellSet {
                row_id: r.clone(),
                key: k.clone(),
                value: b.clone(),
                only_if: None,
            })
            .collect()
    }
}

fn type_of(v: &Value) -> Result<PropType> {
    let s = v.as_str().ok_or_else(|| bad("type 要是字符串"))?;
    match PropType::from_name(s) {
        Some(t) => Ok(t),
        None => {
            let all: Vec<&str> = PropType::ALL.iter().map(|t| t.key()).collect();
            Err(bad(format!(
                "不认识的类型「{s}」，能用的：{}",
                all.join("、")
            )))
        }
    }
}

fn clean_name(v: &Value) -> Result<String> {
    let s = v.as_str().ok_or_else(|| bad("名字要是字符串"))?;
    let n: String = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(NAME_LEN)
        .collect();
    if n.is_empty() {
        return Err(bad("属性要有名字"));
    }
    Ok(n)
}

fn check_unique_name(s: &Schema, name: &str, except: Option<&str>) -> Result<()> {
    if s.props
        .iter()
        .any(|p| Some(p.key.as_str()) != except && p.name.eq_ignore_ascii_case(name))
    {
        return Err(bad(format!("已经有叫「{name}」的属性了")));
    }
    Ok(())
}

fn find_idx(s: &Schema, r: &Value) -> Result<usize> {
    let r = r
        .as_str()
        .ok_or_else(|| bad("要给 prop（属性的 key 或名字）"))?;
    let r = r.trim();
    s.props
        .iter()
        .position(|p| p.key == r)
        .or_else(|| s.props.iter().position(|p| p.name.eq_ignore_ascii_case(r)))
        .ok_or_else(|| {
            let all: Vec<&str> = s.props.iter().map(|p| p.name.as_str()).collect();
            bad(format!("没有属性「{r}」，现在有：{}", all.join("、")))
        })
}

/// 给的选项（名字，或者 `{ id?, name, color?, group? }`）→ 选项；按 id、名字沿用原来的。
fn options_of(v: &Value, old: &[SelectOption]) -> Result<Vec<SelectOption>> {
    let arr = v.as_array().ok_or_else(|| bad("options 要是数组"))?;
    let mut out: Vec<SelectOption> = Vec::with_capacity(arr.len());
    for x in arr {
        let (id, name, color, group) = match x {
            Value::String(s) => (None, s.clone(), None, None),
            Value::Object(o) => (
                o.get("id").and_then(Value::as_str).map(str::to_string),
                o.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                o.get("color").and_then(Value::as_str).map(str::to_string),
                o.get("group").and_then(Value::as_str).map(str::to_string),
            ),
            _ => return Err(bad("选项要是名字或 { name, color? }")),
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(bad("选项要有名字"));
        }
        if out.iter().any(|o| o.name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let prev = old
            .iter()
            .find(|o| id.as_deref() == Some(o.id.as_str()))
            .or_else(|| old.iter().find(|o| o.name.eq_ignore_ascii_case(&name)));
        let mut o = prev.cloned().unwrap_or_default();
        if o.id.is_empty() {
            o.id = id.filter(|i| !i.is_empty()).unwrap_or_else(|| short_id(6));
        }
        o.name = name;
        if let Some(c) = color {
            o.color = c;
        }
        if group.is_some() {
            o.group = group;
        }
        out.push(o);
    }
    Ok(out)
}

/// 各种配置：format、precision、date_format、prefix、formula、relation、rollup、按钮的设置。
fn apply_config(
    p: &mut Prop,
    op: &Map<String, Value>,
    s: &Schema,
    db_id: &str,
    others: &Others,
) -> Result<()> {
    if let Some(f) = op.get("format") {
        p.format = f.as_str().map(str::to_string);
    }
    if let Some(x) = op.get("precision") {
        p.precision = x.as_u64().map(|n| n.min(10) as u8);
    }
    if let Some(x) = op.get("date_format") {
        p.date_format = x.as_str().map(str::to_string);
    }
    if let Some(x) = op.get("prefix") {
        p.prefix = x.as_str().map(str::to_string).filter(|s| !s.is_empty());
    }
    if let Some(x) = op.get("formula") {
        p.formula = Some(
            x.as_str()
                .ok_or_else(|| bad("formula 要是字符串"))?
                .to_string(),
        );
    }
    if p.kind == PropType::Relation {
        let db = op
            .get("relation")
            .and_then(|r| {
                r.as_str()
                    .map(str::to_string)
                    .or_else(|| r["db_id"].as_str().map(str::to_string))
            })
            .or_else(|| op.get("db_id").and_then(Value::as_str).map(str::to_string));
        if let Some(db) = db {
            let db = if db == "self" || db.is_empty() {
                db_id.to_string()
            } else {
                db
            };
            let mut r = p.relation.take().unwrap_or_default();
            r.db_id = db;
            p.relation = Some(r);
        }
        if p.relation.is_none() {
            p.relation = Some(RelationConf {
                db_id: db_id.into(),
                ..Default::default()
            });
        }
    }
    if p.kind == PropType::Rollup {
        let given = op.get("rollup").cloned().unwrap_or(Value::Null);
        let mut r: RollupConf = p.rollup.take().unwrap_or_default();
        let pick = |k: &str| {
            given
                .get(k)
                .or_else(|| op.get(k))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        if let Some(rel) = pick("relation") {
            let i = find_idx(s, &Value::String(rel))?;
            if s.props[i].kind != PropType::Relation {
                return Err(bad(format!("「{}」不是关联", s.props[i].name)));
            }
            r.relation = s.props[i].key.clone();
        }
        if let Some(t) = pick("prop").or_else(|| pick("target")) {
            // 对方库的属性：名字换成 key（引擎按 key 找）
            let target_db = s
                .prop(&r.relation)
                .and_then(|q| q.relation.as_ref())
                .map(|x| x.db_id.clone())
                .unwrap_or_default();
            let ts = if target_db == db_id {
                Some(s)
            } else {
                others.get(&target_db)
            };
            r.prop = match ts.and_then(|ts| ts.find(&t)) {
                Some(q) => q.key.clone(),
                None if ts.is_some() => return Err(bad(format!("关联的库里没有属性「{t}」"))),
                None => t,
            };
        }
        if let Some(f) = pick("fn").or_else(|| pick("func")) {
            let a = Agg::parse(&f).ok_or_else(|| bad(format!("不认识的汇总方式「{f}」")))?;
            r.func = a.key().into();
        }
        if r.func.is_empty() {
            r.func = Agg::Show.key().into();
        }
        p.rollup = Some(r);
    }
    if p.kind == PropType::Button {
        for k in ["label", "set", "comment", "agent"] {
            if let Some(v) = op.get(k) {
                if v.is_null() {
                    p.extra.remove(k);
                } else {
                    p.extra.insert(k.into(), v.clone());
                }
            }
        }
    }
    Ok(())
}

/// 换类型后不再适用的配置去掉。
fn reset_for_type(p: &mut Prop, old: &Prop) {
    use PropType::*;
    if !p.kind.has_options() || !old.kind.has_options() {
        p.options.clear();
    }
    if p.kind != Status {
        for o in &mut p.options {
            o.group = None;
        }
    } else {
        for o in &mut p.options {
            o.group.get_or_insert_with(|| "todo".into());
        }
    }
    if !matches!(p.kind, Number | Formula | Rollup) {
        p.format = None;
        p.precision = None;
    }
    if !matches!(p.kind, Date | Formula | Rollup | CreatedTime | UpdatedTime) {
        p.date_format = None;
    }
    if p.kind != Formula {
        p.formula = None;
    }
    if p.kind != Relation {
        p.relation = None;
    }
    if p.kind != Rollup {
        p.rollup = None;
    }
    if p.kind != UniqueId {
        p.prefix = None;
    }
    if p.kind != Button {
        for k in ["label", "set", "comment", "agent"] {
            p.extra.remove(k);
        }
    }
}

/// 关联到的别的库的表结构（汇总按名字找对方的属性）。
pub type Others = std::collections::HashMap<String, Schema>;

/// 改表结构的结果。
pub struct Applied {
    schema: Schema,
    changes: Vec<Value>,
    cells: Cells,
    /// 新加的双向关联：(这边的属性 key, 对方那一半叫什么, 对方那一半的 key)
    twoway: Vec<(String, Option<String>, String)>,
    /// 删掉（或改成别的类型）的关联：对方库里反向的那一半也去掉
    unpair: Vec<Prop>,
    /// 这批动过的属性（只对它们报公式错误）
    touched: HashSet<String>,
}

/// 按顺序做一批改动。
fn apply_ops(c: &mut Cached, db_id: &str, ops: &[Value], others: &Others) -> Result<Applied> {
    let mut s = c.eng.schema().clone();
    let mut a = Applied {
        schema: Schema::default(),
        changes: vec![],
        cells: Cells::default(),
        twoway: vec![],
        unpair: vec![],
        touched: HashSet::new(),
    };
    if ops.is_empty() {
        return Err(bad("没有要改的"));
    }
    for (n, op) in ops.iter().enumerate() {
        let o = op
            .as_object()
            .ok_or_else(|| bad("每个改动都要是一个对象"))?;
        let kind = o.get("op").and_then(Value::as_str).unwrap_or("");
        let step = |e: Error| match e {
            Error::BadRequest(m) if ops.len() > 1 => {
                Error::BadRequest(format!("第 {} 步：{m}", n + 1))
            }
            e => e,
        };
        apply_one(c, db_id, &mut s, &mut a, kind, o, others).map_err(step)?;
    }
    a.schema = s;
    Ok(a)
}

fn apply_one(
    c: &mut Cached,
    db_id: &str,
    s: &mut Schema,
    a: &mut Applied,
    kind: &str,
    o: &Map<String, Value>,
    others: &Others,
) -> Result<()> {
    match kind {
        "add" => {
            if s.props.len() >= MAX_PROPS {
                return Err(bad(format!("最多 {MAX_PROPS} 个属性")));
            }
            let name = clean_name(o.get("name").unwrap_or(&Value::Null))?;
            check_unique_name(s, &name, None)?;
            let t = type_of(o.get("type").unwrap_or(&json!("text")))?;
            if t == PropType::Title {
                return Err(bad("已经有标题了，只能有一个"));
            }
            let taken: HashSet<String> = s.props.iter().map(|p| p.key.clone()).collect();
            let mut p = Prop::new(&fresh_key(&taken), &name, t);
            if let Some(opts) = o.get("options") {
                p.options = options_of(opts, &[])?;
            }
            fill_options(&mut p);
            apply_config(&mut p, o, s, db_id, others)?;
            if t == PropType::Relation
                && o.get("twoway").and_then(Value::as_bool).unwrap_or(false)
                && let Some(r) = &mut p.relation
                && r.db_id != db_id
            {
                // 对方那一半的 key 先定好，记在这边（撤销后再放回来时认得出）
                let mk = short_id(4);
                r.extra.insert("pair".into(), mk.clone().into());
                let pair = o
                    .get("pair_name")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                a.twoway.push((p.key.clone(), pair, mk));
            }
            let at = match o.get("after").and_then(Value::as_str) {
                Some(r) => find_idx(s, &Value::String(r.into()))? + 1,
                None => s.props.len(),
            };
            a.changes
                .push(json!({ "op": "add", "key": p.key, "name": p.name, "type": p.kind }));
            a.touched.insert(p.key.clone());
            s.props.insert(at.max(1), p);
        }
        "delete" => {
            let i = find_idx(s, o.get("prop").unwrap_or(&Value::Null))?;
            if s.props[i].kind == PropType::Title {
                return Err(bad("标题不能删"));
            }
            let p = s.props.remove(i);
            for id in c.eng.row_ids() {
                a.cells.set(&mut c.eng, &id, &p.key, None);
            }
            if p.kind == PropType::Relation {
                a.unpair.push(p.clone());
            }
            a.changes
                .push(json!({ "op": "delete", "key": p.key, "name": p.name }));
        }
        "rename" => {
            let i = find_idx(s, o.get("prop").unwrap_or(&Value::Null))?;
            let name = clean_name(o.get("name").unwrap_or(&Value::Null))?;
            rename(s, a, i, name)?;
        }
        "move" => {
            let i = find_idx(s, o.get("prop").unwrap_or(&Value::Null))?;
            if s.props[i].kind == PropType::Title {
                return Err(bad("标题总在第一个"));
            }
            let p = s.props.remove(i);
            let at = match o.get("after").and_then(Value::as_str) {
                Some(r) => find_idx(s, &Value::String(r.into()))? + 1,
                None => 1,
            };
            a.changes
                .push(json!({ "op": "move", "key": p.key, "name": p.name }));
            s.props.insert(at.max(1), p);
        }
        "change_type" | "set_options" | "set_formula" | "update" | "set_config" => {
            let i = find_idx(s, o.get("prop").unwrap_or(&Value::Null))?;
            let key = s.props[i].key.clone();
            a.touched.insert(key.clone());
            if let Some(n) = o.get("name").filter(|_| kind == "update") {
                rename(s, a, i, clean_name(n)?)?;
            }
            let old = s.props[i].clone();
            let mut p = old.clone();
            let new_type = match o.get("type") {
                Some(t) if kind == "change_type" || kind == "update" => Some(type_of(t)?),
                None if kind == "change_type" => return Err(bad("要给 type")),
                _ => None,
            }
            .filter(|t| *t != old.kind);
            if let Some(t) = new_type {
                if old.kind == PropType::Title || t == PropType::Title {
                    return Err(bad("标题的类型不能改"));
                }
                if old.is_mirror() {
                    return Err(bad(format!(
                        "「{}」是另一个库的关联在这边的一半，不能改类型（不要了就删掉）",
                        old.name
                    )));
                }
                p.kind = t;
                reset_for_type(&mut p, &old);
                if old.kind == PropType::Relation {
                    a.unpair.push(old.clone());
                }
            }
            if let Some(opts) = o.get("options") {
                if !p.kind.has_options() {
                    return Err(bad(format!("「{}」不是单选、多选或状态，没有选项", p.name)));
                }
                p.options = options_of(opts, &old.options)?;
            }
            if kind == "set_formula" && p.kind != PropType::Formula {
                return Err(bad(format!("「{}」不是公式", p.name)));
            }
            fill_options(&mut p);
            apply_config(&mut p, o, s, db_id, others)?;
            if p == old {
                return Ok(());
            }
            // 类型或选项变了：整列转换（转不了的清空，新出现的选项名加成选项）
            let removed: HashSet<&str> = old
                .options
                .iter()
                .map(|x| x.id.as_str())
                .filter(|id| !p.options.iter().any(|x| x.id == *id))
                .collect();
            if new_type.is_some() {
                c.eng.set_schema(s.clone());
                let conv = c.eng.convert(&key, &p).map_err(|e| bad(e.msg))?;
                p = conv.prop;
                let had: HashSet<String> = conv.values.iter().map(|(r, _)| r.clone()).collect();
                for (row, v) in conv.values {
                    a.cells
                        .set(&mut c.eng, &row, &key, (!v.is_null()).then_some(v));
                }
                // 原来是算出来的、现在要存的：原来没存值；原来存的、现在算：清掉
                for id in c.eng.row_ids() {
                    if !had.contains(&id) || !p.is_writable() {
                        a.cells.set(&mut c.eng, &id, &key, None);
                    }
                }
                a.changes.push(json!({ "op": "change_type", "key": key, "name": p.name, "from": old.kind, "to": p.kind }));
            } else if !removed.is_empty() {
                for id in c.eng.row_ids() {
                    let Some(v) = c.eng.row(&id).and_then(|r| r.props.get(&key).cloned()) else {
                        continue;
                    };
                    let nv = match v {
                        Value::Array(xs) => {
                            let left: Vec<Value> = xs
                                .into_iter()
                                .filter(|x| !x.as_str().is_some_and(|s| removed.contains(s)))
                                .collect();
                            (!left.is_empty()).then_some(Value::Array(left))
                        }
                        Value::String(x) if removed.contains(x.as_str()) => None,
                        v => Some(v),
                    };
                    a.cells.set(&mut c.eng, &id, &key, nv);
                }
            }
            if old.options != p.options && new_type.is_none() {
                a.changes
                    .push(json!({ "op": "set_options", "key": key, "name": p.name }));
            }
            if old.formula != p.formula && p.kind == PropType::Formula && new_type.is_none() {
                a.changes
                    .push(json!({ "op": "set_formula", "key": key, "name": p.name }));
            }
            if new_type.is_none() && old.options == p.options && old.formula == p.formula {
                a.changes
                    .push(json!({ "op": "config", "key": key, "name": p.name }));
            }
            s.props[i] = p;
        }
        "" => return Err(bad("每个改动要有 op")),
        k => {
            return Err(bad(format!(
                "不认识的改动 {k}（能用的：add、rename、change_type、delete、set_options、set_formula、update、move）"
            )));
        }
    }
    Ok(())
}

fn rename(s: &mut Schema, a: &mut Applied, i: usize, name: String) -> Result<()> {
    let key = s.props[i].key.clone();
    check_unique_name(s, &name, Some(&key))?;
    let old = std::mem::replace(&mut s.props[i].name, name.clone());
    if old == name {
        return Ok(());
    }
    for p in &mut s.props {
        if let Some(f) = &p.formula {
            p.formula = Some(oad_db::formula::rename_prop(f, &old, &name));
        }
    }
    a.changes
        .push(json!({ "op": "rename", "key": key, "name": name, "from": old }));
    Ok(())
}

/// 撤销时把表结构放回去：这次改动动过的属性换回改之前的样子（加的去掉、删的放回、改的还原），别的保留现在的。
fn revert_schema(cur: &Schema, before: &Schema, after: &Schema) -> Schema {
    let mut out = cur.clone();
    let keys: Vec<String> = before
        .props
        .iter()
        .chain(after.props.iter())
        .map(|p| p.key.clone())
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    for k in keys {
        if !seen.insert(k.clone()) {
            continue;
        }
        let b = before.prop(&k);
        let a = after.prop(&k);
        if b == a {
            continue;
        }
        let at = out.props.iter().position(|p| p.key == k);
        match (b, at) {
            (None, Some(i)) => {
                out.props.remove(i);
            }
            (Some(bp), Some(i)) => out.props[i] = bp.clone(),
            (Some(bp), None) => {
                // 放回原来的位置：排在 before 里它前面那个属性后面
                let bi = before.props.iter().position(|p| p.key == k).unwrap_or(0);
                let prev = before.props[..bi]
                    .iter()
                    .rev()
                    .find_map(|q| out.props.iter().position(|p| p.key == q.key));
                let at = prev.map_or(out.props.len(), |i| i + 1);
                out.props.insert(at.max(1).min(out.props.len()), bp.clone());
            }
            (None, None) => {}
        }
    }
    // 只是挪了位置：按改之前的顺序排（after 里没有的放后面保持原样）
    let order_b: Vec<&str> = before
        .props
        .iter()
        .map(|p| p.key.as_str())
        .filter(|k| after.prop(k).is_some())
        .collect();
    let order_a: Vec<&str> = after
        .props
        .iter()
        .map(|p| p.key.as_str())
        .filter(|k| before.prop(k).is_some())
        .collect();
    if order_a != order_b {
        let pos = |k: &str| {
            before
                .props
                .iter()
                .position(|p| p.key == k)
                .unwrap_or(usize::MAX)
        };
        out.props.sort_by_key(|p| pos(&p.key));
    }
    // 改名时连带改过的公式：别的属性的公式也换回去
    for p in &mut out.props {
        if let (Some(bp), Some(ap)) = (before.prop(&p.key), after.prop(&p.key))
            && bp.formula != ap.formula
            && p.formula == ap.formula
        {
            p.formula = bp.formula.clone();
        }
    }
    out
}

impl Core {
    /// 改表结构。返回 { schema, seq, changes, warnings? }。
    pub async fn change_schema(
        &self,
        db_id: &str,
        who: &Principal,
        ops: Vec<Value>,
    ) -> Result<Value> {
        let (page, meta) = self.db_for(db_id, who, true).await?;
        // 关联到的库（现有的、这批里新指向的）的表结构
        let mut others = Others::new();
        let mut want: Vec<String> = parse_schema(&meta.schema)?
            .props
            .iter()
            .filter_map(|p| p.relation.as_ref().map(|r| r.db_id.clone()))
            .collect();
        for op in &ops {
            for v in [&op["db_id"], &op["relation"], &op["relation"]["db_id"]] {
                if let Some(x) = v.as_str() {
                    want.push(x.to_string());
                }
            }
        }
        for id in want {
            if id == db_id || others.contains_key(&id) {
                continue;
            }
            if let Some(p) = self.storage.page(&id).await?
                && p.workspace_id == page.workspace_id
                && let Some(m) = self.storage.database(&id).await?
            {
                others.insert(id, parse_schema(&m.schema)?);
            }
        }
        let mut c = self.engine(&page, &meta).await?;
        let before_errors: HashSet<String> = c
            .eng
            .schema_errors()
            .into_iter()
            .map(|e| format!("{}{}", e.key, e.msg))
            .collect();
        let r = apply_ops(&mut c, db_id, &ops, &others).and_then(|a| {
            c.eng.set_schema(a.schema.clone());
            // 这批动过的公式、汇总写错了就不改；别的属性因此出了问题的（比如删了公式用到的列）只提醒
            let mut warnings = vec![];
            for e in c.eng.schema_errors() {
                let name = a
                    .schema
                    .prop(&e.key)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                if a.touched.contains(&e.key) {
                    let src = a
                        .schema
                        .prop(&e.key)
                        .and_then(|p| p.formula.clone())
                        .unwrap_or_default();
                    let at =
                        e.at.map(|i| {
                            format!(
                                "（第 {} 个字附近：{}）",
                                i + 1,
                                src.chars().skip(i).take(12).collect::<String>()
                            )
                        })
                        .unwrap_or_default();
                    return Err(bad(format!("「{name}」：{}{at}", e.msg)));
                }
                if !before_errors.contains(&format!("{}{}", e.key, e.msg)) {
                    warnings.push(format!("「{name}」：{}", e.msg));
                }
            }
            Ok((a, warnings))
        });
        let (a, warnings) = match r {
            Ok(x) => x,
            Err(e) => {
                drop(c);
                self.dbcache.forget(db_id);
                return Err(e);
            }
        };
        let schema = to_value(&a.schema);
        if a.changes.is_empty() && schema == meta.schema {
            return Ok(json!({ "schema": meta.schema, "seq": meta.seq, "changes": [] }));
        }
        let views = eng::check_views(meta.views.clone(), &schema)?;
        let w = SchemaWrite {
            schema: schema.clone(),
            views: (views != meta.views).then(|| views.clone()),
            op: Value::Array(ops).to_string(),
            before: json!({ "schema": meta.schema, "views": meta.views, "cells": a.cells.json(false), "changes": a.changes }),
            after: json!({ "schema": schema, "views": views, "cells": a.cells.json(true) }),
            cells: a.cells.writes(),
        };
        // 引擎里已经是新的表结构了
        c.set_schema_value(schema.clone());
        drop(c);
        let seq = match self.storage.set_db_schema(db_id, w, &person_ref(who)).await {
            Ok(s) => s,
            Err(e) => {
                self.dbcache.forget(db_id);
                return Err(e);
            }
        };
        // 双向关联：对方库里加上 / 去掉反向的那一半
        for (key, pair, mk) in a.twoway {
            self.add_mirror(&page, who, &key, pair, &mk).await?;
        }
        for p in a.unpair {
            self.drop_mirror(&page, who, &p).await?;
        }
        self.emit_db(&page.workspace_id, "db.schema", json!({ "db_id": db_id }));
        if !a.cells.0.is_empty() {
            self.emit_db(
                &page.workspace_id,
                "db.changed",
                json!({ "db_id": db_id, "seq": seq }),
            );
        }
        let mut out = json!({ "schema": schema, "seq": seq, "changes": a.changes });
        if !warnings.is_empty() {
            out["warnings"] = json!(warnings);
        }
        Ok(out)
    }

    /// 撤销某一次表结构改动：动过的属性换回去，因此改了的格子放回原来的值（之后又被人改过的格子不动）。
    pub async fn undo_schema(&self, db_id: &str, who: &Principal, seq: i64) -> Result<Value> {
        let (page, meta) = self.db_for(db_id, who, true).await?;
        let rec = self
            .storage
            .db_schema_op(db_id, seq)
            .await?
            .ok_or_else(|| Error::NotFound(format!("没有第 {seq} 次表结构改动")))?;
        let cur = parse_schema(&meta.schema)?;
        let before = parse_schema(&rec.before["schema"])?;
        let after = parse_schema(&rec.after["schema"])?;
        let mut schema = revert_schema(&cur, &before, &after);
        // 标题总在第一个
        if let Some(i) = schema.props.iter().position(|p| p.kind == PropType::Title)
            && i != 0
        {
            let t = schema.props.remove(i);
            schema.props.insert(0, t);
        }
        let schema_v = to_value(&schema);
        // 视图：没人再改过就整个放回去，不然只去掉指向没了的属性的设置
        let views = if rec.after.get("views").is_some_and(|v| *v == meta.views)
            && rec.before.get("views").is_some()
        {
            eng::check_views(rec.before["views"].clone(), &schema_v)?
        } else {
            eng::check_views(meta.views.clone(), &schema_v)?
        };
        let mut cells = Vec::new();
        let mut back = Vec::new();
        let mut fwd = Vec::new();
        let olds = rec.before["cells"].as_array().cloned().unwrap_or_default();
        let news = rec.after["cells"].as_array().cloned().unwrap_or_default();
        for (o, n) in olds.iter().zip(news.iter()) {
            let (Some(row), Some(key)) = (o[0].as_str(), o[1].as_str()) else {
                continue;
            };
            let old = (!o[2].is_null()).then(|| o[2].clone());
            let new = (!n[2].is_null()).then(|| n[2].clone());
            cells.push(CellSet {
                row_id: row.into(),
                key: key.into(),
                value: old.clone(),
                only_if: Some(new.clone()),
            });
            back.push(json!([row, key, old]));
            fwd.push(json!([row, key, new]));
        }
        cells.sort_by(|a, b| a.row_id.cmp(&b.row_id));
        let w = SchemaWrite {
            schema: schema_v.clone(),
            views: (views != meta.views).then(|| views.clone()),
            op: json!([{ "op": "undo", "seq": seq }]).to_string(),
            before: json!({ "schema": meta.schema, "views": meta.views, "cells": fwd }),
            after: json!({ "schema": schema_v, "views": views, "cells": back }),
            cells,
        };
        let new_seq = self
            .storage
            .set_db_schema(db_id, w, &person_ref(who))
            .await?;
        // 双向关联：撤销加的就去掉对方那一半；放回删掉的就把对方那一半加回来
        for p in &after.props {
            if p.kind == PropType::Relation
                && !p.is_mirror()
                && schema
                    .prop(&p.key)
                    .is_none_or(|q| q.kind != PropType::Relation)
            {
                self.drop_mirror(&page, who, p).await?;
            }
        }
        for p in &schema.props {
            if p.kind == PropType::Relation
                && !p.is_mirror()
                && after
                    .prop(&p.key)
                    .is_none_or(|q| q.kind != PropType::Relation)
                && let Some(pair) = p
                    .relation
                    .as_ref()
                    .and_then(|r| r.extra.get("pair"))
                    .and_then(Value::as_str)
            {
                self.restore_mirror(&page, who, p, pair).await?;
            }
        }
        self.emit_db(&page.workspace_id, "db.schema", json!({ "db_id": db_id }));
        self.emit_db(
            &page.workspace_id,
            "db.changed",
            json!({ "db_id": db_id, "seq": new_seq }),
        );
        Ok(json!({ "schema": schema_v, "views": views, "seq": new_seq }))
    }

    /// 在对方库里加双向关联反向的那一半，返回它的 key。对方就是自己时不加。
    async fn add_mirror(
        &self,
        page: &Page,
        who: &Principal,
        key: &str,
        pair: Option<String>,
        mk: &str,
    ) -> Result<Option<String>> {
        let Some(m) = self.storage.database(&page.id).await? else {
            return Ok(None);
        };
        let s = parse_schema(&m.schema)?;
        let Some(p) = s.prop(key) else {
            return Ok(None);
        };
        let Some(other) = p
            .relation
            .as_ref()
            .map(|r| r.db_id.clone())
            .filter(|d| *d != page.id)
        else {
            return Ok(None);
        };
        let (op, om) = self.db_for(&other, who, true).await?;
        let mut os = parse_schema(&om.schema)?;
        let base = pair.unwrap_or_else(|| {
            if page.title.is_empty() {
                "关联".into()
            } else {
                page.title.clone()
            }
        });
        let mut name = base.clone();
        let mut n = 2;
        while os.props.iter().any(|q| q.name.eq_ignore_ascii_case(&name)) {
            name = format!("{base} {n}");
            n += 1;
        }
        let taken: HashSet<String> = os.props.iter().map(|q| q.key.clone()).collect();
        let mk = if mk.is_empty() || taken.contains(mk) {
            fresh_key(&taken)
        } else {
            mk.to_string()
        };
        let mut mp = Prop::new(&mk, &name, PropType::Relation);
        mp.relation = Some(RelationConf {
            db_id: page.id.clone(),
            mirror_of: Some(key.into()),
            extra: Map::new(),
        });
        os.props.push(mp);
        let after = to_value(&os);
        let w = SchemaWrite {
            schema: after.clone(),
            views: None,
            op: json!([{ "op": "add", "name": name, "type": "relation", "mirror_of": key, "db_id": page.id }]).to_string(),
            before: json!({ "schema": om.schema, "views": om.views, "cells": [] }),
            after: json!({ "schema": after, "views": om.views, "cells": [] }),
            cells: vec![],
        };
        self.storage
            .set_db_schema(&other, w, &person_ref(who))
            .await?;
        self.emit_db(&op.workspace_id, "db.schema", json!({ "db_id": other }));
        Ok(Some(mk))
    }

    /// 把撤销时放回来的双向关联在对方库里的那一半也放回来（还在就不动）。
    async fn restore_mirror(
        &self,
        page: &Page,
        who: &Principal,
        p: &Prop,
        pair: &str,
    ) -> Result<()> {
        let Some(other) = p
            .relation
            .as_ref()
            .map(|r| r.db_id.clone())
            .filter(|d| *d != page.id)
        else {
            return Ok(());
        };
        let Ok((_, om)) = self.db_for(&other, who, true).await else {
            return Ok(());
        };
        let os = parse_schema(&om.schema)?;
        if os.props.iter().any(|q| {
            q.is_mirror()
                && q.relation.as_ref().and_then(|r| r.mirror_of.as_deref()) == Some(&p.key)
        }) {
            return Ok(());
        }
        self.add_mirror(page, who, &p.key, None, pair).await?;
        Ok(())
    }

    /// 去掉对方库里指向这个关联的反向那一半；删的是反向那一半的话什么也不做。
    async fn drop_mirror(&self, page: &Page, who: &Principal, p: &Prop) -> Result<()> {
        if p.is_mirror() {
            return Ok(());
        }
        let Some(other) = p
            .relation
            .as_ref()
            .map(|r| r.db_id.clone())
            .filter(|d| *d != page.id)
        else {
            return Ok(());
        };
        let Ok((op, om)) = self.db_for(&other, who, true).await else {
            return Ok(());
        };
        let mut os = parse_schema(&om.schema)?;
        let before = os.props.len();
        os.props.retain(|q| {
            !(q.is_mirror()
                && q.relation
                    .as_ref()
                    .is_some_and(|r| r.db_id == page.id && r.mirror_of.as_deref() == Some(&p.key)))
        });
        if os.props.len() == before {
            return Ok(());
        }
        let after = to_value(&os);
        let views = eng::check_views(om.views.clone(), &after)?;
        let w = SchemaWrite {
            schema: after.clone(),
            views: (views != om.views).then(|| views.clone()),
            op: json!([{ "op": "delete", "mirror_of": p.key, "db_id": page.id }]).to_string(),
            before: json!({ "schema": om.schema, "views": om.views, "cells": [] }),
            after: json!({ "schema": after, "views": views, "cells": [] }),
            cells: vec![],
        };
        self.storage
            .set_db_schema(&other, w, &person_ref(who))
            .await?;
        self.emit_db(&op.workspace_id, "db.schema", json!({ "db_id": other }));
        Ok(())
    }
}
