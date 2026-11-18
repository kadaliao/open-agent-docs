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

const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_init.sql"),
    include_str!("migrations/002_agents_comments.sql"),
    include_str!("migrations/003_media.sql"),
    include_str!("migrations/004_databases.sql"),
];
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

const PAGE_COLS: &str = "id, workspace_id, parent_id, position, title, icon, created_by, created_at, updated_at, deleted_at, kind";

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
        kind: r.get(10)?,
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
         WHERE workspace_id = ?1 AND parent_id IS ?2 AND deleted_at IS NULL AND id != ?3 AND kind != 'row'
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
            let page = insert_page(&tx, p, None)?;
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
                "SELECT {PAGE_COLS} FROM pages WHERE workspace_id = ?1 AND deleted_at IS NULL AND kind != 'row'
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
            let tx = c.transaction()?;
            let mut p = live_page(&tx, &id)?;
            let old_title = p.title.clone();
            if let Some(t) = title {
                p.title = t;
            }
            if let Some(i) = icon {
                p.icon = i;
            }
            p.updated_at = ids::now_ms();
            tx.execute(
                "UPDATE pages SET title = ?2, icon = ?3, updated_at = ?4 WHERE id = ?1",
                params![p.id, p.title, p.icon, p.updated_at],
            )?;
            // 行的标题就是标题那一格：跟着改，记一条改动
            if p.kind == "row"
                && p.title != old_title
                && let Some(db) = &p.parent_id
                && let Some(meta) = get_db_meta(&tx, db)?
                && let Some(key) = title_key_of(&meta.schema)
                && let Some(mut row) = tx
                    .prepare_cached(&format!("SELECT {ROW_COLS} FROM db_rows WHERE row_id = ?1"))?
                    .query_row([&p.id], db_row_of)
                    .optional()?
            {
                let seq = bump_seq(&tx, db)?;
                let old = row.props.get(&key).cloned();
                let new = (!p.title.is_empty()).then(|| serde_json::Value::String(p.title.clone()));
                match &new {
                    Some(v) => row.props.insert(key.clone(), v.clone()),
                    None => row.props.remove(&key),
                };
                tx.execute(
                    "UPDATE db_rows SET props = ?2, seq = ?3, updated_at = ?4 WHERE row_id = ?1",
                    params![
                        p.id,
                        serde_json::Value::Object(row.props).to_string(),
                        seq,
                        p.updated_at
                    ],
                )?;
                log_op(
                    &tx,
                    &At {
                        db,
                        seq,
                        by: "",
                        at: p.updated_at,
                    },
                    &p.id,
                    &key,
                    &old,
                    &new,
                )?;
            }
            tx.commit()?;
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
            let root = live_page(&tx, &id)?;
            let now = ids::now_ms();
            // 删的是库里的一行：库里也去掉（记一条改动）
            if root.kind == "row"
                && let Some(db) = &root.parent_id
                && let Some(row) = tx
                    .prepare_cached(&format!(
                        "SELECT {ROW_COLS} FROM db_rows WHERE row_id = ?1 AND deleted_at IS NULL"
                    ))?
                    .query_row([&id], db_row_of)
                    .optional()?
            {
                let seq = bump_seq(&tx, db)?;
                tx.execute(
                    "UPDATE db_rows SET deleted_at = ?2, seq = ?3 WHERE row_id = ?1",
                    params![id, now, seq],
                )?;
                log_op(
                    &tx,
                    &At {
                        db,
                        seq,
                        by: "",
                        at: now,
                    },
                    &id,
                    "*",
                    &Some(serde_json::Value::Object(row.props)),
                    &None,
                )?;
            }
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

    async fn members(&self, workspace_id: &str) -> Result<Vec<(User, Role)>> {
        let w = workspace_id.to_string();
        self.read(move |c| {
            let mut st = c.prepare_cached(
                "SELECT u.id, u.name, u.email, u.kind, u.created_at, m.role FROM members m
                 JOIN users u ON u.id = m.user_id WHERE m.workspace_id = ?1",
            )?;
            let rows = st.query_map([w], |r| Ok((user_row(r)?, r.get::<_, String>(5)?)))?;
            let mut out = Vec::new();
            for row in rows {
                let (u, role) = row?;
                if let Some(role) = Role::parse(&role) {
                    out.push((u, role));
                }
            }
            Ok(out)
        })
        .await
    }

    async fn create_agent(&self, a: NewAgent) -> Result<Agent> {
        self.write(move |c| {
            let tx = c.transaction()?;
            let now = ids::now_ms();
            let id = ids::new_id();
            tx.execute(
                "INSERT INTO users (id, name, email, password_hash, external_id, kind, created_at)
                 VALUES (?1, ?2, NULL, NULL, NULL, 'agent', ?3)",
                params![id, a.name, now],
            )?;
            tx.execute(
                &format!(
                    "INSERT INTO agents ({AGENT_COLS}, token_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, ?10)"
                ),
                params![
                    id,
                    a.workspace_id,
                    a.name,
                    a.about,
                    a.scope.as_str(),
                    a.webhook,
                    a.webhook_secret,
                    a.created_by,
                    now,
                    a.token_hash
                ],
            )
            .map_err(|e| match sql_err(e) {
                Error::Conflict(_) => {
                    Error::Conflict(format!("这个工作区已经有叫 {} 的 Agent", a.name))
                }
                e => e,
            })?;
            tx.execute(
                "INSERT INTO members (workspace_id, user_id, role) VALUES (?1, ?2, ?3)",
                params![a.workspace_id, id, a.scope.role().as_str()],
            )?;
            let agent =
                get_agent(&tx, &id)?.ok_or_else(|| Error::Internal("建 Agent 失败".into()))?;
            tx.commit()?;
            Ok(agent)
        })
        .await
    }

    async fn agent(&self, id: &str) -> Result<Option<Agent>> {
        let id = id.to_string();
        self.read(move |c| get_agent(c, &id)).await
    }

    async fn agent_by_token(&self, token_hash: &str) -> Result<Option<Agent>> {
        let t = token_hash.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached(&format!(
                "SELECT {AGENT_COLS} FROM agents WHERE token_hash = ?1 AND revoked_at IS NULL"
            ))?
            .query_row([t], agent_row)
            .optional()?)
        })
        .await
    }

    async fn agents(&self, workspace_id: &str) -> Result<Vec<Agent>> {
        let w = workspace_id.to_string();
        self.read(move |c| {
            let mut st = c.prepare_cached(&format!(
                "SELECT {AGENT_COLS} FROM agents WHERE workspace_id = ?1 ORDER BY created_at, id"
            ))?;
            let rows = st.query_map([w], agent_row)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    async fn update_agent(&self, id: &str, p: AgentPatch) -> Result<Agent> {
        let id = id.to_string();
        self.write(move |c| {
            let tx = c.transaction()?;
            let mut a = get_agent(&tx, &id)?
                .ok_or_else(|| Error::NotFound(format!("Agent {id} 不存在")))?;
            if let Some(n) = p.name {
                a.name = n;
            }
            if let Some(ab) = p.about {
                a.about = ab;
            }
            if let Some(sc) = p.scope {
                a.scope = sc;
            }
            if let Some(w) = p.webhook {
                a.webhook = w;
            }
            tx.execute(
                "UPDATE agents SET name = ?2, about = ?3, scope = ?4, webhook = ?5 WHERE id = ?1",
                params![a.id, a.name, a.about, a.scope.as_str(), a.webhook],
            )
            .map_err(|e| match sql_err(e) {
                Error::Conflict(_) => {
                    Error::Conflict(format!("这个工作区已经有叫 {} 的 Agent", a.name))
                }
                e => e,
            })?;
            tx.execute(
                "UPDATE users SET name = ?2 WHERE id = ?1",
                params![a.id, a.name],
            )?;
            if a.active() {
                tx.execute(
                    "UPDATE members SET role = ?3 WHERE workspace_id = ?1 AND user_id = ?2",
                    params![a.workspace_id, a.id, a.scope.role().as_str()],
                )?;
            }
            tx.commit()?;
            Ok(a)
        })
        .await
    }

    async fn set_agent_token(&self, id: &str, token_hash: &str) -> Result<()> {
        let (id, t) = (id.to_string(), token_hash.to_string());
        self.write(move |c| {
            c.execute(
                "UPDATE agents SET token_hash = ?2 WHERE id = ?1 AND revoked_at IS NULL",
                params![id, t],
            )?;
            Ok(())
        })
        .await
    }

    async fn set_webhook_secret(&self, id: &str, secret: &str) -> Result<()> {
        let (id, s) = (id.to_string(), secret.to_string());
        self.write(move |c| {
            c.execute(
                "UPDATE agents SET webhook_secret = ?2 WHERE id = ?1",
                params![id, s],
            )?;
            Ok(())
        })
        .await
    }

    async fn revoke_agent(&self, id: &str) -> Result<Agent> {
        let id = id.to_string();
        self.write(move |c| {
            let tx = c.transaction()?;
            let a = get_agent(&tx, &id)?.ok_or_else(|| Error::NotFound(format!("Agent {id} 不存在")))?;
            tx.execute(
                "UPDATE agents SET revoked_at = coalesce(revoked_at, ?2), token_hash = NULL WHERE id = ?1",
                params![id, ids::now_ms()],
            )?;
            tx.execute(
                "DELETE FROM members WHERE workspace_id = ?1 AND user_id = ?2",
                params![a.workspace_id, id],
            )?;
            tx.execute("DELETE FROM sessions WHERE user_id = ?1", [&id])?;
            let a = get_agent(&tx, &id)?.unwrap();
            tx.commit()?;
            Ok(a)
        })
        .await
    }

    async fn touch_agent(&self, id: &str, now: i64) -> Result<()> {
        let id = id.to_string();
        self.write(move |c| {
            c.execute(
                "UPDATE agents SET last_used_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
            Ok(())
        })
        .await
    }

    async fn agent_stats(&self, workspace_id: &str) -> Result<Vec<(String, i64, Option<String>)>> {
        let w = workspace_id.to_string();
        self.read(move |c| {
            let mut st = c.prepare_cached(
                "SELECT a.id,
                    (SELECT count(*) FROM agent_notifications n WHERE n.agent_id = a.id AND n.acked_at IS NULL),
                    (SELECT n.error FROM agent_notifications n WHERE n.agent_id = a.id AND n.error IS NOT NULL
                     ORDER BY n.id DESC LIMIT 1)
                 FROM agents a WHERE a.workspace_id = ?1",
            )?;
            let rows = st.query_map([w], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    async fn add_doc_clients(
        &self,
        page_id: &str,
        clients: Vec<(u64, PrincipalKind, String)>,
    ) -> Result<()> {
        let p = page_id.to_string();
        self.write(move |c| {
            let tx = c.transaction()?;
            let now = ids::now_ms();
            {
                let mut st = tx.prepare_cached(
                    "INSERT OR IGNORE INTO doc_clients (page_id, client_id, principal_kind, principal_id, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )?;
                for (cid, kind, who) in clients {
                    st.execute(params![p, cid as i64, kind.as_str(), who, now])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    async fn doc_clients(
        &self,
        page_id: &str,
    ) -> Result<Vec<(u64, PrincipalKind, String, String)>> {
        let p = page_id.to_string();
        self.read(move |c| {
            let mut st = c.prepare_cached(
                "SELECT d.client_id, d.principal_kind, d.principal_id, coalesce(u.name, '') FROM doc_clients d
                 LEFT JOIN users u ON u.id = d.principal_id WHERE d.page_id = ?1",
            )?;
            let rows = st.query_map([p], |r| {
                Ok((
                    r.get::<_, i64>(0)? as u64,
                    PrincipalKind::parse(&r.get::<_, String>(1)?),
                    r.get(2)?,
                    r.get(3)?,
                ))
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    async fn create_thread(&self, t: Thread, first: Comment) -> Result<()> {
        self.write(move |c| {
            let tx = c.transaction()?;
            tx.execute(
                &format!("INSERT INTO comment_threads ({THREAD_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"),
                params![
                    t.id,
                    t.page_id,
                    t.block_id,
                    t.anchor_start,
                    t.anchor_end,
                    t.quote,
                    t.created_by,
                    t.created_at,
                    t.resolved_at,
                    t.resolved_by
                ],
            )?;
            insert_comment(&tx, &first)?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    async fn add_comment(&self, cm: Comment) -> Result<()> {
        self.write(move |c| insert_comment(c, &cm)).await
    }

    async fn thread(&self, id: &str) -> Result<Option<Thread>> {
        let id = id.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached(&format!(
                "SELECT {THREAD_COLS} FROM comment_threads WHERE id = ?1"
            ))?
            .query_row([id], thread_row)
            .optional()?)
        })
        .await
    }

    async fn threads(
        &self,
        page_id: &str,
        resolved: Option<bool>,
    ) -> Result<Vec<(Thread, Vec<Comment>)>> {
        let p = page_id.to_string();
        self.read(move |c| {
            let filter = match resolved {
                None => "",
                Some(true) => "AND resolved_at IS NOT NULL",
                Some(false) => "AND resolved_at IS NULL",
            };
            let threads: Vec<Thread> = c
                .prepare(&format!(
                    "SELECT {THREAD_COLS} FROM comment_threads WHERE page_id = ?1 {filter} ORDER BY created_at, id"
                ))?
                .query_map([&p], thread_row)?
                .collect::<rusqlite::Result<_>>()?;
            let mut st = c.prepare_cached(&format!(
                "SELECT {COMMENT_COLS} FROM comments WHERE thread_id = ?1 ORDER BY created_at, id"
            ))?;
            let mut out = Vec::with_capacity(threads.len());
            for t in threads {
                let cs = st
                    .query_map([&t.id], comment_row)?
                    .collect::<rusqlite::Result<_>>()?;
                out.push((t, cs));
            }
            Ok(out)
        })
        .await
    }

    async fn thread_comments(&self, thread_id: &str) -> Result<Vec<Comment>> {
        let t = thread_id.to_string();
        self.read(move |c| {
            let mut st = c.prepare_cached(&format!(
                "SELECT {COMMENT_COLS} FROM comments WHERE thread_id = ?1 ORDER BY created_at, id"
            ))?;
            Ok(st
                .query_map([t], comment_row)?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    async fn comment(&self, id: &str) -> Result<Option<Comment>> {
        let id = id.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached(&format!(
                "SELECT {COMMENT_COLS} FROM comments WHERE id = ?1"
            ))?
            .query_row([id], comment_row)
            .optional()?)
        })
        .await
    }

    async fn edit_comment(&self, id: &str, body: &str, now: i64) -> Result<()> {
        let (id, b) = (id.to_string(), body.to_string());
        self.write(move |c| {
            c.execute(
                "UPDATE comments SET body = ?2, edited_at = ?3 WHERE id = ?1",
                params![id, b, now],
            )?;
            Ok(())
        })
        .await
    }

    async fn delete_comment(&self, id: &str, now: i64) -> Result<()> {
        let id = id.to_string();
        self.write(move |c| {
            c.execute(
                "UPDATE comments SET deleted_at = coalesce(deleted_at, ?2) WHERE id = ?1",
                params![id, now],
            )?;
            Ok(())
        })
        .await
    }

    async fn resolve_thread(&self, id: &str, resolved: Option<(i64, String)>) -> Result<()> {
        let id = id.to_string();
        self.write(move |c| {
            let (at, by) = resolved.map_or((None, None), |(a, b)| (Some(a), Some(b)));
            c.execute(
                "UPDATE comment_threads SET resolved_at = ?2, resolved_by = ?3 WHERE id = ?1",
                params![id, at, by],
            )?;
            Ok(())
        })
        .await
    }

    async fn add_agent_notifications(
        &self,
        rows: Vec<NewNotification>,
    ) -> Result<Vec<NotificationRow>> {
        self.write(move |c| {
            let tx = c.transaction()?;
            let now = ids::now_ms();
            let mut out = Vec::new();
            for r in rows {
                tx.execute(
                    "INSERT INTO agent_notifications (workspace_id, agent_id, kind, data, created_at, next_attempt_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![r.workspace_id, r.owner_id, r.kind, r.data.to_string(), now, r.next_attempt_at],
                )?;
                out.push(NotificationRow {
                    id: tx.last_insert_rowid(),
                    workspace_id: r.workspace_id,
                    owner_id: r.owner_id,
                    kind: r.kind,
                    data: r.data,
                    created_at: now,
                    delivered_at: None,
                    done_at: None,
                    attempts: 0,
                    error: None,
                });
            }
            tx.commit()?;
            Ok(out)
        })
        .await
    }

    async fn agent_notifications(
        &self,
        scope: NoteScope,
        after: Option<i64>,
        limit: i64,
    ) -> Result<Vec<NotificationRow>> {
        self.read(move |c| {
            let (col, key) = note_scope(&scope);
            let cond = match after {
                Some(_) => "id > ?2",
                None => "acked_at IS NULL AND ?2 IS NULL",
            };
            let mut st = c.prepare(&format!(
                "SELECT {NOTE_COLS} FROM agent_notifications WHERE {col} = ?1 AND {cond} ORDER BY id LIMIT ?3"
            ))?;
            Ok(st
                .query_map(params![key, after, limit], note_row)?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    async fn ack_agent_notifications(
        &self,
        scope: NoteScope,
        ids: Vec<i64>,
        upto: Option<i64>,
        now: i64,
    ) -> Result<u64> {
        self.write(move |c| {
            let tx = c.transaction()?;
            let (col, key) = note_scope(&scope);
            let mut n = 0;
            if let Some(u) = upto {
                n += tx.execute(
                    &format!("UPDATE agent_notifications SET acked_at = ?3 WHERE {col} = ?1 AND id <= ?2 AND acked_at IS NULL"),
                    params![key, u, now],
                )?;
            }
            for id in ids {
                n += tx.execute(
                    &format!("UPDATE agent_notifications SET acked_at = ?3 WHERE {col} = ?1 AND id = ?2 AND acked_at IS NULL"),
                    params![key, id, now],
                )?;
            }
            tx.commit()?;
            Ok(n as u64)
        })
        .await
    }

    async fn due_webhooks(&self, now: i64, limit: i64) -> Result<Vec<NotificationRow>> {
        self.read(move |c| {
            let mut st = c.prepare_cached(&format!(
                "SELECT {NOTE_COLS} FROM agent_notifications
                 WHERE next_attempt_at IS NOT NULL AND next_attempt_at <= ?1 ORDER BY next_attempt_at, id LIMIT ?2"
            ))?;
            Ok(st.query_map(params![now, limit], note_row)?.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    async fn next_webhook_at(&self) -> Result<Option<i64>> {
        self.read(|c| {
            Ok(c.query_row(
                "SELECT min(next_attempt_at) FROM agent_notifications WHERE next_attempt_at IS NOT NULL",
                [],
                |r| r.get(0),
            )?)
        })
        .await
    }

    async fn record_delivery(
        &self,
        id: i64,
        ok: bool,
        error: Option<String>,
        next: Option<i64>,
        now: i64,
    ) -> Result<()> {
        self.write(move |c| {
            if ok {
                c.execute(
                    "UPDATE agent_notifications SET delivered_at = ?2, acked_at = coalesce(acked_at, ?2),
                     attempts = attempts + 1, error = NULL, next_attempt_at = NULL WHERE id = ?1",
                    params![id, now],
                )?;
            } else {
                c.execute(
                    "UPDATE agent_notifications SET attempts = attempts + 1, error = ?2, next_attempt_at = ?3 WHERE id = ?1",
                    params![id, error, next],
                )?;
            }
            Ok(())
        })
        .await
    }

    async fn add_user_notifications(
        &self,
        rows: Vec<NewNotification>,
    ) -> Result<Vec<NotificationRow>> {
        self.write(move |c| {
            let tx = c.transaction()?;
            let now = ids::now_ms();
            let mut out = Vec::new();
            for r in rows {
                tx.execute(
                    "INSERT INTO user_notifications (user_id, workspace_id, kind, data, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![r.owner_id, r.workspace_id, r.kind, r.data.to_string(), now],
                )?;
                out.push(NotificationRow {
                    id: tx.last_insert_rowid(),
                    workspace_id: r.workspace_id,
                    owner_id: r.owner_id,
                    kind: r.kind,
                    data: r.data,
                    created_at: now,
                    delivered_at: None,
                    done_at: None,
                    attempts: 0,
                    error: None,
                });
            }
            tx.commit()?;
            Ok(out)
        })
        .await
    }

    async fn user_notifications(
        &self,
        user_id: &str,
        after: Option<i64>,
        limit: i64,
    ) -> Result<Vec<NotificationRow>> {
        let u = user_id.to_string();
        self.read(move |c| {
            // 收件箱：新的在前；after 往旧的翻页
            let mut st = c.prepare_cached(
                "SELECT id, workspace_id, user_id, kind, data, created_at, NULL, read_at, 0, NULL
                 FROM user_notifications WHERE user_id = ?1 AND (?2 IS NULL OR id < ?2) ORDER BY id DESC LIMIT ?3",
            )?;
            Ok(st.query_map(params![u, after, limit], note_row)?.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    async fn read_user_notifications(
        &self,
        user_id: &str,
        ids: Option<Vec<i64>>,
        now: i64,
    ) -> Result<u64> {
        let u = user_id.to_string();
        self.write(move |c| {
            let tx = c.transaction()?;
            let n = match ids {
                None => tx.execute(
                    "UPDATE user_notifications SET read_at = ?2 WHERE user_id = ?1 AND read_at IS NULL",
                    params![u, now],
                )?,
                Some(ids) => {
                    let mut n = 0;
                    for id in ids {
                        n += tx.execute(
                            "UPDATE user_notifications SET read_at = ?3 WHERE user_id = ?1 AND id = ?2 AND read_at IS NULL",
                            params![u, id, now],
                        )?;
                    }
                    n
                }
            };
            tx.commit()?;
            Ok(n as u64)
        })
        .await
    }

    async fn unread_user_notifications(&self, user_id: &str) -> Result<i64> {
        let u = user_id.to_string();
        self.read(move |c| {
            Ok(c.query_row(
                "SELECT count(*) FROM user_notifications WHERE user_id = ?1 AND read_at IS NULL",
                [u],
                |r| r.get(0),
            )?)
        })
        .await
    }

    async fn create_database(
        &self,
        page: NewPage,
        schema: serde_json::Value,
        views: serde_json::Value,
    ) -> Result<Page> {
        self.write(move |c| {
            let tx = c.transaction()?;
            let page = insert_page(&tx, page, None)?;
            tx.execute(
                "INSERT INTO databases (db_id, schema, views, seq, next_unique) VALUES (?1, ?2, ?3, 0, 1)",
                params![page.id, schema.to_string(), views.to_string()],
            )?;
            tx.commit()?;
            Ok(page)
        })
        .await
    }

    async fn database(&self, db_id: &str) -> Result<Option<DbMeta>> {
        let id = db_id.to_string();
        self.read(move |c| get_db_meta(c, &id)).await
    }

    async fn db_rows(
        &self,
        db_id: &str,
        since: Option<i64>,
    ) -> Result<(i64, Vec<DbRow>, Vec<String>)> {
        let id = db_id.to_string();
        self.read(move |c| {
            let tx = c.unchecked_transaction()?;
            let meta = get_db_meta(&tx, &id)?.ok_or_else(|| Error::NotFound(format!("数据库 {id} 不存在")))?;
            let (rows, deleted) = match since {
                None => {
                    let mut st = tx.prepare_cached(&format!(
                        "SELECT {ROW_COLS} FROM db_rows WHERE db_id = ?1 AND deleted_at IS NULL ORDER BY position, row_id"
                    ))?;
                    let rows: Vec<DbRow> = st.query_map([&id], db_row_of)?.collect::<rusqlite::Result<_>>()?;
                    (rows, Vec::new())
                }
                Some(s) => {
                    let mut st = tx.prepare_cached(&format!(
                        "SELECT {ROW_COLS} FROM db_rows WHERE db_id = ?1 AND seq > ?2 ORDER BY position, row_id"
                    ))?;
                    let all: Vec<DbRow> = st.query_map(params![id, s], db_row_of)?.collect::<rusqlite::Result<_>>()?;
                    let (gone, live): (Vec<DbRow>, Vec<DbRow>) = all.into_iter().partition(|r| r.deleted_at.is_some());
                    (live, gone.into_iter().map(|r| r.id).collect())
                }
            };
            tx.commit()?;
            Ok((meta.seq, rows, deleted))
        })
        .await
    }

    async fn db_row(&self, row_id: &str) -> Result<Option<DbRow>> {
        let id = row_id.to_string();
        self.read(move |c| {
            Ok(
                c.prepare_cached(&format!("SELECT {ROW_COLS} FROM db_rows WHERE row_id = ?1"))?
                    .query_row([id], db_row_of)
                    .optional()?,
            )
        })
        .await
    }

    async fn add_db_rows(
        &self,
        db_id: &str,
        rows: Vec<NewRow>,
        by: &str,
    ) -> Result<(Vec<String>, i64)> {
        let (id, by) = (db_id.to_string(), by.to_string());
        self.write(move |c| {
            let tx = c.transaction()?;
            let db = live_page(&tx, &id)?;
            let seq = bump_seq(&tx, &id)?;
            let mut next: i64 = tx.query_row("SELECT next_unique FROM databases WHERE db_id = ?1", [&id], |r| r.get(0))?;
            // 现有行的位置（按顺序），新行插进去
            let mut order: Vec<(String, String)> = tx
                .prepare_cached("SELECT row_id, position FROM db_rows WHERE db_id = ?1 AND deleted_at IS NULL ORDER BY position, row_id")?
                .query_map([&id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let now = ids::now_ms();
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                let place = match &r.after {
                    Some(a) => Placement::After(a.clone()),
                    None => Placement::Last,
                };
                let position = tree::position_for(&order, &place)?;
                let page = insert_page(
                    &tx,
                    NewPage {
                        workspace_id: db.workspace_id.clone(),
                        parent_id: Some(id.clone()),
                        place: Placement::Last,
                        title: r.title.clone(),
                        icon: None,
                        created_by: Some(by.clone()),
                        initial_state: r.initial_state,
                        kind: "row".into(),
                    },
                    Some(position.clone()),
                )?;
                let props = serde_json::Value::Object(r.props);
                tx.prepare_cached(&format!(
                    "INSERT INTO db_rows ({ROW_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?7, ?8, NULL)"
                ))?
                .execute(params![page.id, id, seq, position, props.to_string(), next, now, by])?;
                log_op(&tx, &At { db: &id, seq, by: &by, at: now }, &page.id, "*", &None, &Some(props))?;
                next += 1;
                // 维持顺序（插在 after 后面或者最后）
                let at = match &r.after {
                    Some(a) => order.iter().position(|(rid, _)| rid == a).map_or(order.len(), |i| i + 1),
                    None => order.len(),
                };
                order.insert(at, (page.id.clone(), position));
                out.push(page.id);
            }
            tx.execute("UPDATE databases SET next_unique = ?2 WHERE db_id = ?1", params![id, next])?;
            tx.commit()?;
            Ok((out, seq))
        })
        .await
    }

    async fn patch_db_rows(
        &self,
        db_id: &str,
        title_key: &str,
        ops: Vec<CellOp>,
        by: &str,
    ) -> Result<(i64, Vec<CellChange>)> {
        let (id, tk, by) = (db_id.to_string(), title_key.to_string(), by.to_string());
        self.write(move |c| {
            let tx = c.transaction()?;
            let cur: i64 = tx
                .query_row("SELECT seq FROM databases WHERE db_id = ?1", [&id], |r| r.get(0))
                .optional()?
                .ok_or_else(|| Error::NotFound(format!("数据库 {id} 不存在")))?;
            let seq = cur + 1;
            let now = ids::now_ms();
            let mut changes = Vec::new();
            for op in ops {
                let mut row = tx
                    .prepare_cached(&format!("SELECT {ROW_COLS} FROM db_rows WHERE row_id = ?1 AND db_id = ?2 AND deleted_at IS NULL"))?
                    .query_row(params![op.row_id, id], db_row_of)
                    .optional()?
                    .ok_or_else(|| Error::NotFound(format!("库里没有行 {}", op.row_id)))?;
                let mut changed = false;
                for (k, v) in op.set {
                    let old = row.props.get(&k).cloned();
                    let new = (!v.is_null()).then_some(v);
                    if old == new {
                        continue;
                    }
                    match &new {
                        Some(v) => row.props.insert(k.clone(), v.clone()),
                        None => row.props.remove(&k),
                    };
                    if k == tk {
                        let t = new.as_ref().and_then(|v| v.as_str()).unwrap_or("").to_string();
                        tx.execute("UPDATE pages SET title = ?2, updated_at = ?3 WHERE id = ?1", params![row.id, t, now])?;
                    }
                    log_op(&tx, &At { db: &id, seq, by: &by, at: now }, &row.id, &k, &old, &new)?;
                    changes.push(CellChange { row_id: row.id.clone(), prop: k, old, new });
                    changed = true;
                }
                // 挪位置：排在 after 后面（None 是最前面）
                if let Some(after) = op.after {
                    let order: Vec<(String, String)> = tx
                        .prepare_cached("SELECT row_id, position FROM db_rows WHERE db_id = ?1 AND deleted_at IS NULL AND row_id != ?2 ORDER BY position, row_id")?
                        .query_map(params![id, row.id], |r| Ok((r.get(0)?, r.get(1)?)))?
                        .collect::<rusqlite::Result<_>>()?;
                    let place = match after {
                        Some(a) => Placement::After(a),
                        None => Placement::First,
                    };
                    let pos = tree::position_for(&order, &place).map_err(|_| Error::BadRequest("after 不是这个库里的行".into()))?;
                    if pos != row.position {
                        let (old, new) = (Some(row.position.clone().into()), Some(pos.clone().into()));
                        tx.execute("UPDATE db_rows SET position = ?2 WHERE row_id = ?1", params![row.id, pos])?;
                        tx.execute("UPDATE pages SET position = ?2 WHERE id = ?1", params![row.id, pos])?;
                        log_op(&tx, &At { db: &id, seq, by: &by, at: now }, &row.id, "@position", &old, &new)?;
                        changes.push(CellChange { row_id: row.id.clone(), prop: "@position".into(), old, new });
                        tx.execute("UPDATE db_rows SET seq = ?2 WHERE row_id = ?1", params![row.id, seq])?;
                    }
                }
                if changed {
                    tx.prepare_cached("UPDATE db_rows SET props = ?2, seq = ?3, updated_at = ?4, updated_by = ?5 WHERE row_id = ?1")?
                        .execute(params![row.id, serde_json::Value::Object(row.props).to_string(), seq, now, by])?;
                }
            }
            let seq = if changes.is_empty() {
                cur
            } else {
                tx.execute("UPDATE databases SET seq = ?2 WHERE db_id = ?1", params![id, seq])?;
                seq
            };
            tx.commit()?;
            Ok((seq, changes))
        })
        .await
    }

    async fn delete_db_rows(&self, db_id: &str, ids: Vec<String>, by: &str) -> Result<i64> {
        let (id, by) = (db_id.to_string(), by.to_string());
        self.write(move |c| {
            let tx = c.transaction()?;
            let seq = bump_seq(&tx, &id)?;
            let now = ids::now_ms();
            for rid in ids {
                let Some(row) = tx
                    .prepare_cached(&format!("SELECT {ROW_COLS} FROM db_rows WHERE row_id = ?1 AND db_id = ?2 AND deleted_at IS NULL"))?
                    .query_row(params![rid, id], db_row_of)
                    .optional()?
                else {
                    continue;
                };
                tx.execute("UPDATE db_rows SET deleted_at = ?2, seq = ?3 WHERE row_id = ?1", params![rid, now, seq])?;
                tx.execute("UPDATE pages SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL", params![rid, now])?;
                log_op(&tx, &At { db: &id, seq, by: &by, at: now }, &rid, "*", &Some(serde_json::Value::Object(row.props)), &None)?;
            }
            tx.commit()?;
            Ok(seq)
        })
        .await
    }

    async fn set_db_schema(&self, db_id: &str, w: SchemaWrite, by: &str) -> Result<i64> {
        let (id, by) = (db_id.to_string(), by.to_string());
        self.write(move |c| {
            let tx = c.transaction()?;
            let seq = bump_seq(&tx, &id)?;
            let now = ids::now_ms();
            tx.execute("UPDATE databases SET schema = ?2 WHERE db_id = ?1", params![id, w.schema.to_string()])?;
            if let Some(v) = &w.views {
                tx.execute("UPDATE databases SET views = ?2 WHERE db_id = ?1", params![id, v.to_string()])?;
            }
            tx.execute(
                "INSERT INTO db_schema_ops (db_id, seq, op, before, after, by, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, seq, w.op, w.before.to_string(), w.after.to_string(), by, now],
            )?;
            // 按行分好，一行读一次写一次
            let mut by_row: Vec<(String, Vec<CellSet>)> = Vec::new();
            for cs in w.cells {
                match by_row.last_mut() {
                    Some((r, v)) if *r == cs.row_id => v.push(cs),
                    _ => by_row.push((cs.row_id.clone(), vec![cs])),
                }
            }
            for (rid, sets) in by_row {
                let Some(mut row) = tx
                    .prepare_cached(&format!("SELECT {ROW_COLS} FROM db_rows WHERE row_id = ?1 AND db_id = ?2"))?
                    .query_row(params![rid, id], db_row_of)
                    .optional()?
                else {
                    continue;
                };
                let mut changed = false;
                for cs in sets {
                    let cur = row.props.get(&cs.key).cloned();
                    if cs.only_if.as_ref().is_some_and(|e| *e != cur) || cur == cs.value {
                        continue;
                    }
                    match &cs.value {
                        Some(v) => row.props.insert(cs.key.clone(), v.clone()),
                        None => row.props.remove(&cs.key),
                    };
                    log_op(&tx, &At { db: &id, seq, by: &by, at: now }, &rid, &cs.key, &cur, &cs.value)?;
                    changed = true;
                }
                if changed {
                    tx.prepare_cached("UPDATE db_rows SET props = ?2, seq = ?3 WHERE row_id = ?1")?
                        .execute(params![rid, serde_json::Value::Object(row.props).to_string(), seq])?;
                }
            }
            tx.commit()?;
            Ok(seq)
        })
        .await
    }

    async fn db_schema_op(&self, db_id: &str, seq: i64) -> Result<Option<SchemaOpRow>> {
        let id = db_id.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached("SELECT seq, op, before, after, by, at FROM db_schema_ops WHERE db_id = ?1 AND seq = ?2")?
                .query_row(params![id, seq], |r| {
                    let b: Option<String> = r.get(2)?;
                    let a: Option<String> = r.get(3)?;
                    Ok(SchemaOpRow {
                        seq: r.get(0)?,
                        op: r.get(1)?,
                        before: b.and_then(|x| serde_json::from_str(&x).ok()).unwrap_or_default(),
                        after: a.and_then(|x| serde_json::from_str(&x).ok()).unwrap_or_default(),
                        by: r.get(4)?,
                        at: r.get(5)?,
                    })
                })
                .optional()?)
        })
        .await
    }

    async fn set_db_views(&self, db_id: &str, views: serde_json::Value) -> Result<()> {
        let id = db_id.to_string();
        self.write(move |c| {
            let n = c.execute(
                "UPDATE databases SET views = ?2 WHERE db_id = ?1",
                params![id, views.to_string()],
            )?;
            if n == 0 {
                return Err(Error::NotFound(format!("数据库 {id} 不存在")));
            }
            Ok(())
        })
        .await
    }

    async fn unfurl(&self, url_key: &str) -> Result<Option<UnfurlRow>> {
        let k = url_key.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached(
                "SELECT url_key, data, status, fetched_at, expires_at, error, failures FROM unfurls WHERE url_key = ?1",
            )?
            .query_row([k], |r| {
                let data: Option<String> = r.get(1)?;
                Ok(UnfurlRow {
                    url_key: r.get(0)?,
                    data: data.and_then(|d| serde_json::from_str(&d).ok()),
                    ok: r.get::<_, String>(2)? == "ok",
                    fetched_at: r.get(3)?,
                    expires_at: r.get(4)?,
                    error: r.get(5)?,
                    failures: r.get(6)?,
                })
            })
            .optional()?)
        })
        .await
    }

    async fn put_unfurl(&self, row: UnfurlRow) -> Result<()> {
        self.write(move |c| {
            c.prepare_cached(
                "INSERT INTO unfurls (url_key, data, status, fetched_at, expires_at, error, failures)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (url_key) DO UPDATE SET
                    data = coalesce(excluded.data, unfurls.data), status = excluded.status,
                    fetched_at = excluded.fetched_at, expires_at = excluded.expires_at,
                    error = excluded.error, failures = excluded.failures",
            )?
            .execute(params![
                row.url_key,
                row.data.map(|d| d.to_string()),
                if row.ok { "ok" } else { "error" },
                row.fetched_at,
                row.expires_at,
                row.error,
                row.failures
            ])?;
            Ok(())
        })
        .await
    }

    async fn put_blob(&self, b: BlobInfo) -> Result<()> {
        self.write(move |c| {
            c.prepare_cached(
                "INSERT OR IGNORE INTO blobs (hash, size, mime, width, height, blurhash, duration_ms, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?
            .execute(params![b.hash, b.size, b.mime, b.width, b.height, b.blurhash, b.duration_ms, ids::now_ms()])?;
            Ok(())
        })
        .await
    }

    async fn blob(&self, hash: &str) -> Result<Option<BlobInfo>> {
        let h = hash.to_string();
        self.read(move |c| {
            Ok(
                c.prepare_cached(&format!("SELECT {BLOB_COLS} FROM blobs WHERE hash = ?1"))?
                    .query_row([h], |r| blob_row(r, 0))
                    .optional()?,
            )
        })
        .await
    }

    async fn create_file(&self, f: FileRow) -> Result<()> {
        self.write(move |c| {
            c.execute(
                "INSERT INTO files (id, workspace_id, hash, name, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![f.id, f.workspace_id, f.blob.hash, f.name, f.created_by, f.created_at],
            )?;
            Ok(())
        })
        .await
    }

    async fn file(&self, id: &str) -> Result<Option<FileRow>> {
        let id = id.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached(&format!(
                "SELECT f.id, f.workspace_id, f.name, f.created_by, f.created_at, {} FROM files f
                 JOIN blobs b ON b.hash = f.hash WHERE f.id = ?1",
                BLOB_COLS
                    .split(", ")
                    .map(|x| format!("b.{x}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))?
            .query_row([id], |r| {
                Ok(FileRow {
                    id: r.get(0)?,
                    workspace_id: r.get(1)?,
                    name: r.get(2)?,
                    created_by: r.get(3)?,
                    created_at: r.get(4)?,
                    blob: blob_row(r, 5)?,
                })
            })
            .optional()?)
        })
        .await
    }

    async fn put_media(&self, hash: &str, source_url: &str) -> Result<()> {
        let (h, u) = (hash.to_string(), source_url.to_string());
        self.write(move |c| {
            c.execute(
                "INSERT OR IGNORE INTO media (hash, source_url, created_at) VALUES (?1, ?2, ?3)",
                params![h, u, ids::now_ms()],
            )?;
            Ok(())
        })
        .await
    }

    async fn media_by_source(&self, source_url: &str) -> Result<Option<BlobInfo>> {
        let u = source_url.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached(&format!(
                "SELECT {} FROM media m JOIN blobs b ON b.hash = m.hash WHERE m.source_url = ?1 LIMIT 1",
                BLOB_COLS.split(", ").map(|x| format!("b.{x}")).collect::<Vec<_>>().join(", ")
            ))?
            .query_row([u], |r| blob_row(r, 0))
            .optional()?)
        })
        .await
    }

    async fn is_media(&self, hash: &str) -> Result<bool> {
        let h = hash.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached("SELECT 1 FROM media WHERE hash = ?1")?
                .query_row([h], |_| Ok(()))
                .optional()?
                .is_some())
        })
        .await
    }

    async fn purge_notifications(&self, before: i64) -> Result<u64> {
        self.write(move |c| {
            let a = c.execute(
                "DELETE FROM agent_notifications WHERE created_at < ?1",
                [before],
            )?;
            let b = c.execute(
                "DELETE FROM user_notifications WHERE created_at < ?1",
                [before],
            )?;
            Ok((a + b) as u64)
        })
        .await
    }
}

const AGENT_COLS: &str = "id, workspace_id, name, about, scope, webhook, webhook_secret, created_by, created_at, last_used_at, revoked_at";

fn agent_row(r: &Row) -> rusqlite::Result<Agent> {
    Ok(Agent {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        name: r.get(2)?,
        about: r.get(3)?,
        scope: Scope::parse(&r.get::<_, String>(4)?).unwrap_or(Scope::Read),
        webhook: r.get(5)?,
        webhook_secret: r.get(6)?,
        created_by: r.get(7)?,
        created_at: r.get(8)?,
        last_used_at: r.get(9)?,
        revoked_at: r.get(10)?,
    })
}

fn get_agent(c: &Connection, id: &str) -> Result<Option<Agent>> {
    Ok(
        c.prepare_cached(&format!("SELECT {AGENT_COLS} FROM agents WHERE id = ?1"))?
            .query_row([id], agent_row)
            .optional()?,
    )
}

const THREAD_COLS: &str = "id, page_id, block_id, anchor_start, anchor_end, quote, created_by, created_at, resolved_at, resolved_by";

fn thread_row(r: &Row) -> rusqlite::Result<Thread> {
    Ok(Thread {
        id: r.get(0)?,
        page_id: r.get(1)?,
        block_id: r.get(2)?,
        anchor_start: r.get(3)?,
        anchor_end: r.get(4)?,
        quote: r.get(5)?,
        created_by: r.get(6)?,
        created_at: r.get(7)?,
        resolved_at: r.get(8)?,
        resolved_by: r.get(9)?,
    })
}

const COMMENT_COLS: &str =
    "id, thread_id, author_kind, author_id, body, created_at, edited_at, deleted_at";

fn comment_row(r: &Row) -> rusqlite::Result<Comment> {
    Ok(Comment {
        id: r.get(0)?,
        thread_id: r.get(1)?,
        author_kind: PrincipalKind::parse(&r.get::<_, String>(2)?),
        author_id: r.get(3)?,
        body: r.get(4)?,
        created_at: r.get(5)?,
        edited_at: r.get(6)?,
        deleted_at: r.get(7)?,
    })
}

fn insert_comment(c: &Connection, cm: &Comment) -> Result<()> {
    c.execute(
        &format!("INSERT INTO comments ({COMMENT_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
        params![
            cm.id,
            cm.thread_id,
            cm.author_kind.as_str(),
            cm.author_id,
            cm.body,
            cm.created_at,
            cm.edited_at,
            cm.deleted_at
        ],
    )?;
    Ok(())
}

const NOTE_COLS: &str =
    "id, workspace_id, agent_id, kind, data, created_at, delivered_at, acked_at, attempts, error";

fn note_row(r: &Row) -> rusqlite::Result<NotificationRow> {
    let data: String = r.get(4)?;
    Ok(NotificationRow {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        owner_id: r.get(2)?,
        kind: r.get(3)?,
        data: serde_json::from_str(&data).unwrap_or(serde_json::Value::Null),
        created_at: r.get(5)?,
        delivered_at: r.get(6)?,
        done_at: r.get(7)?,
        attempts: r.get(8)?,
        error: r.get(9)?,
    })
}

fn note_scope(s: &NoteScope) -> (&'static str, String) {
    match s {
        NoteScope::Agent(a) => ("agent_id", a.clone()),
        NoteScope::Workspace(w) => ("workspace_id", w.clone()),
    }
}

const BLOB_COLS: &str = "hash, size, mime, width, height, blurhash, duration_ms";

fn blob_row(r: &Row, at: usize) -> rusqlite::Result<BlobInfo> {
    Ok(BlobInfo {
        hash: r.get(at)?,
        size: r.get(at + 1)?,
        mime: r.get(at + 2)?,
        width: r.get(at + 3)?,
        height: r.get(at + 4)?,
        blurhash: r.get(at + 5)?,
        duration_ms: r.get(at + 6)?,
    })
}

/// 插一个页面（含正文的第一份快照）。position 不给就按 p.place 在同级里算。
fn insert_page(tx: &Connection, p: NewPage, position: Option<String>) -> Result<Page> {
    let ws_ok = tx
        .query_row(
            "SELECT 1 FROM workspaces WHERE id = ?1",
            [&p.workspace_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !ws_ok {
        return Err(Error::NotFound(format!("工作区 {} 不存在", p.workspace_id)));
    }
    let id = ids::new_id();
    let position = match position {
        Some(pos) => pos,
        None => {
            if let Some(parent) = &p.parent_id {
                check_parent(tx, &p.workspace_id, parent)?;
            }
            let sib = siblings(tx, &p.workspace_id, p.parent_id.as_deref(), &id)?;
            tree::position_for(&sib, &p.place)?
        }
    };
    let now = ids::now_ms();
    let page = Page {
        position,
        id,
        workspace_id: p.workspace_id,
        parent_id: p.parent_id,
        title: p.title,
        icon: p.icon,
        created_by: p.created_by,
        created_at: now,
        updated_at: now,
        deleted_at: None,
        kind: p.kind,
    };
    tx.prepare_cached(&format!(
        "INSERT INTO pages ({PAGE_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10)"
    ))?
    .execute(params![
        page.id,
        page.workspace_id,
        page.parent_id,
        page.position,
        page.title,
        page.icon,
        page.created_by,
        page.created_at,
        page.updated_at,
        page.kind
    ])?;
    tx.prepare_cached(
        "INSERT INTO doc_snapshots (page_id, state, updated_at) VALUES (?1, ?2, ?3)",
    )?
    .execute(params![page.id, p.initial_state, now])?;
    Ok(page)
}

fn get_db_meta(c: &Connection, db_id: &str) -> Result<Option<DbMeta>> {
    Ok(
        c.prepare_cached("SELECT db_id, schema, views, seq FROM databases WHERE db_id = ?1")?
            .query_row([db_id], |r| {
                let schema: String = r.get(1)?;
                let views: String = r.get(2)?;
                Ok(DbMeta {
                    db_id: r.get(0)?,
                    schema: serde_json::from_str(&schema).unwrap_or_default(),
                    views: serde_json::from_str(&views).unwrap_or_default(),
                    seq: r.get(3)?,
                })
            })
            .optional()?,
    )
}

const ROW_COLS: &str = "row_id, db_id, seq, position, props, unique_id, created_at, created_by, updated_at, updated_by, deleted_at";

fn db_row_of(r: &Row) -> rusqlite::Result<DbRow> {
    let props: String = r.get(4)?;
    Ok(DbRow {
        id: r.get(0)?,
        db_id: r.get(1)?,
        seq: r.get(2)?,
        position: r.get(3)?,
        props: serde_json::from_str(&props).unwrap_or_default(),
        unique_id: r.get(5)?,
        created_at: r.get(6)?,
        created_by: r.get(7)?,
        updated_at: r.get(8)?,
        updated_by: r.get(9)?,
        deleted_at: r.get(10)?,
    })
}

/// 库的 seq 加一，返回新的。
fn bump_seq(tx: &Connection, db_id: &str) -> Result<i64> {
    let seq: Option<i64> = tx
        .prepare_cached("UPDATE databases SET seq = seq + 1 WHERE db_id = ?1 RETURNING seq")?
        .query_row([db_id], |r| r.get(0))
        .optional()?;
    seq.ok_or_else(|| Error::NotFound(format!("数据库 {db_id} 不存在")))
}

fn json_opt(v: &Option<serde_json::Value>) -> Option<String> {
    v.as_ref().map(|v| v.to_string())
}

/// 记一格改动时的公共部分：哪个库、第几版、谁、什么时候。
struct At<'a> {
    db: &'a str,
    seq: i64,
    by: &'a str,
    at: i64,
}

fn log_op(
    tx: &Connection,
    w: &At,
    row: &str,
    prop: &str,
    old: &Option<serde_json::Value>,
    new: &Option<serde_json::Value>,
) -> Result<()> {
    tx.prepare_cached(
        "INSERT INTO db_ops (db_id, seq, row_id, prop, old, new, by, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?
    .execute(params![w.db, w.seq, row, prop, json_opt(old), json_opt(new), w.by, w.at])?;
    Ok(())
}

/// 表结构里标题属性的 key（第一个 type = title 的）。
fn title_key_of(schema: &serde_json::Value) -> Option<String> {
    schema["props"]
        .as_array()?
        .iter()
        .find(|p| p["type"] == "title")
        .and_then(|p| p["key"].as_str())
        .map(str::to_string)
}
