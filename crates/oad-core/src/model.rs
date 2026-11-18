//! 领域对象。时间都是毫秒时间戳（UTC）。
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Viewer,
    Editor,
    Owner,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Editor => "editor",
            Role::Owner => "owner",
        }
    }
    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "viewer" => Some(Role::Viewer),
            "editor" => Some(Role::Editor),
            "owner" => Some(Role::Owner),
            _ => None,
        }
    }
    pub fn can_write(self) -> bool {
        self >= Role::Editor
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PrincipalKind {
    User,
    Agent,
}

impl PrincipalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PrincipalKind::User => "user",
            PrincipalKind::Agent => "agent",
        }
    }
    pub fn parse(s: &str) -> PrincipalKind {
        if s == "agent" {
            PrincipalKind::Agent
        } else {
            PrincipalKind::User
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub kind: PrincipalKind,
    pub created_at: i64,
}

/// 发起请求的是谁。
#[derive(Clone, Debug, PartialEq)]
pub struct Principal {
    pub user_id: String,
    pub name: String,
    pub kind: PrincipalKind,
}

impl From<&User> for Principal {
    fn from(u: &User) -> Self {
        Principal {
            user_id: u.id.clone(),
            name: u.name.clone(),
            kind: u.kind,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Page {
    pub id: String,
    pub workspace_id: String,
    pub parent_id: Option<String>,
    pub position: String,
    pub title: String,
    pub icon: Option<String>,
    pub created_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
}

/// 放在同级的哪里。
#[derive(Clone, Debug, PartialEq)]
pub enum Placement {
    First,
    Last,
    After(String),
}

pub struct NewPage {
    pub workspace_id: String,
    pub parent_id: Option<String>,
    pub place: Placement,
    pub title: String,
    pub icon: Option<String>,
    pub created_by: Option<String>,
    /// 正文的初始状态（Yrs v1），存成第一份快照。
    pub initial_state: Vec<u8>,
}

pub struct NewUser {
    pub name: String,
    pub email: Option<String>,
    pub password_hash: Option<String>,
    pub external_id: Option<String>,
    pub kind: PrincipalKind,
}

/// 存着的正文：最近一次快照 + 之后的增量。
#[derive(Default)]
pub struct DocLog {
    pub snapshot: Option<Vec<u8>>,
    pub updates: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Read,
    Write,
    Admin,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Read => "read",
            Scope::Write => "write",
            Scope::Admin => "admin",
        }
    }
    pub fn parse(s: &str) -> Option<Scope> {
        match s {
            "read" => Some(Scope::Read),
            "write" => Some(Scope::Write),
            "admin" => Some(Scope::Admin),
            _ => None,
        }
    }
    /// 在工作区成员表里对应的角色。
    pub fn role(self) -> Role {
        match self {
            Scope::Read => Role::Viewer,
            Scope::Write | Scope::Admin => Role::Editor,
        }
    }
    pub fn can_write(self) -> bool {
        self != Scope::Read
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Agent {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub about: String,
    pub scope: Scope,
    pub webhook: Option<String>,
    #[serde(skip)]
    pub webhook_secret: String,
    pub created_by: Option<String>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

impl Agent {
    pub fn active(&self) -> bool {
        self.revoked_at.is_none()
    }
}

/// 一串评论。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Thread {
    pub id: String,
    pub page_id: String,
    pub block_id: Option<String>,
    pub anchor_start: Option<String>,
    pub anchor_end: Option<String>,
    pub quote: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
    pub resolved_by: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Comment {
    pub id: String,
    pub thread_id: String,
    pub author_kind: PrincipalKind,
    pub author_id: String,
    pub body: String,
    pub created_at: i64,
    pub edited_at: Option<i64>,
    pub deleted_at: Option<i64>,
}

/// 存着的一条通知（Agent 的或人的）。`data` 是 JSON。
#[derive(Clone, Debug, PartialEq)]
pub struct NotificationRow {
    pub id: i64,
    pub workspace_id: String,
    /// Agent 编号或用户编号。
    pub owner_id: String,
    pub kind: String,
    pub data: serde_json::Value,
    pub created_at: i64,
    pub delivered_at: Option<i64>,
    /// Agent 的：处理完的时间；人的：已读的时间。
    pub done_at: Option<i64>,
    pub attempts: i64,
    pub error: Option<String>,
}

/// 读哪些 Agent 通知。
#[derive(Clone, Debug)]
pub enum NoteScope {
    /// 这个 Agent 自己的。
    Agent(String),
    /// 工作区里所有 Agent 的（admin）。
    Workspace(String),
}

pub struct NewAgent {
    pub workspace_id: String,
    pub name: String,
    pub about: String,
    pub scope: Scope,
    pub webhook: Option<String>,
    pub webhook_secret: String,
    pub token_hash: String,
    pub created_by: Option<String>,
}

#[derive(Default, Debug)]
pub struct AgentPatch {
    pub name: Option<String>,
    pub about: Option<String>,
    pub scope: Option<Scope>,
    pub webhook: Option<Option<String>>,
}

pub struct NewNotification {
    pub workspace_id: String,
    /// Agent 编号或用户编号。
    pub owner_id: String,
    pub kind: String,
    pub data: serde_json::Value,
    /// 要投 webhook 时第一次投的时间。
    pub next_attempt_at: Option<i64>,
}

/// 一条链接解析缓存。
#[derive(Clone, Debug, PartialEq)]
pub struct UnfurlRow {
    pub url_key: String,
    pub data: Option<serde_json::Value>,
    pub ok: bool,
    pub fetched_at: i64,
    pub expires_at: i64,
    pub error: Option<String>,
    pub failures: i64,
}

/// 存下来的一份内容的信息。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BlobInfo {
    pub hash: String,
    pub size: i64,
    pub mime: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub blurhash: Option<String>,
    pub duration_ms: Option<i64>,
}

/// 上传的文件。
#[derive(Clone, Debug, PartialEq)]
pub struct FileRow {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub created_by: Option<String>,
    pub created_at: i64,
    pub blob: BlobInfo,
}
