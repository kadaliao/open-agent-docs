//! 领域逻辑：页面树、正文（CRDT）、登录、事件。不含 HTTP，HTTP 在 oad-server。
//!
//! 可替换的部分是 trait：[`Storage`]（首版 SQLite）、[`AuthProvider`]（内置账号 / 可信请求头 / 不登录）。
pub mod auth;
mod error;
pub mod events;
pub mod hub;
pub mod ids;
pub mod model;
pub mod position;
pub mod storage;
pub mod tree;

pub use auth::{AuthMode, AuthProvider, AuthRequest};
pub use error::{Error, Result};
pub use events::{Event, EventKind, Events};
pub use hub::{DocHub, HubConfig, Writer};
pub use model::*;
pub use oad_doc::{Node, PageDoc};
pub use storage::Storage;

use std::sync::Arc;

pub const DEFAULT_WORKSPACE: &str = "默认工作区";
const LOCAL_USER_EMAIL: &str = "local@localhost";

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
        Ok(Arc::new(Core {
            hub: DocHub::new(storage.clone(), events.clone(), hub),
            storage,
            events,
            auth: provider,
            default_workspace,
        }))
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
            })
            .await?;
        self.events.emit(EventKind::Created, page.clone());
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
            self.events.emit(EventKind::Updated, page.clone());
        }
        if patch.parent_id.is_some() || patch.after.is_some() {
            let parent = patch.parent_id.unwrap_or_else(|| page.parent_id.clone());
            let place = match patch.after {
                None => Placement::Last,
                Some(None) => Placement::First,
                Some(Some(a)) => Placement::After(a),
            };
            page = self.storage.move_page(id, parent, place).await?;
            self.events.emit(EventKind::Moved, page.clone());
        }
        match self.storage.page(id).await? {
            Some(p) => Ok(p),
            None => Ok(page),
        }
    }

    pub async fn delete_page(&self, id: &str, who: &Principal) -> Result<Vec<Page>> {
        self.page_for(id, who, true).await?;
        let gone = self.storage.delete_page(id).await?;
        for p in &gone {
            self.events.emit(EventKind::Deleted, p.clone());
        }
        Ok(gone)
    }

    /// 正文（节点树）和当前版本。
    pub async fn content(&self, id: &str, who: &Principal) -> Result<(Node, Vec<u8>)> {
        self.page_for(id, who, false).await?;
        self.hub.read(id, |d| (d.to_node(), d.version())).await
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
            .edit(id, Some(w), move |d| {
                on_version(d, base.as_deref(), |d| {
                    d.set_content(&doc);
                    Ok(())
                })
            })
            .await
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
        let (node, version) = self
            .hub
            .read(id, move |d| (d.block(&b), d.version()))
            .await?;
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
            .edit(id, Some(w), move |d| {
                on_version(d, base.as_deref(), |d| Ok(d.replace_block(&b, &nodes)?))
            })
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
            .edit(id, Some(w), move |d| match after {
                Some(a) => Ok(d.insert_after(Some(&a), &nodes)?),
                None => Ok(d.append(&nodes)),
            })
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
            .edit(id, Some(w), move |d| Ok(d.delete_block(&b)?))
            .await
    }

    /// 关机：所有打开的页面存好。
    pub async fn shutdown(&self) {
        self.hub.shutdown().await;
    }
}

/// 在 base 版本上执行 f（没给 base 就直接在当前文档上）：在那个版本的副本上改，再把改动合并回来。
fn on_version<R>(
    d: &PageDoc,
    base: Option<&[u8]>,
    f: impl FnOnce(&PageDoc) -> Result<R>,
) -> Result<R> {
    let Some(base) = base else {
        return f(d);
    };
    let fork = d
        .fork_at(base, oad_doc::server_client_id())
        .map_err(|e| Error::BadRequest(format!("base 版本不对：{e}")))?;
    let sv = fork.state_vector();
    let r = f(&fork)?;
    d.apply_update(&fork.encode_diff(&sv)?)?;
    Ok(r)
}
