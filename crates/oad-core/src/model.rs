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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
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
