//! 数据库的计算引擎：纯逻辑，不碰 IO。服务器直接用；`wasm` feature 编成 WASM 给网页用（packages/db-wasm），两边同一份代码。
//!
//! - [`schema`]：表结构、行、视图、增量改动（存的 / 传的样子）
//! - [`normalize`]：给的值（名字、各种写法的日期…）→ 存的样子；[`display`]：存的样子 → 给人看的文字
//! - [`formula`]：公式（解析、算）；[`aggregate`]：汇总和每列底部的汇总
//! - [`Engine`]：装着一个库的行（和关联到的库），答查询（筛选、排序、分组、汇总行）、算公式 / 汇总 / 反向关联（按行缓存，改动时失效）
//! - [`api`]：同样的接口，进出都是 JSON 字符串（WASM 用它，测试拿它对照）
pub mod aggregate;
pub mod api;
pub mod convert;
pub mod date;
pub mod display;
mod engine;
pub mod formula;
pub mod normalize;
pub mod num;
mod query;
pub mod schema;
mod value;
#[cfg(feature = "wasm")]
pub mod wasm;

pub use aggregate::Agg;
pub use date::Clock;
pub use engine::{Conversion, Engine, SchemaError};
pub use normalize::Normalized;
pub use query::{Group, QueryResult};
pub use schema::{Change, Person, Prop, PropType, RowData, Schema, SelectOption, Sort, View};

use std::fmt;

/// 出错：一句说清楚哪里不对、该怎么写的中文。
#[derive(Clone, Debug, PartialEq)]
pub struct DbError {
    pub msg: String,
    /// 公式出错时，出错处在公式里的位置（UTF-16 下标，和 JS 字符串一致）
    pub at: Option<usize>,
}

impl DbError {
    pub fn new(msg: impl Into<String>) -> DbError {
        DbError {
            msg: msg.into(),
            at: None,
        }
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for DbError {}

/// 规范化、显示时要查的：现在几点、人的名字、关联的库里的行。
pub trait Lookup {
    fn clock(&self) -> &Clock;
    /// 人的名字（`user:1` → 张三）
    fn person_name(&self, id: &str) -> Option<String>;
    /// 按名字找人（不分大小写）
    fn person_by_name(&self, name: &str) -> Option<String>;
    /// 那个库加载了没有（没加载就不核对行在不在）
    fn has_table(&self, db: &str) -> bool;
    /// 行的标题；没有这一行是 None
    fn row_title(&self, db: &str, id: &str) -> Option<String>;
    /// 按标题找行（不分大小写，同名的取第一行）
    fn row_by_title(&self, db: &str, title: &str) -> Option<String>;
}

/// 什么都查不到的 [`Lookup`]：只有时钟。
pub struct Bare(pub Clock);

impl Lookup for Bare {
    fn clock(&self) -> &Clock {
        &self.0
    }
    fn person_name(&self, _: &str) -> Option<String> {
        None
    }
    fn person_by_name(&self, _: &str) -> Option<String> {
        None
    }
    fn has_table(&self, _: &str) -> bool {
        false
    }
    fn row_title(&self, _: &str, _: &str) -> Option<String> {
        None
    }
    fn row_by_title(&self, _: &str, _: &str) -> Option<String> {
        None
    }
}
