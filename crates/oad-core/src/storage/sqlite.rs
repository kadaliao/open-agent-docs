//! SQLite（WAL）。写都交给一个专门的写线程（一条连接，按顺序执行），读用一个小连接池跑在 blocking 线程上，
//! 不占 tokio 的工作线程。
use super::Storage;
use crate::model::*;
use crate::{Error, Result, ids, tree};
use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use tokio::sync::oneshot;

const MIGRATIONS: &[&str] = &[include_str!("migrations/001_init.sql")];
const READERS: usize = 4;

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

pub struct Sqlite {
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
        Ok(Sqlite {
            writer: tx,
            readers: Arc::new(Mutex::new(Vec::new())),
            path,
        })
    }

    async fn write<R, F>(&self, f: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<R> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.writer
            .send(Box::new(move |c| {
                let _ = tx.send(f(c));
            }))
            .map_err(|_| Error::Storage("写线程已退出".into()))?;
        rx.await
            .map_err(|_| Error::Storage("写线程已退出".into()))?
    }

    async fn read<R, F>(&self, f: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&Connection) -> Result<R> + Send + 'static,
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
            let r = f(&conn);
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

const PAGE_COLS: &str = "id, workspace_id, parent_id, position, title, icon, created_by, created_at, updated_at, deleted_at";

fn page_row(r: &Row) -> rusqlite::Result<Page> {
    Ok(Page {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        parent_id: r.get(2)?,
        position: r.get(3)?,
        title: r.get(4)?,
        icon: r.get(5)?,
        created_by: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
        deleted_at: r.get(9)?,
    })
}

const USER_COLS: &str = "id, name, email, kind, created_at";

fn user_row(r: &Row) -> rusqlite::Result<User> {
    Ok(User {
        id: r.get(0)?,
        name: r.get(1)?,
        email: r.get(2)?,
        kind: PrincipalKind::parse(&r.get::<_, String>(3)?),
        created_at: r.get(4)?,
    })
}

fn get_page(c: &Connection, id: &str) -> Result<Option<Page>> {
    Ok(c.query_row(
        &format!("SELECT {PAGE_COLS} FROM pages WHERE id = ?1"),
        [id],
        page_row,
    )
    .optional()?)
}

fn live_page(c: &Connection, id: &str) -> Result<Page> {
    match get_page(c, id)? {
        Some(p) if p.deleted_at.is_none() => Ok(p),
        _ => Err(Error::NotFound(format!("页面 {id} 不存在"))),
    }
}

/// 检查新父页面：存在、同一工作区、不在回收站。
fn check_parent(c: &Connection, ws: &str, parent: &str) -> Result<()> {
    match get_page(c, parent)? {
        Some(p) if p.deleted_at.is_none() && p.workspace_id == ws => Ok(()),
        _ => Err(Error::BadRequest(format!("父页面 {parent} 不存在"))),
    }
}

/// 同级（不含 except）按顺序的 (id, position)。
fn siblings(
    c: &Connection,
    ws: &str,
    parent: Option<&str>,
    except: &str,
) -> Result<Vec<(String, String)>> {
    let mut st = c.prepare_cached(
        "SELECT id, position FROM pages
         WHERE workspace_id = ?1 AND parent_id IS ?2 AND deleted_at IS NULL AND id != ?3
         ORDER BY position, id",
    )?;
    let rows = st.query_map(params![ws, parent, except], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn ancestors(c: &Connection, id: &str) -> Result<Vec<String>> {
    let mut st = c.prepare_cached(
        "WITH RECURSIVE anc(id, parent_id) AS (
            SELECT id, parent_id FROM pages WHERE id = ?1
            UNION ALL
            SELECT p.id, p.parent_id FROM pages p JOIN anc ON p.id = anc.parent_id
         ) SELECT id FROM anc",
    )?;
    let rows = st.query_map([id], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn sql_err(e: rusqlite::Error) -> Error {
    if let rusqlite::Error::SqliteFailure(f, _) = &e
        && f.code == rusqlite::ErrorCode::ConstraintViolation
    {
        return Error::Conflict(e.to_string());
    }
    Error::Storage(e.to_string())
}

#[async_trait]
impl Storage for Sqlite {
    async fn create_user(&self, u: NewUser) -> Result<User> {
        self.write(move |c| {
            let user = User {
                id: ids::new_id(),
                name: u.name,
                email: u.email,
                kind: u.kind,
                created_at: ids::now_ms(),
            };
            c.execute(
                "INSERT INTO users (id, name, email, password_hash, external_id, kind, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    user.id,
                    user.name,
                    user.email,
                    u.password_hash,
                    u.external_id,
                    user.kind.as_str(),
                    user.created_at
                ],
            )
            .map_err(sql_err)?;
            Ok(user)
        })
        .await
    }

    async fn user(&self, id: &str) -> Result<Option<User>> {
        let id = id.to_string();
        self.read(move |c| {
            Ok(c.query_row(
                &format!("SELECT {USER_COLS} FROM users WHERE id = ?1"),
                [id],
                user_row,
            )
            .optional()?)
        })
        .await
    }

    async fn user_by_email(&self, email: &str) -> Result<Option<(User, Option<String>)>> {
        let email = email.to_string();
        self.read(move |c| {
            Ok(c.query_row(
                &format!("SELECT {USER_COLS}, password_hash FROM users WHERE email = ?1"),
                [email],
                |r| Ok((user_row(r)?, r.get(5)?)),
            )
            .optional()?)
        })
        .await
    }

    async fn user_by_external(&self, external_id: &str) -> Result<Option<User>> {
        let ext = external_id.to_string();
        self.read(move |c| {
            Ok(c.query_row(
                &format!("SELECT {USER_COLS} FROM users WHERE external_id = ?1"),
                [ext],
                user_row,
            )
            .optional()?)
        })
        .await
    }

    async fn count_users(&self) -> Result<u64> {
        self.read(|c| {
            Ok(c.query_row("SELECT count(*) FROM users", [], |r| r.get::<_, i64>(0))? as u64)
        })
        .await
    }

    async fn set_password(&self, user_id: &str, password_hash: &str) -> Result<()> {
        let (id, h) = (user_id.to_string(), password_hash.to_string());
        self.write(move |c| {
            c.execute(
                "UPDATE users SET password_hash = ?2 WHERE id = ?1",
                params![id, h],
            )?;
            Ok(())
        })
        .await
    }

    async fn create_session(&self, token_hash: &str, user_id: &str, expires_at: i64) -> Result<()> {
        let (t, u) = (token_hash.to_string(), user_id.to_string());
        self.write(move |c| {
            let now = ids::now_ms();
            c.execute(
                "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
                params![t, u, now, expires_at],
            )?;
            // 顺手清掉过期的
            c.execute("DELETE FROM sessions WHERE expires_at < ?1", [now])?;
            Ok(())
        })
        .await
    }

    async fn session_user(&self, token_hash: &str, now: i64) -> Result<Option<User>> {
        let t = token_hash.to_string();
        self.read(move |c| {
            Ok(c.query_row(
                "SELECT u.id, u.name, u.email, u.kind, u.created_at FROM sessions s
                 JOIN users u ON u.id = s.user_id WHERE s.token_hash = ?1 AND s.expires_at > ?2",
                params![t, now],
                user_row,
            )
            .optional()?)
        })
        .await
    }

    async fn delete_session(&self, token_hash: &str) -> Result<()> {
        let t = token_hash.to_string();
        self.write(move |c| {
            c.execute("DELETE FROM sessions WHERE token_hash = ?1", [t])?;
            Ok(())
        })
        .await
    }

    async fn create_workspace(&self, name: &str) -> Result<Workspace> {
        let name = name.to_string();
        self.write(move |c| {
            let w = Workspace {
                id: ids::new_id(),
                name,
                created_at: ids::now_ms(),
            };
            c.execute(
                "INSERT INTO workspaces (id, name, created_at) VALUES (?1, ?2, ?3)",
                params![w.id, w.name, w.created_at],
            )?;
            Ok(w)
        })
        .await
    }

    async fn workspaces(&self) -> Result<Vec<Workspace>> {
        self.read(|c| {
            let mut st =
                c.prepare("SELECT id, name, created_at FROM workspaces ORDER BY created_at, id")?;
            let rows = st.query_map([], |r| {
                Ok(Workspace {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    async fn set_member(&self, workspace_id: &str, user_id: &str, role: Role) -> Result<()> {
        let (w, u) = (workspace_id.to_string(), user_id.to_string());
        self.write(move |c| {
            c.execute(
                "INSERT INTO members (workspace_id, user_id, role) VALUES (?1, ?2, ?3)
                 ON CONFLICT (workspace_id, user_id) DO UPDATE SET role = excluded.role",
                params![w, u, role.as_str()],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }

    async fn member_role(&self, workspace_id: &str, user_id: &str) -> Result<Option<Role>> {
        let (w, u) = (workspace_id.to_string(), user_id.to_string());
        self.read(move |c| {
            let r: Option<String> = c
                .prepare_cached(
                    "SELECT role FROM members WHERE workspace_id = ?1 AND user_id = ?2",
                )?
                .query_row(params![w, u], |r| r.get(0))
                .optional()?;
            Ok(r.as_deref().and_then(Role::parse))
        })
        .await
    }

    async fn user_workspaces(&self, user_id: &str) -> Result<Vec<(Workspace, Role)>> {
        let u = user_id.to_string();
        self.read(move |c| {
            let mut st = c.prepare(
                "SELECT w.id, w.name, w.created_at, m.role FROM members m
                 JOIN workspaces w ON w.id = m.workspace_id WHERE m.user_id = ?1
                 ORDER BY w.created_at, w.id",
            )?;
            let rows = st.query_map([u], |r| {
                Ok((
                    Workspace {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        created_at: r.get(2)?,
                    },
                    r.get::<_, String>(3)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (w, role) = row?;
                if let Some(role) = Role::parse(&role) {
                    out.push((w, role));
                }
            }
            Ok(out)
        })
        .await
    }

    async fn create_page(&self, p: NewPage) -> Result<Page> {
        self.write(move |c| {
            let tx = c.transaction()?;
            let ws_ok: bool = tx
                .query_row(
                    "SELECT 1 FROM workspaces WHERE id = ?1",
                    [&p.workspace_id],
                    |_| Ok(true),
                )
                .optional()?
                .unwrap_or(false);
            if !ws_ok {
                return Err(Error::NotFound(format!("工作区 {} 不存在", p.workspace_id)));
            }
            if let Some(parent) = &p.parent_id {
                check_parent(&tx, &p.workspace_id, parent)?;
            }
            let id = ids::new_id();
            let sib = siblings(&tx, &p.workspace_id, p.parent_id.as_deref(), &id)?;
            let now = ids::now_ms();
            let page = Page {
                position: tree::position_for(&sib, &p.place)?,
                id,
                workspace_id: p.workspace_id,
                parent_id: p.parent_id,
                title: p.title,
                icon: p.icon,
                created_by: p.created_by,
                created_at: now,
                updated_at: now,
                deleted_at: None,
            };
            tx.execute(
                &format!("INSERT INTO pages ({PAGE_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)"),
                params![
                    page.id,
                    page.workspace_id,
                    page.parent_id,
                    page.position,
                    page.title,
                    page.icon,
                    page.created_by,
                    page.created_at,
                    page.updated_at
                ],
            )?;
            tx.execute(
                "INSERT INTO doc_snapshots (page_id, state, updated_at) VALUES (?1, ?2, ?3)",
                params![page.id, p.initial_state, now],
            )?;
            tx.commit()?;
            Ok(page)
        })
        .await
    }

    async fn page(&self, id: &str) -> Result<Option<Page>> {
        let id = id.to_string();
        self.read(move |c| get_page(c, &id)).await
    }

    async fn pages(&self, workspace_id: &str) -> Result<Vec<Page>> {
        let w = workspace_id.to_string();
        self.read(move |c| {
            let mut st = c.prepare_cached(&format!(
                "SELECT {PAGE_COLS} FROM pages WHERE workspace_id = ?1 AND deleted_at IS NULL
                 ORDER BY parent_id, position, id"
            ))?;
            let rows = st.query_map([w], page_row)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    async fn update_page(
        &self,
        id: &str,
        title: Option<String>,
        icon: Option<Option<String>>,
    ) -> Result<Page> {
        let id = id.to_string();
        self.write(move |c| {
            let mut p = live_page(c, &id)?;
            if let Some(t) = title {
                p.title = t;
            }
            if let Some(i) = icon {
                p.icon = i;
            }
            p.updated_at = ids::now_ms();
            c.execute(
                "UPDATE pages SET title = ?2, icon = ?3, updated_at = ?4 WHERE id = ?1",
                params![p.id, p.title, p.icon, p.updated_at],
            )?;
            Ok(p)
        })
        .await
    }

    async fn move_page(
        &self,
        id: &str,
        parent_id: Option<String>,
        place: Placement,
    ) -> Result<Page> {
        let id = id.to_string();
        self.write(move |c| {
            let tx = c.transaction()?;
            let mut p = live_page(&tx, &id)?;
            if let Some(parent) = &parent_id {
                check_parent(&tx, &p.workspace_id, parent)?;
                tree::check_cycle(&id, &ancestors(&tx, parent)?)?;
            }
            let sib = siblings(&tx, &p.workspace_id, parent_id.as_deref(), &id)?;
            p.position = tree::position_for(&sib, &place)?;
            p.parent_id = parent_id;
            p.updated_at = ids::now_ms();
            tx.execute(
                "UPDATE pages SET parent_id = ?2, position = ?3, updated_at = ?4 WHERE id = ?1",
                params![p.id, p.parent_id, p.position, p.updated_at],
            )?;
            tx.commit()?;
            Ok(p)
        })
        .await
    }

    async fn delete_page(&self, id: &str) -> Result<Vec<Page>> {
        let id = id.to_string();
        self.write(move |c| {
            let tx = c.transaction()?;
            live_page(&tx, &id)?;
            let now = ids::now_ms();
            let ids: Vec<String> = {
                let mut st = tx.prepare(
                    "WITH RECURSIVE sub(id, depth) AS (
                        SELECT id, 0 FROM pages WHERE id = ?1
                        UNION ALL
                        SELECT p.id, sub.depth + 1 FROM pages p JOIN sub ON p.parent_id = sub.id
                        WHERE p.deleted_at IS NULL
                     ) SELECT id FROM sub ORDER BY depth",
                )?;
                st.query_map([&id], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            };
            let mut out = Vec::with_capacity(ids.len());
            for pid in &ids {
                tx.execute(
                    "UPDATE pages SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                    params![pid, now],
                )?;
                if let Some(p) = get_page(&tx, pid)? {
                    out.push(p);
                }
            }
            tx.commit()?;
            Ok(out)
        })
        .await
    }

    async fn load_doc(&self, page_id: &str) -> Result<DocLog> {
        let id = page_id.to_string();
        self.read(move |c| {
            // 快照和增量在同一个读事务里读：中间插进来一次压缩的话，会漏掉被删的增量
            let tx = c.unchecked_transaction()?;
            let snapshot = tx
                .prepare_cached("SELECT state FROM doc_snapshots WHERE page_id = ?1")?
                .query_row([&id], |r| r.get(0))
                .optional()?;
            let updates = tx
                .prepare_cached("SELECT data FROM doc_updates WHERE page_id = ?1 ORDER BY seq")?
                .query_map([&id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            tx.commit()?;
            Ok(DocLog { snapshot, updates })
        })
        .await
    }

    async fn append_update(&self, page_id: &str, update: Vec<u8>) -> Result<()> {
        let id = page_id.to_string();
        self.write(move |c| {
            let tx = c.transaction()?;
            let now = ids::now_ms();
            tx.prepare_cached(
                "INSERT INTO doc_updates (page_id, data, created_at) VALUES (?1, ?2, ?3)",
            )?
            .execute(params![id, update, now])?;
            tx.prepare_cached("UPDATE pages SET updated_at = ?2 WHERE id = ?1")?
                .execute(params![id, now])?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    async fn compact(&self, page_id: &str, snapshot: Vec<u8>) -> Result<()> {
        let id = page_id.to_string();
        self.write(move |c| {
            let tx = c.transaction()?;
            tx.execute(
                "INSERT INTO doc_snapshots (page_id, state, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (page_id) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
                params![id, snapshot, ids::now_ms()],
            )?;
            tx.execute("DELETE FROM doc_updates WHERE page_id = ?1", [&id])?;
            tx.commit()?;
            Ok(())
        })
        .await
    }
}
