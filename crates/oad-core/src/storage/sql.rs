//! [`Storage`] 的实现：SQL 和逻辑只有这一份，跑在 SQLite（[`super::sqlite`]）或 Postgres（[`super::pg`]）上。
//! 读写都是一个同步闭包，交给引擎去跑：SQLite 写交给写线程、读用连接池；Postgres 每次一个事务，写的时候整个库排队
//! （和 SQLite 只有一个写入者一样，算位置、加 seq 不会撞）。SQL 按 SQLite 写，Postgres 那边换写法见 [`super::db::pg_sql`]。
use super::Storage;
use super::db::{Conn, DbError, OptionalExt, Row, RowResult, params};
use crate::model::*;
use crate::{Error, Result, ids, tree};
use async_trait::async_trait;
use std::future::Future;

/// 跑读写闭包的引擎。
pub trait Engine: Send + Sync + 'static {
    fn run<R, F>(&self, write: bool, f: F) -> impl Future<Output = Result<R>> + Send
    where
        R: Send + 'static,
        F: FnOnce(&mut Conn) -> Result<R> + Send + 'static;
}

/// 存储：引擎 + 共用的 SQL。用 [`super::sqlite::Sqlite`] 或 [`super::pg::Postgres`]。
pub struct SqlStore<E> {
    pub(super) engine: E,
}

impl<E: Engine> SqlStore<E> {
    async fn write<R, F>(&self, f: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Conn) -> Result<R> + Send + 'static,
    {
        self.engine.run(true, f).await
    }

    async fn read<R, F>(&self, f: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Conn) -> Result<R> + Send + 'static,
    {
        self.engine.run(false, f).await
    }
}

const PAGE_COLS: &str = "id, workspace_id, parent_id, position, title, icon, created_by, created_at, updated_at, deleted_at, kind";

fn page_row(r: &Row) -> RowResult<Page> {
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

fn user_row(r: &Row) -> RowResult<User> {
    Ok(User {
        id: r.get(0)?,
        name: r.get(1)?,
        email: r.get(2)?,
        kind: PrincipalKind::parse(&r.get::<String>(3)?),
        created_at: r.get(4)?,
    })
}

fn get_page(c: &mut Conn, id: &str) -> Result<Option<Page>> {
    Ok(c.query_row(
        &format!("SELECT {PAGE_COLS} FROM pages WHERE id = ?1"),
        params![id],
        page_row,
    )
    .optional()?)
}

fn live_page(c: &mut Conn, id: &str) -> Result<Page> {
    match get_page(c, id)? {
        Some(p) if p.deleted_at.is_none() => Ok(p),
        _ => Err(Error::NotFound(format!("页面 {id} 不存在"))),
    }
}

/// 检查新父页面：存在、同一工作区、不在回收站。
fn check_parent(c: &mut Conn, ws: &str, parent: &str) -> Result<()> {
    match get_page(c, parent)? {
        Some(p) if p.deleted_at.is_none() && p.workspace_id == ws => Ok(()),
        _ => Err(Error::BadRequest(format!("父页面 {parent} 不存在"))),
    }
}

/// 同级（不含 except）按顺序的 (id, position)。
fn siblings(
    c: &mut Conn,
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
    Ok(rows)
}

fn ancestors(c: &mut Conn, id: &str) -> Result<Vec<String>> {
    let mut st = c.prepare_cached(
        "WITH RECURSIVE anc(id, parent_id) AS (
            SELECT id, parent_id FROM pages WHERE id = ?1
            UNION ALL
            SELECT p.id, p.parent_id FROM pages p JOIN anc ON p.id = anc.parent_id
         ) SELECT id FROM anc",
    )?;
    let rows = st.query_map(params![id], |r| r.get(0))?;
    Ok(rows)
}

fn sql_err(e: DbError) -> Error {
    match e {
        DbError::Constraint(m) => Error::Conflict(m),
        e => e.into(),
    }
}

#[async_trait]
impl<E: Engine> Storage for SqlStore<E> {
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
                params![id],
                user_row,
            )
            .optional()?)
        })
        .await
    }

    async fn user_by_email(&self, email: &str) -> Result<Option<(User, Option<String>)>> {
        let email = email.to_string();
        self.read(move |c| {
            // 邮箱不分大小写：SQLite 靠列上的 COLLATE NOCASE；Postgres 用 lower()（lower(email) 上有唯一索引）
            let sql = if c.is_pg() {
                "SELECT id, name, email, kind, created_at, password_hash FROM users WHERE lower(email) = lower(?1)"
            } else {
                "SELECT id, name, email, kind, created_at, password_hash FROM users WHERE email = ?1"
            };
            Ok(c.query_row(
                sql,
                params![email],
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
                params![ext],
                user_row,
            )
            .optional()?)
        })
        .await
    }

    async fn count_users(&self) -> Result<u64> {
        self.read(|c| {
            Ok(c.query_row("SELECT count(*) FROM users", params![], |r| r.get::<i64>(0))? as u64)
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
            c.execute("DELETE FROM sessions WHERE expires_at < ?1", params![now])?;
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
            c.execute("DELETE FROM sessions WHERE token_hash = ?1", params![t])?;
            Ok(())
        })
        .await
    }

    async fn create_workspace(&self, name: &str) -> Result<Workspace> {
        let name = name.to_string();
        self.write(move |c| {
            let w = Workspace {
                icon: None,
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
            let mut st = c.prepare_cached(
                "SELECT id, name, created_at, icon FROM workspaces ORDER BY created_at, id",
            )?;
            let rows = st.query_map(params![], |r| {
                Ok(Workspace {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                    icon: r.get(3)?,
                })
            })?;
            Ok(rows)
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
            let mut st = c.prepare_cached(
                "SELECT w.id, w.name, w.created_at, m.role, w.icon FROM members m
                 JOIN workspaces w ON w.id = m.workspace_id WHERE m.user_id = ?1
                 ORDER BY w.created_at, w.id",
            )?;
            let rows = st.query_map(params![u], |r| {
                Ok((
                    Workspace {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        created_at: r.get(2)?,
                        icon: r.get(4)?,
                    },
                    r.get::<String>(3)?,
                ))
            })?;
            let mut out = Vec::new();
            for (w, role) in rows {
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
            let mut tx = c.transaction()?;
            let page = insert_page(&mut tx, p, None)?;
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
                 ORDER BY parent_id NULLS FIRST, position, id"
            ))?;
            let rows = st.query_map(params![w], page_row)?;
            Ok(rows)
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
            let mut tx = c.transaction()?;
            let mut p = live_page(&mut tx, &id)?;
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
                && let Some(meta) = get_db_meta(&mut tx, db)?
                && let Some(key) = title_key_of(&meta.schema)
                && let Some(mut row) = tx
                    .query_row(
                        &format!("SELECT {ROW_COLS} FROM db_rows WHERE row_id = ?1"),
                        params![&p.id],
                        db_row_of,
                    )
                    .optional()?
            {
                let seq = bump_seq(&mut tx, db)?;
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
                    &mut tx,
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
            let mut tx = c.transaction()?;
            let mut p = live_page(&mut tx, &id)?;
            if let Some(parent) = &parent_id {
                check_parent(&mut tx, &p.workspace_id, parent)?;
                tree::check_cycle(&id, &ancestors(&mut tx, parent)?)?;
            }
            let sib = siblings(&mut tx, &p.workspace_id, parent_id.as_deref(), &id)?;
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
            let mut tx = c.transaction()?;
            let root = live_page(&mut tx, &id)?;
            let now = ids::now_ms();
            // 删的是库里的一行：库里也去掉（记一条改动）
            if root.kind == "row"
                && let Some(db) = &root.parent_id
                && let Some(row) = tx
                    .query_row(
                        &format!("SELECT {ROW_COLS} FROM db_rows WHERE row_id = ?1 AND deleted_at IS NULL"),
                        params![&id],
                        db_row_of,
                    )
                    .optional()?
            {
                let seq = bump_seq(&mut tx, db)?;
                tx.execute(
                    "UPDATE db_rows SET deleted_at = ?2, seq = ?3 WHERE row_id = ?1",
                    params![id, now, seq],
                )?;
                log_op(
                    &mut tx,
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
                let mut st = tx.prepare_cached(
                    "WITH RECURSIVE sub(id, depth) AS (
                        SELECT id, 0 FROM pages WHERE id = ?1
                        UNION ALL
                        SELECT p.id, sub.depth + 1 FROM pages p JOIN sub ON p.parent_id = sub.id
                        WHERE p.deleted_at IS NULL
                     ) SELECT id FROM sub ORDER BY depth",
                )?;
                st.query_map(params![&id], |r| r.get(0))?

            };
            let mut out = Vec::with_capacity(ids.len());
            for pid in &ids {
                tx.execute(
                    "UPDATE pages SET deleted_at = ?2, trash_root = ?3 WHERE id = ?1 AND deleted_at IS NULL",
                    params![pid, now, id],
                )?;
                if let Some(p) = get_page(&mut tx, pid)? {
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
            let mut tx = c.transaction()?;
            let snapshot = tx
                .prepare_cached("SELECT state FROM doc_snapshots WHERE page_id = ?1")?
                .query_row(params![&id], |r| r.get(0))
                .optional()?;
            let updates = tx
                .prepare_cached("SELECT data FROM doc_updates WHERE page_id = ?1 ORDER BY seq")?
                .query_map(params![&id], |r| r.get(0))?;
            tx.commit()?;
            Ok(DocLog { snapshot, updates })
        })
        .await
    }

    async fn append_update(&self, page_id: &str, update: Vec<u8>) -> Result<()> {
        let id = page_id.to_string();
        self.write(move |c| {
            let mut tx = c.transaction()?;
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
            let mut tx = c.transaction()?;
            tx.execute(
                "INSERT INTO doc_snapshots (page_id, state, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (page_id) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
                params![id, snapshot, ids::now_ms()],
            )?;
            tx.execute("DELETE FROM doc_updates WHERE page_id = ?1", params![&id])?;
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
            let rows = st.query_map(params![w], |r| Ok((user_row(r)?, r.get::<String>(5)?)))?;
            let mut out = Vec::new();
            for (u, role) in rows {
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
            let mut tx = c.transaction()?;
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
                get_agent(&mut tx, &id)?.ok_or_else(|| Error::Internal("建 Agent 失败".into()))?;
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
            .query_row(params![t], agent_row)
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
            let rows = st.query_map(params![w], agent_row)?;
            Ok(rows)
        })
        .await
    }

    async fn update_agent(&self, id: &str, p: AgentPatch) -> Result<Agent> {
        let id = id.to_string();
        self.write(move |c| {
            let mut tx = c.transaction()?;
            let mut a = get_agent(&mut tx, &id)?
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
            let mut tx = c.transaction()?;
            let a = get_agent(&mut tx, &id)?.ok_or_else(|| Error::NotFound(format!("Agent {id} 不存在")))?;
            tx.execute(
                "UPDATE agents SET revoked_at = coalesce(revoked_at, ?2), token_hash = NULL WHERE id = ?1",
                params![id, ids::now_ms()],
            )?;
            tx.execute(
                "DELETE FROM members WHERE workspace_id = ?1 AND user_id = ?2",
                params![a.workspace_id, id],
            )?;
            tx.execute("DELETE FROM sessions WHERE user_id = ?1", params![&id])?;
            let a = get_agent(&mut tx, &id)?.unwrap();
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
            let rows = st.query_map(params![w], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows)
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
            let mut tx = c.transaction()?;
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
            let rows = st.query_map(params![p], |r| {
                Ok((
                    r.get::<i64>(0)? as u64,
                    PrincipalKind::parse(&r.get::<String>(1)?),
                    r.get(2)?,
                    r.get(3)?,
                ))
            })?;
            Ok(rows)
        })
        .await
    }

    async fn create_thread(&self, t: Thread, first: Comment) -> Result<()> {
        self.write(move |c| {
            let mut tx = c.transaction()?;
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
            insert_comment(&mut tx, &first)?;
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
            .query_row(params![id], thread_row)
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
                .prepare_cached(&format!(
                    "SELECT {THREAD_COLS} FROM comment_threads WHERE page_id = ?1 {filter} ORDER BY created_at, id"
                ))?
                .query_map(params![&p], thread_row)?;
            let mut st = c.prepare_cached(&format!(
                "SELECT {COMMENT_COLS} FROM comments WHERE thread_id = ?1 ORDER BY created_at, id"
            ))?;
            let mut out = Vec::with_capacity(threads.len());
            for t in threads {
                let cs = st
                    .query_map(params![&t.id], comment_row)?;
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
            Ok(st.query_map(params![t], comment_row)?)
        })
        .await
    }

    async fn comment(&self, id: &str) -> Result<Option<Comment>> {
        let id = id.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached(&format!(
                "SELECT {COMMENT_COLS} FROM comments WHERE id = ?1"
            ))?
            .query_row(params![id], comment_row)
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
            let mut tx = c.transaction()?;
            let now = ids::now_ms();
            let mut out = Vec::new();
            for r in rows {
                let id: i64 = tx.query_row(
                    "INSERT INTO agent_notifications (workspace_id, agent_id, kind, data, created_at, next_attempt_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
                    params![r.workspace_id, r.owner_id, r.kind, r.data.to_string(), now, r.next_attempt_at],
                    |r| r.get(0),
                )?;
                out.push(NotificationRow {
                    id,
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
                None => "acked_at IS NULL AND CAST(?2 AS BIGINT) IS NULL",
            };
            let mut st = c.prepare_cached(&format!(
                "SELECT {NOTE_COLS} FROM agent_notifications WHERE {col} = ?1 AND {cond} ORDER BY id LIMIT ?3"
            ))?;
            Ok(st
                .query_map(params![key, after, limit], note_row)?
                )
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
            let mut tx = c.transaction()?;
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
            Ok(st.query_map(params![now, limit], note_row)?)
        })
        .await
    }

    async fn next_webhook_at(&self) -> Result<Option<i64>> {
        self.read(|c| {
            Ok(c.query_row(
                "SELECT min(next_attempt_at) FROM agent_notifications WHERE next_attempt_at IS NOT NULL",
                params![],
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
            let mut tx = c.transaction()?;
            let now = ids::now_ms();
            let mut out = Vec::new();
            for r in rows {
                let id: i64 = tx.query_row(
                    "INSERT INTO user_notifications (user_id, workspace_id, kind, data, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
                     RETURNING id",
                    params![r.owner_id, r.workspace_id, r.kind, r.data.to_string(), now],
                    |r| r.get(0),
                )?;
                out.push(NotificationRow {
                    id,
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
                 FROM user_notifications WHERE user_id = ?1 AND (id < ?2 OR ?2 IS NULL) ORDER BY id DESC LIMIT ?3",
            )?;
            Ok(st.query_map(params![u, after, limit], note_row)?)
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
            let mut tx = c.transaction()?;
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
                params![u],
                |r| r.get(0),
            )?)
        })
        .await
    }

    async fn all_pages(&self) -> Result<Vec<Page>> {
        self.read(|c| {
            let mut st = c.prepare_cached(&format!("SELECT {PAGE_COLS} FROM pages"))?;
            let rows = st.query_map(params![], page_row)?;
            Ok(rows)
        })
        .await
    }

    async fn db_seqs(&self) -> Result<Vec<(String, i64)>> {
        self.read(|c| {
            let mut st = c.prepare_cached("SELECT db_id, seq FROM databases")?;
            let rows = st.query_map(params![], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows)
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
            let mut tx = c.transaction()?;
            let page = insert_page(&mut tx, page, None)?;
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
            let mut tx = c.transaction()?;
            let meta = get_db_meta(&mut tx, &id)?.ok_or_else(|| Error::NotFound(format!("数据库 {id} 不存在")))?;
            let (rows, deleted) = match since {
                None => {
                    let mut st = tx.prepare_cached(&format!(
                        "SELECT {ROW_COLS} FROM db_rows WHERE db_id = ?1 AND deleted_at IS NULL ORDER BY position, row_id"
                    ))?;
                    let rows: Vec<DbRow> = st.query_map(params![&id], db_row_of)?;
                    (rows, Vec::new())
                }
                Some(s) => {
                    let mut st = tx.prepare_cached(&format!(
                        "SELECT {ROW_COLS} FROM db_rows WHERE db_id = ?1 AND seq > ?2 ORDER BY position, row_id"
                    ))?;
                    let all: Vec<DbRow> = st.query_map(params![id, s], db_row_of)?;
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
                    .query_row(params![id], db_row_of)
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
            let mut tx = c.transaction()?;
            let db = live_page(&mut tx, &id)?;
            let seq = bump_seq(&mut tx, &id)?;
            let mut next: i64 = tx.query_row("SELECT next_unique FROM databases WHERE db_id = ?1", params![&id], |r| r.get(0))?;
            // 现有行的位置（按顺序），新行插进去
            let mut order: Vec<(String, String)> = tx
                .prepare_cached("SELECT row_id, position FROM db_rows WHERE db_id = ?1 AND deleted_at IS NULL ORDER BY position, row_id")?
                .query_map(params![&id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let now = ids::now_ms();
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                let place = match &r.after {
                    Some(a) => Placement::After(a.clone()),
                    None => Placement::Last,
                };
                let position = tree::position_for(&order, &place)?;
                let page = insert_page(
                    &mut tx,
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
                log_op(&mut tx, &At { db: &id, seq, by: &by, at: now }, &page.id, "*", &None, &Some(props))?;
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
            let mut tx = c.transaction()?;
            let cur: i64 = tx
                .query_row("SELECT seq FROM databases WHERE db_id = ?1", params![&id], |r| r.get(0))
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
                    log_op(&mut tx, &At { db: &id, seq, by: &by, at: now }, &row.id, &k, &old, &new)?;
                    changes.push(CellChange { row_id: row.id.clone(), prop: k, old, new });
                    changed = true;
                }
                // 挪位置：排在 after 后面（None 是最前面）
                if let Some(after) = op.after {
                    let order: Vec<(String, String)> = tx
                        .prepare_cached("SELECT row_id, position FROM db_rows WHERE db_id = ?1 AND deleted_at IS NULL AND row_id != ?2 ORDER BY position, row_id")?
                        .query_map(params![id, row.id], |r| Ok((r.get(0)?, r.get(1)?)))?;
                    let place = match after {
                        Some(a) => Placement::After(a),
                        None => Placement::First,
                    };
                    let pos = tree::position_for(&order, &place).map_err(|_| Error::BadRequest("after 不是这个库里的行".into()))?;
                    if pos != row.position {
                        let (old, new) = (Some(row.position.clone().into()), Some(pos.clone().into()));
                        tx.execute("UPDATE db_rows SET position = ?2 WHERE row_id = ?1", params![row.id, pos])?;
                        tx.execute("UPDATE pages SET position = ?2 WHERE id = ?1", params![row.id, pos])?;
                        log_op(&mut tx, &At { db: &id, seq, by: &by, at: now }, &row.id, "@position", &old, &new)?;
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
            let mut tx = c.transaction()?;
            let seq = bump_seq(&mut tx, &id)?;
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
                tx.execute(
                    "UPDATE pages SET deleted_at = ?2, trash_root = ?1 WHERE id = ?1 AND deleted_at IS NULL",
                    params![rid, now],
                )?;
                log_op(&mut tx, &At { db: &id, seq, by: &by, at: now }, &rid, "*", &Some(serde_json::Value::Object(row.props)), &None)?;
            }
            tx.commit()?;
            Ok(seq)
        })
        .await
    }

    async fn set_db_schema(&self, db_id: &str, w: SchemaWrite, by: &str) -> Result<i64> {
        let (id, by) = (db_id.to_string(), by.to_string());
        self.write(move |c| {
            let mut tx = c.transaction()?;
            let seq = bump_seq(&mut tx, &id)?;
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
                    log_op(&mut tx, &At { db: &id, seq, by: &by, at: now }, &rid, &cs.key, &cur, &cs.value)?;
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

    async fn user_prefs(&self, user_id: &str) -> Result<UserPrefs> {
        let id = user_id.to_string();
        self.read(move |c| {
            Ok(
                c.prepare_cached("SELECT avatar, locale, timezone FROM users WHERE id = ?1")?
                    .query_row(params![id], |r| {
                        Ok(UserPrefs {
                            avatar: r.get(0)?,
                            locale: r.get(1)?,
                            timezone: r.get(2)?,
                        })
                    })
                    .optional()?
                    .unwrap_or_default(),
            )
        })
        .await
    }

    async fn update_user(&self, user_id: &str, p: PrefsPatch) -> Result<()> {
        let id = user_id.to_string();
        self.write(move |c| {
            let mut tx = c.transaction()?;
            if let Some(n) = p.name {
                tx.execute("UPDATE users SET name = ?2 WHERE id = ?1", params![id, n])?;
            }
            if let Some(a) = p.avatar {
                tx.execute("UPDATE users SET avatar = ?2 WHERE id = ?1", params![id, a])?;
            }
            if let Some(l) = p.locale {
                tx.execute("UPDATE users SET locale = ?2 WHERE id = ?1", params![id, l])?;
            }
            if let Some(t) = p.timezone {
                tx.execute(
                    "UPDATE users SET timezone = ?2 WHERE id = ?1",
                    params![id, t],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    async fn password_hash(&self, user_id: &str) -> Result<Option<String>> {
        let id = user_id.to_string();
        self.read(move |c| {
            Ok(
                c.prepare_cached("SELECT password_hash FROM users WHERE id = ?1")?
                    .query_row(params![id], |r| r.get(0))
                    .optional()?
                    .flatten(),
            )
        })
        .await
    }

    async fn delete_user_sessions(&self, user_id: &str, keep: Option<&str>) -> Result<()> {
        let (id, keep) = (user_id.to_string(), keep.map(str::to_string));
        self.write(move |c| {
            c.execute(
                "DELETE FROM sessions WHERE user_id = ?1 AND (token_hash != ?2 OR ?2 IS NULL)",
                params![id, keep],
            )?;
            Ok(())
        })
        .await
    }

    async fn remove_member(&self, workspace_id: &str, user_id: &str) -> Result<bool> {
        let (w, u) = (workspace_id.to_string(), user_id.to_string());
        self.write(move |c| {
            Ok(c.execute(
                "DELETE FROM members WHERE workspace_id = ?1 AND user_id = ?2",
                params![w, u],
            )? > 0)
        })
        .await
    }

    async fn update_workspace(
        &self,
        id: &str,
        name: Option<String>,
        icon: Option<Option<String>>,
    ) -> Result<Workspace> {
        let id = id.to_string();
        self.write(move |c| {
            let mut tx = c.transaction()?;
            if let Some(n) = name {
                tx.execute(
                    "UPDATE workspaces SET name = ?2 WHERE id = ?1",
                    params![id, n],
                )?;
            }
            if let Some(i) = icon {
                tx.execute(
                    "UPDATE workspaces SET icon = ?2 WHERE id = ?1",
                    params![id, i],
                )?;
            }
            let w = tx
                .query_row(
                    "SELECT id, name, created_at, icon FROM workspaces WHERE id = ?1",
                    params![&id],
                    |r| {
                        Ok(Workspace {
                            id: r.get(0)?,
                            name: r.get(1)?,
                            created_at: r.get(2)?,
                            icon: r.get(3)?,
                        })
                    },
                )
                .optional()?
                .ok_or_else(|| Error::NotFound(format!("工作区 {id} 不存在")))?;
            tx.commit()?;
            Ok(w)
        })
        .await
    }

    async fn workspace(&self, id: &str) -> Result<Option<Workspace>> {
        let id = id.to_string();
        self.read(move |c| {
            Ok(c.query_row(
                "SELECT id, name, created_at, icon FROM workspaces WHERE id = ?1",
                params![&id],
                |r| {
                    Ok(Workspace {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        created_at: r.get(2)?,
                        icon: r.get(3)?,
                    })
                },
            )
            .optional()?)
        })
        .await
    }

    async fn create_invite(&self, inv: Invite) -> Result<()> {
        self.write(move |c| {
            c.execute(
                "INSERT INTO invites (token, workspace_id, role, created_by, created_at, expires_at, uses) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
                params![inv.token, inv.workspace_id, inv.role.as_str(), inv.created_by, inv.created_at, inv.expires_at],
            )?;
            Ok(())
        })
        .await
    }

    async fn invites(&self, workspace_id: &str) -> Result<Vec<Invite>> {
        let w = workspace_id.to_string();
        self.read(move |c| {
            let mut st = c.prepare_cached(&format!(
                "SELECT {INVITE_COLS} FROM invites WHERE workspace_id = ?1 AND revoked_at IS NULL ORDER BY created_at DESC"
            ))?;
            let rows = st.query_map(params![w], invite_row)?;
            Ok(rows)
        })
        .await
    }

    async fn invite(&self, token: &str) -> Result<Option<Invite>> {
        let t = token.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached(&format!(
                "SELECT {INVITE_COLS} FROM invites WHERE token = ?1"
            ))?
            .query_row(params![t], invite_row)
            .optional()?)
        })
        .await
    }

    async fn revoke_invite(&self, token: &str) -> Result<bool> {
        let t = token.to_string();
        self.write(move |c| {
            Ok(c.execute(
                "UPDATE invites SET revoked_at = ?2 WHERE token = ?1 AND revoked_at IS NULL",
                params![t, ids::now_ms()],
            )? > 0)
        })
        .await
    }

    async fn use_invite(&self, token: &str) -> Result<()> {
        let t = token.to_string();
        self.write(move |c| {
            c.execute(
                "UPDATE invites SET uses = uses + 1 WHERE token = ?1",
                params![t],
            )?;
            Ok(())
        })
        .await
    }

    async fn share_for_page(&self, page_id: &str) -> Result<Option<Share>> {
        let id = page_id.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached(&format!(
                "SELECT {SHARE_COLS} FROM shares WHERE page_id = ?1 AND revoked_at IS NULL"
            ))?
            .query_row(params![id], share_row)
            .optional()?)
        })
        .await
    }

    async fn put_share(
        &self,
        page_id: &str,
        include_children: bool,
        indexable: bool,
        by: &str,
    ) -> Result<Share> {
        let (id, by) = (page_id.to_string(), by.to_string());
        self.write(move |c| {
            let mut tx = c.transaction()?;
            let n = tx.execute(
                "UPDATE shares SET include_children = ?2, indexable = ?3 WHERE page_id = ?1 AND revoked_at IS NULL",
                params![id, include_children, indexable],
            )?;
            if n == 0 {
                tx.execute(
                    "INSERT INTO shares (id, page_id, token, include_children, indexable, created_by, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![ids::new_id(), id, ids::new_token()[..32].to_string(), include_children, indexable, by, ids::now_ms()],
                )?;
            }
            let s = tx
                .prepare_cached(&format!("SELECT {SHARE_COLS} FROM shares WHERE page_id = ?1 AND revoked_at IS NULL"))?
                .query_row(params![&id], share_row)?;
            tx.commit()?;
            Ok(s)
        })
        .await
    }

    async fn revoke_share(&self, page_id: &str) -> Result<bool> {
        let id = page_id.to_string();
        self.write(move |c| {
            Ok(c.execute(
                "UPDATE shares SET revoked_at = ?2 WHERE page_id = ?1 AND revoked_at IS NULL",
                params![id, ids::now_ms()],
            )? > 0)
        })
        .await
    }

    async fn share_by_token(&self, token: &str) -> Result<Option<Share>> {
        let t = token.to_string();
        self.read(move |c| {
            Ok(
                c.prepare_cached(&format!("SELECT {SHARE_COLS} FROM shares WHERE token = ?1"))?
                    .query_row(params![t], share_row)
                    .optional()?,
            )
        })
        .await
    }

    async fn ancestors(&self, page_id: &str) -> Result<Vec<String>> {
        let id = page_id.to_string();
        self.read(move |c| {
            let mut v = ancestors(c, &id)?;
            v.retain(|x| *x != id);
            Ok(v)
        })
        .await
    }

    async fn page_uses_file(&self, page_id: &str, file_id: &str) -> Result<bool> {
        let (p, f) = (page_id.to_string(), file_id.to_string());
        self.read(move |c| {
            Ok(c.prepare_cached(
                "SELECT EXISTS (SELECT 1 FROM file_refs WHERE page_id = ?1 AND file_id = ?2)",
            )?
            .query_row(params![p, f], |r| r.get(0))?)
        })
        .await
    }

    async fn trash(&self, workspace_id: &str) -> Result<Vec<Page>> {
        let w = workspace_id.to_string();
        self.read(move |c| {
            let mut st = c.prepare_cached(&format!(
                "SELECT {PAGE_COLS} FROM pages p WHERE workspace_id = ?1 AND deleted_at IS NOT NULL {TRASH_ROOT}
                 ORDER BY deleted_at DESC, id"
            ))?;
            let rows = st.query_map(params![w], page_row)?;
            Ok(rows)
        })
        .await
    }

    async fn restore_page(&self, id: &str) -> Result<Vec<Page>> {
        let id = id.to_string();
        self.write(move |c| {
            let mut tx = c.transaction()?;
            let root = get_page(&mut tx, &id)?
                .ok_or_else(|| Error::NotFound(format!("页面 {id} 不存在")))?;
            if root.deleted_at.is_none() {
                return Err(Error::BadRequest("这个页面不在回收站里".into()));
            }
            let parent_live = match &root.parent_id {
                Some(p) => get_page(&mut tx, p)?.is_some_and(|x| x.deleted_at.is_none()),
                None => true,
            };
            let now = ids::now_ms();
            if root.kind == "row" {
                let db = root.parent_id.clone().unwrap_or_default();
                if !parent_live {
                    return Err(Error::BadRequest(
                        "这一行所在的库也在回收站里，先恢复库".into(),
                    ));
                }
                if let Some(row) = tx
                    .query_row(
                        &format!("SELECT {ROW_COLS} FROM db_rows WHERE row_id = ?1"),
                        params![&id],
                        db_row_of,
                    )
                    .optional()?
                    && row.deleted_at.is_some()
                {
                    let seq = bump_seq(&mut tx, &db)?;
                    tx.execute(
                        "UPDATE db_rows SET deleted_at = NULL, seq = ?2 WHERE row_id = ?1",
                        params![id, seq],
                    )?;
                    log_op(
                        &mut tx,
                        &At {
                            db: &db,
                            seq,
                            by: "",
                            at: now,
                        },
                        &id,
                        "*",
                        &None,
                        &Some(serde_json::Value::Object(row.props)),
                    )?;
                }
            } else if !parent_live {
                // 上级不在了：放到最外层的最后
                let sib = siblings(&mut tx, &root.workspace_id, None, &id)?;
                let pos = tree::position_for(&sib, &Placement::Last)?;
                tx.execute(
                    "UPDATE pages SET parent_id = NULL, position = ?2 WHERE id = ?1",
                    params![id, pos],
                )?;
            }
            // 连同一起删的（删除时间一样的）子孙
            let ids: Vec<String> = {
                let mut st = tx.prepare_cached(
                    "WITH RECURSIVE sub(id) AS (
                        SELECT id FROM pages WHERE id = ?1
                        UNION ALL
                        SELECT p.id FROM pages p JOIN sub ON p.parent_id = sub.id
                        WHERE p.deleted_at IS NOT NULL AND p.trash_root = ?1
                     ) SELECT id FROM sub",
                )?;
                st.query_map(params![id], |r| r.get(0))?
            };
            let mut out = Vec::with_capacity(ids.len());
            for pid in &ids {
                tx.execute(
                    "UPDATE pages SET deleted_at = NULL, trash_root = NULL WHERE id = ?1",
                    params![pid],
                )?;
                if let Some(p) = get_page(&mut tx, pid)? {
                    out.push(p);
                }
            }
            tx.commit()?;
            Ok(out)
        })
        .await
    }

    async fn purge_page(&self, id: &str) -> Result<(Vec<String>, Vec<String>)> {
        let id = id.to_string();
        self.write(move |c| {
            let mut tx = c.transaction()?;
            let root = get_page(&mut tx, &id)?.ok_or_else(|| Error::NotFound(format!("页面 {id} 不存在")))?;
            if root.deleted_at.is_none() {
                return Err(Error::BadRequest("要先移到回收站才能彻底删除".into()));
            }
            let ids: Vec<String> = {
                let mut st = tx.prepare_cached(
                    "WITH RECURSIVE sub(id, depth) AS (
                        SELECT id, 0 FROM pages WHERE id = ?1
                        UNION ALL
                        SELECT p.id, sub.depth + 1 FROM pages p JOIN sub ON p.parent_id = sub.id
                     ) SELECT id FROM sub ORDER BY depth DESC",
                )?;
                st.query_map(params![&id], |r| r.get(0))?
            };
            let mut files: Vec<String> = Vec::new();
            for pid in &ids {
                let mut st = tx.prepare_cached("SELECT file_id FROM file_refs WHERE page_id = ?1")?;
                files.extend(st.query_map(params![pid], |r| r.get::<String>(0))?);
            }
            for pid in &ids {
                tx.execute("DELETE FROM pages WHERE id = ?1", params![pid])?;
            }
            // 没有别的页面用的文件、没有别的文件和图片用的内容
            files.sort();
            files.dedup();
            let mut blobs = Vec::new();
            for f in files {
                let used: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM file_refs WHERE file_id = ?1)", params![&f], |r| r.get(0))?;
                if used {
                    continue;
                }
                let Some(hash) = tx
                    .query_row("SELECT hash FROM files WHERE id = ?1", params![&f], |r| r.get::<String>(0))
                    .optional()?
                else {
                    continue;
                };
                tx.execute("DELETE FROM files WHERE id = ?1", params![&f])?;
                let shared: bool = tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM files WHERE hash = ?1) OR EXISTS (SELECT 1 FROM media WHERE hash = ?1)",
                    params![&hash],
                    |r| r.get(0),
                )?;
                if !shared {
                    tx.execute("DELETE FROM blobs WHERE hash = ?1", params![&hash])?;
                    blobs.push(hash);
                }
            }
            tx.commit()?;
            Ok((ids, blobs))
        })
        .await
    }

    async fn trash_before(&self, before: i64) -> Result<Vec<String>> {
        self.read(move |c| {
            let mut st = c.prepare_cached(&format!(
                "SELECT id FROM pages p WHERE deleted_at IS NOT NULL AND deleted_at < ?1 {TRASH_ROOT}"
            ))?;
            let rows = st.query_map(params![before], |r| r.get(0))?;
            Ok(rows)
        })
        .await
    }

    async fn set_deleted_by(&self, ids: Vec<String>, by: &str) -> Result<()> {
        let by = by.to_string();
        self.write(move |c| {
            let mut tx = c.transaction()?;
            for id in ids {
                tx.prepare_cached("UPDATE pages SET deleted_by = ?2 WHERE id = ?1")?
                    .execute(params![id, by])?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    async fn deleted_by(&self, id: &str) -> Result<Option<String>> {
        let id = id.to_string();
        self.read(move |c| {
            Ok(
                c.prepare_cached("SELECT deleted_by FROM pages WHERE id = ?1")?
                    .query_row(params![id], |r| r.get(0))
                    .optional()?
                    .flatten(),
            )
        })
        .await
    }

    async fn add_file_refs(&self, page_id: &str, files: Vec<String>) -> Result<()> {
        let id = page_id.to_string();
        self.write(move |c| {
            let mut tx = c.transaction()?;
            for f in files {
                tx.prepare_cached("INSERT OR IGNORE INTO file_refs (page_id, file_id) SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM pages WHERE id = ?1)")?
                    .execute(params![id, f])?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    async fn add_version(&self, page_id: &str, v: NewVersion) -> Result<i64> {
        let id = page_id.to_string();
        self.write(move |c| {
            let authors = serde_json::to_string(&v.authors).unwrap_or_else(|_| "[]".into());
            let vid: i64 = c
                .prepare_cached(
                    "INSERT INTO doc_versions (page_id, state, label, created_by, created_at, kind, authors, state_vector)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) RETURNING id",
                )?
                .query_row(
                    params![id, v.snapshot, v.label, v.created_by, ids::now_ms(), v.kind, authors, v.state_vector],
                    |r| r.get(0),
                )?;
            Ok(vid)
        })
        .await
    }

    async fn versions(
        &self,
        page_id: &str,
        limit: usize,
        before: Option<i64>,
    ) -> Result<Vec<VersionMeta>> {
        let id = page_id.to_string();
        self.read(move |c| {
            let mut st = c.prepare_cached(
                "SELECT id, page_id, kind, label, authors, created_by, created_at FROM doc_versions
                 WHERE page_id = ?1 AND (id < ?3 OR ?3 IS NULL) ORDER BY id DESC LIMIT ?2",
            )?;
            let rows = st.query_map(params![id, limit as i64, before], version_meta)?;
            Ok(rows)
        })
        .await
    }

    async fn version(&self, page_id: &str, vid: i64) -> Result<Option<(VersionMeta, Vec<u8>)>> {
        let id = page_id.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached(
                "SELECT id, page_id, kind, label, authors, created_by, created_at, state FROM doc_versions
                 WHERE page_id = ?1 AND id = ?2",
            )?
            .query_row(params![id, vid], |r| Ok((version_meta(r)?, r.get(7)?)))
            .optional()?)
        })
        .await
    }

    async fn db_ops(
        &self,
        db_id: &str,
        row: Option<&str>,
        prop: Option<&str>,
        limit: usize,
        before_seq: Option<i64>,
    ) -> Result<Vec<DbOpRow>> {
        let (id, row, prop) = (
            db_id.to_string(),
            row.map(str::to_string),
            prop.map(str::to_string),
        );
        self.read(move |c| {
            let mut st = c.prepare_cached(
                "SELECT seq, row_id, prop, old, new, by, at FROM db_ops
                 WHERE db_id = ?1 AND (row_id = ?2 OR ?2 IS NULL) AND (prop = ?3 OR ?3 IS NULL) AND (seq < ?5 OR ?5 IS NULL)
                 ORDER BY id DESC LIMIT ?4",
            )?;
            let rows = st.query_map(params![id, row, prop, limit as i64, before_seq], |r| {
                let old: Option<String> = r.get(3)?;
                let new: Option<String> = r.get(4)?;
                Ok(DbOpRow {
                    seq: r.get(0)?,
                    row_id: r.get(1)?,
                    prop: r.get(2)?,
                    old: old.and_then(|x| serde_json::from_str(&x).ok()),
                    new: new.and_then(|x| serde_json::from_str(&x).ok()),
                    by: r.get(5)?,
                    at: r.get(6)?,
                })
            })?;
            Ok(rows)
        })
        .await
    }

    async fn db_schema_ops(
        &self,
        db_id: &str,
        limit: usize,
        before_seq: Option<i64>,
    ) -> Result<Vec<SchemaOpRow>> {
        let id = db_id.to_string();
        self.read(move |c| {
            let mut st = c.prepare_cached(
                "SELECT seq, op, before, after, by, at FROM db_schema_ops WHERE db_id = ?1 AND (seq < ?3 OR ?3 IS NULL)
                 ORDER BY seq DESC LIMIT ?2",
            )?;
            let rows = st.query_map(params![id, limit as i64, before_seq], |r| {
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
            })?;
            Ok(rows)
        })
        .await
    }

    async fn undo_db_ops(
        &self,
        db_id: &str,
        seq: i64,
        title_key: &str,
        by: &str,
    ) -> Result<(i64, Vec<(String, String)>, Vec<String>)> {
        let (id, tk, by) = (db_id.to_string(), title_key.to_string(), by.to_string());
        self.write(move |c| {
            let mut tx = c.transaction()?;
            let ops: Vec<(String, String, Option<String>, Option<String>)> = tx
                .prepare_cached("SELECT row_id, prop, old, new FROM db_ops WHERE db_id = ?1 AND seq = ?2 ORDER BY id DESC")?
                .query_map(params![id, seq], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            if ops.is_empty() {
                return Err(Error::NotFound(format!("第 {seq} 次没有格子改动")));
            }
            let new_seq = bump_seq(&mut tx, &id)?;
            let now = ids::now_ms();
            let at = At { db: &id, seq: new_seq, by: &by, at: now };
            let parse = |x: &Option<String>| x.as_ref().and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok());
            let mut titles = Vec::new();
            let mut back = Vec::new();
            for (rid, prop, old, new) in ops {
                let (old, new) = (parse(&old), parse(&new));
                let Some(mut row) = tx
                    .prepare_cached(&format!("SELECT {ROW_COLS} FROM db_rows WHERE row_id = ?1 AND db_id = ?2"))?
                    .query_row(params![rid, id], db_row_of)
                    .optional()?
                else {
                    continue;
                };
                match prop.as_str() {
                    // 新建的行：删掉
                    "*" if old.is_none() => {
                        if row.deleted_at.is_none() {
                            tx.execute("UPDATE db_rows SET deleted_at = ?2, seq = ?3 WHERE row_id = ?1", params![rid, now, new_seq])?;
                            tx.execute(
                    "UPDATE pages SET deleted_at = ?2, trash_root = ?1 WHERE id = ?1 AND deleted_at IS NULL",
                    params![rid, now],
                )?;
                            log_op(&mut tx, &at, &rid, "*", &Some(serde_json::Value::Object(row.props)), &None)?;
                        }
                    }
                    // 删掉的行：放回
                    "*" => {
                        if row.deleted_at.is_some() {
                            tx.execute("UPDATE db_rows SET deleted_at = NULL, seq = ?2 WHERE row_id = ?1", params![rid, new_seq])?;
                            tx.execute("UPDATE pages SET deleted_at = NULL, trash_root = NULL WHERE id = ?1", params![rid])?;
                            log_op(&mut tx, &at, &rid, "*", &None, &Some(serde_json::Value::Object(row.props)))?;
                            back.push(rid.clone());
                        }
                    }
                    "@position" => {
                        let cur = Some(serde_json::Value::String(row.position.clone()));
                        if cur == new
                            && let Some(p) = old.as_ref().and_then(|v| v.as_str())
                        {
                            tx.execute("UPDATE db_rows SET position = ?2, seq = ?3 WHERE row_id = ?1", params![rid, p, new_seq])?;
                            tx.execute("UPDATE pages SET position = ?2 WHERE id = ?1", params![rid, p])?;
                            log_op(&mut tx, &at, &rid, "@position", &cur, &old)?;
                        }
                    }
                    k => {
                        let cur = row.props.get(k).cloned();
                        // 之后又被人改过的不动
                        if cur != new || row.deleted_at.is_some() {
                            continue;
                        }
                        match &old {
                            Some(v) => row.props.insert(k.to_string(), v.clone()),
                            None => row.props.remove(k),
                        };
                        if k == tk {
                            let t = old.as_ref().and_then(|v| v.as_str()).unwrap_or("").to_string();
                            tx.execute("UPDATE pages SET title = ?2, updated_at = ?3 WHERE id = ?1", params![rid, t, now])?;
                            titles.push((rid.clone(), t));
                        }
                        tx.prepare_cached("UPDATE db_rows SET props = ?2, seq = ?3, updated_at = ?4, updated_by = ?5 WHERE row_id = ?1")?
                            .execute(params![rid, serde_json::Value::Object(row.props).to_string(), new_seq, now, by])?;
                        log_op(&mut tx, &at, &rid, k, &cur, &old)?;
                    }
                }
            }
            tx.commit()?;
            Ok((new_seq, titles, back))
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
            .query_row(params![k], |r| {
                let data: Option<String> = r.get(1)?;
                Ok(UnfurlRow {
                    url_key: r.get(0)?,
                    data: data.and_then(|d| serde_json::from_str(&d).ok()),
                    ok: r.get::<String>(2)? == "ok",
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
                    .query_row(params![h], |r| blob_row(r, 0))
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
            .query_row(params![id], |r| {
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
            .query_row(params![u], |r| blob_row(r, 0))
            .optional()?)
        })
        .await
    }

    async fn is_media(&self, hash: &str) -> Result<bool> {
        let h = hash.to_string();
        self.read(move |c| {
            Ok(c.prepare_cached("SELECT 1 FROM media WHERE hash = ?1")?
                .query_row(params![h], |_| Ok(()))
                .optional()?
                .is_some())
        })
        .await
    }

    async fn purge_notifications(&self, before: i64) -> Result<u64> {
        self.write(move |c| {
            let a = c.execute(
                "DELETE FROM agent_notifications WHERE created_at < ?1",
                params![before],
            )?;
            let b = c.execute(
                "DELETE FROM user_notifications WHERE created_at < ?1",
                params![before],
            )?;
            Ok((a + b) as u64)
        })
        .await
    }
}

const AGENT_COLS: &str = "id, workspace_id, name, about, scope, webhook, webhook_secret, created_by, created_at, last_used_at, revoked_at";

fn agent_row(r: &Row) -> RowResult<Agent> {
    Ok(Agent {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        name: r.get(2)?,
        about: r.get(3)?,
        scope: Scope::parse(&r.get::<String>(4)?).unwrap_or(Scope::Read),
        webhook: r.get(5)?,
        webhook_secret: r.get(6)?,
        created_by: r.get(7)?,
        created_at: r.get(8)?,
        last_used_at: r.get(9)?,
        revoked_at: r.get(10)?,
    })
}

fn get_agent(c: &mut Conn, id: &str) -> Result<Option<Agent>> {
    Ok(
        c.prepare_cached(&format!("SELECT {AGENT_COLS} FROM agents WHERE id = ?1"))?
            .query_row(params![id], agent_row)
            .optional()?,
    )
}

const THREAD_COLS: &str = "id, page_id, block_id, anchor_start, anchor_end, quote, created_by, created_at, resolved_at, resolved_by";

fn thread_row(r: &Row) -> RowResult<Thread> {
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

fn comment_row(r: &Row) -> RowResult<Comment> {
    Ok(Comment {
        id: r.get(0)?,
        thread_id: r.get(1)?,
        author_kind: PrincipalKind::parse(&r.get::<String>(2)?),
        author_id: r.get(3)?,
        body: r.get(4)?,
        created_at: r.get(5)?,
        edited_at: r.get(6)?,
        deleted_at: r.get(7)?,
    })
}

fn insert_comment(c: &mut Conn, cm: &Comment) -> Result<()> {
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

fn note_row(r: &Row) -> RowResult<NotificationRow> {
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

fn blob_row(r: &Row, at: usize) -> RowResult<BlobInfo> {
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
fn insert_page(tx: &mut Conn, p: NewPage, position: Option<String>) -> Result<Page> {
    let ws_ok = tx
        .query_row(
            "SELECT 1 FROM workspaces WHERE id = ?1",
            params![&p.workspace_id],
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

fn get_db_meta(c: &mut Conn, db_id: &str) -> Result<Option<DbMeta>> {
    Ok(
        c.prepare_cached("SELECT db_id, schema, views, seq FROM databases WHERE db_id = ?1")?
            .query_row(params![db_id], |r| {
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

const INVITE_COLS: &str =
    "token, workspace_id, role, created_by, created_at, expires_at, revoked_at, uses";

fn invite_row(r: &Row) -> RowResult<Invite> {
    Ok(Invite {
        token: r.get(0)?,
        workspace_id: r.get(1)?,
        role: Role::parse(&r.get::<String>(2)?).unwrap_or(Role::Viewer),
        created_by: r.get(3)?,
        created_at: r.get(4)?,
        expires_at: r.get(5)?,
        revoked_at: r.get(6)?,
        uses: r.get(7)?,
    })
}

const SHARE_COLS: &str =
    "id, page_id, token, include_children, indexable, created_by, created_at, revoked_at";

fn share_row(r: &Row) -> RowResult<Share> {
    Ok(Share {
        id: r.get(0)?,
        page_id: r.get(1)?,
        token: r.get(2)?,
        include_children: r.get(3)?,
        indexable: r.get(4)?,
        created_by: r.get(5)?,
        created_at: r.get(6)?,
        revoked_at: r.get(7)?,
    })
}

/// 回收站里单独删的（上级没有跟它同时删）。
const TRASH_ROOT: &str = "AND (p.trash_root = p.id OR p.trash_root IS NULL AND (p.parent_id IS NULL OR NOT EXISTS (SELECT 1 FROM pages q WHERE q.id = p.parent_id AND q.deleted_at = p.deleted_at)))";

const ROW_COLS: &str = "row_id, db_id, seq, position, props, unique_id, created_at, created_by, updated_at, updated_by, deleted_at";

fn db_row_of(r: &Row) -> RowResult<DbRow> {
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
fn bump_seq(tx: &mut Conn, db_id: &str) -> Result<i64> {
    let seq: Option<i64> = tx
        .prepare_cached("UPDATE databases SET seq = seq + 1 WHERE db_id = ?1 RETURNING seq")?
        .query_row(params![db_id], |r| r.get(0))
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
    tx: &mut Conn,
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

fn version_meta(r: &Row) -> RowResult<VersionMeta> {
    let authors: String = r.get(4)?;
    Ok(VersionMeta {
        id: r.get(0)?,
        page_id: r.get(1)?,
        kind: r.get(2)?,
        label: r.get(3)?,
        authors: serde_json::from_str(&authors).unwrap_or_default(),
        created_by: r.get(5)?,
        created_at: r.get(6)?,
    })
}
