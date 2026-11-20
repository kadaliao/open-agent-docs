//! 收 Agent 的 webhook：验签，处理，回 2xx（不回 2xx 服务器会退避重试）。
//!
//!     OAD_WEBHOOK_SECRET=whsec_… cargo run --example webhook
//!
//! 建 Agent 时 webhook 填 `http://<这台机器>:3001/oad`（本机开发时服务器要设 `OAD_WEBHOOK_ALLOW_PRIVATE=1`）。
use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use open_agent_docs as oad;
use std::sync::Arc;

async fn hook(State(secret): State<Arc<String>>, headers: HeaderMap, body: Bytes) -> StatusCode {
    // 用收到的原始字节验签，别先解析成 JSON 再序列化
    match oad::verify_webhook(&secret, &headers, &body) {
        Ok(note) => {
            println!(
                "#{} {} 在《{}》里叫了 {}：{}",
                note.id, note.comment.body, note.page.title, note.agent.name, note.prompt
            );
            // 处理完了。webhook 送到（回了 2xx）就算这条处理过了，不用再 ack
            StatusCode::OK
        }
        Err(oad::Error::Webhook(kind)) => {
            eprintln!("验签失败：{kind}");
            StatusCode::UNAUTHORIZED
        }
        Err(e) => {
            eprintln!("看不懂：{e}");
            StatusCode::BAD_REQUEST
        }
    }
}

#[tokio::main]
async fn main() {
    let secret = std::env::var("OAD_WEBHOOK_SECRET")
        .expect("设 OAD_WEBHOOK_SECRET（建 Agent 时给的 whsec_…）");
    let app = Router::new()
        .route("/oad", post(hook))
        .with_state(Arc::new(secret));
    let l = tokio::net::TcpListener::bind("0.0.0.0:3001").await.unwrap();
    println!("在 http://{}/oad 收 webhook", l.local_addr().unwrap());
    axum::serve(l, app).await.unwrap();
}
