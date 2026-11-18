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

#[derive(Clone, Debug)]
pub struct Event {
    pub kind: EventKind,
    pub page: Page,
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
    pub fn emit(&self, kind: EventKind, page: Page) {
        let _ = self.tx.send(Event { kind, page });
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}
