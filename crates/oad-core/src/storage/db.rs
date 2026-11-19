//! 存储代码和数据库驱动之间的一层薄封装：同一份 SQL、同一份代码跑在 SQLite（rusqlite）或 Postgres（postgres）上。
//! 用法照着 rusqlite 来：[`Conn::prepare_cached`]、`execute`、`query_row`、`query_map`、[`OptionalExt::optional`]、
//! [`Conn::transaction`]。参数写成 [`params!`]（借用，不复制字符串），读列用 [`Row::get`]。
//! SQL 按 SQLite 的写法写；Postgres 准备语句时换成它的写法（[`pg_sql`]），语句按连接缓存。
use crate::Error;
use postgres::types::{IsNull, ToSql, Type, to_sql_checked};
use std::ops::{Deref, DerefMut};

/// 一个参数。
#[derive(Debug, Clone, Copy)]
pub enum Value<'a> {
    Null,
    Int(i64),
    Text(&'a str),
    Blob(&'a [u8]),
}

pub trait ToValue {
    fn to_value(&self) -> Value<'_>;
}

impl<T: ToValue + ?Sized> ToValue for &T {
    fn to_value(&self) -> Value<'_> {
        (**self).to_value()
    }
}

impl<T: ToValue> ToValue for Option<T> {
    fn to_value(&self) -> Value<'_> {
        match self {
            Some(v) => v.to_value(),
            None => Value::Null,
        }
    }
}

macro_rules! int_value {
    ($($t:ty),*) => {$(
        impl ToValue for $t {
            fn to_value(&self) -> Value<'_> {
                Value::Int(*self as i64)
            }
        }
    )*};
}
int_value!(i64, i32, u32, bool);

impl ToValue for str {
    fn to_value(&self) -> Value<'_> {
        Value::Text(self)
    }
}

impl ToValue for String {
    fn to_value(&self) -> Value<'_> {
        Value::Text(self)
    }
}

impl ToValue for [u8] {
    fn to_value(&self) -> Value<'_> {
        Value::Blob(self)
    }
}

impl ToValue for Vec<u8> {
    fn to_value(&self) -> Value<'_> {
        Value::Blob(self)
    }
}

/// `params![a, b, c]`：参数列表（借用各个值）。
macro_rules! params {
    ($($x:expr),* $(,)?) => {
        &[$($crate::storage::db::ToValue::to_value(&$x)),*][..]
    };
}
pub(crate) use params;

impl rusqlite::ToSql for Value<'_> {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, ValueRef};
        Ok(ToSqlOutput::Borrowed(match *self {
            Value::Null => ValueRef::Null,
            Value::Int(i) => ValueRef::Integer(i),
            Value::Text(s) => ValueRef::Text(s.as_bytes()),
            Value::Blob(b) => ValueRef::Blob(b),
        }))
    }
}

/// Postgres 要什么类型就按什么类型写（INTEGER 列都是 BIGINT，BLOB 是 BYTEA），NULL 什么类型都行。
impl ToSql for Value<'_> {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut bytes::BytesMut,
    ) -> std::result::Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match *self {
            Value::Null => Ok(IsNull::Yes),
            Value::Int(i) => match *ty {
                Type::INT8 => i.to_sql(ty, out),
                Type::INT4 => i32::try_from(i)?.to_sql(ty, out),
                Type::INT2 => i16::try_from(i)?.to_sql(ty, out),
                Type::BOOL => (i != 0).to_sql(ty, out),
                Type::FLOAT8 => (i as f64).to_sql(ty, out),
                _ => i.to_string().as_str().to_sql(&Type::TEXT, out),
            },
            Value::Text(s) => match *ty {
                Type::BYTEA => s.as_bytes().to_sql(ty, out),
                Type::INT8 => s.parse::<i64>()?.to_sql(ty, out),
                _ => s.to_sql(&Type::TEXT, out),
            },
            Value::Blob(b) => b.to_sql(&Type::BYTEA, out),
        }
    }

    fn accepts(_: &Type) -> bool {
        true
    }

    to_sql_checked!();
}

/// 驱动报的错：没有行、违反约束（唯一、外键、非空……），或者别的。
#[derive(Debug)]
pub enum DbError {
    NoRows,
    Constraint(String),
    Other(String),
}

pub type RowResult<T> = std::result::Result<T, DbError>;

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::NoRows => f.write_str("Query returned no rows"),
            DbError::Constraint(m) | DbError::Other(m) => f.write_str(m),
        }
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        match &e {
            rusqlite::Error::QueryReturnedNoRows => DbError::NoRows,
            rusqlite::Error::SqliteFailure(f, _)
                if f.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                DbError::Constraint(e.to_string())
            }
            _ => DbError::Other(e.to_string()),
        }
    }
}

impl From<postgres::Error> for DbError {
    fn from(e: postgres::Error) -> Self {
        match e.as_db_error() {
            // 23xxx：违反约束
            Some(d) if d.code().code().starts_with("23") => DbError::Constraint(d.to_string()),
            Some(d) => DbError::Other(d.to_string()),
            None => DbError::Other(e.to_string()),
        }
    }
}

impl From<DbError> for Error {
    fn from(e: DbError) -> Self {
        Error::Storage(e.to_string())
    }
}

pub trait OptionalExt<T> {
    /// 没有行算 None。
    fn optional(self) -> RowResult<Option<T>>;
}

impl<T> OptionalExt<T> for RowResult<T> {
    fn optional(self) -> RowResult<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(DbError::NoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Postgres 那边的连接（在一个事务里），由 [`super::pg`] 实现。
pub trait PgClient {
    fn prepare(&mut self, sql: &str) -> RowResult<postgres::Statement>;
    fn query(
        &mut self,
        st: &postgres::Statement,
        params: &[Value],
    ) -> RowResult<Vec<postgres::Row>>;
    fn execute(&mut self, st: &postgres::Statement, params: &[Value]) -> RowResult<u64>;
}

/// 一条连接。
pub enum Conn<'a> {
    Lite(&'a rusqlite::Connection),
    /// Postgres：整个读 / 写已经在一个事务里了。
    Pg(&'a mut (dyn PgClient + 'a)),
}

impl<'c> Conn<'c> {
    pub fn is_pg(&self) -> bool {
        matches!(self, Conn::Pg(_))
    }

    pub fn prepare_cached(&mut self, sql: &str) -> RowResult<Stmt<'_>> {
        match self {
            Conn::Lite(c) => Ok(Stmt::Lite(c.prepare_cached(sql)?)),
            Conn::Pg(c) => {
                let st = c.prepare(sql)?;
                Ok(Stmt::Pg(&mut **c, st))
            }
        }
    }

    pub fn execute(&mut self, sql: &str, params: &[Value]) -> RowResult<usize> {
        self.prepare_cached(sql)?.execute(params)
    }

    pub fn query_row<T, F>(&mut self, sql: &str, params: &[Value], f: F) -> RowResult<T>
    where
        F: FnOnce(&Row) -> RowResult<T>,
    {
        self.prepare_cached(sql)?.query_row(params, f)
    }

    pub fn query_map<T, F>(&mut self, sql: &str, params: &[Value], f: F) -> RowResult<Vec<T>>
    where
        F: FnMut(&Row) -> RowResult<T>,
    {
        self.prepare_cached(sql)?.query_map(params, f)
    }

    /// 开事务。Postgres 上整个读 / 写本来就在一个事务里，这里什么也不做（出错时整个回滚）。
    pub fn transaction(&mut self) -> RowResult<Tx<'_, 'c>> {
        if let Conn::Lite(c) = self {
            c.execute_batch("BEGIN DEFERRED")?;
        }
        Ok(Tx {
            conn: self,
            done: false,
        })
    }
}

/// 事务：没 commit 就丢掉的回滚。当 [`Conn`] 用。
pub struct Tx<'a, 'c> {
    conn: &'a mut Conn<'c>,
    done: bool,
}

impl Tx<'_, '_> {
    pub fn commit(mut self) -> RowResult<()> {
        if let Conn::Lite(c) = self.conn {
            c.execute_batch("COMMIT")?;
        }
        self.done = true;
        Ok(())
    }
}

impl Drop for Tx<'_, '_> {
    fn drop(&mut self) {
        if !self.done
            && let Conn::Lite(c) = self.conn
        {
            let _ = c.execute_batch("ROLLBACK");
        }
    }
}

impl<'c> Deref for Tx<'_, 'c> {
    type Target = Conn<'c>;
    fn deref(&self) -> &Conn<'c> {
        self.conn
    }
}

impl<'c> DerefMut for Tx<'_, 'c> {
    fn deref_mut(&mut self) -> &mut Conn<'c> {
        self.conn
    }
}

/// 准备好的语句。
pub enum Stmt<'a> {
    Lite(rusqlite::CachedStatement<'a>),
    Pg(&'a mut (dyn PgClient + 'a), postgres::Statement),
}

impl Stmt<'_> {
    pub fn execute(&mut self, params: &[Value]) -> RowResult<usize> {
        match self {
            Stmt::Lite(st) => Ok(st.execute(rusqlite::params_from_iter(params))?),
            Stmt::Pg(c, st) => Ok(c.execute(st, params)? as usize),
        }
    }

    /// 第一行；没有行报 [`DbError::NoRows`]。
    pub fn query_row<T, F>(&mut self, params: &[Value], f: F) -> RowResult<T>
    where
        F: FnOnce(&Row) -> RowResult<T>,
    {
        match self {
            Stmt::Lite(st) => {
                let mut rows = st.query(rusqlite::params_from_iter(params))?;
                match rows.next()? {
                    Some(r) => f(&Row::Lite(r)),
                    None => Err(DbError::NoRows),
                }
            }
            Stmt::Pg(c, st) => match c.query(st, params)?.first() {
                Some(r) => f(&Row::Pg(r)),
                None => Err(DbError::NoRows),
            },
        }
    }

    /// 所有行。
    pub fn query_map<T, F>(&mut self, params: &[Value], mut f: F) -> RowResult<Vec<T>>
    where
        F: FnMut(&Row) -> RowResult<T>,
    {
        match self {
            Stmt::Lite(st) => {
                let mut rows = st.query(rusqlite::params_from_iter(params))?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    out.push(f(&Row::Lite(r))?);
                }
                Ok(out)
            }
            Stmt::Pg(c, st) => c
                .query(st, params)?
                .iter()
                .map(|r| f(&Row::Pg(r)))
                .collect(),
        }
    }
}

/// 一行。
pub enum Row<'a> {
    Lite(&'a rusqlite::Row<'a>),
    Pg(&'a postgres::Row),
}

impl Row<'_> {
    pub fn get<T: FromCell>(&self, i: usize) -> RowResult<T> {
        match self {
            Row::Lite(r) => Ok(T::lite(r, i)?),
            Row::Pg(r) => T::pg(pg_cell(r, i)?),
        }
    }
}

/// Postgres 读出来的一格。
pub enum Cell {
    Null,
    Int(i64),
    Text(String),
    Blob(Vec<u8>),
}

fn pg_cell(r: &postgres::Row, i: usize) -> RowResult<Cell> {
    let ty = r
        .columns()
        .get(i)
        .ok_or_else(|| DbError::Other(format!("没有第 {i} 列")))?
        .type_()
        .clone();
    let v = match ty {
        Type::INT8 => r.try_get::<_, Option<i64>>(i)?.map(Cell::Int),
        Type::INT4 => r.try_get::<_, Option<i32>>(i)?.map(|v| Cell::Int(v.into())),
        Type::INT2 => r.try_get::<_, Option<i16>>(i)?.map(|v| Cell::Int(v.into())),
        Type::BOOL => r
            .try_get::<_, Option<bool>>(i)?
            .map(|v| Cell::Int(v.into())),
        Type::BYTEA => r.try_get::<_, Option<Vec<u8>>>(i)?.map(Cell::Blob),
        _ => r.try_get::<_, Option<String>>(i)?.map(Cell::Text),
    };
    Ok(v.unwrap_or(Cell::Null))
}

/// 能从一格读出来的类型。
pub trait FromCell: Sized {
    fn lite(r: &rusqlite::Row, i: usize) -> rusqlite::Result<Self>;
    fn pg(c: Cell) -> RowResult<Self>;
}

fn bad(c: &Cell, want: &str) -> DbError {
    let got = match c {
        Cell::Null => "NULL",
        Cell::Int(_) => "整数",
        Cell::Text(_) => "文本",
        Cell::Blob(_) => "二进制",
    };
    DbError::Other(format!("列的类型不对：要{want}，是{got}"))
}

impl FromCell for i64 {
    fn lite(r: &rusqlite::Row, i: usize) -> rusqlite::Result<Self> {
        r.get(i)
    }
    fn pg(c: Cell) -> RowResult<Self> {
        match c {
            Cell::Int(v) => Ok(v),
            c => Err(bad(&c, "整数")),
        }
    }
}

impl FromCell for bool {
    fn lite(r: &rusqlite::Row, i: usize) -> rusqlite::Result<Self> {
        r.get(i)
    }
    fn pg(c: Cell) -> RowResult<Self> {
        match c {
            Cell::Int(v) => Ok(v != 0),
            c => Err(bad(&c, "整数")),
        }
    }
}

impl FromCell for String {
    fn lite(r: &rusqlite::Row, i: usize) -> rusqlite::Result<Self> {
        r.get(i)
    }
    fn pg(c: Cell) -> RowResult<Self> {
        match c {
            Cell::Text(v) => Ok(v),
            c => Err(bad(&c, "文本")),
        }
    }
}

impl FromCell for Vec<u8> {
    fn lite(r: &rusqlite::Row, i: usize) -> rusqlite::Result<Self> {
        r.get(i)
    }
    fn pg(c: Cell) -> RowResult<Self> {
        match c {
            Cell::Blob(v) => Ok(v),
            Cell::Text(v) => Ok(v.into_bytes()),
            c => Err(bad(&c, "二进制")),
        }
    }
}

impl<T: FromCell + rusqlite::types::FromSql> FromCell for Option<T> {
    fn lite(r: &rusqlite::Row, i: usize) -> rusqlite::Result<Self> {
        r.get(i)
    }
    fn pg(c: Cell) -> RowResult<Self> {
        match c {
            Cell::Null => Ok(None),
            c => T::pg(c).map(Some),
        }
    }
}

/// 把按 SQLite 写的 SQL 换成 Postgres 的：`?N` → `$N`，`x IS ?N` → `x IS NOT DISTINCT FROM $N`，
/// `INSERT OR IGNORE` → `INSERT … ON CONFLICT DO NOTHING`。引号里的不动。
pub fn pg_sql(sql: &str) -> String {
    let (sql, ignore) = match sql.trim_start().strip_prefix("INSERT OR IGNORE INTO") {
        Some(rest) => (format!("INSERT INTO{rest}"), true),
        None => (sql.to_string(), false),
    };
    let mut out = String::with_capacity(sql.len() + 32);
    let mut quoted = false;
    let b = sql.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let ch = b[i];
        if ch == b'\'' {
            quoted = !quoted;
        } else if !quoted && ch == b'?' && b.get(i + 1).is_some_and(u8::is_ascii_digit) {
            if out.ends_with(" IS ") {
                out.push_str("NOT DISTINCT FROM ");
            }
            out.push('$');
            i += 1;
            continue;
        }
        // 按字节拷，多字节字符原样拼回去
        let len = utf8_len(ch);
        out.push_str(&sql[i..i + len]);
        i += len;
    }
    if ignore {
        out.push_str(" ON CONFLICT DO NOTHING");
    }
    out
}

fn utf8_len(b: u8) -> usize {
    match b {
        0..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::pg_sql;

    #[test]
    fn translate() {
        assert_eq!(
            pg_sql("SELECT a FROM t WHERE b = ?1 AND c IS ?2 AND d = '?3' AND ?2 IS NULL"),
            "SELECT a FROM t WHERE b = $1 AND c IS NOT DISTINCT FROM $2 AND d = '?3' AND $2 IS NULL"
        );
        assert_eq!(
            pg_sql("INSERT OR IGNORE INTO t (a) VALUES (?1)"),
            "INSERT INTO t (a) VALUES ($1) ON CONFLICT DO NOTHING"
        );
        assert_eq!(pg_sql("-- 中文 ?1\nSELECT ?10"), "-- 中文 $1\nSELECT $10");
    }
}
