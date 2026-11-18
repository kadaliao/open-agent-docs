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

    /// 工作区成员（含 Agent）。
    async fn members(&self, workspace_id: &str) -> Result<Vec<(User, Role)>>;

    // Agent
    /// 建 Agent：users 里一行、agents 里一行、按 scope 加进工作区。名字重复报 Conflict。
    async fn create_agent(&self, a: NewAgent) -> Result<Agent>;
    async fn agent(&self, id: &str) -> Result<Option<Agent>>;
    async fn agent_by_token(&self, token_hash: &str) -> Result<Option<Agent>>;
    async fn agents(&self, workspace_id: &str) -> Result<Vec<Agent>>;
    async fn update_agent(&self, id: &str, patch: AgentPatch) -> Result<Agent>;
    async fn set_agent_token(&self, id: &str, token_hash: &str) -> Result<()>;
    async fn set_webhook_secret(&self, id: &str, secret: &str) -> Result<()>;
    /// 撤销：作废 token、移出工作区，名字留着。
    async fn revoke_agent(&self, id: &str) -> Result<Agent>;
    async fn touch_agent(&self, id: &str, now: i64) -> Result<()>;
    /// 每个 Agent 没处理的通知数、最近一次 webhook 错误。
    async fn agent_stats(&self, workspace_id: &str) -> Result<Vec<(String, i64, Option<String>)>>;

    // 作者
    async fn add_doc_clients(
        &self,
        page_id: &str,
        clients: Vec<(u64, PrincipalKind, String)>,
    ) -> Result<()>;
    /// (client id, 类型, 编号, 名字)
    async fn doc_clients(&self, page_id: &str)
    -> Result<Vec<(u64, PrincipalKind, String, String)>>;

    // 评论
    async fn create_thread(&self, t: Thread, first: Comment) -> Result<()>;
    async fn add_comment(&self, c: Comment) -> Result<()>;
    async fn thread(&self, id: &str) -> Result<Option<Thread>>;
    /// resolved：None = 全部，Some(b) = 只要解决了 / 没解决的。
    async fn threads(
        &self,
        page_id: &str,
        resolved: Option<bool>,
    ) -> Result<Vec<(Thread, Vec<Comment>)>>;
    async fn thread_comments(&self, thread_id: &str) -> Result<Vec<Comment>>;
    async fn comment(&self, id: &str) -> Result<Option<Comment>>;
    async fn edit_comment(&self, id: &str, body: &str, now: i64) -> Result<()>;
    async fn delete_comment(&self, id: &str, now: i64) -> Result<()>;
    /// Some((时间, 谁)) = 解决，None = 重新打开。
    async fn resolve_thread(&self, id: &str, resolved: Option<(i64, String)>) -> Result<()>;

    // 通知
    async fn add_agent_notifications(
        &self,
        rows: Vec<NewNotification>,
    ) -> Result<Vec<NotificationRow>>;
    /// after 之后的（after 为 None 时只要没处理的），按编号排。
    async fn agent_notifications(
        &self,
        scope: NoteScope,
        after: Option<i64>,
        limit: i64,
    ) -> Result<Vec<NotificationRow>>;
    async fn ack_agent_notifications(
        &self,
        scope: NoteScope,
        ids: Vec<i64>,
        upto: Option<i64>,
        now: i64,
    ) -> Result<u64>;
    /// 到时间该投 webhook 的。
    async fn due_webhooks(&self, now: i64, limit: i64) -> Result<Vec<NotificationRow>>;
    /// 下一条要投的时间。
    async fn next_webhook_at(&self) -> Result<Option<i64>>;
    /// 记下一次投递的结果：送到了（也算处理完），或者失败（next 为下次时间，None = 放弃）。
    async fn record_delivery(
        &self,
        id: i64,
        ok: bool,
        error: Option<String>,
        next: Option<i64>,
        now: i64,
    ) -> Result<()>;
    async fn add_user_notifications(
        &self,
        rows: Vec<NewNotification>,
    ) -> Result<Vec<NotificationRow>>;
    async fn user_notifications(
        &self,
        user_id: &str,
        after: Option<i64>,
        limit: i64,
    ) -> Result<Vec<NotificationRow>>;
    async fn read_user_notifications(
        &self,
        user_id: &str,
        ids: Option<Vec<i64>>,
        now: i64,
    ) -> Result<u64>;
    async fn unread_user_notifications(&self, user_id: &str) -> Result<i64>;
    /// 删掉早于 before 的通知。
    async fn purge_notifications(&self, before: i64) -> Result<u64>;

    // 链接解析缓存
    async fn unfurl(&self, url_key: &str) -> Result<Option<UnfurlRow>>;
    async fn put_unfurl(&self, row: UnfurlRow) -> Result<()>;

    // 文件
    /// 记下一份内容（已有就不动）。
    async fn put_blob(&self, b: BlobInfo) -> Result<()>;
    async fn blob(&self, hash: &str) -> Result<Option<BlobInfo>>;
    async fn create_file(&self, f: FileRow) -> Result<()>;
    async fn file(&self, id: &str) -> Result<Option<FileRow>>;
    /// 转存的外部图片：记下来源。
    async fn put_media(&self, hash: &str, source_url: &str) -> Result<()>;
    /// 这个网址转存过的图片。
    async fn media_by_source(&self, source_url: &str) -> Result<Option<BlobInfo>>;
    /// 是不是转存的图片。
    async fn is_media(&self, hash: &str) -> Result<bool>;
}
