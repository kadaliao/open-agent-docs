//! Postgres。用同步的 postgres 驱动跑在 blocking 线程上，连接放在一个小池子里，每条连接缓存准备好的语句。
//! 每次读写是一个事务：读用 REPEATABLE READ（几条 SELECT 看到同一个快照，和 SQLite 的读事务一样）；
//! 写先拿一把事务级的 advisory 锁，同一个 schema 同时只有一个写事务（和 SQLite 只有一个写入者一样，
//! 多个进程连同一个库也成立）。SQL 和逻辑在 [`super::sql`]，表结构在 migrations_pg/。
//! 地址里写 `sslmode=require` 时走 TLS（rustls，按公共 CA 验证书），否则不加密。
use super::db::{Conn, DbError, PgClient, RowResult, Value, pg_sql};
use super::sql::{Engine, SqlStore};
use crate::{Error, Result};
use postgres::types::ToSql;
use postgres::{Client, Config, IsolationLevel, NoTls, Statement, Transaction};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

const MIGRATIONS: &[&str] = &[
    include_str!("migrations_pg/001_init.sql"),
    include_str!("migrations_pg/002_agents_comments.sql"),
    include_str!("migrations_pg/003_media.sql"),
    include_str!("migrations_pg/004_databases.sql"),
    include_str!("migrations_pg/005_phase5.sql"),
];
/// 默认最多几条连接。
pub const MAX_CONNS: usize = 10;
/// 写事务排队用的锁（按 schema 分）。
const WRITE_LOCK: &str = "SELECT pg_advisory_xact_lock(hashtext(coalesce(current_schema(), '')))";

pub type Postgres = SqlStore<PgEngine>;

pub struct PgEngine {
    pool: Arc<Pool>,
}

struct Pool {
    config: Config,
    idle: Mutex<Vec<PgConn>>,
    permits: Arc<Semaphore>,
}

struct PgConn {
    client: Client,
    /// 原来的 SQL → 准备好的语句
    stmts: HashMap<String, Statement>,
}

/// TLS：rustls（ring）+ webpki 的根证书。
fn tls() -> tokio_postgres_rustls::MakeRustlsConnect {
    crate::notify::install_tls();
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tokio_postgres_rustls::MakeRustlsConnect::new(cfg)
}

fn pe(e: postgres::Error) -> Error {
    DbError::from(e).into()
}

impl Pool {
    fn take(&self) -> Result<PgConn> {
        if let Some(c) = self.idle.lock().unwrap().pop() {
            return Ok(c);
        }
        Ok(PgConn {
            client: match self.config.get_ssl_mode() {
                // sslmode=require：加密，按公共 CA 验证书
                postgres::config::SslMode::Require => self.config.connect(tls()).map_err(pe)?,
                _ => self.config.connect(NoTls).map_err(pe)?,
            },
            stmts: HashMap::new(),
        })
    }

    fn put(&self, c: PgConn) {
        if !c.client.is_closed() {
            self.idle.lock().unwrap().push(c);
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        // postgres::Client 关连接要 block_on，不能在 tokio 的工作线程上做：换个线程关
        let idle = std::mem::take(&mut *self.idle.lock().unwrap());
        if !idle.is_empty() {
            let _ = std::thread::Builder::new()
                .name("oad-pg-close".into())
                .spawn(move || drop(idle));
        }
    }
}

/// 一个事务里的连接。
struct PgTx<'t> {
    tx: Transaction<'t>,
    stmts: &'t mut HashMap<String, Statement>,
}

fn refs<'a>(params: &'a [Value]) -> Vec<&'a (dyn ToSql + Sync)> {
    params.iter().map(|v| v as &(dyn ToSql + Sync)).collect()
}

impl PgClient for PgTx<'_> {
    fn prepare(&mut self, sql: &str) -> RowResult<Statement> {
        if let Some(st) = self.stmts.get(sql) {
            return Ok(st.clone());
        }
        let st = self.tx.prepare(&pg_sql(sql))?;
        self.stmts.insert(sql.to_string(), st.clone());
        Ok(st)
    }

    fn query(&mut self, st: &Statement, params: &[Value]) -> RowResult<Vec<postgres::Row>> {
        Ok(self.tx.query(st, &refs(params))?)
    }

    fn execute(&mut self, st: &Statement, params: &[Value]) -> RowResult<u64> {
        Ok(self.tx.execute(st, &refs(params))?)
    }
}

impl PgConn {
    fn run<R>(&mut self, write: bool, f: impl FnOnce(&mut Conn) -> Result<R>) -> Result<R> {
        let tx = if write {
            self.client.transaction()
        } else {
            self.client
                .build_transaction()
                .isolation_level(IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()
        }
        .map_err(pe)?;
        let mut t = PgTx {
            tx,
            stmts: &mut self.stmts,
        };
        if write {
            let st = t.prepare(WRITE_LOCK)?;
            t.execute(&st, &[])?;
        }
        match f(&mut Conn::Pg(&mut t)) {
            Ok(v) => {
                t.tx.commit().map_err(pe)?;
                Ok(v)
            }
            Err(e) => {
                let _ = t.tx.rollback();
                Err(e)
            }
        }
    }
}

fn migrate(c: &mut Client) -> Result<()> {
    let mut tx = c.transaction().map_err(pe)?;
    tx.batch_execute(&format!(
        "{WRITE_LOCK}; CREATE TABLE IF NOT EXISTS oad_migrations (version BIGINT NOT NULL)"
    ))
    .map_err(pe)?;
    let v: i64 = tx
        .query_one("SELECT coalesce(max(version), 0) FROM oad_migrations", &[])
        .map_err(pe)?
        .get(0);
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(v as usize) {
        tx.batch_execute(sql).map_err(pe)?;
        tx.execute(
            "INSERT INTO oad_migrations (version) VALUES ($1)",
            &[&((i + 1) as i64)],
        )
        .map_err(pe)?;
    }
    tx.commit().map_err(pe)
}

/// schema 名只能是小写字母、数字、下划线（拼进 SQL 里）。
fn check_schema(s: &str) -> Result<()> {
    let ok = !s.is_empty()
        && s.len() <= 63
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && !s.as_bytes()[0].is_ascii_digit();
    if ok {
        Ok(())
    } else {
        Err(Error::BadRequest(format!("schema 名不合法：{s}")))
    }
}

impl Postgres {
    /// 连上库并升级表结构。url 如 `postgres://user:pass@host:5432/db`。
    pub async fn connect(url: &str) -> Result<Postgres> {
        Self::connect_with(url, None, MAX_CONNS).await
    }

    /// 同上，表都放在 schema 里（没有就建）；最多 max_conns 条连接。
    pub async fn connect_with(
        url: &str,
        schema: Option<&str>,
        max_conns: usize,
    ) -> Result<Postgres> {
        let mut config: Config = url
            .parse()
            .map_err(|e| Error::BadRequest(format!("数据库地址不对：{e}")))?;
        if let Some(s) = schema {
            check_schema(s)?;
            let opts = format!(
                "{} -c search_path={s}",
                config.get_options().unwrap_or_default()
            );
            config.options(opts.trim());
        }
        let pool = Arc::new(Pool {
            config,
            idle: Mutex::new(Vec::new()),
            permits: Arc::new(Semaphore::new(max_conns.max(1))),
        });
        let p = pool.clone();
        let schema = schema.map(str::to_string);
        tokio::task::spawn_blocking(move || {
            let mut c = p.take()?;
            if let Some(s) = &schema {
                let r = c
                    .client
                    .batch_execute(&format!("CREATE SCHEMA IF NOT EXISTS {s}"));
                match r.map_err(DbError::from) {
                    // 别的进程同时在建：不要紧
                    Ok(()) | Err(DbError::Constraint(_)) => {}
                    Err(e) => return Err(Error::Storage(format!("建 schema {s} 失败：{e}"))),
                }
            }
            migrate(&mut c.client)?;
            p.put(c);
            Ok(())
        })
        .await
        .map_err(|e| Error::Internal(e.to_string()))??;
        Ok(SqlStore {
            engine: PgEngine { pool },
        })
    }
}

impl Engine for PgEngine {
    fn run<R, F>(&self, write: bool, f: F) -> impl Future<Output = Result<R>> + Send
    where
        R: Send + 'static,
        F: FnOnce(&mut Conn) -> Result<R> + Send + 'static,
    {
        let pool = self.pool.clone();
        async move {
            let permit = pool
                .permits
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| Error::Internal(e.to_string()))?;
            tokio::task::spawn_blocking(move || {
                let mut c = pool.take()?;
                let r = c.run(write, f);
                pool.put(c);
                drop(permit);
                r
            })
            .await
            .map_err(|e| Error::Internal(e.to_string()))?
        }
    }
}
