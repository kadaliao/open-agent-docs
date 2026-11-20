//! 测试用的假服务器：本机随便一个端口上跑一个 axum 路由。
#![allow(dead_code)]
use axum::Router;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 起一个假服务器，返回地址（`http://127.0.0.1:<port>`）
pub async fn serve(app: Router) -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(l, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// 数请求
#[derive(Clone, Default)]
pub struct Counter(Arc<AtomicUsize>);

impl Counter {
    /// 加一，返回加之前的值
    pub fn bump(&self) -> usize {
        self.0.fetch_add(1, Ordering::SeqCst)
    }

    pub fn get(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

pub fn me_json(name: &str) -> serde_json::Value {
    serde_json::json!({
        "user": { "id": "u1", "name": name, "kind": "user" },
        "workspaces": [
            { "id": "w1", "name": "一号", "role": "owner" },
            { "id": "w2", "name": "二号", "role": "editor" }
        ]
    })
}

/// 客户端：连假服务器，重试的等待短一点也没关系（用默认规则）
pub fn client(base: &str) -> open_agent_docs::Client {
    open_agent_docs::Client::builder()
        .base_url(base)
        .token("oad_test")
        .build()
        .unwrap()
}
