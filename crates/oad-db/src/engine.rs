//! 引擎：装着一个库（主库）的行，和关联、汇总要用到的别的库。公式、汇总、反向关联读的时候算，按行缓存；
//! 改动（apply）时只让受影响的缓存失效：改了的行；和读到主库别的行的列（汇总、反向关联、关联标题）整列。
use crate::aggregate::{Agg, aggregate};
use crate::date::{Clock, DateTime};
use crate::formula::{self, Compiled, Ctx, FormulaError, V};
use crate::normalize::{Normalized, normalize};
use crate::schema::{Change, Person, Prop, PropType, RowData, Schema, SelectOption};
use crate::value::{Cell, OptIndex, cell_of};
use crate::{DbError, Lookup};
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::{Map, Value};
use std::cell::{Cell as Flag, RefCell};
use std::sync::Arc;

/// 跨库（沿关联）最多走几层：汇总套汇总……多了当成空的。
pub const MAX_HOPS: u8 = 6;

pub(crate) struct Row {
    pub data: RowData,
    pub cells: Vec<Cell>,
}

/// 每个属性解析好的东西。
#[derive(Default)]
pub(crate) struct PropRt {
    pub opts: OptIndex,
    /// 选项名（公式里的样子）
    pub opt_names: Vec<V>,
    pub formula: Option<Result<Compiled, FormulaError>>,
    /// 关联 / 反向关联：对方库；汇总：它的关联指向的库
    pub target: Option<usize>,
    /// 反向关联：对方库里那个关联属性的序号
    pub mirror_src: Option<usize>,
    /// 汇总：这个库里的关联属性、对方库里的属性、算法
    pub rollup: Option<(usize, Option<usize>, Agg)>,
    /// 这一列除了本行还读哪些库的行（那些库变了整列重算）
    pub reads: FxHashSet<usize>,
    pub uses_clock: bool,
    /// 设置有问题（公式写错、成环、汇总的关联不存在）
    pub error: Option<SchemaError>,
}

/// 表结构里有问题的属性。
#[derive(Clone, Debug, PartialEq)]
pub struct SchemaError {
    pub key: String,
    pub msg: String,
    /// 公式出错的位置（UTF-16）
    pub at: Option<usize>,
}

pub(crate) struct Table {
    pub id: String,
    pub schema: Schema,
    pub rt: Vec<PropRt>,
    pub by_key: FxHashMap<String, usize>,
    pub by_name: FxHashMap<String, usize>,
    pub title: Option<usize>,
    pub rows: Vec<Row>,
    pub index: FxHashMap<Box<str>, usize>,
    /// 每个算出来的属性一列缓存（别的属性是空的 Vec）
    pub calc: Vec<RefCell<Vec<Option<V>>>>,
}

impl Table {
    fn new(id: String, schema: Schema) -> Table {
        let mut t = Table {
            id,
            schema,
            rt: vec![],
            by_key: FxHashMap::default(),
            by_name: FxHashMap::default(),
            title: None,
            rows: vec![],
            index: FxHashMap::default(),
            calc: vec![],
        };
        t.index_schema();
        t
    }

    /// 只看自己表结构的部分：按 key / 名字找属性、选项序号。
    fn index_schema(&mut self) {
        self.by_key.clear();
        self.by_name.clear();
        self.rt.clear();
        for (i, p) in self.schema.props.iter().enumerate() {
            self.by_key.entry(p.key.clone()).or_insert(i);
            self.by_name
                .entry(p.name.trim().to_lowercase())
                .or_insert(i);
            let mut rt = PropRt::default();
            for (k, o) in p.options.iter().enumerate() {
                rt.opts
                    .by_id
                    .entry(o.id.as_str().into())
                    .or_insert(k as u32);
                rt.opts
                    .by_name
                    .entry(o.name.trim().to_lowercase())
                    .or_insert(k as u32);
                rt.opt_names.push(V::str(o.name.as_str()));
            }
            self.rt.push(rt);
        }
        self.title = self
            .schema
            .props
            .iter()
            .position(|p| p.kind == PropType::Title);
    }

    pub fn find(&self, r: &str) -> Option<usize> {
        let r = r.trim();
        self.by_key
            .get(r)
            .or_else(|| self.by_name.get(&r.to_lowercase()))
            .copied()
    }

    fn make_row(&self, data: RowData, clock: &Clock) -> Row {
        let cells = self
            .schema
            .props
            .iter()
            .zip(&self.rt)
            .map(|(p, rt)| {
                if p.is_computed() {
                    Cell::Empty
                } else {
                    cell_of(p.kind, data.props.get(&p.key), &data, &rt.opts, clock)
                }
            })
            .collect();
        Row { data, cells }
    }

    fn rebuild_cells(&mut self, clock: &Clock) {
        let rows = std::mem::take(&mut self.rows);
        self.rows = rows
            .into_iter()
            .map(|r| self.make_row(r.data, clock))
            .collect();
    }

    fn reindex(&mut self) {
        self.index = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r.data.id.as_str().into(), i))
            .collect();
    }

    fn reset_calc(&mut self) {
        let n = self.rows.len();
        self.calc = self
            .schema
            .props
            .iter()
            .map(|p| {
                RefCell::new(if p.is_computed() {
                    vec![None; n]
                } else {
                    vec![]
                })
            })
            .collect();
    }

    pub fn title_of(&self, r: usize) -> &str {
        match self.title.map(|t| &self.rows[r].cells[t]) {
            Some(Cell::Text(s, _)) => s,
            _ => "",
        }
    }
}

type Backlinks = Arc<FxHashMap<Box<str>, Vec<u32>>>;

/// 计算引擎。不是线程安全的（缓存用 RefCell），服务器用时放进 Mutex。
pub struct Engine {
    pub(crate) tables: Vec<Table>,
    pub(crate) by_id: FxHashMap<String, usize>,
    pub(crate) people: FxHashMap<String, String>,
    pub(crate) people_by_name: FxHashMap<String, String>,
    pub(crate) clock: Clock,
    backlinks: RefCell<FxHashMap<(usize, usize), Backlinks>>,
    truncated: Flag<bool>,
}

/// 改类型时整列的转换结果。
#[derive(Clone, Debug, PartialEq)]
pub struct Conversion {
    /// 每一行转好的值（null 是清空），只列有值的行
    pub values: Vec<(String, Value)>,
    /// 新的属性（转的时候新加的选项已经放进去）
    pub prop: Prop,
    pub new_options: Vec<SelectOption>,
}

struct RowCtx<'a> {
    eng: &'a Engine,
    t: usize,
    r: usize,
    hops: u8,
}

impl Ctx for RowCtx<'_> {
    fn prop(&self, idx: usize) -> V {
        self.eng.fval(self.t, self.r, idx, self.hops)
    }
    fn clock(&self) -> &Clock {
        &self.eng.clock
    }
}

impl Engine {
    pub fn new(schema: Schema) -> Engine {
        let mut e = Engine {
            tables: vec![Table::new(String::new(), schema)],
            by_id: FxHashMap::default(),
            people: FxHashMap::default(),
            people_by_name: FxHashMap::default(),
            clock: Clock::default(),
            backlinks: RefCell::new(FxHashMap::default()),
            truncated: Flag::new(false),
        };
        e.relink();
        e
    }

    /// 主库自己的 id（关联指向自己时认得出来）。
    pub fn set_id(&mut self, id: &str) {
        if let Some(old) = self
            .by_id
            .iter()
            .find(|(_, v)| **v == 0)
            .map(|(k, _)| k.clone())
        {
            self.by_id.remove(&old);
        }
        self.tables[0].id = id.to_string();
        if !id.is_empty() {
            self.by_id.insert(id.to_string(), 0);
        }
        self.relink();
    }

    pub fn id(&self) -> &str {
        &self.tables[0].id
    }

    pub fn schema(&self) -> &Schema {
        &self.tables[0].schema
    }

    pub fn clock(&self) -> Clock {
        self.clock
    }

    /// 人和名字（规范化时认名字，显示时写名字）。
    pub fn set_people(&mut self, people: Vec<Person>) {
        self.people.clear();
        self.people_by_name.clear();
        for p in people {
            self.people_by_name
                .entry(p.name.trim().to_lowercase())
                .or_insert_with(|| p.id.clone());
            self.people.insert(p.id, p.name);
        }
        self.clear_where(|t, p| {
            matches!(t.schema.props[p].kind, PropType::Formula | PropType::Rollup)
        });
    }

    /// 改时钟：到了下一分钟，用了 now() / today() 的公式重算；换了时区全部重算。
    pub fn set_clock(&mut self, clock: Clock) {
        let old = self.clock;
        self.clock = clock;
        if old.tz != clock.tz {
            for t in &mut self.tables {
                t.rebuild_cells(&clock);
                t.reset_calc();
            }
            self.backlinks.borrow_mut().clear();
        } else if old.local_min(old.now_ms) != clock.local_min(clock.now_ms) {
            self.clear_where(|t, p| t.rt[p].uses_clock);
        }
    }

    /// 换掉主库的所有行（按给的顺序）。
    pub fn load_rows(&mut self, rows: Vec<RowData>) {
        let clock = self.clock;
        let t = &mut self.tables[0];
        t.rows = rows.into_iter().map(|r| t.make_row(r, &clock)).collect();
        t.reindex();
        self.after_rows_changed(0);
        self.tables[0].reset_calc();
    }

    /// 关联、汇总要用到的别的库（再给一次是换掉）。
    pub fn load_related(&mut self, db_id: &str, schema: Schema, rows: Vec<RowData>) {
        if db_id.is_empty() || db_id == self.tables[0].id {
            return;
        }
        let clock = self.clock;
        let idx = match self.by_id.get(db_id) {
            Some(&i) => {
                self.tables[i].schema = schema;
                self.tables[i].index_schema();
                i
            }
            None => {
                self.tables.push(Table::new(db_id.to_string(), schema));
                self.by_id.insert(db_id.to_string(), self.tables.len() - 1);
                self.tables.len() - 1
            }
        };
        let t = &mut self.tables[idx];
        t.rows = rows.into_iter().map(|r| t.make_row(r, &clock)).collect();
        t.reindex();
        self.relink();
    }

    /// 换主库的表结构。
    pub fn set_schema(&mut self, schema: Schema) {
        let clock = self.clock;
        let t = &mut self.tables[0];
        t.schema = schema;
        t.index_schema();
        t.rebuild_cells(&clock);
        // 选项变了：关联到这个库的表的格子也要重建（它们自己的选项不变，不用）
        self.relink();
    }

    /// 增量改动，返回受影响的行 id（改了的行，和通过关联连着它们、算出来的值可能变了的主库行）。
    pub fn apply(&mut self, changes: Vec<Change>) -> Vec<String> {
        let clock = self.clock;
        let mut changed: Vec<String> = vec![];
        let mut linked: FxHashSet<String> = FxHashSet::default();
        // 删的行先从 id 表里去掉，最后一次性挪走（一批删很多行时不用每删一行重排一次）
        let mut dead: FxHashSet<usize> = FxHashSet::default();
        let self_links = self.self_link_props();
        for c in changes {
            let t = &mut self.tables[0];
            let at = t.index.get(c.row_id.as_str()).copied();
            if let Some(i) = at {
                for &p in &self_links {
                    if let Cell::Ids(ids) = &t.rows[i].cells[p] {
                        linked.extend(ids.iter().map(|x| x.to_string()));
                    }
                }
            }
            if c.deleted {
                if let Some(i) = at {
                    t.index.remove(c.row_id.as_str());
                    dead.insert(i);
                }
            } else if let Some(mut row) = c.row {
                row.id = c.row_id.clone();
                let new = t.make_row(row, &clock);
                match at {
                    Some(i) => t.rows[i] = new,
                    None => {
                        t.rows.push(new);
                        t.index.insert(c.row_id.as_str().into(), t.rows.len() - 1);
                    }
                }
            } else if let Some(set) = c.props {
                let mut data = match at {
                    Some(i) => t.rows[i].data.clone(),
                    None => RowData {
                        id: c.row_id.clone(),
                        ..Default::default()
                    },
                };
                for (k, v) in set {
                    let blank = v.is_null()
                        || v.as_str() == Some("")
                        || v.as_array().is_some_and(|a| a.is_empty());
                    if blank {
                        data.props.remove(&k);
                    } else {
                        data.props.insert(k, v);
                    }
                }
                let new = t.make_row(data, &clock);
                match at {
                    Some(i) => t.rows[i] = new,
                    None => {
                        t.rows.push(new);
                        t.index.insert(c.row_id.as_str().into(), t.rows.len() - 1);
                    }
                }
            } else {
                continue;
            }
            let t = &self.tables[0];
            if let Some(&i) = t.index.get(c.row_id.as_str()) {
                for &p in &self_links {
                    if let Cell::Ids(ids) = &t.rows[i].cells[p] {
                        linked.extend(ids.iter().map(|x| x.to_string()));
                    }
                }
            }
            changed.push(c.row_id);
        }
        if changed.is_empty() {
            return changed;
        }
        let deleted = !dead.is_empty();
        if deleted {
            let t = &mut self.tables[0];
            let mut i = 0;
            t.rows.retain(|_| {
                i += 1;
                !dead.contains(&(i - 1))
            });
            t.reindex();
        }
        // 缓存失效
        let t = &mut self.tables[0];
        if deleted {
            t.reset_calc();
        } else {
            let n = t.rows.len();
            for (p, col) in t.calc.iter_mut().enumerate() {
                let col = col.get_mut();
                if t.schema.props[p].is_computed() {
                    col.resize(n, None);
                    for id in &changed {
                        if let Some(&i) = t.index.get(id.as_str()) {
                            col[i] = None;
                        }
                    }
                }
            }
        }
        self.after_rows_changed(0);
        let mut out = changed;
        if self.tables[0].rt.iter().any(|r| r.reads.contains(&0)) {
            // 指着改了的行的那些行（它们的汇总、反向关联读到改了的行）
            for &sp in &self_links {
                let map = self.backlink_map(0, sp);
                for id in &out {
                    for &i in map.get(id.as_str()).into_iter().flatten() {
                        linked.insert(self.tables[0].rows[i as usize].data.id.clone());
                    }
                }
            }
            let have: FxHashSet<&str> = out.iter().map(String::as_str).collect();
            let mut extra: Vec<String> = linked
                .into_iter()
                .filter(|x| {
                    !have.contains(x.as_str()) && self.tables[0].index.contains_key(x.as_str())
                })
                .collect();
            extra.sort();
            out.extend(extra);
        }
        out
    }

    /// 主库里指向自己的（存值的）关联属性。
    fn self_link_props(&self) -> Vec<usize> {
        let t = &self.tables[0];
        (0..t.schema.props.len())
            .filter(|&p| {
                t.schema.props[p].kind == PropType::Relation
                    && !t.schema.props[p].is_mirror()
                    && t.rt[p].target == Some(0)
            })
            .collect()
    }

    /// 某个库的行变了：反查表重建，读到这个库的列整列失效。
    fn after_rows_changed(&mut self, changed: usize) {
        self.backlinks.borrow_mut().retain(|k, _| k.0 != changed);
        self.clear_where(|t, p| t.rt[p].reads.contains(&changed));
    }

    fn clear_where(&mut self, f: impl Fn(&Table, usize) -> bool) {
        for t in &mut self.tables {
            for p in 0..t.schema.props.len() {
                if t.schema.props[p].is_computed() && f(t, p) {
                    let n = t.rows.len();
                    let col = t.calc[p].get_mut();
                    col.clear();
                    col.resize(n, None);
                }
            }
        }
    }

    // ---------- 表结构解析、互相引用 ----------

    fn resolve_db(&self, db: &str, own: usize) -> Option<usize> {
        if db.is_empty() || db == "self" || db == self.tables[own].id {
            return Some(own);
        }
        self.by_id.get(db).copied()
    }

    /// 表结构或者库变了：关联指向、公式、汇总、成环、依赖重新算；缓存全清。
    fn relink(&mut self) {
        let n = self.tables.len();
        for t in 0..n {
            for p in 0..self.tables[t].schema.props.len() {
                let (target, mirror_src, rollup, formula, error) = self.link_prop(t, p);
                let rt = &mut self.tables[t].rt[p];
                rt.target = target;
                rt.mirror_src = mirror_src;
                rt.rollup = rollup;
                rt.formula = formula;
                rt.error = error;
                rt.reads.clear();
                rt.uses_clock = false;
            }
        }
        self.mark_cycles();
        // 依赖：读哪些库、用没用时钟
        let mut memo: FxHashMap<(usize, usize), (FxHashSet<usize>, bool)> = FxHashMap::default();
        for t in 0..n {
            for p in 0..self.tables[t].schema.props.len() {
                let (reads, clock) = self.deps(t, p, &mut memo, 0);
                let rt = &mut self.tables[t].rt[p];
                rt.reads = reads;
                rt.uses_clock = clock;
            }
        }
        for t in &mut self.tables {
            t.reset_calc();
        }
        self.backlinks.borrow_mut().clear();
    }

    #[allow(clippy::type_complexity)]
    fn link_prop(
        &self,
        t: usize,
        p: usize,
    ) -> (
        Option<usize>,
        Option<usize>,
        Option<(usize, Option<usize>, Agg)>,
        Option<Result<Compiled, FormulaError>>,
        Option<SchemaError>,
    ) {
        let tb = &self.tables[t];
        let prop = &tb.schema.props[p];
        let err = |msg: String, at: Option<usize>| {
            Some(SchemaError {
                key: prop.key.clone(),
                msg,
                at,
            })
        };
        match prop.kind {
            PropType::Relation => {
                let db = prop
                    .relation
                    .as_ref()
                    .map(|r| r.db_id.as_str())
                    .unwrap_or("");
                let target = self.resolve_db(db, t);
                let mut mirror_src = None;
                if let (Some(m), Some(tt)) = (
                    prop.relation
                        .as_ref()
                        .and_then(|r| r.mirror_of.as_deref())
                        .filter(|m| !m.is_empty()),
                    target,
                ) {
                    mirror_src = self.tables[tt].by_key.get(m).copied().filter(|&q| {
                        let q = &self.tables[tt].schema.props[q];
                        q.kind == PropType::Relation && !q.is_mirror()
                    });
                }
                (target, mirror_src, None, None, None)
            }
            PropType::Rollup => {
                let Some(conf) = &prop.rollup else {
                    return (
                        None,
                        None,
                        None,
                        None,
                        err("汇总还没设置要沿哪个关联、取哪个属性".into(), None),
                    );
                };
                let Some(rel) = tb
                    .by_key
                    .get(&conf.relation)
                    .copied()
                    .filter(|&q| tb.schema.props[q].kind == PropType::Relation)
                else {
                    return (
                        None,
                        None,
                        None,
                        None,
                        err("汇总用的关联属性不存在了".into(), None),
                    );
                };
                let relp = &tb.schema.props[rel];
                let db = relp
                    .relation
                    .as_ref()
                    .map(|r| r.db_id.as_str())
                    .unwrap_or("");
                let target = self.resolve_db(db, t);
                let agg = Agg::parse(&conf.func).unwrap_or(Agg::Count);
                let tp = target.and_then(|tt| self.tables[tt].by_key.get(&conf.prop).copied());
                let e = if target.is_none() {
                    None // 对方库还没加载：先空着
                } else if tp.is_none() && agg != Agg::Count {
                    err(format!("关联的库里没有属性「{}」了", conf.prop), None)
                } else {
                    None
                };
                (target, None, Some((rel, tp, agg)), None, e)
            }
            PropType::Formula => {
                let src = prop.formula.as_deref().unwrap_or("");
                if src.trim().is_empty() {
                    return (None, None, None, None, None);
                }
                let c = self.compile(t, src);
                let e = c.as_ref().err().map(|e| SchemaError {
                    key: prop.key.clone(),
                    msg: e.describe(src),
                    at: Some(e.utf16_pos(src)),
                });
                (None, None, None, Some(c), e)
            }
            _ => (None, None, None, None, None),
        }
    }

    fn compile(&self, t: usize, src: &str) -> Result<Compiled, FormulaError> {
        let tb = &self.tables[t];
        formula::parse(src, &|name: &str| {
            tb.by_name
                .get(&name.trim().to_lowercase())
                .copied()
                .ok_or_else(|| {
                    let names: Vec<&str> =
                        tb.schema.props.iter().map(|p| p.name.as_str()).collect();
                    format!("没有属性「{name}」，现在有：{}", names.join("、"))
                })
        })
    }

    /// 一个算出来的属性直接依赖的别的算出来的属性（同库或别的库）。
    fn edges(&self, t: usize, p: usize) -> Vec<(usize, usize)> {
        let tb = &self.tables[t];
        let rt = &tb.rt[p];
        let computed = |tt: usize, q: usize| self.tables[tt].schema.props[q].kind.is_calculated();
        let mut out = vec![];
        if let Some(Ok(c)) = &rt.formula {
            out.extend(c.refs.iter().filter(|&&q| computed(t, q)).map(|&q| (t, q)));
        }
        if let (Some((_, Some(tp), _)), Some(tt)) = (rt.rollup, rt.target)
            && computed(tt, tp)
        {
            out.push((tt, tp));
        }
        out
    }

    /// 公式、汇总互相引用成环的都标成错的（值是空的）。
    fn mark_cycles(&mut self) {
        let n = self.tables.len();
        let mut state: FxHashMap<(usize, usize), u8> = FxHashMap::default();
        let mut bad: FxHashSet<(usize, usize)> = FxHashSet::default();
        fn visit(
            e: &Engine,
            node: (usize, usize),
            state: &mut FxHashMap<(usize, usize), u8>,
            stack: &mut Vec<(usize, usize)>,
            bad: &mut FxHashSet<(usize, usize)>,
        ) {
            match state.get(&node) {
                Some(2) => return,
                Some(1) => {
                    if let Some(i) = stack.iter().position(|x| *x == node) {
                        bad.extend(stack[i..].iter().copied());
                    }
                    return;
                }
                _ => {}
            }
            state.insert(node, 1);
            stack.push(node);
            for next in e.edges(node.0, node.1) {
                visit(e, next, state, stack, bad);
            }
            stack.pop();
            state.insert(node, 2);
        }
        for t in 0..n {
            for p in 0..self.tables[t].schema.props.len() {
                visit(self, (t, p), &mut state, &mut vec![], &mut bad);
            }
        }
        for (t, p) in bad {
            let tb = &mut self.tables[t];
            let prop = &tb.schema.props[p];
            let selfref = matches!(&tb.rt[p].formula, Some(Ok(c)) if c.refs.contains(&p));
            let msg = if selfref {
                "公式不能用到它自己".to_string()
            } else {
                format!("「{}」和别的公式 / 汇总互相引用成了环", prop.name)
            };
            let key = prop.key.clone();
            let rt = &mut tb.rt[p];
            if rt.formula.is_some() {
                rt.formula = Some(Err(FormulaError::new(msg.clone(), 0)));
            }
            if rt.rollup.is_some() {
                rt.rollup = None;
            }
            rt.error = Some(SchemaError { key, msg, at: None });
        }
    }

    fn deps(
        &self,
        t: usize,
        p: usize,
        memo: &mut FxHashMap<(usize, usize), (FxHashSet<usize>, bool)>,
        depth: u8,
    ) -> (FxHashSet<usize>, bool) {
        if let Some(x) = memo.get(&(t, p)) {
            return x.clone();
        }
        let mut reads = FxHashSet::default();
        let mut clock = false;
        if depth > 32 {
            return (reads, clock);
        }
        let tb = &self.tables[t];
        let prop = &tb.schema.props[p];
        let rt = &tb.rt[p];
        match prop.kind {
            PropType::Relation => {
                if let Some(tt) = rt.target {
                    reads.insert(tt);
                }
            }
            PropType::Formula => {
                if let Some(Ok(c)) = &rt.formula {
                    clock |= c.uses_clock;
                    for &q in &c.refs {
                        let qp = &tb.schema.props[q];
                        if qp.kind == PropType::Relation || qp.kind.is_calculated() {
                            let (r, c2) = self.deps(t, q, memo, depth + 1);
                            reads.extend(r);
                            clock |= c2;
                        }
                    }
                }
            }
            PropType::Rollup => {
                if let (Some((rel, tp, _)), Some(tt)) = (rt.rollup, rt.target) {
                    reads.insert(tt);
                    let (r, _) = self.deps(t, rel, memo, depth + 1);
                    reads.extend(r);
                    if let Some(tp) = tp {
                        let q = &self.tables[tt].schema.props[tp];
                        if q.kind == PropType::Relation || q.kind.is_calculated() {
                            let (r, c2) = self.deps(tt, tp, memo, depth + 1);
                            reads.extend(r);
                            clock |= c2;
                        }
                    }
                }
            }
            _ => {}
        }
        memo.insert((t, p), (reads.clone(), clock));
        (reads, clock)
    }

    /// 表结构里有问题的属性（公式写错、成环、汇总的关联不在了）。
    pub fn schema_errors(&self) -> Vec<SchemaError> {
        self.tables[0]
            .rt
            .iter()
            .filter_map(|r| r.error.clone())
            .collect()
    }

    // ---------- 取值、算 ----------

    /// 一个属性在公式、汇总里的样子。
    pub(crate) fn fval(&self, t: usize, r: usize, p: usize, hops: u8) -> V {
        let tb = &self.tables[t];
        let prop = &tb.schema.props[p];
        let rt = &tb.rt[p];
        if prop.is_computed() {
            let v = self.calc(t, r, p, hops);
            if prop.is_mirror() {
                return self.titles_v(rt.target, &v);
            }
            return v;
        }
        use PropType::*;
        let cell = &tb.rows[r].cells[p];
        match prop.kind {
            Title | Text | Url | Email | Phone => match cell {
                Cell::Text(s, _) => V::str(&**s),
                _ => V::str(""),
            },
            Number | UniqueId => match cell {
                Cell::Num(x) => V::Num(*x),
                _ => V::Null,
            },
            Select | Status => match cell {
                Cell::Opt(i) => rt.opt_names[*i as usize].clone(),
                _ => V::str(""),
            },
            MultiSelect => match cell {
                Cell::Opts(v) => V::list(
                    v.iter()
                        .map(|i| rt.opt_names[*i as usize].clone())
                        .collect(),
                ),
                _ => V::list(vec![]),
            },
            Date => match cell {
                Cell::Date(s, e) => V::Date(*s, *e),
                _ => V::Null,
            },
            CreatedTime | UpdatedTime => match cell {
                Cell::Ms(ms) => V::date(DateTime {
                    min: self.clock.local_min(*ms),
                    time: true,
                }),
                _ => V::Null,
            },
            Checkbox => V::Bool(matches!(cell, Cell::Bool(true))),
            Person => match cell {
                Cell::Ids(v) => V::list(v.iter().map(|x| V::str(self.person(x))).collect()),
                _ => V::list(vec![]),
            },
            CreatedBy | UpdatedBy => match cell {
                Cell::Ids(v) => V::str(v.first().map_or(String::new(), |x| self.person(x))),
                _ => V::str(""),
            },
            Relation => match cell {
                Cell::Ids(v) => match rt.target {
                    Some(tt) => {
                        let ot = &self.tables[tt];
                        V::list(
                            v.iter()
                                .filter_map(|x| ot.index.get(x).map(|&i| V::str(ot.title_of(i))))
                                .collect(),
                        )
                    }
                    None => V::list(vec![]),
                },
                _ => V::list(vec![]),
            },
            Files => match cell {
                Cell::Files(v) => V::list(v.iter().map(|(n, _)| V::str(&**n)).collect()),
                _ => V::list(vec![]),
            },
            _ => V::Null,
        }
    }

    fn titles_v(&self, target: Option<usize>, ids: &V) -> V {
        let Some(tt) = target else {
            return V::list(vec![]);
        };
        let ot = &self.tables[tt];
        V::list(
            ids.items()
                .iter()
                .filter_map(|x| match x {
                    V::Str(s) => ot.index.get(&**s).map(|&i| V::str(ot.title_of(i))),
                    _ => None,
                })
                .collect(),
        )
    }

    pub(crate) fn person(&self, id: &str) -> String {
        self.people
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_string())
    }

    /// 算出来的属性的值（缓存）：公式、汇总是算出来的样子，反向关联是对方行 id 的列表。
    pub(crate) fn calc(&self, t: usize, r: usize, p: usize, hops: u8) -> V {
        if let Some(Some(v)) = self.tables[t].calc[p].borrow().get(r) {
            return v.clone();
        }
        let before = self.truncated.replace(false);
        let v = self.compute(t, r, p, hops);
        let cut = self.truncated.get();
        if !cut && let Some(slot) = self.tables[t].calc[p].borrow_mut().get_mut(r) {
            *slot = Some(v.clone());
        }
        self.truncated.set(before || cut);
        v
    }

    fn compute(&self, t: usize, r: usize, p: usize, hops: u8) -> V {
        let tb = &self.tables[t];
        let prop = &tb.schema.props[p];
        let rt = &tb.rt[p];
        if prop.is_mirror() {
            let Some(src) = rt.target else { return V::Null };
            let rows = self.mirror_rows(t, r, p);
            if rows.is_empty() {
                return V::Null;
            }
            let st = &self.tables[src];
            return V::list(
                rows.iter()
                    .map(|&i| V::str(st.rows[i].data.id.as_str()))
                    .collect(),
            );
        }
        match prop.kind {
            PropType::Formula => match &rt.formula {
                Some(Ok(c)) => formula::run(
                    &c.expr,
                    &RowCtx {
                        eng: self,
                        t,
                        r,
                        hops,
                    },
                ),
                _ => V::Null,
            },
            PropType::Rollup => {
                let (Some((rel, tp, agg)), Some(tt)) = (rt.rollup, rt.target) else {
                    return V::Null;
                };
                if hops >= MAX_HOPS {
                    self.truncated.set(true);
                    return V::Null;
                }
                let rows = self.links(t, r, rel);
                let vals: Vec<V> = rows
                    .iter()
                    .map(|&i| tp.map_or(V::Null, |q| self.fval(tt, i, q, hops + 1)))
                    .collect();
                formula::finish(aggregate(agg, &vals, rows.len()))
            }
            _ => V::Null,
        }
    }

    /// 关联属性（存的或反向的）在这一行指着对方库的哪些行。
    pub(crate) fn links(&self, t: usize, r: usize, p: usize) -> Vec<usize> {
        let tb = &self.tables[t];
        let prop = &tb.schema.props[p];
        if prop.is_mirror() {
            return self.mirror_rows(t, r, p);
        }
        let Some(tt) = tb.rt[p].target else {
            return vec![];
        };
        let ot = &self.tables[tt];
        match &tb.rows[r].cells[p] {
            Cell::Ids(v) => v.iter().filter_map(|x| ot.index.get(x).copied()).collect(),
            _ => vec![],
        }
    }

    /// 反查表：库 `src` 的关联属性 `sp` 指着的行 id → 指着它的那些行（按顺序）。
    fn backlink_map(&self, src: usize, sp: usize) -> Backlinks {
        if let Some(m) = self.backlinks.borrow().get(&(src, sp)) {
            return m.clone();
        }
        let mut m: FxHashMap<Box<str>, Vec<u32>> = FxHashMap::default();
        for (i, row) in self.tables[src].rows.iter().enumerate() {
            if let Cell::Ids(ids) = &row.cells[sp] {
                for id in ids {
                    let v = m.entry(id.clone()).or_default();
                    if v.last() != Some(&(i as u32)) {
                        v.push(i as u32);
                    }
                }
            }
        }
        let m = Arc::new(m);
        self.backlinks.borrow_mut().insert((src, sp), m.clone());
        m
    }

    /// 反向关联：对方库里哪些行的那个关联指着这一行（按对方的顺序）。
    pub(crate) fn mirror_rows(&self, t: usize, r: usize, p: usize) -> Vec<usize> {
        let rt = &self.tables[t].rt[p];
        let (Some(src), Some(sp)) = (rt.target, rt.mirror_src) else {
            return vec![];
        };
        let map = self.backlink_map(src, sp);
        let id = self.tables[t].rows[r].data.id.as_str();
        map.get(id)
            .map_or(vec![], |v| v.iter().map(|&i| i as usize).collect())
    }

    /// 一格在主库里存的样子（公式、汇总、反向关联、自动的属性是算出来的样子）。
    pub(crate) fn value_at(&self, t: usize, r: usize, p: usize) -> Value {
        let tb = &self.tables[t];
        let prop = &tb.schema.props[p];
        let row = &tb.rows[r];
        if prop.is_computed() {
            return v_json(&self.calc(t, r, p, 0));
        }
        match prop.kind {
            PropType::CreatedTime => row.data.created_at.map_or(Value::Null, crate::num::to_json),
            PropType::UpdatedTime => row
                .data
                .updated_at
                .or(row.data.created_at)
                .map_or(Value::Null, crate::num::to_json),
            PropType::CreatedBy => row
                .data
                .created_by
                .clone()
                .map_or(Value::Null, Value::String),
            PropType::UpdatedBy => row
                .data
                .updated_by
                .clone()
                .or(row.data.created_by.clone())
                .map_or(Value::Null, Value::String),
            PropType::UniqueId => match &row.cells[p] {
                Cell::Num(x) => crate::num::to_json(*x),
                _ => Value::Null,
            },
            PropType::Button => Value::Null,
            _ => row
                .data
                .props
                .get(&prop.key)
                .cloned()
                .unwrap_or(Value::Null),
        }
    }

    // ---------- 对外 ----------

    pub fn row_ids(&self) -> Vec<String> {
        self.tables[0]
            .rows
            .iter()
            .map(|r| r.data.id.clone())
            .collect()
    }

    pub fn row(&self, id: &str) -> Option<&RowData> {
        let t = &self.tables[0];
        t.index.get(id).map(|&i| &t.rows[i].data)
    }

    pub fn len(&self) -> usize {
        self.tables[0].rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tables[0].rows.is_empty()
    }

    /// 按 key 或名字找主库的属性。
    pub fn find_prop(&self, r: &str) -> Option<&Prop> {
        let t = &self.tables[0];
        t.find(r).map(|i| &t.schema.props[i])
    }

    /// 一行算出来的值：公式、汇总、反向关联（对方行 id 的列表），还有创建 / 编辑时间（`{start, time}`）、创建 / 编辑人、自动编号。
    pub fn computed(&self, row_id: &str) -> Map<String, Value> {
        let t = &self.tables[0];
        let mut out = Map::new();
        let Some(&r) = t.index.get(row_id) else {
            return out;
        };
        for (p, prop) in t.schema.props.iter().enumerate() {
            let v = if prop.is_computed() {
                v_json(&self.calc(0, r, p, 0))
            } else if matches!(prop.kind, PropType::CreatedTime | PropType::UpdatedTime) {
                match &t.rows[r].cells[p] {
                    Cell::Ms(ms) => crate::normalize::date_value(
                        DateTime {
                            min: self.clock.local_min(*ms),
                            time: true,
                        },
                        None,
                    ),
                    _ => Value::Null,
                }
            } else if prop.kind.is_auto() {
                self.value_at(0, r, p)
            } else {
                continue;
            };
            out.insert(prop.key.clone(), v);
        }
        out
    }

    /// 一格的值（存的或算出来的）。
    pub fn value(&self, row_id: &str, key: &str) -> Value {
        let t = &self.tables[0];
        match (t.index.get(row_id), t.find(key)) {
            (Some(&r), Some(p)) => self.value_at(0, r, p),
            _ => Value::Null,
        }
    }

    /// 给人看的文字。`key` 是主库的属性 key（也认名字）。
    pub fn display(&self, key: &str, v: &Value) -> String {
        match self.find_prop(key) {
            Some(p) => crate::display::display(p, v, self),
            None => crate::display::display(&Prop::new("", "", PropType::Text), v, self),
        }
    }

    /// 一行每一格给人看的文字：{ key: 文字 }（空的不列）。
    pub fn display_row(&self, row_id: &str) -> Map<String, Value> {
        let t = &self.tables[0];
        let mut out = Map::new();
        let Some(&r) = t.index.get(row_id) else {
            return out;
        };
        for (p, prop) in t.schema.props.iter().enumerate() {
            let s = crate::display::display(prop, &self.value_at(0, r, p), self);
            if !s.is_empty() {
                out.insert(prop.key.clone(), Value::String(s));
            }
        }
        out
    }

    /// 检查一个公式：语法、属性名；给了 `key`（这个公式属于哪个属性）还查用没用到自己、成没成环。
    pub fn check_formula(&self, src: &str, key: Option<&str>) -> Result<(), FormulaError> {
        let c = self.compile(0, src)?;
        let Some(me) = key.and_then(|k| self.tables[0].by_key.get(k).copied()) else {
            return Ok(());
        };
        if c.refs.contains(&me) {
            return Err(FormulaError::new("公式不能用到它自己", 0));
        }
        // 从它用到的属性往下走，走回自己就是环
        let mut seen: FxHashSet<(usize, usize)> = FxHashSet::default();
        let mut stack: Vec<(usize, usize)> = c.refs.iter().map(|&q| (0, q)).collect();
        while let Some(n) = stack.pop() {
            if n == (0, me) {
                let name = &self.tables[0].schema.props[me].name;
                return Err(FormulaError::new(
                    format!("「{name}」和别的公式 / 汇总互相引用成了环"),
                    0,
                ));
            }
            if seen.insert(n) {
                stack.extend(self.edges(n.0, n.1));
            }
        }
        Ok(())
    }

    /// 给的值 → 存的样子（不新加选项：没有的选项报错）。
    pub fn normalize(&self, key: &str, input: &Value) -> Result<Value, DbError> {
        self.normalize_with(key, input, false).map(|n| n.value)
    }

    /// 给的值 → 存的样子；`create` 时单选、多选没有的名字新加选项（在 `new_options` 里，调用方写回表结构）。
    pub fn normalize_with(
        &self,
        key: &str,
        input: &Value,
        create: bool,
    ) -> Result<Normalized, DbError> {
        let p = self
            .find_prop(key)
            .ok_or_else(|| DbError::new(format!("没有属性「{key}」")))?;
        normalize(p, input, self, create)
    }

    /// 一整行（键是 key 或名字）→ 存的样子 { key: 值 }。新加的选项按属性 key 分开给。
    #[allow(clippy::type_complexity)]
    pub fn normalize_row(
        &self,
        given: &Map<String, Value>,
        create: bool,
    ) -> Result<(Map<String, Value>, Vec<(String, Vec<SelectOption>)>), DbError> {
        let mut out = Map::new();
        let mut added: Vec<(String, Vec<SelectOption>)> = vec![];
        for (k, v) in given {
            let mut p = self
                .find_prop(k)
                .ok_or_else(|| {
                    let names: Vec<&str> = self
                        .schema()
                        .props
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect();
                    DbError::new(format!("没有属性「{k}」，现在有：{}", names.join("、")))
                })?
                .clone();
            if let Some((_, opts)) = added.iter().find(|(key, _)| *key == p.key) {
                p.options.extend(opts.iter().cloned());
            }
            let n = normalize(&p, v, self, create)?;
            if !n.new_options.is_empty() {
                match added.iter_mut().find(|(key, _)| *key == p.key) {
                    Some((_, o)) => o.extend(n.new_options),
                    None => added.push((p.key.clone(), n.new_options)),
                }
            }
            out.insert(p.key.clone(), n.value);
        }
        Ok((out, added))
    }

    /// 改类型（或者改了选项）时整列怎么转：`key` 现在的属性，`new` 改成的样子。
    pub fn convert(&self, key: &str, new: &Prop) -> Result<Conversion, DbError> {
        let t = &self.tables[0];
        let p = t
            .find(key)
            .ok_or_else(|| DbError::new(format!("没有属性「{key}」")))?;
        let old = &t.schema.props[p];
        let mut ids = vec![];
        let mut vals = vec![];
        for r in 0..t.rows.len() {
            let v = self.value_at(0, r, p);
            if !v.is_null() {
                ids.push(t.rows[r].data.id.clone());
                vals.push(v);
            }
        }
        let (out, prop, new_options) = crate::convert::convert_column(old, new, &vals, self);
        Ok(Conversion {
            values: ids.into_iter().zip(out).collect(),
            prop,
            new_options,
        })
    }
}

/// 公式里的值 → 存的样子。
pub(crate) fn v_json(v: &V) -> Value {
    match v {
        V::Null => Value::Null,
        V::Num(x) => crate::num::to_json(*x),
        V::Str(s) => Value::String(s.to_string()),
        V::Bool(b) => Value::Bool(*b),
        V::Date(s, e) => crate::normalize::date_value(*s, *e),
        V::List(l) => Value::Array(l.iter().map(v_json).collect()),
    }
}

impl Lookup for Engine {
    fn clock(&self) -> &Clock {
        &self.clock
    }
    fn person_name(&self, id: &str) -> Option<String> {
        self.people.get(id).cloned()
    }
    fn person_by_name(&self, name: &str) -> Option<String> {
        self.people_by_name
            .get(&name.trim().to_lowercase())
            .cloned()
    }
    fn has_table(&self, db: &str) -> bool {
        self.resolve_db(db, 0).is_some()
    }
    fn row_title(&self, db: &str, id: &str) -> Option<String> {
        let t = &self.tables[self.resolve_db(db, 0)?];
        t.index.get(id).map(|&i| t.title_of(i).to_string())
    }
    fn row_by_title(&self, db: &str, title: &str) -> Option<String> {
        let t = &self.tables[self.resolve_db(db, 0)?];
        let low = title.trim().to_lowercase();
        let ti = t.title?;
        t.rows
            .iter()
            .find(|r| r.cells[ti].lower() == Some(low.as_str()))
            .map(|r| r.data.id.clone())
    }
}
