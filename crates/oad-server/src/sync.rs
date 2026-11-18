//! `GET /api/v1/sync/{page_id}`：标准 y-websocket 协议。消息原样转给页面的同步中心，同步中心要发的原样发出去。
//! 没登录、没权限、页面不存在：升级后发 auth 的 permission-denied 再关连接。
use crate::AppState;
use crate::api::authenticate;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::request::Parts;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use oad_core::Role;
use yrs::sync::Message;
use yrs::updates::encoder::Encode;

pub async fn handler(
    State(s): State<AppState>,
    Path(page_id): Path<String>,
    ws: WebSocketUpgrade,
    parts: Parts,
) -> Response {
    let access = match authenticate(&s, &parts).await {
        Ok(Some(p)) => s
            .core
            .page_for(&page_id, &p, false)
            .await
            .map(|(_, role)| role)
            .map_err(|e| e.code().to_string()),
        Ok(None) => Err("unauthorized".to_string()),
        Err(e) => Err(e.code.to_string()),
    };
    ws.on_upgrade(move |socket| serve(socket, s, page_id, access))
}

async fn serve(mut socket: WebSocket, s: AppState, page_id: String, access: Result<Role, String>) {
    let role = match access {
        Ok(r) => r,
        Err(reason) => {
            let msg = Message::Auth(Some(reason)).encode_v1();
            let _ = socket.send(WsMessage::Binary(msg.into())).await;
            let _ = socket.send(WsMessage::Close(None)).await;
            return;
        }
    };
    let conn = match s.core.hub.join(&page_id, !role.can_write()).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(page = %page_id, "加入同步失败：{e}");
            let _ = socket.send(WsMessage::Close(None)).await;
            return;
        }
    };
    let (handle, mut rx) = (conn.handle, conn.rx);
    let (mut sink, mut stream) = socket.split();
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Some(b) => {
                    if sink.send(WsMessage::Binary(b)).await.is_err() {
                        break;
                    }
                }
                // 同步中心把它踢了（太慢）或已关闭
                None => {
                    let _ = sink.send(WsMessage::Close(None)).await;
                    break;
                }
            },
            inc = stream.next() => match inc {
                Some(Ok(WsMessage::Binary(b))) => {
                    if !handle.send(b.to_vec()) {
                        break;
                    }
                }
                Some(Ok(WsMessage::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
}
