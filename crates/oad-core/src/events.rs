//! 页面树的实时事件（给 SSE）。进程内广播，慢的订阅者会丢掉旧事件（lagged），重新拉列表即可。
use crate::model::Page;
use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum EventKind {
    #[serde(rename = "page.created")]
    Created,
    #[serde(rename = "page.updated")]
    Updated,
    #[serde(rename = "page.moved")]
    Moved,
    #[serde(rename = "page.deleted")]
    Deleted,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Created => "page.created",
            EventKind::Updated => "page.updated",
            EventKind::Moved => "page.moved",
            EventKind::Deleted => "page.deleted",
        }
    }
}

/// 工作区里的一条实时事件。
#[derive(Clone, Debug)]
pub struct Event {
    pub workspace_id: String,
    /// 事件名：page.created、comment.created、notification ……
    pub name: &'static str,
    pub data: serde_json::Value,
    /// 只发给这个用户（人的通知）；None = 工作区里的人都能收。
    pub user_id: Option<String>,
}

#[derive(Clone)]
pub struct Events {
    tx: broadcast::Sender<Event>,
}

impl Default for Events {
    fn default() -> Self {
        Events {
            tx: broadcast::channel(1024).0,
        }
    }
}

impl Events {
    pub fn emit(&self, ev: Event) {
        let _ = self.tx.send(ev);
    }
    pub fn page(&self, kind: EventKind, page: Page) {
        self.emit(Event {
            workspace_id: page.workspace_id.clone(),
            name: kind.as_str(),
            data: serde_json::to_value(&page).unwrap_or_default(),
            user_id: None,
        });
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}
