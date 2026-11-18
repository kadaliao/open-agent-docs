//! 存储接口。首版只有 SQLite（[`sqlite::Sqlite`]），以后加 Postgres 实现同一个 trait。
//! 页面树的改动（建、移动、删除）要在一个事务里读同级、算位置、查环，所以做成整体的方法；
//! 位置和查环的算法在 [`crate::position`] / [`crate::tree`]，各实现共用。
pub mod sqlite;

use crate::Result;
use crate::model::*;
use async_trait::async_trait;

#[async_trait]
pub trait Storage: Send + Sync + 'static {
    // 用户
    async fn create_user(&self, u: NewUser) -> Result<User>;
    async fn user(&self, id: &str) -> Result<Option<User>>;
    /// 用户和密码哈希。
    async fn user_by_email(&self, email: &str) -> Result<Option<(User, Option<String>)>>;
    async fn user_by_external(&self, external_id: &str) -> Result<Option<User>>;
    async fn count_users(&self) -> Result<u64>;
    async fn set_password(&self, user_id: &str, password_hash: &str) -> Result<()>;

    // 会话（只存令牌哈希）
    async fn create_session(&self, token_hash: &str, user_id: &str, expires_at: i64) -> Result<()>;
    async fn session_user(&self, token_hash: &str, now: i64) -> Result<Option<User>>;
    async fn delete_session(&self, token_hash: &str) -> Result<()>;

    // 工作区和成员
    async fn create_workspace(&self, name: &str) -> Result<Workspace>;
    async fn workspaces(&self) -> Result<Vec<Workspace>>;
    async fn set_member(&self, workspace_id: &str, user_id: &str, role: Role) -> Result<()>;
    async fn member_role(&self, workspace_id: &str, user_id: &str) -> Result<Option<Role>>;
    async fn user_workspaces(&self, user_id: &str) -> Result<Vec<(Workspace, Role)>>;

    // 页面
    async fn create_page(&self, p: NewPage) -> Result<Page>;
    /// 含回收站里的。
    async fn page(&self, id: &str) -> Result<Option<Page>>;
    /// 不含回收站里的，按 (parent, position) 排。
    async fn pages(&self, workspace_id: &str) -> Result<Vec<Page>>;
    async fn update_page(
        &self,
        id: &str,
        title: Option<String>,
        icon: Option<Option<String>>,
    ) -> Result<Page>;
    /// 移动到 parent 下的 place；不能移到自己或子孙下面。
    async fn move_page(
        &self,
        id: &str,
        parent_id: Option<String>,
        place: Placement,
    ) -> Result<Page>;
    /// 连子页一起进回收站，返回这次进回收站的页面。
    async fn delete_page(&self, id: &str) -> Result<Vec<Page>>;

    // 正文
    async fn load_doc(&self, page_id: &str) -> Result<DocLog>;
    /// 追加一条增量，顺带更新 pages.updated_at。
    async fn append_update(&self, page_id: &str, update: Vec<u8>) -> Result<()>;
    /// 存一份完整状态当快照，删掉它已经包含的增量（此刻之前存的全部）。
    async fn compact(&self, page_id: &str, snapshot: Vec<u8>) -> Result<()>;
}
