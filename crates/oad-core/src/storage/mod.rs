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

    // 回收站
    /// 回收站里的：单独删的页面、库、行（跟着上级一起删的不单列）。新删的在前。
    async fn trash(&self, workspace_id: &str) -> Result<Vec<Page>>;
    /// 放回来：连同一起删的子页面（删除时间一样的）；上级不在了就放到最外层。行要库还在。返回放回来的页面。
    async fn restore_page(&self, id: &str) -> Result<Vec<Page>>;
    /// 彻底删（必须在回收站里）：连子页面、正文、版本、评论、行；没有别的页面用的上传文件也删掉。
    /// 返回 (删掉的页面 id, 要从文件存储里删的 blob)。
    async fn purge_page(&self, id: &str) -> Result<(Vec<String>, Vec<String>)>;
    /// 在回收站里放了超过这个时刻的（单独删的那些）。
    async fn trash_before(&self, before: i64) -> Result<Vec<String>>;
    /// 记下页面用了这些上传文件。
    async fn add_file_refs(&self, page_id: &str, files: Vec<String>) -> Result<()>;
    /// 记下谁删的（回收站显示）。
    async fn set_deleted_by(&self, ids: Vec<String>, by: &str) -> Result<()>;
    async fn deleted_by(&self, id: &str) -> Result<Option<String>>;

    // 个人、成员、邀请
    async fn user_prefs(&self, user_id: &str) -> Result<UserPrefs>;
    async fn update_user(&self, user_id: &str, p: PrefsPatch) -> Result<()>;
    async fn password_hash(&self, user_id: &str) -> Result<Option<String>>;
    /// 删掉这个人的会话（keep 留着这一个）。
    async fn delete_user_sessions(&self, user_id: &str, keep: Option<&str>) -> Result<()>;
    async fn remove_member(&self, workspace_id: &str, user_id: &str) -> Result<bool>;
    async fn update_workspace(
        &self,
        id: &str,
        name: Option<String>,
        icon: Option<Option<String>>,
    ) -> Result<Workspace>;
    async fn workspace(&self, id: &str) -> Result<Option<Workspace>>;
    async fn create_invite(&self, inv: Invite) -> Result<()>;
    /// 没撤销的，新的在前。
    async fn invites(&self, workspace_id: &str) -> Result<Vec<Invite>>;
    async fn invite(&self, token: &str) -> Result<Option<Invite>>;
    async fn revoke_invite(&self, token: &str) -> Result<bool>;
    async fn use_invite(&self, token: &str) -> Result<()>;

    // 分享
    /// 这个页面现在的分享（没撤销的）。
    async fn share_for_page(&self, page_id: &str) -> Result<Option<Share>>;
    /// 分享：已经有就改设置（链接不变），没有就新建。
    async fn put_share(
        &self,
        page_id: &str,
        include_children: bool,
        indexable: bool,
        by: &str,
    ) -> Result<Share>;
    async fn revoke_share(&self, page_id: &str) -> Result<bool>;
    async fn share_by_token(&self, token: &str) -> Result<Option<Share>>;
    /// 页面的上级链（从父页面到最外层）。
    async fn ancestors(&self, page_id: &str) -> Result<Vec<String>>;
    /// 页面用过这个文件吗。
    async fn page_uses_file(&self, page_id: &str, file_id: &str) -> Result<bool>;

    // 历史版本
    async fn add_version(&self, page_id: &str, v: NewVersion) -> Result<i64>;
    /// 新的在前。
    async fn versions(&self, page_id: &str, limit: usize) -> Result<Vec<VersionMeta>>;
    /// 一版和它的快照。
    async fn version(&self, page_id: &str, id: i64) -> Result<Option<(VersionMeta, Vec<u8>)>>;
    /// 库的格子改动：按行、属性过滤，新的在前。
    async fn db_ops(
        &self,
        db_id: &str,
        row: Option<&str>,
        prop: Option<&str>,
        limit: usize,
    ) -> Result<Vec<DbOpRow>>;
    /// 表结构改动，新的在前。
    async fn db_schema_ops(&self, db_id: &str, limit: usize) -> Result<Vec<SchemaOpRow>>;
    /// 撤销第 seq 次的格子改动（新建的行删掉、删掉的放回、格子换回原来的值；之后又被改过的格子不动），记成新的一次。
    /// 返回 (新 seq, 改了标题的行和新标题, 放回来的行)。
    #[allow(clippy::type_complexity)]
    async fn undo_db_ops(
        &self,
        db_id: &str,
        seq: i64,
        title_key: &str,
        by: &str,
    ) -> Result<(i64, Vec<(String, String)>, Vec<String>)>;

    /// 所有页面（含回收站里的，搜索索引启动时对比用）。
    async fn all_pages(&self) -> Result<Vec<Page>>;
    /// 所有库现在的 seq。
    async fn db_seqs(&self) -> Result<Vec<(String, i64)>>;

    // 数据库
    /// 建库：一个 kind = database 的页面 + 表结构、视图。
    async fn create_database(
        &self,
        page: NewPage,
        schema: serde_json::Value,
        views: serde_json::Value,
    ) -> Result<Page>;
    async fn database(&self, db_id: &str) -> Result<Option<DbMeta>>;
    /// 行：since 为 None 时是全部没删的；否则是 seq > since 的（改过的和删掉的分开给）。返回 (现在的 seq, 行, 删掉的行编号)。
    async fn db_rows(
        &self,
        db_id: &str,
        since: Option<i64>,
    ) -> Result<(i64, Vec<DbRow>, Vec<String>)>;
    async fn db_row(&self, row_id: &str) -> Result<Option<DbRow>>;
    /// 加行（每行也是一个 kind = row 的页面），一次加的共用一个 seq。返回 (新行编号, seq)。
    async fn add_db_rows(
        &self,
        db_id: &str,
        rows: Vec<NewRow>,
        by: &str,
    ) -> Result<(Vec<String>, i64)>;
    /// 改格子（每格最后写入为准）；title_key 那一格同时改行页面的标题。返回 (seq, 真改了的格子)。
    async fn patch_db_rows(
        &self,
        db_id: &str,
        title_key: &str,
        ops: Vec<CellOp>,
        by: &str,
    ) -> Result<(i64, Vec<CellChange>)>;
    /// 删行（行页面进回收站）。返回 seq。
    async fn delete_db_rows(&self, db_id: &str, ids: Vec<String>, by: &str) -> Result<i64>;
    /// 换表结构（和视图），记一条表结构改动，顺带改格子。返回 seq。
    async fn set_db_schema(&self, db_id: &str, w: SchemaWrite, by: &str) -> Result<i64>;
    async fn db_schema_op(&self, db_id: &str, seq: i64) -> Result<Option<SchemaOpRow>>;
    async fn set_db_views(&self, db_id: &str, views: serde_json::Value) -> Result<()>;
}
