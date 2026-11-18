//! 改类型时值怎么转：同类的原样留着；别的先变成给人看的文字，再按新类型认（单选、多选没有的选项新加），认不出就清空。
use crate::Lookup;
use crate::display::display;
use crate::normalize::normalize;
use crate::schema::{Prop, PropType, SelectOption};
use serde_json::Value;

/// 一列的值从 `old` 转成 `new`。返回转好的值（null 是清空）和新加的选项（已经放进返回的属性里）。
pub fn convert_column(
    old: &Prop,
    new: &Prop,
    values: &[Value],
    look: &dyn Lookup,
) -> (Vec<Value>, Prop, Vec<SelectOption>) {
    let mut target = new.clone();
    let mut added = vec![];
    let out = values
        .iter()
        .map(|v| {
            let (val, opts) = convert_value(old, &target, v, look);
            target.options.extend(opts.iter().cloned());
            added.extend(opts);
            val
        })
        .collect();
    (out, target, added)
}

/// 一个值从 `old` 转成 `new`：转好的值和要新加的选项。
pub fn convert_value(
    old: &Prop,
    new: &Prop,
    v: &Value,
    look: &dyn Lookup,
) -> (Value, Vec<SelectOption>) {
    use PropType::*;
    if v.is_null() || !new.is_writable() {
        return (Value::Null, vec![]);
    }
    let same = old.kind == new.kind
        && (!old.kind.has_options() || old.options == new.options)
        && (old.kind != Relation
            || old.relation.as_ref().map(|r| &r.db_id) == new.relation.as_ref().map(|r| &r.db_id));
    if same && !old.is_computed() {
        return (v.clone(), vec![]);
    }
    // 多选变单选：留第一个
    let v = match (old.kind, new.kind, v) {
        (MultiSelect, Select | Status, Value::Array(a)) => {
            a.first().cloned().unwrap_or(Value::Null)
        }
        _ => v.clone(),
    };
    // 关联到同一个库：行 id 原样
    if old.kind == Relation
        && new.kind == Relation
        && old.relation.as_ref().map(|r| &r.db_id) == new.relation.as_ref().map(|r| &r.db_id)
    {
        return (v, vec![]);
    }
    let text = match (old.kind, &v) {
        (Number, Value::Number(_)) if new.kind != Text => {
            crate::num::text(v.as_f64().unwrap_or(0.0))
        }
        (Date, _) if new.kind == Date => return (v, vec![]),
        (Person | CreatedBy | UpdatedBy, _) if new.kind == Person => {
            return (
                match v {
                    Value::String(s) => Value::Array(vec![Value::String(s)]),
                    v => v,
                },
                vec![],
            );
        }
        (Checkbox, Value::Bool(b)) => (if *b { "是" } else { "否" }).to_string(),
        _ => display(old, &v, look),
    };
    match normalize(new, &Value::String(text), look, true) {
        Ok(n) => (n.value, n.new_options),
        Err(_) => (Value::Null, vec![]),
    }
}
