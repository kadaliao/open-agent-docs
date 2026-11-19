//! 领域逻辑：页面树、正文（CRDT）、登录、事件。不含 HTTP，HTTP 在 oad-server。
//!
//! 可替换的部分是 trait：[`Storage`]（首版 SQLite）、[`AuthProvider`]（内置账号 / 可信请求头 / 不登录）。
pub mod agents;
pub mod auth;
pub mod blob;
pub mod comments;
pub mod db;
pub mod dbcsv;
pub mod dbengine;
pub mod dbschema;
mod error;
pub mod events;
pub mod history;
pub mod hub;
pub mod i18n;
pub mod ids;
pub mod media;
pub mod members;
pub mod model;
pub mod notify;
pub mod position;
pub mod search;
pub mod share;
pub mod ssrf;
pub mod storage;
pub mod trash;
pub mod tree;
pub mod tz;
pub mod unfurl;

pub use auth::{AuthMode, AuthProvider, AuthRequest};
pub use blob::{BlobStore, FsBlobs, MemBlobs};
pub use comments::{Anchor, AuthorRef, CommentView, NotifySpec, ThreadView};
pub use error::{Error, Result};
pub use events::{Event, EventKind, Events};
pub use hub::{DocHub, HubConfig, Targets, Writer};
pub use model::*;
pub use notify::{Notifier, NotifyConfig};
pub use oad_doc::{Node, PageDoc};
pub use storage::Storage;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const DEFAULT_WORKSPACE: &str = "默认工作区";
const LOCAL_USER_EMAIL: &str = "local@localhost";

/// 文件和链接解析的配置。
#[derive(Clone)]
pub struct MediaConfig {
    pub blobs: Arc<dyn BlobStore>,
    /// 抓外部网页、图片：超时、代理（`OAD_FETCH_PROXY`）、允许的内网网段（测试、本机开发）。
    pub fetch: oad_unfurl::FetchConfig,
    /// 上传上限（字节）。
    pub max_upload: usize,
    /// 开不开链接解析（默认开）。
    pub unfurl: bool,
    /// 搜索索引的目录（`OAD_DATA_DIR/search`）；不给放内存里，每次启动重建。
    pub search_dir: Option<std::path::PathBuf>,
}

impl Default for MediaConfig {
    fn default() -> Self {
        MediaConfig {
            blobs: Arc::new(MemBlobs::default()),
            fetch: oad_unfurl::FetchConfig::default(),
            max_upload: 100 << 20,
            unfurl: true,
            search_dir: None,
        }
    }
}

/// 登录方式的配置。
#[derive(Clone, Debug)]
pub enum AuthConfig {
    Local,
    Header { secret: String },
    None,
}

/// 把用户加进默认工作区：工作区里还没人就当所有者，否则是编辑者。已经是成员的不动。
pub async fn join_default(
    storage: &dyn Storage,
    workspace_id: &str,
    user_id: &str,
) -> Result<Role> {
    if let Some(r) = storage.member_role(workspace_id, user_id).await? {
        return Ok(r);
    }
    let first = storage.count_users().await? <= 1;
    let role = if first { Role::Owner } else { Role::Editor };
    storage.set_member(workspace_id, user_id, role).await?;
    Ok(role)
}

pub struct Core {
    pub storage: Arc<dyn Storage>,
    pub hub: DocHub,
    pub events: Events,
    pub auth: Arc<dyn AuthProvider>,
    /// 默认工作区（新用户加进这里）。
    pub default_workspace: Workspace,
    /// 通知：存、投 webhook、叫醒等着的。
    pub notes: Arc<Notifier>,
    /// 文件和图片。
    pub media: Arc<media::Media>,
    /// 链接解析（没开时为 None）。
    pub unfurls: Option<Arc<unfurl::UnfurlService>>,
    /// 全文搜索。
    pub search: Arc<search::SearchService>,
    /// 关机信号：SSE、WebSocket 等长连接看到它就结束，HTTP 服务才能停下来。
    shutdown: tokio::sync::watch::Sender<bool>,
    /// Agent 最后一次记使用时间的时刻（少写库）。
    touched: Mutex<HashMap<String, i64>>,
    /// 数据库的计算引擎（缓存）。
    pub dbcache: dbengine::DbCache,
    /// 服务器的时区（数据库算“今天”、相对日期、创建时间的显示）：`OAD_TZ`（IANA 名字）> 系统的。
    pub timezone: jiff::tz::TimeZone,
    /// 本地登录模式下怎么注册（`OAD_SIGNUP`），见 [`Core::signup_mode`]。
    signup: std::sync::atomic::AtomicU8,
}

/// 页面属性的修改。`Option<Option<_>>`：外层 None = 不改，Some(None) = 清空。
#[derive(Default, Debug)]
pub struct PagePatch {
    pub title: Option<String>,
    pub icon: Option<Option<String>>,
    /// 移动：新的父页面（Some(None) = 移到根）。
    pub parent_id: Option<Option<String>>,
    /// 移动：同级里排在它前面的页面（Some(None) = 放最前）。给了 parent_id 没给 after 就放最后。
    pub after: Option<Option<String>>,
}

impl Core {
    /// 建好各部分；没有工作区时建默认工作区，不登录模式下建本机用户。
    pub async fn open(
        storage: Arc<dyn Storage>,
        auth: AuthConfig,
        hub: HubConfig,
    ) -> Result<Arc<Core>> {
        Self::open_with(storage, auth, hub, NotifyConfig::default()).await
    }

    /// 同 [`Core::open`]，带通知的配置。会起一个后台任务投 webhook、清旧通知，关机时停。
    pub async fn open_with(
        storage: Arc<dyn Storage>,
        auth: AuthConfig,
        hub: HubConfig,
        notify: NotifyConfig,
    ) -> Result<Arc<Core>> {
        Self::open_full(storage, auth, hub, notify, MediaConfig::default()).await
    }

    /// 全部配置：再加上文件存哪、链接解析怎么抓。
    pub async fn open_full(
        storage: Arc<dyn Storage>,
        auth: AuthConfig,
        hub: HubConfig,
        notify: NotifyConfig,
        media_cfg: MediaConfig,
    ) -> Result<Arc<Core>> {
        let default_workspace = match storage.workspaces().await?.into_iter().next() {
            Some(w) => w,
            None => storage.create_workspace(DEFAULT_WORKSPACE).await?,
        };
        let provider: Arc<dyn AuthProvider> = match auth {
            AuthConfig::Local => Arc::new(auth::LocalAuth::new(storage.clone())),
            AuthConfig::Header { secret } => Arc::new(auth::HeaderAuth::new(
                storage.clone(),
                secret,
                default_workspace.id.clone(),
            )),
            AuthConfig::None => {
                let user = match storage.user_by_email(LOCAL_USER_EMAIL).await? {
                    Some((u, _)) => u,
                    None => {
                        storage
                            .create_user(NewUser {
                                name: "本机用户".into(),
                                email: Some(LOCAL_USER_EMAIL.into()),
                                password_hash: None,
                                external_id: None,
                                kind: PrincipalKind::User,
                            })
                            .await?
                    }
                };
                storage
                    .set_member(&default_workspace.id, &user.id, Role::Owner)
                    .await?;
                Arc::new(auth::NoAuth::new(&user))
            }
        };
        let events = Events::default();
        let notes = Notifier::new(storage.clone(), events.clone(), notify);
        let fetcher = oad_unfurl::Fetcher::new(media_cfg.fetch.clone())
            .map_err(|e| Error::Internal(e.to_string()))?;
        let media = Arc::new(media::Media::new(
            storage.clone(),
            media_cfg.blobs.clone(),
            fetcher,
            media_cfg.max_upload,
        ));
        let unfurls = if media_cfg.unfurl {
            let u = oad_unfurl::Unfurler::new(media_cfg.fetch.clone())
                .map_err(|e| Error::Internal(e.to_string()))?;
            Some(unfurl::UnfurlService::new(
                storage.clone(),
                media.clone(),
                u,
            ))
        } else {
            None
        };
        let hub = DocHub::new(storage.clone(), events.clone(), hub);
        if let Some(u) = &unfurls {
            hub.set_prefetch(u.prefetch_sink());
        }
        let (stop_tx, _) = tokio::sync::watch::channel(false);
        let mut stop_rx = stop_tx.subscribe();
        let (search, index_sink) = search::SearchService::start(
            media_cfg.search_dir.as_deref(),
            storage.clone(),
            events.subscribe(),
            async move {
                let _ = stop_rx.wait_for(|s| *s).await;
            },
        )?;
        hub.set_indexer(index_sink);
        let core = Arc::new(Core {
            search,
            hub,
            media,
            unfurls,
            storage,
            events,
            auth: provider,
            default_workspace,
            notes: notes.clone(),
            shutdown: stop_tx,
            touched: Mutex::new(HashMap::new()),
            dbcache: Default::default(),
            timezone: tz::server_tz(),
            signup: std::sync::atomic::AtomicU8::new(members::SignupMode::from_env() as u8),
        });
        tokio::spawn(notes.run(Box::pin(core.on_shutdown())));
        core.spawn_trash_purger();
        Ok(core)
    }

    /// 建内置账号用户（带密码），加进默认工作区。
    pub async fn add_user(
        &self,
        email: &str,
        name: &str,
        password: &str,
        role: Option<Role>,
    ) -> Result<User> {
        let email = email.trim();
        if email.is_empty() || !email.contains('@') {
            return Err(Error::BadRequest("邮箱格式不对".into()));
        }
        if password.len() < 8 {
            return Err(Error::BadRequest("密码至少 8 位".into()));
        }
        let user = self
            .storage
            .create_user(NewUser {
                name: if name.is_empty() {
                    email.split('@').next().unwrap_or(email).into()
                } else {
                    name.into()
                },
                email: Some(email.into()),
                password_hash: Some(auth::hash_password(password).await?),
                external_id: None,
                kind: PrincipalKind::User,
            })
            .await
            .map_err(|e| match e {
                Error::Conflict(_) => Error::Conflict(format!("邮箱 {email} 已经注册过")),
                e => e,
            })?;
        match role {
            Some(r) => {
                self.storage
                    .set_member(&self.default_workspace.id, &user.id, r)
                    .await?
            }
            None => {
                join_default(&*self.storage, &self.default_workspace.id, &user.id).await?;
            }
        }
        Ok(user)
    }

    /// 首次启动：还没有用户时用给定的邮箱密码建管理员。返回是否建了。
    /// 某人看日期用的时区。现在都是服务器的；以后按人 / Agent 的偏好（users.timezone）。
    pub fn timezone_for(&self, _who: Option<&Principal>) -> jiff::tz::TimeZone {
        self.timezone.clone()
    }

    /// 现在的时钟（数据库引擎用）：服务器时区在此刻的偏移。
    pub fn db_clock(&self) -> oad_db::Clock {
        let now = oad_db::Clock::system().now_ms;
        oad_db::Clock::new(now, tz::offset_minutes(&self.timezone, now))
    }

    pub async fn ensure_admin(&self, email: &str, password: &str) -> Result<bool> {
        if self.storage.count_users().await? > 0 {
            return Ok(false);
        }
        self.add_user(email, "", password, Some(Role::Owner))
            .await?;
        Ok(true)
    }

    /// 在工作区里的角色；不是成员报 forbidden。
    pub async fn role(&self, workspace_id: &str, who: &Principal) -> Result<Role> {
        self.storage
            .member_role(workspace_id, &who.user_id)
            .await?
            .ok_or_else(|| Error::Forbidden("不是这个工作区的成员".into()))
    }

    pub async fn require(&self, workspace_id: &str, who: &Principal, write: bool) -> Result<Role> {
        let role = self.role(workspace_id, who).await?;
        if write && !role.can_write() {
            return Err(Error::Forbidden("只读权限".into()));
        }
        Ok(role)
    }

    /// 页面（不含回收站里的）和调用者在那个工作区的角色。
    pub async fn page_for(&self, id: &str, who: &Principal, write: bool) -> Result<(Page, Role)> {
        let page = match self.storage.page(id).await? {
            Some(p) if p.deleted_at.is_none() => p,
            _ => return Err(Error::NotFound(format!("页面 {id} 不存在"))),
        };
        let role = self.require(&page.workspace_id, who, write).await?;
        Ok((page, role))
    }

    pub async fn list_pages(&self, workspace_id: &str, who: &Principal) -> Result<Vec<Page>> {
        self.require(workspace_id, who, false).await?;
        self.storage.pages(workspace_id).await
    }

    pub async fn create_page(
        &self,
        workspace_id: &str,
        who: &Principal,
        parent_id: Option<String>,
        after: Option<String>,
        title: Option<String>,
        icon: Option<String>,
    ) -> Result<Page> {
        self.require(workspace_id, who, true).await?;
        let title = title.unwrap_or_default();
        // 正文的初始状态：标题写进 Y.Text "title"，和 pages.title 一致
        let doc = PageDoc::new(oad_doc::server_client_id());
        doc.set_title(&title);
        let page = self
            .storage
            .create_page(NewPage {
                workspace_id: workspace_id.into(),
                parent_id,
                place: after.map(Placement::After).unwrap_or(Placement::Last),
                title,
                icon,
                created_by: Some(who.user_id.clone()),
                initial_state: doc.encode_state(),
                kind: "page".into(),
            })
            .await?;
        self.events.page(EventKind::Created, page.clone());
        Ok(page)
    }

    pub async fn update_page(&self, id: &str, who: &Principal, patch: PagePatch) -> Result<Page> {
        let (mut page, _) = self.page_for(id, who, true).await?;
        if let Some(title) = patch.title {
            // 标题在正文文档里，写进去后由同步中心同步到 pages.title（并发 page.updated）
            self.hub.set_title(id, title).await?;
        }
        if patch.icon.is_some() {
            page = self.storage.update_page(id, None, patch.icon).await?;
            self.events.page(EventKind::Updated, page.clone());
        }
        if patch.parent_id.is_some() || patch.after.is_some() {
            let parent = patch.parent_id.unwrap_or_else(|| page.parent_id.clone());
            let place = match patch.after {
                None => Placement::Last,
                Some(None) => Placement::First,
                Some(Some(a)) => Placement::After(a),
            };
            page = self.storage.move_page(id, parent, place).await?;
            self.events.page(EventKind::Moved, page.clone());
        }
        match self.storage.page(id).await? {
            Some(p) => Ok(p),
            None => Ok(page),
        }
    }

    pub async fn delete_page(&self, id: &str, who: &Principal) -> Result<Vec<Page>> {
        let (page, _) = self.page_for(id, who, true).await?;
        let gone = self.storage.delete_page(id).await?;
        self.storage
            .set_deleted_by(vec![id.to_string()], &dbengine::person_ref(who))
            .await?;
        // 库里的行不在侧栏里，不发 page.deleted；删的是一行就发 db.changed
        for p in gone.iter().filter(|p| p.kind != "row") {
            self.events.page(EventKind::Deleted, p.clone());
        }
        if page.kind == "row"
            && let Some(db) = &page.parent_id
            && let Some(m) = self.storage.database(db).await?
        {
            self.events.emit(Event {
                workspace_id: page.workspace_id.clone(),
                name: "db.changed",
                data: serde_json::json!({ "db_id": db, "seq": m.seq }),
                user_id: None,
            });
        }
        Ok(gone)
    }

    /// 正文（节点树）和当前版本。
    pub async fn content(&self, id: &str, who: &Principal) -> Result<(Node, Vec<u8>)> {
        self.page_for(id, who, false).await?;
        self.hub.content(id).await
    }

    /// 整页替换：对得上的块沿用编号、只改变了的地方。给了 base（读的时候拿到的版本）就在那个版本上改，
    /// 这期间别人写的不会被覆盖。
    pub async fn set_content(
        &self,
        id: &str,
        who: &Principal,
        doc: Node,
        base: Option<Vec<u8>>,
    ) -> Result<()> {
        self.page_for(id, who, true).await?;
        let w = Writer {
            principal: who.clone(),
            block: None,
        };
        self.hub
            .edit(
                id,
                Some(w),
                base,
                Targets {
                    all: true,
                    ..Default::default()
                },
                move |d| {
                    d.set_content(&doc);
                    Ok(())
                },
            )
            .await
    }

    /// 当前版本（写完之后给 Agent 接着改用）。
    pub async fn version(&self, id: &str, who: &Principal) -> Result<Vec<u8>> {
        self.page_for(id, who, false).await?;
        self.hub.version(id).await
    }

    /// 一块和当前版本。
    pub async fn block(
        &self,
        id: &str,
        block_id: &str,
        who: &Principal,
    ) -> Result<(Node, Vec<u8>)> {
        self.page_for(id, who, false).await?;
        let b = block_id.to_string();
        let (node, version) = self.hub.block(id, &b).await?;
        let node = node.ok_or_else(|| Error::NotFound(format!("找不到块 {block_id}")))?;
        Ok((node, version))
    }

    /// 替换一块，返回替换后的块编号。base 同 [`Core::set_content`]。
    pub async fn replace_block(
        &self,
        id: &str,
        block_id: &str,
        who: &Principal,
        nodes: Vec<Node>,
        base: Option<Vec<u8>>,
    ) -> Result<Vec<String>> {
        self.page_for(id, who, true).await?;
        let b = block_id.to_string();
        let w = Writer {
            principal: who.clone(),
            block: Some(b.clone()),
        };
        self.hub
            .edit(
                id,
                Some(w),
                base,
                Targets {
                    content: vec![b.clone()],
                    ..Default::default()
                },
                move |d| Ok(d.replace_block(&b, &nodes)?),
            )
            .await
    }

    /// 插入：after 为 None 时加到最后。返回新块的编号。
    pub async fn insert_blocks(
        &self,
        id: &str,
        after: Option<String>,
        who: &Principal,
        nodes: Vec<Node>,
    ) -> Result<Vec<String>> {
        self.page_for(id, who, true).await?;
        let w = Writer {
            principal: who.clone(),
            block: after.clone(),
        };
        self.hub
            .edit(
                id,
                Some(w),
                None,
                Targets::default(),
                move |d| match after {
                    Some(a) => Ok(d.insert_after(Some(&a), &nodes)?),
                    None => Ok(d.append(&nodes)),
                },
            )
            .await
    }

    pub async fn delete_block(&self, id: &str, block_id: &str, who: &Principal) -> Result<()> {
        self.page_for(id, who, true).await?;
        let b = block_id.to_string();
        let w = Writer {
            principal: who.clone(),
            block: Some(b.clone()),
        };
        self.hub
            .edit(id, Some(w), None, Targets::default(), move |d| {
                Ok(d.delete_block(&b)?)
            })
            .await
    }

    /// 按块改，ops 依次执行、一次事务（一个出错就都不改）。返回每一步涉及的块编号。
    pub async fn edit_blocks(
        &self,
        id: &str,
        who: &Principal,
        base: Option<Vec<u8>>,
        ops: Vec<BlockOp>,
    ) -> Result<Vec<Vec<String>>> {
        self.page_for(id, who, true).await?;
        if ops.is_empty() {
            return Err(Error::BadRequest("ops 是空的".into()));
        }
        let w = Writer {
            principal: who.clone(),
            block: ops.iter().find_map(|o| o.block().map(str::to_string)),
        };
        let mut targets = Targets::default();
        for op in &ops {
            match op {
                BlockOp::Replace { block, .. } => targets.content.push(block.clone()),
                BlockOp::InsertAfter { block, .. } | BlockOp::InsertBefore { block, .. } => {
                    targets.refs.push(block.clone())
                }
                // 删一块：不管在那之后它有没有变，删的都是同一个元素
                BlockOp::Delete { block } => targets.refs.push(block.clone()),
                BlockOp::Append { .. } => {}
            }
        }
        self.hub
            .edit(id, Some(w), base, targets, move |d| {
                // 直接写在文档上、没法撤回：先检查一遍，有一步不行就都不改
                let mut gone = std::collections::HashSet::new();
                for (i, op) in ops.iter().enumerate() {
                    if let Some(b) = op.block()
                        && (gone.contains(b) || !d.has_block(b))
                    {
                        return Err(Error::NotFound(format!("第 {} 步：找不到块 {b}", i + 1)));
                    }
                    if let BlockOp::Delete { block } = op {
                        gone.insert(block.clone());
                    }
                }
                let md = |m: &str| oad_markdown::parse(m).content;
                let mut out = Vec::new();
                for (i, op) in ops.iter().enumerate() {
                    let r = match op {
                        BlockOp::Replace { block, markdown } => {
                            d.replace_block(block, &md(markdown))
                        }
                        BlockOp::InsertAfter { block, markdown } => {
                            d.insert_after(Some(block), &md(markdown))
                        }
                        BlockOp::InsertBefore { block, markdown } => {
                            d.insert_before(block, &md(markdown))
                        }
                        BlockOp::Append { markdown } => Ok(d.append(&md(markdown))),
                        BlockOp::Delete { block } => {
                            d.delete_block(block).map(|_| vec![block.clone()])
                        }
                    };
                    out.push(r.map_err(|e| match Error::from(e) {
                        Error::NotFound(m) => Error::NotFound(format!("第 {} 步：{m}", i + 1)),
                        e => e,
                    })?);
                }
                Ok(out)
            })
            .await
    }

    /// 关机：先通知长连接结束，再把所有打开的页面存好。
    pub async fn shutdown(&self) {
        self.shutdown.send_replace(true);
        self.hub.shutdown().await;
    }

    /// 等关机信号（已经在关机就马上返回）。
    pub fn on_shutdown(&self) -> impl Future<Output = ()> + Send + use<> {
        let mut rx = self.shutdown.subscribe();
        async move {
            let _ = rx.wait_for(|&v| v).await;
        }
    }
}

/// edit_blocks 的一步。
#[derive(Clone, Debug, PartialEq)]
pub enum BlockOp {
    Replace { block: String, markdown: String },
    InsertAfter { block: String, markdown: String },
    InsertBefore { block: String, markdown: String },
    Append { markdown: String },
    Delete { block: String },
}

impl BlockOp {
    pub fn block(&self) -> Option<&str> {
        match self {
            BlockOp::Replace { block, .. }
            | BlockOp::InsertAfter { block, .. }
            | BlockOp::InsertBefore { block, .. }
            | BlockOp::Delete { block } => Some(block),
            BlockOp::Append { .. } => None,
        }
    }
}
