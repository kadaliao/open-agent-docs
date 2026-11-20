//! 事件流（对着本机的假服务器）：按事件名解析、断了重连带 Last-Event-ID、401 不重连。
mod common;

use axum::Router;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use common::{Counter, serve};
use futures_util::{StreamExt, TryStreamExt};
use oad::types::WorkspaceEvent;
use open_agent_docs as oad;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn sse(chunks: Vec<String>, keep_open: bool) -> Response {
    let s = futures_util::stream::iter(
        chunks
            .into_iter()
            .map(|c| Ok::<_, std::io::Error>(c.into_bytes())),
    );
    let body = if keep_open {
        Body::from_stream(s.chain(futures_util::stream::pending()))
    } else {
        Body::from_stream(s)
    };
    ([("content-type", "text/event-stream")], body).into_response()
}

fn note(id: i64, body: &str) -> String {
    let n = json!({
        "id": id, "created_at": 1, "type": "comment",
        "agent": { "id": "a1", "name": "bot" },
        "page": { "id": "p1", "title": "周报", "url": "http://x/p/p1" },
        "thread_id": "t1",
        "comment": { "id": "c1", "author": { "name": "小王" }, "body": body },
        "prompt": "看看"
    });
    format!("id: {id}\nevent: notification\ndata: {n}\n\n")
}

/// 一次连接：（Last-Event-ID，查询参数）
type Conn = (Option<String>, HashMap<String, String>);

#[derive(Clone, Default)]
struct Seen {
    n: Counter,
    conns: Arc<Mutex<Vec<Conn>>>,
}

/// 第一次连：发一条就断；第二次连要带 Last-Event-ID 和 after，再发一条
#[tokio::test]
async fn agent_stream_reconnects_with_last_event_id() {
    let seen = Seen::default();
    let app = Router::new()
        .route(
            "/api/v1/agent/notifications/stream",
            get(
                |State(s): State<Seen>,
                 headers: HeaderMap,
                 Query(q): Query<HashMap<String, String>>| async move {
                    let last = headers
                        .get("last-event-id")
                        .map(|v| v.to_str().unwrap().to_string());
                    s.conns.lock().unwrap().push((last, q));
                    match s.n.bump() {
                        0 => sse(vec![": ok\n\n".into(), note(1, "@bot 一")], false),
                        _ => sse(vec![": ok\n\n".into(), note(2, "@bot 二")], true),
                    }
                },
            ),
        )
        .with_state(seen.clone());
    let base = serve(app).await;
    let c = oad::Client::builder()
        .base_url(&base)
        .token("oad_bot")
        .reconnect_delay(Duration::from_millis(50))
        .build()
        .unwrap();
    let stream = c.agent().notifications().stream().after(0).await.unwrap();
    let got: Vec<_> = tokio::time::timeout(
        Duration::from_secs(5),
        stream.take(2).try_collect::<Vec<_>>(),
    )
    .await
    .expect("5 秒内收到两条")
    .unwrap();
    assert_eq!(got[0].name, "notification");
    assert_eq!(got[0].id.as_deref(), Some("1"));
    assert_eq!(got[0].data.comment.body, "@bot 一");
    assert_eq!(got[1].data.id, 2);
    let conns = seen.conns.lock().unwrap().clone();
    assert_eq!(conns[0].0, None);
    assert_eq!(conns[0].1.get("after").map(String::as_str), Some("0"));
    assert_eq!(conns[1].0.as_deref(), Some("1"));
    assert_eq!(conns[1].1.get("after").map(String::as_str), Some("1"));
}

/// 工作区事件按名字解析成枚举；不认识的给原始 JSON
#[tokio::test]
async fn workspace_events_typed() {
    let page = json!({ "id": "p9", "workspace_id": "w1", "position": "a", "title": "新页面", "created_at": 1, "updated_at": 1 });
    let app = Router::new().route(
        "/api/v1/workspaces/{ws}/events",
        get(move || {
            let page = page.clone();
            async move {
                sse(
                    vec![
                        ": ok\n\n".into(),
                        format!("event: page.created\ndata: {page}\n\n"),
                        "event: db.changed\ndata: {\"db_id\":\"d1\",\"seq\":7}\n\n".into(),
                        "event: future.thing\ndata: {\"x\":1}\n\n".into(),
                        "event: resync\ndata: {}\n\n".into(),
                    ],
                    true,
                )
            }
        }),
    );
    let base = serve(app).await;
    let c = common::client(&base);
    let events: Vec<_> = c
        .workspaces()
        .events("w1")
        .await
        .unwrap()
        .take(4)
        .try_collect()
        .await
        .unwrap();
    match &events[0].data {
        WorkspaceEvent::PageCreated(p) => {
            assert_eq!((p.id.as_str(), p.title.as_str()), ("p9", "新页面"))
        }
        other => panic!("{other:?}"),
    }
    match &events[1].data {
        WorkspaceEvent::DbChanged(d) => {
            assert_eq!((d.db_id.as_deref(), d.seq), (Some("d1"), Some(7)))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(events[2].name, "future.thing");
    assert!(matches!(&events[2].data, WorkspaceEvent::Other(v) if v["x"] == 1));
    assert!(matches!(events[3].data, WorkspaceEvent::Resync(_)));
}

/// 重连时 401：给出错误然后结束；一开始就 403：`.await` 直接报错
#[tokio::test]
async fn fatal_errors_stop() {
    let n = Counter::default();
    let app = Router::new()
        .route(
            "/api/v1/workspaces/{ws}/events",
            get(|State(n): State<Counter>| async move {
                if n.bump() == 0 {
                    sse(vec!["event: resync\ndata: {}\n\n".into()], false)
                } else {
                    (StatusCode::UNAUTHORIZED, axum::Json(json!({ "error": { "code": "unauthorized", "message": "token 过期了" } }))).into_response()
                }
            }),
        )
        .route(
            "/api/v1/agent/notifications/stream",
            get(|| async { (StatusCode::FORBIDDEN, axum::Json(json!({ "error": { "code": "forbidden", "message": "只有 Agent 能调" } }))) }),
        )
        .with_state(n.clone());
    let base = serve(app).await;
    let c = oad::Client::builder()
        .base_url(&base)
        .reconnect_delay(Duration::from_millis(20))
        .build()
        .unwrap();
    let mut s = c.workspaces().events("w1").await.unwrap();
    assert!(matches!(s.next().await, Some(Ok(ev)) if ev.name == "resync"));
    let e = s.next().await.unwrap().unwrap_err();
    assert!(e.is_unauthorized(), "{e:?}");
    assert!(s.next().await.is_none());
    assert_eq!(n.get(), 2);

    let e = c.agent().notifications().stream().await.unwrap_err();
    assert!(e.is_forbidden());
}

/// 服务器暂时出错（503）时退避重连，不报错
#[tokio::test]
async fn transient_errors_retry() {
    let n = Counter::default();
    let app = Router::new()
        .route(
            "/api/v1/workspaces/{ws}/events",
            get(|State(n): State<Counter>| async move {
                match n.bump() {
                    0 => sse(vec!["event: resync\ndata: {}\n\n".into()], false),
                    1 | 2 => StatusCode::SERVICE_UNAVAILABLE.into_response(),
                    _ => sse(
                        vec!["event: page.purged\ndata: {\"id\":\"p1\"}\n\n".into()],
                        true,
                    ),
                }
            }),
        )
        .with_state(n.clone());
    let base = serve(app).await;
    let c = oad::Client::builder()
        .base_url(&base)
        .max_retries(0)
        .reconnect_delay(Duration::from_millis(20))
        .build()
        .unwrap();
    let got: Vec<_> = tokio::time::timeout(
        Duration::from_secs(5),
        c.workspaces()
            .events("w1")
            .await
            .unwrap()
            .take(2)
            .try_collect::<Vec<_>>(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(&got[1].data, WorkspaceEvent::PagePurged(p) if p.id.as_deref() == Some("p1")));
    assert_eq!(n.get(), 4);
}
