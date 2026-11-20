//! SQLite（WAL）。写都交给一个专门的写线程（一条连接，按顺序执行），读用一个小连接池跑在 blocking 线程上，
//! 不占 tokio 的工作线程。SQL 和逻辑在 [`super::sql`]。
use super::db::Conn;
use super::sql::{Engine, SqlStore};
use crate::{Error, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use tokio::sync::oneshot;

const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_init.sql"),
    include_str!("migrations/002_agents_comments.sql"),
    include_str!("migrations/003_media.sql"),
    include_str!("migrations/004_databases.sql"),
    include_str!("migrations/005_phase5.sql"),
    include_str!("migrations/006_settings.sql"),
];
const READERS: usize = 4;
/// 每条连接缓存的语句数（所有 SQL 都走缓存，够放下全部）。
const STMT_CACHE: usize = 256;

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

pub type Sqlite = SqlStore<SqliteEngine>;

pub struct SqliteEngine {
    writer: mpsc::Sender<Job>,
    readers: Arc<Mutex<Vec<Connection>>>,
    path: PathBuf,
}

fn open(path: &Path) -> Result<Connection> {
    let c = Connection::open(path)?;
    c.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )?;
    c.set_prepared_statement_cache_capacity(STMT_CACHE);
    Ok(c)
}

fn migrate(c: &mut Connection) -> Result<()> {
    let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(v as usize) {
        let tx = c.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

impl Sqlite {
    /// 打开（没有就建）数据库文件并升级表结构。
    pub fn open(path: impl AsRef<Path>) -> Result<Sqlite> {
        let path = path.as_ref().to_path_buf();
        let mut w = open(&path)?;
        migrate(&mut w)?;
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("oad-sqlite-writer".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    job(&mut w);
                }
            })
            .map_err(|e| Error::Internal(e.to_string()))?;
        Ok(SqlStore {
            engine: SqliteEngine {
                writer: tx,
                readers: Arc::new(Mutex::new(Vec::new())),
                path,
            },
        })
    }
}

impl SqliteEngine {
    async fn write<R, F>(&self, f: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Conn) -> Result<R> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.writer
            .send(Box::new(move |c| {
                let _ = tx.send(f(&mut Conn::Lite(c)));
            }))
            .map_err(|_| Error::Storage("写线程已退出".into()))?;
        rx.await
            .map_err(|_| Error::Storage("写线程已退出".into()))?
    }

    async fn read<R, F>(&self, f: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Conn) -> Result<R> + Send + 'static,
    {
        let pool = self.readers.clone();
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.lock().unwrap().pop();
            let conn = match conn {
                Some(c) => c,
                None => {
                    let c = open(&path)?;
                    c.pragma_update(None, "query_only", 1)?;
                    c
                }
            };
            let r = f(&mut Conn::Lite(&conn));
            let mut p = pool.lock().unwrap();
            if p.len() < READERS {
                p.push(conn);
            }
            r
        })
        .await
        .map_err(|e| Error::Internal(e.to_string()))?
    }
}

impl Engine for SqliteEngine {
    async fn run<R, F>(&self, write: bool, f: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Conn) -> Result<R> + Send + 'static,
    {
        if write {
            self.write(f).await
        } else {
            self.read(f).await
        }
    }
}
