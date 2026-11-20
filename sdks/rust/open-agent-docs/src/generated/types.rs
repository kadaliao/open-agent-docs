// 生成的，不要手改：改 docs/openapi.json 或 sdks/operations.json，再跑 `cargo run -p codegen`（在 sdks/rust 下）。
//! 模型：`docs/openapi.json` 的 `components.schemas`。
//!
//! 只在请求里用的模型，可空可省的字段是 [`Nullable`](crate::Nullable)（不发 / 发 null / 发值）；
//! 其余的可省字段是 `Option`（`None` 不发）。
#![allow(
    rustdoc::broken_intra_doc_links,
    rustdoc::invalid_html_tags,
    rustdoc::bare_urls,
    clippy::large_enum_variant,
    clippy::doc_lazy_continuation,
    clippy::doc_overindented_list_items
)]

use serde::{Deserialize, Serialize};

/// 处理完了：给 ids，或者 upto（编号 ≤ upto 的都算），至少给一个。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ack {
    /// 这些编号
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<i64>>,
    /// 编号 ≤ 这个的
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub upto: crate::Nullable<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Acked {
    /// 标了几条
    pub acked: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddRow {
    /// 排在这一行后面（不给放最后）
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub after: crate::Nullable<String>,
    /// 行页面的正文（Markdown）
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub body: crate::Nullable<String>,
    /// 属性：键是属性的 key 或名字，值是存的样子（选项也可以写名字）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub props: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddRows {
    pub rows: Vec<AddRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddedRows {
    /// 新行的 id（和给的顺序一样）
    pub ids: Vec<String>,
    pub seq: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Agent {
    pub about: String,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<i64>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<i64>,
    pub scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook: Option<String>,
    pub workspace_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentList {
    pub agents: Vec<AgentView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentNotes {
    /// 取满了一页时是最后一条的编号，带 `?cursor=` 接着取；没有就是取完了
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub notifications: Vec<AgentNotification>,
}

/// Agent 收到的一条通知（webhook 的正文；长轮询 / SSE 取到的也是这样）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentNotification {
    pub agent: NoteAgent,
    /// 评论挂在哪一块上（对整页的评论是 null）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    pub comment: NoteComment,
    pub created_at: i64,
    /// 通知编号（处理完带回来当 after / cursor）
    pub id: i64,
    pub page: NotePage,
    /// 一段可以直接当消息发给 Agent 的话（按 Agent 的语言）
    pub prompt: String,
    /// 选中的原文
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    pub thread_id: String,
    /// comment
    pub r#type: String,
}

/// Agent 列表里的一项：加上没处理的通知数和最近的 webhook 错误。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentView {
    pub about: String,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<i64>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<i64>,
    pub scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook: Option<String>,
    pub workspace_id: String,
    pub pending: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_error: Option<String>,
}

/// 评论挂在哪：一段文字（start / end 是网页给的 Yjs RelativePosition，服务器原样存）、整段（只有 block_id）或整页（None）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Anchor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
}

/// 登录方式和站点设置。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfigBody {
    /// 允许跨源嵌入的宿主（`OAD_ALLOWED_ORIGINS`，加上 `OAD_PUBLIC_URL` 的 origin），如 `https://app.example.com`。
    /// iframe 里的网页只认这些 origin（和同源）发来的 `oad:token`。旧版服务器没有这个字段
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_origins: Option<Vec<String>>,
    /// 登录方式：local（账号密码）/ header（宿主反代带身份头）/ none（不登录，单人）/ jwt（宿主签的 JWT）
    pub mode: AuthMode,
    pub signup: SignupMode,
    /// 服务器的时区（IANA 名字）
    pub timezone: String,
}

///
/// 服务器以后加了新的值时是 `Other`。
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AuthMode {
    /// `local`
    #[default]
    Local,
    /// `header`
    Header,
    /// `none`
    None,
    /// `jwt`
    Jwt,
    /// 这个版本的 SDK 还不认识的值
    Other(String),
}

impl AuthMode {
    /// 线上的写法
    pub fn as_str(&self) -> &str {
        match self {
            AuthMode::Local => "local",
            AuthMode::Header => "header",
            AuthMode::None => "none",
            AuthMode::Jwt => "jwt",
            AuthMode::Other(s) => s,
        }
    }
}

impl From<&str> for AuthMode {
    fn from(s: &str) -> Self {
        match s {
            "local" => AuthMode::Local,
            "header" => AuthMode::Header,
            "none" => AuthMode::None,
            "jwt" => AuthMode::Jwt,
            other => AuthMode::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for AuthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for AuthMode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AuthMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(AuthMode::from(s.as_str()))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthorRef {
    pub id: String,
    pub kind: PrincipalKind,
    pub name: String,
}

/// 每个 Yjs client id（字符串）是谁写的。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Authors {
    pub clients: std::collections::HashMap<String, AuthorRef>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlockBody {
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub after: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub base: crate::Nullable<String>,
    pub markdown: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlockContent {
    pub block: DocNode,
    pub markdown: String,
    pub version: String,
}

/// 对比里的一块。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlockDiff {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// 这块里在两版之间写了字的人
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<Vec<VersionAuthor>>,
    pub id: String,
    /// added / removed / changed / same
    pub op: String,
    /// 块的类型（paragraph、heading…）
    pub r#type: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlockIds {
    pub ids: Vec<String>,
}

/// 一格的改动。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CellChange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<serde_json::Value>,
    pub prop: String,
    pub row_id: String,
}

/// 改一行的几格：值为 null 的清掉。给了 `after` 是挪位置（排在那一行后面，null 是最前面）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CellOp {
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub after: crate::Nullable<String>,
    pub row_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set: Option<std::collections::HashMap<String, serde_json::Value>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommentView {
    pub author: AuthorRef,
    /// 删掉的是空字符串。
    pub body: String,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<i64>,
    pub id: String,
    pub thread_id: String,
}

/// 正文：`markdown`（默认）、`doc`（`?format=json`）或 `html`（`?format=html`，只读），带当前版本（改的时候作为 base 带回来）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocNode>,
    /// 只读 HTML（`?format=html`）：外面包 `<div class="oad-content">`，配 `/embed/content.css` 显示。
    /// 只有白名单里的标签和属性；公式、链接卡片是带 `data-*` 的占位；设了 `OAD_PUBLIC_URL` 时文件、页面链接是完整地址
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
    /// 顶层块一共几个（给了 `limit` 才有）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    pub version: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreateAgent {
    /// 介绍（给人和别的 Agent 看）
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub about: crate::Nullable<String>,
    /// 1 到 40 个字，不能有空白和 @ [ ] ( ) < > 引号 逗号
    pub name: String,
    /// read / write / admin
    pub scope: String,
    /// 收通知的 webhook 地址
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub webhook: crate::Nullable<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreatePage {
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub after: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub icon: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub parent_id: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub title: crate::Nullable<String>,
}

/// 新建的 Agent；token 和 webhook_secret 只有这一次能看到。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreatedAgent {
    pub agent: Agent,
    /// `oad_…`，当 Bearer token 用
    pub token: String,
    /// `whsec_…`，验 webhook 的签名
    pub webhook_secret: String,
}

/// 库的历史。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbHistory {
    /// 新的在前；同一次改动（同一个 seq）的格子不会分在两页
    pub items: Vec<DbHistoryItem>,
    /// 还有下一页时给
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// 库现在的 seq
    pub seq: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DbHistoryItemBy {
    AuthorRef(AuthorRef),
    String(String),
}

impl Default for DbHistoryItemBy {
    fn default() -> Self {
        DbHistoryItemBy::AuthorRef(Default::default())
    }
}

impl From<AuthorRef> for DbHistoryItemBy {
    fn from(v: AuthorRef) -> Self {
        DbHistoryItemBy::AuthorRef(v)
    }
}

impl From<String> for DbHistoryItemBy {
    fn from(v: String) -> Self {
        DbHistoryItemBy::String(v)
    }
}

/// 历史里的一条。`kind`：`cell`（一格改了）、`row`（加 / 删了一行）、`schema`（表结构改了，只在不按行 / 属性筛时有）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbHistoryItem {
    pub at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<DbHistoryItemBy>,
    /// schema 有：顺带改了几格
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cells: Option<u64>,
    /// schema 有：这次动了哪些属性
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<Vec<serde_json::Value>>,
    /// cell / row / schema
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<serde_json::Value>,
    /// cell 有：属性的 key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prop: Option<String>,
    /// cell 有：属性的名字
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prop_name: Option<String>,
    /// cell / row 有
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_id: Option<String>,
    /// cell / row 有
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_title: Option<String>,
    pub seq: i64,
    /// row / schema 有：一句话说明
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// schema 有：撤销过了
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undone: Option<bool>,
}

/// 库的信息（GET /databases/{id}）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbInfo {
    pub page: Page,
    pub schema: DbSchema,
    pub seq: i64,
    pub views: Vec<DbView>,
}

/// 一个属性（一列）。`key` 是短随机串，改名不动数据；各类型自己的配置只在那个类型上有。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbProp {
    /// button 按下时通知的 Agent（id）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// button 按下时发的评论
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// 日期的显示：iso | zh | slash | relative
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_format: Option<String>,
    /// 数字的格式：number | comma | percent | cny | usd（number、formula、rollup）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// formula 的公式
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    pub key: String,
    /// button 的文字
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub name: String,
    /// select / multi_select / status 的选项
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<SelectOption>>,
    /// 显示几位小数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precision: Option<u32>,
    /// unique_id 的前缀（T-12）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation: Option<RelationConf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollup: Option<RollupConf>,
    /// button 按下时设的值：`{ 属性 key 或名字: 值 }`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set: Option<serde_json::Value>,
    pub r#type: PropType,
}

/// 库里的一行。`props` 是存的样子（键是属性的 key），标题也在里面（和行页面的标题同步）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbRow {
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub db_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
    pub id: String,
    pub position: String,
    pub props: std::collections::HashMap<String, serde_json::Value>,
    pub seq: i64,
    pub unique_id: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

/// 表结构。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbSchema {
    /// 下一个自动编号
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_id: Option<u64>,
    pub props: Vec<DbProp>,
}

/// 排序：`dir: "asc" | "desc"`。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbSort {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    pub prop: String,
}

/// 撤销的结果：格子改动只有 `seq`；表结构改动还有新的 `schema`、`views`。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbUndone {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<DbSchema>,
    /// 撤销后库的 seq
    pub seq: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub views: Option<Vec<DbView>>,
}

/// 一个视图。存的时候按表结构清理（去掉指向已删属性的排序、分组、隐藏、列宽）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbView {
    /// 日历按哪个日期属性
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar_by: Option<String>,
    /// 筛选：`{ op: "and" | "or", items: [{ prop, op, value } 或嵌套的组] }`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<serde_json::Value>,
    /// 按哪个属性分组（看板的列）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    /// 按日期分组时怎么分：day | week | month | year | relative
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_date: Option<String>,
    /// 隐藏的属性 key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<Vec<String>>,
    /// 去掉没有行的分组
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_empty_groups: Option<bool>,
    pub id: String,
    pub name: String,
    /// 列的顺序（属性 key）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sorts: Option<Vec<DbSort>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_group_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_group_date: Option<String>,
    /// 每列底部的汇总：`{ key: fn }`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totals: Option<serde_json::Value>,
    pub r#type: ViewType,
    /// 列宽：`{ key: 像素 }`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widths: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Deleted {
    /// 进了回收站的页面（连子页面）
    pub deleted: Vec<String>,
}

/// 对比的另一头：版本号，或 `"current"`。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DiffTo {
    Integer(i64),
    String(String),
}

impl Default for DiffTo {
    fn default() -> Self {
        DiffTo::Integer(Default::default())
    }
}

impl From<i64> for DiffTo {
    fn from(v: i64) -> Self {
        DiffTo::Integer(v)
    }
}

impl From<String> for DiffTo {
    fn from(v: String) -> Self {
        DiffTo::String(v)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocMark {
    /// 没有属性时不给
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<serde_json::Value>,
    pub r#type: String,
}

/// 正文的节点树（和编辑器的 ProseMirror JSON 一样）：`{ type, attrs?, content?, text?, marks? }`。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocNode {
    /// 没有属性时不给
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<DocNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marks: Option<Vec<DocMark>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub r#type: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditComment {
    /// 新的正文（Markdown）
    pub body: String,
}

/// 统一的错误：`{ error: { code, message } }`。message 按调用者的语言（中文 / 英文）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ErrorBody {
    pub error: ErrorInfo,
}

/// 错误码：not_found 404、bad_request 400、unauthorized 401、forbidden 403、conflict 409、gone 410（邀请过期之类）、
/// too_large 413、too_many_requests 429（带 Retry-After）、bad_gateway 502（去外部网站取东西失败）、internal 500。
///
/// 服务器以后加了新的值时是 `Other`。
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// `not_found`
    #[default]
    NotFound,
    /// `bad_request`
    BadRequest,
    /// `unauthorized`
    Unauthorized,
    /// `forbidden`
    Forbidden,
    /// `conflict`
    Conflict,
    /// `gone`
    Gone,
    /// `too_large`
    TooLarge,
    /// `too_many_requests`
    TooManyRequests,
    /// `bad_gateway`
    BadGateway,
    /// `internal`
    Internal,
    /// 这个版本的 SDK 还不认识的值
    Other(String),
}

impl ErrorCode {
    /// 线上的写法
    pub fn as_str(&self) -> &str {
        match self {
            ErrorCode::NotFound => "not_found",
            ErrorCode::BadRequest => "bad_request",
            ErrorCode::Unauthorized => "unauthorized",
            ErrorCode::Forbidden => "forbidden",
            ErrorCode::Conflict => "conflict",
            ErrorCode::Gone => "gone",
            ErrorCode::TooLarge => "too_large",
            ErrorCode::TooManyRequests => "too_many_requests",
            ErrorCode::BadGateway => "bad_gateway",
            ErrorCode::Internal => "internal",
            ErrorCode::Other(s) => s,
        }
    }
}

impl From<&str> for ErrorCode {
    fn from(s: &str) -> Self {
        match s {
            "not_found" => ErrorCode::NotFound,
            "bad_request" => ErrorCode::BadRequest,
            "unauthorized" => ErrorCode::Unauthorized,
            "forbidden" => ErrorCode::Forbidden,
            "conflict" => ErrorCode::Conflict,
            "gone" => ErrorCode::Gone,
            "too_large" => ErrorCode::TooLarge,
            "too_many_requests" => ErrorCode::TooManyRequests,
            "bad_gateway" => ErrorCode::BadGateway,
            "internal" => ErrorCode::Internal,
            other => ErrorCode::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for ErrorCode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(ErrorCode::from(s.as_str()))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ErrorInfo {
    pub code: ErrorCode,
    /// 给人看的原因
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ids {
    /// 要删的行（进回收站）
    pub ids: Vec<String>,
}

/// multipart 导入的字段。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportForm {
    /// CSV 文件（第一行是列名）
    pub file: String,
    /// 放在哪个页面下
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub parent_id: crate::Nullable<String>,
    /// 库名
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub title: crate::Nullable<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Inbox {
    /// 取满了一页时是最后一条（最旧的）的编号，带 `?cursor=` 往旧的翻；没有就是到头了
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// 新的在前
    pub notifications: Vec<InboxNotification>,
    /// 没读的总数
    pub unread: i64,
}

/// 收件箱里的一条。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InboxNotification {
    pub created_at: i64,
    /// 页面、评论串、谁写的之类（按 type 不同）
    pub data: serde_json::Value,
    pub id: i64,
    /// 没读是 null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<i64>,
    /// mention / reply / resolved
    pub r#type: String,
    pub workspace_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InviteBy {
    pub name: String,
}

/// 邀请链接打开时看到的（不用登录）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InviteInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// 谁发的邀请；发邀请的人不在了是 null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invited_by: Option<InviteBy>,
    pub role: Role,
    /// 能不能凭它注册
    pub signup: bool,
    pub workspace: InviteWorkspace,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InviteList {
    /// 还能用的
    pub invites: Vec<InviteView>,
}

/// 一条邀请链接。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InviteView {
    pub created_at: i64,
    /// 毫秒时间戳；null = 不过期
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// 凭它加入后的角色
    pub role: Role,
    pub token: String,
    /// 发给别人的完整链接
    pub url: String,
    /// 用过几次
    pub uses: i64,
}

/// 邀请到哪个工作区。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InviteWorkspace {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
}

/// 我是谁、在哪些工作区。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Me {
    pub user: MeUser,
    pub workspaces: Vec<MeWorkspace>,
}

/// 改个人资料：只改给了的字段；给 null 是清掉。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MePatch {
    /// 头像（emoji 或图片地址）
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub avatar: crate::Nullable<String>,
    /// zh / en；null = 跟浏览器
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub locale: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub name: crate::Nullable<String>,
    /// IANA 时区名；null = 跟服务器
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub timezone: crate::Nullable<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeUser {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub id: String,
    pub kind: PrincipalKind,
    /// zh / en / null（跟浏览器）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeWorkspace {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub id: String,
    pub name: String,
    pub role: Role,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Member {
    pub role: Role,
    pub user: MemberUser,
}

/// 工作区的人（按加入先后）和 Agent。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemberList {
    pub agents: Vec<AgentView>,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemberRole {
    pub role: Role,
    pub user_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemberUser {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub id: String,
    pub name: String,
}

/// 建库（POST /workspaces/{ws}/databases 的正文）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewDb {
    /// 放在同级里这个页面后面
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub after: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub parent_id: crate::Nullable<String>,
    /// 表结构 `{ props: [{ name, type, … }] }`，和 DbSchema 一样，但属性可以不给 key（自动分配），
    /// 选项可以只写名字（`options: ["做", "完"]`）。不给只有标题
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub schema: crate::Nullable<serde_json::Value>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub title: crate::Nullable<String>,
    /// 视图（和 DbView 一样，id 可以不给）；不给是一个表格视图
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub views: crate::Nullable<Vec<serde_json::Value>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewInvite {
    /// 多少秒后过期；不给 = 不过期
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub expires_in: crate::Nullable<i64>,
    /// viewer / editor / admin / owner
    pub role: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewSecret {
    /// 新的 webhook 签名密钥
    pub webhook_secret: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewThread {
    /// 挂在哪；不给 = 整页
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub anchor: crate::Nullable<Anchor>,
    /// Markdown；`@[名字](agent:<id>)` / `@[名字](user:<id>)` 或 `@名字` 叫人
    pub body: String,
    /// 叫谁：不给 / null = 按规则，false = 谁也不叫，`["agent:<id>", ...]` = 只叫这些
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub notify: crate::Nullable<Notify>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewToken {
    /// 新 token（旧的马上失效）
    pub token: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NoteAgent {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NoteComment {
    /// `{ kind, id, name }`
    pub author: serde_json::Value,
    pub body: String,
    pub id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotePage {
    pub id: String,
    pub title: String,
    pub url: String,
}

/// 评论时叫谁：false = 谁也不叫，`["agent:<id>", ...]` = 只叫这些（不给 / null = 按规则）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Notify {
    Bool(bool),
    List(Vec<String>),
}

impl Default for Notify {
    fn default() -> Self {
        Notify::Bool(Default::default())
    }
}

impl From<bool> for Notify {
    fn from(v: bool) -> Self {
        Notify::Bool(v)
    }
}

impl From<Vec<String>> for Notify {
    fn from(v: Vec<String>) -> Self {
        Notify::List(v)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Page {
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub id: String,
    /// page / database / row。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub position: String,
    pub title: String,
    pub updated_at: i64,
    pub workspace_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageList {
    /// 不含回收站里的、库里的行；按 (parent_id, position) 排
    pub pages: Vec<Page>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PasswordBody {
    /// 现在的密码
    pub current: String,
    /// 新密码
    pub new: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PatchAgent {
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub about: crate::Nullable<String>,
    /// 通知里那段话用的语言：zh / en；null = 跟工作区
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub locale: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub name: crate::Nullable<String>,
    /// read / write / admin
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub scope: crate::Nullable<String>,
    /// null = 不用 webhook
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub webhook: crate::Nullable<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PatchPage {
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub after: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub icon: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub parent_id: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub title: crate::Nullable<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PatchRows {
    pub ops: Vec<CellOp>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PatchThread {
    /// true = 解决，false = 重新打开
    pub resolved: bool,
}

/// 改了的格子和改完的 seq。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Patched {
    /// 真的变了的格子（值没变的不在里面）
    pub changes: Vec<CellChange>,
    pub seq: i64,
}

/// 按按钮的结果。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pressed {
    /// 按钮设的属性里真的变了的格子
    pub changes: Vec<CellChange>,
    pub seq: i64,
    /// 按钮带评论时，发在这一行上的评论串
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

///
/// 服务器以后加了新的值时是 `Other`。
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PrincipalKind {
    /// `user`
    #[default]
    User,
    /// `agent`
    Agent,
    /// 这个版本的 SDK 还不认识的值
    Other(String),
}

impl PrincipalKind {
    /// 线上的写法
    pub fn as_str(&self) -> &str {
        match self {
            PrincipalKind::User => "user",
            PrincipalKind::Agent => "agent",
            PrincipalKind::Other(s) => s,
        }
    }
}

impl From<&str> for PrincipalKind {
    fn from(s: &str) -> Self {
        match s {
            "user" => PrincipalKind::User,
            "agent" => PrincipalKind::Agent,
            other => PrincipalKind::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for PrincipalKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for PrincipalKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PrincipalKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(PrincipalKind::from(s.as_str()))
    }
}

/// 属性类型。
///
/// 服务器以后加了新的值时是 `Other`。
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PropType {
    /// `title`
    #[default]
    Title,
    /// `text`
    Text,
    /// `number`
    Number,
    /// `select`
    Select,
    /// `multi_select`
    MultiSelect,
    /// `status`
    Status,
    /// `date`
    Date,
    /// `person`
    Person,
    /// `checkbox`
    Checkbox,
    /// `url`
    Url,
    /// `email`
    Email,
    /// `phone`
    Phone,
    /// `files`
    Files,
    /// `created_time`
    CreatedTime,
    /// `updated_time`
    UpdatedTime,
    /// `created_by`
    CreatedBy,
    /// `updated_by`
    UpdatedBy,
    /// `relation`
    Relation,
    /// `rollup`
    Rollup,
    /// `formula`
    Formula,
    /// `unique_id`
    UniqueId,
    /// `button`
    Button,
    /// 这个版本的 SDK 还不认识的值
    Other(String),
}

impl PropType {
    /// 线上的写法
    pub fn as_str(&self) -> &str {
        match self {
            PropType::Title => "title",
            PropType::Text => "text",
            PropType::Number => "number",
            PropType::Select => "select",
            PropType::MultiSelect => "multi_select",
            PropType::Status => "status",
            PropType::Date => "date",
            PropType::Person => "person",
            PropType::Checkbox => "checkbox",
            PropType::Url => "url",
            PropType::Email => "email",
            PropType::Phone => "phone",
            PropType::Files => "files",
            PropType::CreatedTime => "created_time",
            PropType::UpdatedTime => "updated_time",
            PropType::CreatedBy => "created_by",
            PropType::UpdatedBy => "updated_by",
            PropType::Relation => "relation",
            PropType::Rollup => "rollup",
            PropType::Formula => "formula",
            PropType::UniqueId => "unique_id",
            PropType::Button => "button",
            PropType::Other(s) => s,
        }
    }
}

impl From<&str> for PropType {
    fn from(s: &str) -> Self {
        match s {
            "title" => PropType::Title,
            "text" => PropType::Text,
            "number" => PropType::Number,
            "select" => PropType::Select,
            "multi_select" => PropType::MultiSelect,
            "status" => PropType::Status,
            "date" => PropType::Date,
            "person" => PropType::Person,
            "checkbox" => PropType::Checkbox,
            "url" => PropType::Url,
            "email" => PropType::Email,
            "phone" => PropType::Phone,
            "files" => PropType::Files,
            "created_time" => PropType::CreatedTime,
            "updated_time" => PropType::UpdatedTime,
            "created_by" => PropType::CreatedBy,
            "updated_by" => PropType::UpdatedBy,
            "relation" => PropType::Relation,
            "rollup" => PropType::Rollup,
            "formula" => PropType::Formula,
            "unique_id" => PropType::UniqueId,
            "button" => PropType::Button,
            other => PropType::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for PropType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for PropType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PropType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(PropType::from(s.as_str()))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PutContent {
    /// 读的时候拿到的版本：在那个版本上改，这期间别人写的不会被覆盖
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub base: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub doc: crate::Nullable<DocNode>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub markdown: crate::Nullable<String>,
}

/// 分享设置；不给正文就都是 false。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PutShare {
    /// 子页面也公开
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_children: Option<bool>,
    /// 允许搜索引擎收录
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexable: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PutViewsObject {
    pub views: Vec<ViewObj>,
}

/// 换掉全部视图的正文：视图的数组，或 `{ views: [...] }`。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PutViews {
    List(Vec<ViewObj>),
    Object(PutViewsObject),
}

impl Default for PutViews {
    fn default() -> Self {
        PutViews::List(Default::default())
    }
}

impl From<Vec<ViewObj>> for PutViews {
    fn from(v: Vec<ViewObj>) -> Self {
        PutViews::List(v)
    }
}

impl From<PutViewsObject> for PutViews {
    fn from(v: PutViewsObject) -> Self {
        PutViews::Object(v)
    }
}

/// 一个分组。空值的分组 `key` 是 ""，排在最后。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QueryGroup {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub count: u64,
    /// 子分组（视图有 sub_group_by 时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<QueryGroup>>,
    pub ids: Vec<String>,
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totals: Option<serde_json::Value>,
}

/// 服务器算的查询（给 Agent、给不想加载全部数据的客户端）。给了 view_id 就从那个视图出发，别的字段覆盖它。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QueryReq {
    /// 筛选 `{ op: "and" | "or", items: [...] }`
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub filter: crate::Nullable<serde_json::Value>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub group_by: crate::Nullable<String>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub limit: crate::Nullable<u64>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub offset: crate::Nullable<u64>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub search: crate::Nullable<String>,
    /// 排序 `[{ key, dir }]`
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub sorts: crate::Nullable<Vec<serde_json::Value>>,
    /// 每列底部的汇总：`{ 属性 key: 汇总方式 }`
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub totals: crate::Nullable<serde_json::Value>,
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub view_id: crate::Nullable<String>,
}

/// 查询结果。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QueryResult {
    /// 按 group_by 分的组；没分组不给
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<QueryGroup>>,
    /// 按排序的行 id（给了 limit / offset 是那一段）
    pub ids: Vec<String>,
    /// 这一段的行，带算出来的值
    pub rows: Vec<QueryRow>,
    /// 库现在的 seq
    pub seq: i64,
    /// 符合条件的总行数
    pub total: u64,
    /// 每列底部的汇总（键是属性的 key）；没要不给
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totals: Option<serde_json::Value>,
}

/// 查出来的一行：存的样子（`props`，键是属性的 key，空值不存），加上 `computed`（公式、汇总、反向关联算出来的值）。
/// 和 `DbRow` 不同：不带 `db_id`、`position`（顺序看 `ids`），时间是毫秒数。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QueryRow {
    pub computed: serde_json::Value,
    /// 毫秒时间戳
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub id: String,
    pub props: serde_json::Value,
    pub seq: i64,
    /// 自动编号
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<i64>,
    /// 毫秒时间戳
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadCount {
    /// 标了几条
    pub read: u64,
}

/// 标成已读：给 ids，或者 `all: true`。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadNotes {
    /// 全部
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    /// 这些编号
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub ids: crate::Nullable<Vec<i64>>,
}

/// 关联：指向哪个库；`mirror_of` 有值时是双向关联的反向那一半。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelationConf {
    pub db_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror_of: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reply {
    /// Markdown
    pub body: String,
    /// 同 NewThread.notify
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub notify: crate::Nullable<Notify>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Restored {
    /// 恢复后新存的一版（kind = restore）
    pub version: VersionMeta,
}

/// 放回来的那个页面，加上 `restored`（连同放回来的所有页面 id）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RestoredPage {
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub id: String,
    /// page / database / row。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub position: String,
    pub title: String,
    pub updated_at: i64,
    pub workspace_id: String,
    /// 放回来的所有页面（这个和一起删的子页面）
    pub restored: Vec<String>,
}

///
/// 服务器以后加了新的值时是 `Other`。
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Role {
    /// `viewer`
    #[default]
    Viewer,
    /// `editor`
    Editor,
    /// `admin`
    Admin,
    /// `owner`
    Owner,
    /// 这个版本的 SDK 还不认识的值
    Other(String),
}

impl Role {
    /// 线上的写法
    pub fn as_str(&self) -> &str {
        match self {
            Role::Viewer => "viewer",
            Role::Editor => "editor",
            Role::Admin => "admin",
            Role::Owner => "owner",
            Role::Other(s) => s,
        }
    }
}

impl From<&str> for Role {
    fn from(s: &str) -> Self {
        match s {
            "viewer" => Role::Viewer,
            "editor" => Role::Editor,
            "admin" => Role::Admin,
            "owner" => Role::Owner,
            other => Role::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Role {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Role {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Role::from(s.as_str()))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoleBody {
    /// viewer / editor / admin / owner
    pub role: String,
}

/// 汇总：沿关联属性 `relation` 取对方库的属性 `prop`，按 `fn` 归总。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RollupConf {
    pub r#fn: String,
    pub prop: String,
    pub relation: String,
}

/// 增量（GET /databases/{id}/rows）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RowsDelta {
    pub deleted: Vec<String>,
    pub rows: Vec<DbRow>,
    pub seq: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveBody {
    /// 这一版的名字
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub label: crate::Nullable<String>,
}

/// 改完的表结构。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SchemaChanged {
    /// 动了哪些属性（`{ op, key, name, … }`）
    pub changes: Vec<serde_json::Value>,
    /// 新的表结构
    pub schema: DbSchema,
    /// 这次改动的 seq（历史里撤销用；什么也没变时是现在的 seq）
    pub seq: i64,
    /// 改了但有问题的地方（比如公式有错）；没有就不给
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SchemaOps {
    /// 依次做的改动：`{ op: "add" | "update" | "delete" | "move" | …, … }`，见 docs/phase4-contract.md
    pub ops: Vec<serde_json::Value>,
}

/// 撤销后的表结构和视图。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SchemaUndone {
    pub schema: DbSchema,
    pub seq: i64,
    pub views: Vec<DbView>,
}

///
/// 服务器以后加了新的值时是 `Other`。
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Scope {
    /// `read`
    #[default]
    Read,
    /// `write`
    Write,
    /// `admin`
    Admin,
    /// 这个版本的 SDK 还不认识的值
    Other(String),
}

impl Scope {
    /// 线上的写法
    pub fn as_str(&self) -> &str {
        match self {
            Scope::Read => "read",
            Scope::Write => "write",
            Scope::Admin => "admin",
            Scope::Other(s) => s,
        }
    }
}

impl From<&str> for Scope {
    fn from(s: &str) -> Self {
        match s {
            "read" => Scope::Read,
            "write" => Scope::Write,
            "admin" => Scope::Admin,
            other => Scope::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Scope {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Scope {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Scope::from(s.as_str()))
    }
}

/// 一条搜索结果（GET /workspaces/{ws}/search）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    /// 这个页面命中了几处（标题、正文的段）
    pub blocks: u64,
    /// 行所在的库
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// page / database / row
    pub kind: String,
    pub page_id: String,
    pub score: f64,
    pub snippet: String,
    pub title: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchResults {
    pub results: Vec<SearchResult>,
}

/// 选项。status 的有 `group`：todo / doing / done。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectOption {
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub id: String,
    pub name: String,
}

/// 改完后库的 seq。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Seq {
    pub seq: i64,
}

/// 页面的公开分享链接。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShareView {
    pub created_at: i64,
    /// 子页面也公开
    pub include_children: bool,
    /// 允许搜索引擎收录
    pub indexable: bool,
    pub token: String,
    /// 公开的网址（`/s/<token>`）
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SignupBody {
    pub email: String,
    /// 邀请的 token（OAD_SIGNUP=invite 时必须有），注册完直接加入那个工作区
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub invite: crate::Nullable<String>,
    /// 显示的名字；不给就用邮箱 @ 前面那段
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub password: String,
}

/// 注册方式。
///
/// 服务器以后加了新的值时是 `Other`。
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SignupMode {
    /// `invite`
    #[default]
    Invite,
    /// `open`
    Open,
    /// `off`
    Off,
    /// 这个版本的 SDK 还不认识的值
    Other(String),
}

impl SignupMode {
    /// 线上的写法
    pub fn as_str(&self) -> &str {
        match self {
            SignupMode::Invite => "invite",
            SignupMode::Open => "open",
            SignupMode::Off => "off",
            SignupMode::Other(s) => s,
        }
    }
}

impl From<&str> for SignupMode {
    fn from(s: &str) -> Self {
        match s {
            "invite" => SignupMode::Invite,
            "open" => SignupMode::Open,
            "off" => SignupMode::Off,
            other => SignupMode::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for SignupMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for SignupMode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SignupMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(SignupMode::from(s.as_str()))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThreadList {
    pub threads: Vec<ThreadView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThreadView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    pub comments: Vec<CommentView>,
    pub created_at: i64,
    pub created_by: AuthorRef,
    pub id: String,
    pub page_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<AuthorRef>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrashItem {
    pub deleted_at: i64,
    /// 谁删的；没记是 null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_by: Option<AuthorRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// 原来在哪个页面下（行是所在的库）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_title: Option<String>,
    pub title: String,
}

/// 回收站里的页面、库、行（只列单独删的，新删的在前）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrashList {
    pub items: Vec<TrashItem>,
    /// 还有下一页时给
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Undo {
    /// 要撤销的那次表结构改动
    pub seq: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UndoBody {
    /// 要撤销的那次改动（历史里的 seq）
    pub seq: i64,
}

/// 链接卡片。除了 kind / provider / source / url 都可能缺；按 kind 还有别的字段
/// （canonical_url、description、site_name、favicon、author、thumbnail、media、published_at、embed；
/// 上传的文件有 name、mime、size、width、height、blurhash、duration_ms；本站的库有 db_id、view_id、icon、views）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UnfurlCard {
    /// video / post / repo / issue / pull_request / article / image / file / bookmark / database
    pub kind: String,
    /// youtube / x / github / bilibili / wechat / generic；上传的文件是 upload，本站的库是 oad
    pub provider: String,
    /// api / syndication / oembed / html / url_only
    pub source: String,
    /// 过期了先给旧的，后台在刷新
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub url: String,
}

/// multipart 上传的表单。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UploadForm {
    /// 文件（文件名取这个字段的 filename）
    pub file: String,
}

/// 上传接口的返回。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Uploaded {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurhash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    pub id: String,
    pub mime: String,
    pub name: String,
    pub size: i64,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
}

/// 一版里改过的人。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionAuthor {
    pub id: String,
    pub kind: PrincipalKind,
    pub name: String,
}

/// 某一版的内容。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionContent {
    pub doc: DocNode,
    pub markdown: String,
    pub version: VersionMeta,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionDiff {
    /// 按块的增删改，按新的顺序
    pub blocks: Vec<BlockDiff>,
    pub from: i64,
    pub to: DiffTo,
}

/// 版本列表，新的在前。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionList {
    /// 还有下一页时给，带上它再取
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub versions: Vec<VersionMeta>,
}

/// 历史版本（不含快照）。kind：auto（停下来 / 改得多了自动存）、agent（Agent 的一次写入）、restore（恢复）、manual（手动存，带名字）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionMeta {
    pub authors: Vec<VersionAuthor>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub id: i64,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub page_id: String,
}

/// 一个视图（DbView 的样子；id 不给或重复时分配新的，type 不给是 table，name 不给是“视图”）。
pub type ViewObj = serde_json::Value;

/// 视图类型。
///
/// 服务器以后加了新的值时是 `Other`。
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ViewType {
    /// `table`
    #[default]
    Table,
    /// `board`
    Board,
    /// `calendar`
    Calendar,
    /// `gallery`
    Gallery,
    /// `list`
    List,
    /// 这个版本的 SDK 还不认识的值
    Other(String),
}

impl ViewType {
    /// 线上的写法
    pub fn as_str(&self) -> &str {
        match self {
            ViewType::Table => "table",
            ViewType::Board => "board",
            ViewType::Calendar => "calendar",
            ViewType::Gallery => "gallery",
            ViewType::List => "list",
            ViewType::Other(s) => s,
        }
    }
}

impl From<&str> for ViewType {
    fn from(s: &str) -> Self {
        match s {
            "table" => ViewType::Table,
            "board" => ViewType::Board,
            "calendar" => ViewType::Calendar,
            "gallery" => ViewType::Gallery,
            "list" => ViewType::List,
            other => ViewType::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for ViewType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for ViewType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ViewType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(ViewType::from(s.as_str()))
    }
}

/// 库的全部视图。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Views {
    pub views: Vec<DbView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WsPatch {
    /// 图标（emoji 或图片地址）；null = 去掉
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub icon: crate::Nullable<String>,
    /// 新名字
    #[serde(skip_serializing_if = "crate::Nullable::is_omitted")]
    pub name: crate::Nullable<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommentCreatedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread: Option<ThreadView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbChangedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbSchemaEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PagePurgedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// 工作区事件流（`client.workspaces().events(ws)`）里一条事件的数据，按事件名解析好。
///
/// 不认识的事件名、或者数据和模型对不上时是 `Other`（原始 JSON）。
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum WorkspaceEvent {
    /// `comment.created`：SSE：新评论
    CommentCreated(CommentCreatedEvent),
    /// `comment.deleted`：SSE：评论删了
    CommentDeleted(serde_json::Value),
    /// `comment.updated`：SSE：评论改了
    CommentUpdated(serde_json::Value),
    /// `db.changed`：SSE：库的行变了，拉 rows?since=
    DbChanged(DbChangedEvent),
    /// `db.schema`：SSE：库的表结构或视图变了
    DbSchema(DbSchemaEvent),
    /// `notification`：SSE：给本人的通知（收件箱）
    Notification(serde_json::Value),
    /// `page.created`：SSE（GET /workspaces/{ws}/events）：新建 / 从回收站放回的页面
    PageCreated(Page),
    /// `page.deleted`：SSE：页面进了回收站（行不发）
    PageDeleted(Page),
    /// `page.moved`：SSE：页面挪了位置
    PageMoved(Page),
    /// `page.purged`：SSE：页面彻底删了
    PagePurged(PagePurgedEvent),
    /// `page.updated`：SSE：页面改了标题、图标（行的标题改了也有，kind = row）
    PageUpdated(Page),
    /// `resync`：SSE：落下了事件，重新拉列表
    Resync(serde_json::Value),
    /// `thread.updated`：SSE：评论串解决 / 重新打开
    ThreadUpdated(serde_json::Value),
    /// `workspace.updated`：SSE：工作区改了名字、图标
    WorkspaceUpdated(Workspace),
    /// 别的事件（原始 JSON）
    Other(serde_json::Value),
}

impl WorkspaceEvent {
    /// 按事件名解析 `data`。
    pub fn parse(name: &str, data: serde_json::Value) -> WorkspaceEvent {
        fn conv<T: serde::de::DeserializeOwned>(v: &serde_json::Value) -> Option<T> {
            T::deserialize(v).ok()
        }
        let parsed = match name {
            "comment.created" => conv(&data).map(WorkspaceEvent::CommentCreated),
            "comment.deleted" => conv(&data).map(WorkspaceEvent::CommentDeleted),
            "comment.updated" => conv(&data).map(WorkspaceEvent::CommentUpdated),
            "db.changed" => conv(&data).map(WorkspaceEvent::DbChanged),
            "db.schema" => conv(&data).map(WorkspaceEvent::DbSchema),
            "notification" => conv(&data).map(WorkspaceEvent::Notification),
            "page.created" => conv(&data).map(WorkspaceEvent::PageCreated),
            "page.deleted" => conv(&data).map(WorkspaceEvent::PageDeleted),
            "page.moved" => conv(&data).map(WorkspaceEvent::PageMoved),
            "page.purged" => conv(&data).map(WorkspaceEvent::PagePurged),
            "page.updated" => conv(&data).map(WorkspaceEvent::PageUpdated),
            "resync" => conv(&data).map(WorkspaceEvent::Resync),
            "thread.updated" => conv(&data).map(WorkspaceEvent::ThreadUpdated),
            "workspace.updated" => conv(&data).map(WorkspaceEvent::WorkspaceUpdated),
            _ => None,
        };
        parsed.unwrap_or(WorkspaceEvent::Other(data))
    }
}
