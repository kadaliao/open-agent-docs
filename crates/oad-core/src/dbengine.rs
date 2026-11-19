//! 服务器这边用 oad-db 的地方：表结构、视图的检查，引擎缓存（规范化、查询、公式、显示都是 oad-db 的 Engine 算）。
//! 引擎本身都在 oad-db 里，这里只做和存储、权限之间的对接。
use crate::db::short_id;
use crate::model::*;
use crate::{Core, Error, Result};
use oad_db::schema::{Person, Prop, PropType, RowData, Schema};
use oad_db::{Change, Engine};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 缓存几个库的引擎（每个装着全部行）。
const CACHED_DBS: usize = 16;

/// 一个库的引擎和它对应的版本。
pub struct Cached {
    pub eng: Engine,
    seq: i64,
    schema: Value,
    people: Vec<Person>,
    /// 关联到的库 → 加载时的 seq
    related: HashMap<String, i64>,
    /// 规范化时新建了选项（Agent 写了没有的选项名），要存回表结构。
    pub new_options: bool,
    /// 行 id → 位置（挪了位置的要重载，保持顺序）
    pos: HashMap<String, String>,
}

/// (最近用的时刻, 引擎)
type Slot = (u64, Arc<Mutex<Cached>>);

/// 引擎缓存：库 id → 引擎。用的时候锁住那一个库的（tokio 锁，可以跨 await）。
#[derive(Default)]
pub struct DbCache {
    map: std::sync::Mutex<(u64, HashMap<String, Slot>)>,
}

impl DbCache {
    fn slot(&self, db_id: &str) -> Option<Arc<Mutex<Cached>>> {
        let mut g = self.map.lock().unwrap();
        g.0 += 1;
        let t = g.0;
        let e = g.1.get_mut(db_id)?;
        e.0 = t;
        Some(e.1.clone())
    }

    fn put(&self, db_id: &str, c: Arc<Mutex<Cached>>) {
        let mut g = self.map.lock().unwrap();
        g.0 += 1;
        let t = g.0;
        g.1.insert(db_id.to_string(), (t, c));
        while g.1.len() > CACHED_DBS {
            let Some(old) =
                g.1.iter()
                    .min_by_key(|(_, (t, _))| *t)
                    .map(|(k, _)| k.clone())
            else {
                break;
            };
            g.1.remove(&old);
        }
    }

    pub fn forget(&self, db_id: &str) {
        self.map.lock().unwrap().1.remove(db_id);
    }

    pub fn len(&self) -> usize {
        self.map.lock().unwrap().1.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 存的一行 → 引擎的一行。
pub fn row_data(r: &DbRow) -> RowData {
    RowData {
        id: r.id.clone(),
        seq: r.seq,
        props: r.props.clone(),
        created_at: Some(r.created_at as f64),
        created_by: r.created_by.clone(),
        updated_at: Some(r.updated_at as f64),
        updated_by: r.updated_by.clone(),
        unique_id: Some(r.unique_id),
    }
}

/// 写进 created_by、updated_by、db_ops.by 的样子（和“人员”属性的值一样）：`user:<id>` / `agent:<id>`。
pub fn person_ref(who: &Principal) -> String {
    let pre = if who.kind == PrincipalKind::Agent {
        "agent"
    } else {
        "user"
    };
    format!("{pre}:{}", who.user_id)
}

pub fn parse_schema(v: &Value) -> Result<Schema> {
    serde_json::from_value(v.clone()).map_err(|e| Error::BadRequest(format!("表结构不对：{e}")))
}

pub fn to_value(s: &Schema) -> Value {
    serde_json::to_value(s).unwrap_or_default()
}

/// 新表结构：补 key、选项 id；第一个属性是标题（没有就加一个）；名字不能重复、不能空。
pub fn check_schema(mut v: Value) -> Result<Value> {
    // 没给 key 的补空（下面换成随机的）；类型可以写中文名、别名
    if let Some(props) = v.get_mut("props").and_then(Value::as_array_mut) {
        for p in props {
            let Some(o) = p.as_object_mut() else {
                return Err(Error::BadRequest("每个属性要是 { name, type, ... }".into()));
            };
            o.entry("key").or_insert("".into());
            let t = o
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("text")
                .to_string();
            let kind = PropType::from_name(&t)
                .ok_or_else(|| Error::BadRequest(format!("不认识的类型「{t}」")))?;
            o.insert("type".into(), kind.key().into());
            // 选项可以只写名字
            if let Some(Value::Array(opts)) = o.get_mut("options") {
                for x in opts.iter_mut() {
                    if let Value::String(n) = x {
                        *x = serde_json::json!({ "id": "", "name": n });
                    } else if let Some(xo) = x.as_object_mut() {
                        xo.entry("id").or_insert("".into());
                    }
                }
            }
        }
    }
    let mut s = parse_schema(&v)?;
    let mut taken: HashSet<String> = HashSet::new();
    let mut names: HashSet<String> = HashSet::new();
    for p in &mut s.props {
        if p.key.is_empty() || taken.contains(&p.key) {
            p.key = fresh_key(&taken);
        }
        taken.insert(p.key.clone());
        p.name = p.name.split_whitespace().collect::<Vec<_>>().join(" ");
        if p.name.is_empty() {
            return Err(Error::BadRequest("属性要有名字".into()));
        }
        if !names.insert(p.name.to_lowercase()) {
            return Err(Error::BadRequest(format!("有两个叫「{}」的属性", p.name)));
        }
        if p.kind == PropType::Unknown {
            return Err(Error::BadRequest(format!("「{}」的类型不认识", p.name)));
        }
        fill_options(p);
    }
    match s.props.iter().position(|p| p.kind == PropType::Title) {
        Some(0) => {}
        Some(i) => {
            let t = s.props.remove(i);
            s.props.insert(0, t);
        }
        None => {
            let mut name = "名称".to_string();
            if names.contains(&name) {
                name = "标题".into();
            }
            s.props
                .insert(0, Prop::new(&fresh_key(&taken), &name, PropType::Title));
        }
    }
    if s.props.iter().filter(|p| p.kind == PropType::Title).count() > 1 {
        return Err(Error::BadRequest("只能有一个标题属性".into()));
    }
    s.next_id.get_or_insert(1);
    Ok(to_value(&s))
}

pub fn fresh_key(taken: &HashSet<String>) -> String {
    loop {
        let k = short_id(4);
        if !taken.contains(&k) {
            return k;
        }
    }
}

/// 选项补上 id、颜色；状态没有选项时给默认的三组。
pub fn fill_options(p: &mut Prop) {
    if p.kind == PropType::Status && p.options.is_empty() {
        for (name, group, color) in [
            ("未开始", "todo", "gray"),
            ("进行中", "doing", "blue"),
            ("已完成", "done", "green"),
        ] {
            p.options.push(oad_db::schema::SelectOption {
                id: short_id(6),
                name: name.into(),
                color: color.into(),
                group: Some(group.into()),
                extra: Map::new(),
            });
        }
    }
    let mut seen = HashSet::new();
    for o in &mut p.options {
        if o.id.is_empty() || !seen.insert(o.id.clone()) {
            o.id = short_id(6);
            seen.insert(o.id.clone());
        }
        if o.color.is_empty() {
            o.color = "default".into();
        }
    }
}

pub const VIEW_TYPES: [&str; 5] = ["table", "board", "calendar", "gallery", "list"];

/// 视图：补 id、类型；去掉指向已经没有的属性的设置（筛选条件、排序、分组、隐藏、顺序、列宽）。
pub fn check_views(v: Value, schema: &Value) -> Result<Value> {
    let s = parse_schema(schema)?;
    let keys: HashSet<&str> = s.props.iter().map(|p| p.key.as_str()).collect();
    let arr = v
        .as_array()
        .ok_or_else(|| Error::BadRequest("视图要是数组".into()))?;
    let mut ids = HashSet::new();
    let mut out = Vec::with_capacity(arr.len());
    for x in arr {
        let mut o = x
            .as_object()
            .cloned()
            .ok_or_else(|| Error::BadRequest("视图要是对象".into()))?;
        let id = o
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let id = match id {
            Some(i) if ids.insert(i.clone()) => i,
            _ => {
                let i = short_id(6);
                ids.insert(i.clone());
                i
            }
        };
        o.insert("id".into(), id.into());
        let t = o
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("table")
            .to_string();
        if !VIEW_TYPES.contains(&t.as_str()) {
            return Err(Error::BadRequest(format!(
                "不认识的视图类型 {t}（只能是 {}）",
                VIEW_TYPES.join("、")
            )));
        }
        o.insert("type".into(), t.into());
        if o.get("name")
            .and_then(Value::as_str)
            .is_none_or(|n| n.trim().is_empty())
        {
            o.insert("name".into(), "视图".into());
        }
        for k in ["hidden", "order"] {
            if let Some(Value::Array(a)) = o.get_mut(k) {
                a.retain(|x| x.as_str().is_some_and(|s| keys.contains(s)));
            }
        }
        if let Some(Value::Object(w)) = o.get_mut("widths") {
            w.retain(|k, _| keys.contains(k.as_str()));
        }
        if let Some(Value::Array(a)) = o.get_mut("sorts") {
            a.retain(|x| x["prop"].as_str().is_some_and(|s| keys.contains(s)));
        }
        for k in ["group_by", "sub_group_by", "calendar_by"] {
            if o.get(k)
                .and_then(Value::as_str)
                .is_some_and(|s| !keys.contains(s))
            {
                o.remove(k);
            }
        }
        if let Some(f) = o.get_mut("filter") {
            prune_filter(f, &keys);
        }
        out.push(Value::Object(o));
    }
    Ok(Value::Array(out))
}

fn prune_filter(f: &mut Value, keys: &HashSet<&str>) {
    if let Some(Value::Array(items)) = f.get_mut("items") {
        items.retain_mut(|it| {
            if it.get("items").is_some() {
                prune_filter(it, keys);
                true
            } else {
                it["prop"].as_str().is_some_and(|s| keys.contains(s))
            }
        });
    }
}

pub fn title_key(schema: &Value) -> Result<String> {
    schema["props"]
        .as_array()
        .and_then(|a| a.iter().find(|p| p["type"] == "title"))
        .and_then(|p| p["key"].as_str())
        .map(str::to_string)
        .ok_or_else(|| Error::Internal("表结构里没有标题".into()))
}

/// 一行要写的属性：键是 key 或名字，值是存的样子或名字；都换成 key → 存的样子。只读的属性报错。
/// create：写了没有的选项名时新建选项（放进引擎的表结构、置上 new_options，调用方存回去）。
pub fn normalize_props(
    c: &mut Cached,
    input: &Map<String, Value>,
    create: bool,
) -> Result<Map<String, Value>> {
    let (out, added) = c
        .eng
        .normalize_row(input, create)
        .map_err(|e| Error::BadRequest(e.msg))?;
    if !added.is_empty() {
        let mut s = c.eng.schema().clone();
        for (key, opts) in added {
            if let Some(p) = s.props.iter_mut().find(|p| p.key == key) {
                for o in opts {
                    if !p.options.iter().any(|x| x.id == o.id) {
                        p.options.push(o);
                    }
                }
            }
        }
        c.schema = to_value(&s);
        c.eng.set_schema(s);
        c.new_options = true;
    }
    Ok(out)
}

impl Cached {
    /// 现在（含新加的选项）的表结构。
    pub fn schema_value(&self) -> Value {
        to_value(self.eng.schema())
    }

    /// 引擎里已经换成了这个表结构（改表结构时）。
    pub fn set_schema_value(&mut self, v: Value) {
        self.schema = v;
    }
}

impl Core {
    /// 工作区里的人（规范化、显示“人员”用）。
    pub(crate) async fn db_people(&self, workspace_id: &str) -> Result<Vec<Person>> {
        let mut out: Vec<Person> = self
            .storage
            .members(workspace_id)
            .await?
            .into_iter()
            .map(|(u, _)| {
                let pre = if u.kind == PrincipalKind::Agent {
                    "agent"
                } else {
                    "user"
                };
                Person {
                    id: format!("{pre}:{}", u.id),
                    name: u.name,
                }
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out.dedup_by(|a, b| a.id == b.id);
        Ok(out)
    }

    /// 这个库的引擎（缓存的，按 seq 补上增量；表结构、人、关联的库变了就换掉）。锁住返回。
    pub(crate) async fn engine(
        &self,
        page: &Page,
        meta: &DbMeta,
    ) -> Result<tokio::sync::OwnedMutexGuard<Cached>> {
        let people = self.db_people(&page.workspace_id).await?;
        let schema = parse_schema(&meta.schema)?;
        // 关联到的库现在的 seq
        let mut related: HashMap<String, (i64, Value)> = HashMap::new();
        for p in &schema.props {
            let Some(r) = &p.relation else { continue };
            if r.db_id.is_empty() || r.db_id == page.id || related.contains_key(&r.db_id) {
                continue;
            }
            let Some(other) = self.storage.page(&r.db_id).await? else {
                continue;
            };
            if other.workspace_id != page.workspace_id || other.deleted_at.is_some() {
                continue;
            }
            let Some(m) = self.storage.database(&r.db_id).await? else {
                continue;
            };
            related.insert(r.db_id.clone(), (m.seq, m.schema));
        }
        let slot = match self.dbcache.slot(&page.id) {
            Some(s) => s,
            None => {
                let mut eng = Engine::new(schema.clone());
                eng.set_id(&page.id);
                let c = Arc::new(Mutex::new(Cached {
                    eng,
                    seq: -1,
                    schema: meta.schema.clone(),
                    people: vec![],
                    related: HashMap::new(),
                    new_options: false,
                    pos: HashMap::new(),
                }));
                self.dbcache.put(&page.id, c.clone());
                c
            }
        };
        let mut c = slot.lock_owned().await;
        c.new_options = false;
        c.eng.set_clock(self.db_clock());
        if c.people != people {
            c.eng.set_people(people.clone());
            c.people = people;
        }
        if c.schema != meta.schema {
            c.eng.set_schema(schema);
            c.schema = meta.schema.clone();
        }
        for (id, (seq, sch)) in &related {
            if c.related.get(id) == Some(seq) {
                continue;
            }
            let (_, rows, _) = self.storage.db_rows(id, None).await?;
            c.eng
                .load_related(id, parse_schema(sch)?, rows.iter().map(row_data).collect());
            c.related.insert(id.clone(), *seq);
        }
        // 行：第一次全量；之后补增量（有新行、挪了位置时全量重载，保持库里的顺序）
        if c.seq < meta.seq || c.seq < 0 {
            let full = c.seq < 0;
            let (mut seq, mut rows, deleted) = self
                .storage
                .db_rows(&page.id, (!full).then_some(c.seq))
                .await?;
            let reorder = full || rows.iter().any(|r| c.pos.get(&r.id) != Some(&r.position));
            if reorder {
                if !full {
                    (seq, rows, _) = self.storage.db_rows(&page.id, None).await?;
                }
                c.pos = rows
                    .iter()
                    .map(|r| (r.id.clone(), r.position.clone()))
                    .collect();
                c.eng.load_rows(rows.iter().map(row_data).collect());
            } else {
                let mut changes: Vec<Change> = rows
                    .iter()
                    .map(|r| Change {
                        row_id: r.id.clone(),
                        row: Some(row_data(r)),
                        ..Default::default()
                    })
                    .collect();
                for id in &deleted {
                    c.pos.remove(id);
                }
                changes.extend(deleted.into_iter().map(|id| Change {
                    row_id: id,
                    deleted: true,
                    ..Default::default()
                }));
                c.eng.apply(changes);
            }
            c.seq = seq;
        }
        Ok(c)
    }
}

/// 表结构给 Agent 看：每个属性一行（名字、类型、选项 / 公式 / 关联…）。`db_title` 把关联的库 id 换成库名。
pub fn schema_text(
    s: &Schema,
    lang: crate::i18n::Lang,
    db_title: &dyn Fn(&str) -> Option<String>,
) -> String {
    use crate::i18n::Lang;
    let tr =
        |zh: &str, args: &[(&str, &str)]| crate::i18n::t_in(&[crate::i18n::CORE], lang, zh, args);
    let en = lang == Lang::En;
    // 中文用全角括号、冒号；英文用半角，类型写英文的 key（number、multi_select…）
    let (open, close, colon) = if en {
        (" (", ")", ": ")
    } else {
        ("（", "）", "：")
    };
    let mut out = String::new();
    for p in &s.props {
        let kind = match (p.kind, en) {
            (PropType::Title, false) => "标题",
            (k, false) => k.zh(),
            (k, true) => k.key(),
        };
        let mut line = format!("- {}{open}{kind}", p.name);
        if p.kind.has_options() && !p.options.is_empty() {
            let names: Vec<&str> = p.options.iter().map(|o| o.name.as_str()).collect();
            line.push_str(&format!("{colon}{}", names.join(" / ")));
        }
        if let Some(f) = &p.formula {
            line.push_str(&format!("{colon}{f}"));
        }
        if let Some(r) = &p.relation {
            let name = db_title(&r.db_id).unwrap_or_else(|| r.db_id.clone());
            if p.is_mirror() {
                line.push_str(&tr(
                    "：《{name}》那边关联的反向，自动算",
                    &[("name", &name)],
                ));
            } else {
                line.push_str(&tr("：指向《{name}》的行", &[("name", &name)]));
            }
        }
        if let Some(r) = &p.rollup {
            let rel = s
                .prop(&r.relation)
                .map(|x| x.name.clone())
                .unwrap_or_else(|| r.relation.clone());
            let agg = oad_db::Agg::parse(&r.func)
                .map(|a| if en { a.key() } else { a.zh() })
                .unwrap_or(&r.func)
                .to_string();
            line.push_str(&tr(
                "：沿「{rel}」取「{prop}」，{agg}",
                &[("rel", &rel), ("prop", &r.prop), ("agg", &agg)],
            ));
        }
        if let Some(f) = &p.format {
            line.push_str(&tr("，格式 {format}", &[("format", f)]));
        }
        line.push_str(close);
        if !p.is_writable() && p.kind != PropType::Title {
            line.push_str(&tr(" · 只读", &[]));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// 是不是标题类型的写法（title / 标题 / 名称 / name）。
pub fn is_title_type(s: &str) -> bool {
    PropType::from_name(s) == Some(PropType::Title)
}

/// 引擎给人看的文字（oad-db 的 display 出中文的几种：是、无标题、今天 / 明天……）按语言换。多值是 ", " 分开的。
pub fn localize_display(text: &str, lang: crate::i18n::Lang) -> String {
    if lang == crate::i18n::Lang::Zh || text.is_empty() {
        return text.to_string();
    }
    text.split(", ")
        .map(|part| {
            for (zh, en) in [("是", "Yes"), ("否", "No"), ("无标题", "Untitled")] {
                if part == zh {
                    return en.to_string();
                }
            }
            // 相对日期：“今天”“明天 14:00”
            for (zh, en) in [
                ("今天", "Today"),
                ("明天", "Tomorrow"),
                ("昨天", "Yesterday"),
                ("后天", "In 2 days"),
                ("前天", "2 days ago"),
            ] {
                if let Some(rest) = part.strip_prefix(zh)
                    && (rest.is_empty() || rest.starts_with(' '))
                {
                    return format!("{en}{rest}");
                }
            }
            part.to_string()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::localize_display;
    use crate::i18n::Lang;

    #[test]
    fn display_in_english() {
        assert_eq!(localize_display("是", Lang::En), "Yes");
        assert_eq!(localize_display("无标题, 甲", Lang::En), "Untitled, 甲");
        assert_eq!(localize_display("明天 14:00", Lang::En), "Tomorrow 14:00");
        assert_eq!(
            localize_display("今天会议", Lang::En),
            "今天会议",
            "用户写的字不动"
        );
        assert_eq!(localize_display("是", Lang::Zh), "是");
    }
}
