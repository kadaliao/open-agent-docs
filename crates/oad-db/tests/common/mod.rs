//! 测试共用：固定的时钟、建表结构和行的小工具。
#![allow(dead_code)]
use oad_db::schema::{RelationConf, RollupConf};
use oad_db::{Clock, Engine, Person, Prop, PropType, RowData, Schema, SelectOption};
use serde_json::{Map, Value};

/// 2026-11-15（周日）10:30 北京时间
pub const NOW: i64 = 1_794_709_800_000;

pub fn clock() -> Clock {
    Clock::new(NOW, 480)
}

pub fn p(key: &str, name: &str, kind: PropType) -> Prop {
    Prop::new(key, name, kind)
}

pub fn options(names: &[&str]) -> Vec<SelectOption> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| SelectOption {
            id: format!("o{}", i + 1),
            name: n.to_string(),
            color: "blue".into(),
            group: None,
            extra: Map::new(),
        })
        .collect()
}

pub fn with_options(mut prop: Prop, names: &[&str]) -> Prop {
    prop.options = options(names);
    prop
}

pub fn status(key: &str, name: &str) -> Prop {
    let mut s = p(key, name, PropType::Status);
    s.options = vec![
        SelectOption {
            id: "s1".into(),
            name: "未开始".into(),
            color: "gray".into(),
            group: Some("todo".into()),
            extra: Map::new(),
        },
        SelectOption {
            id: "s2".into(),
            name: "进行中".into(),
            color: "blue".into(),
            group: Some("doing".into()),
            extra: Map::new(),
        },
        SelectOption {
            id: "s3".into(),
            name: "已完成".into(),
            color: "green".into(),
            group: Some("done".into()),
            extra: Map::new(),
        },
    ];
    s
}

pub fn formula(key: &str, name: &str, src: &str) -> Prop {
    let mut f = p(key, name, PropType::Formula);
    f.formula = Some(src.into());
    f
}

pub fn relation(key: &str, name: &str, db: &str) -> Prop {
    let mut r = p(key, name, PropType::Relation);
    r.relation = Some(RelationConf {
        db_id: db.into(),
        mirror_of: None,
        extra: Map::new(),
    });
    r
}

pub fn mirror(key: &str, name: &str, db: &str, of: &str) -> Prop {
    let mut r = p(key, name, PropType::Relation);
    r.relation = Some(RelationConf {
        db_id: db.into(),
        mirror_of: Some(of.into()),
        extra: Map::new(),
    });
    r
}

pub fn rollup(key: &str, name: &str, rel: &str, target: &str, f: &str) -> Prop {
    let mut r = p(key, name, PropType::Rollup);
    r.rollup = Some(RollupConf {
        relation: rel.into(),
        prop: target.into(),
        func: f.into(),
        extra: Map::new(),
    });
    r
}

pub fn schema(props: Vec<Prop>) -> Schema {
    Schema {
        props,
        ..Default::default()
    }
}

pub fn row(id: &str, props: Value) -> RowData {
    RowData {
        id: id.into(),
        props: props.as_object().cloned().unwrap_or_default(),
        created_at: Some(NOW as f64),
        created_by: Some("user:1".into()),
        ..Default::default()
    }
}

pub fn people() -> Vec<Person> {
    vec![
        Person {
            id: "user:1".into(),
            name: "张三".into(),
        },
        Person {
            id: "user:2".into(),
            name: "李四".into(),
        },
        Person {
            id: "agent:7".into(),
            name: "Writer".into(),
        },
    ]
}

pub fn engine(s: Schema, rows: Vec<RowData>) -> Engine {
    let mut e = Engine::new(s);
    e.set_id("db1");
    e.set_clock(clock());
    e.set_people(people());
    e.load_rows(rows);
    e
}

pub fn view(v: Value) -> oad_db::View {
    serde_json::from_value(v).unwrap()
}

pub fn ids(e: &Engine, v: Value) -> Vec<String> {
    e.query(&view(v)).unwrap().ids
}

pub fn titles(e: &Engine, v: Value) -> Vec<String> {
    e.query(&view(v))
        .unwrap()
        .ids
        .iter()
        .map(|id| e.value(id, "title").as_str().unwrap_or("").to_string())
        .collect()
}
