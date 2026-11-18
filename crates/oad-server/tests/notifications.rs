//! Agent 通知的三种取法：webhook（签名、重试、放弃）、SSE（Last-Event-ID 续传）、长轮询（新通知来了马上返回）；ack。
mod common;
use axum::http::{HeaderMap, StatusCode};
use common::*;
use futures_util::StreamExt;
use hmac::{Hmac, KeyInit, Mac};
use oad_core::{AuthConfig, NotifyConfig};
use oad_server::ServerConfig;
use serde_json::{Value, json};
use sha2::Sha256;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Hook {
    got: Vec<(HeaderMap, String, Instant)>,
    /// 依次回这些状态码，用完了回 200。
    statuses: Vec<u16>,
}

/// 本机的 webhook 接收端。
async fn receiver() -> (String, Arc<Mutex<Hook>>) {
    let state: Arc<Mutex<Hook>> = Default::default();
    let st = state.clone();
    let app = axum::Router::new().route(
        "/hook",
        axum::routing::post(move |headers: HeaderMap, body: String| {
            let st = st.clone();
            async move {
                let mut h = st.lock().unwrap();
                h.got.push((headers, body, Instant::now()));
                let code = if h.statuses.is_empty() {
                    200
                } else {
                    h.statuses.remove(0)
                };
                StatusCode::from_u16(code).unwrap()
            }
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (url, state)
}

fn notify_cfg(retry: Vec<Duration>) -> NotifyConfig {
    NotifyConfig {
        allow_private: true,
        retry,
        public_url: Some("https://docs.example.com".into()),
        ..NotifyConfig::default()
    }
}

async fn server(dir: &std::path::Path, retry: Vec<Duration>) -> Server {
    Server::start_full(
        dir,
        AuthConfig::None,
        fast_hub(),
        ServerConfig::default(),
        notify_cfg(retry),
    )
    .await
}

/// 本机用户在新页面上评论并 @ name，返回线程编号。
async fn poke(s: &Server, c: &reqwest::Client, name: &str, page: &str) -> String {
    let (st, t) = json(
        c.post(s.api(&format!("/pages/{page}/comments")))
            .json(&json!({ "body": format!("@{name} 请看一下") })),
    )
    .await;
    assert_eq!(st, 201, "{t}");
    t["id"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn webhook_signature_and_retry() {
    let dir = tempfile::tempdir().unwrap();
    let s = server(
        dir.path(),
        vec![Duration::from_millis(200), Duration::from_millis(200)],
    )
    .await;
    let c = http();
    let (url, hook) = receiver().await;
    hook.lock().unwrap().statuses = vec![500];
    let agents = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (agent, token, secret) = make_agent(c.post(&agents), "钩子", "write", Some(&url)).await;
    let page = page_with(&s, &c, "钩子页", "一段\n").await;
    let thread = poke(&s, &c, "钩子", &page).await;

    // 第一次 500，200ms 后重试成功
    eventually("投了两次", async || hook.lock().unwrap().got.len() >= 2).await;
    let (headers, body, at2, at1) = {
        let h = hook.lock().unwrap();
        let (headers, body, at2) = h.got[1].clone();
        (headers, body, at2, h.got[0].2)
    };
    assert!(at2.duration_since(at1) >= Duration::from_millis(150));
    let ts = headers["x-oad-timestamp"].to_str().unwrap();
    let sig = headers["x-oad-signature"].to_str().unwrap();
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{ts}.{body}").as_bytes());
    let want: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(sig, format!("sha256={want}"));
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        headers["x-oad-delivery"].to_str().unwrap(),
        v["id"].to_string()
    );
    assert_eq!(v["agent"]["id"], agent.as_str());
    assert_eq!(v["thread_id"], thread.as_str());
    assert_eq!(
        v["page"]["url"],
        format!("https://docs.example.com/p/{page}")
    );
    assert!(v["prompt"].as_str().unwrap().contains("钩子页"));
    // 送到就算处理完了
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (_, left) = json(c.get(s.api("/agent/notifications")).bearer_auth(&token)).await;
    assert_eq!(left["notifications"].as_array().unwrap().len(), 0);
    let (_, list) = json(c.get(&agents)).await;
    assert!(list["agents"][0]["webhook_error"].is_null());
    assert_eq!(list["agents"][0]["pending"], 0);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(hook.lock().unwrap().got.len(), 2, "送到后不再投");
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn webhook_gives_up_and_records_error() {
    let dir = tempfile::tempdir().unwrap();
    let s = server(dir.path(), vec![Duration::from_millis(100)]).await;
    let c = http();
    let (url, hook) = receiver().await;
    hook.lock().unwrap().statuses = vec![503; 10];
    let agents = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (_, token, _) = make_agent(c.post(&agents), "坏钩子", "write", Some(&url)).await;
    let page = page_with(&s, &c, "p", "一段\n").await;
    poke(&s, &c, "坏钩子", &page).await;
    eventually("记下错误", async || {
        let (_, list) = json(c.get(&agents)).await;
        list["agents"][0]["webhook_error"].as_str() == Some("HTTP 503")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        hook.lock().unwrap().got.len(),
        2,
        "第一次 + 重试一次，然后放弃"
    );
    // 没送到：还能用长轮询拿
    let (_, left) = json(c.get(s.api("/agent/notifications")).bearer_auth(&token)).await;
    assert_eq!(left["notifications"].as_array().unwrap().len(), 1);
    s.stop().await;
}

/// 读 SSE 直到拿到 n 个 notification 事件：(id, 数据)。
async fn sse_events(res: reqwest::Response, n: usize) -> Vec<(String, Value)> {
    let mut stream = res.bytes_stream();
    let mut buf = String::new();
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while out.len() < n {
        let chunk = tokio::time::timeout_at(deadline, stream.next())
            .await
            .expect("等 SSE 超时")
            .unwrap()
            .unwrap();
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(i) = buf.find("\n\n") {
            let block: String = buf.drain(..i + 2).collect();
            let (mut id, mut data) = (String::new(), String::new());
            for line in block.lines() {
                if let Some(v) = line.strip_prefix("id:") {
                    id = v.trim().into();
                } else if let Some(v) = line.strip_prefix("data:") {
                    data.push_str(v.trim());
                }
            }
            if !data.is_empty() {
                out.push((id, serde_json::from_str(&data).unwrap()));
            }
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sse_resume_long_poll_and_ack() {
    let dir = tempfile::tempdir().unwrap();
    let s = server(dir.path(), vec![]).await;
    let c = http();
    let agents = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (_, token, _) = make_agent(c.post(&agents), "收信", "write", None).await;
    let page = page_with(&s, &c, "p", "一段\n").await;
    // 人不能连 Agent 的 SSE
    let (st, _) = json(c.get(s.api("/agent/notifications/stream"))).await;
    assert_eq!(st, 403);

    // SSE：先补发已有的，再实时推
    poke(&s, &c, "收信", &page).await;
    let res = c
        .get(s.api("/agent/notifications/stream"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    let reading = tokio::spawn(sse_events(res, 2));
    tokio::time::sleep(Duration::from_millis(100)).await;
    poke(&s, &c, "收信", &page).await;
    let evs = reading.await.unwrap();
    assert_eq!(evs[0].0, evs[0].1["id"].to_string());
    let first = evs[0].0.clone();
    // 断开后又来一条；用 Last-Event-ID 续上：从第二条开始
    poke(&s, &c, "收信", &page).await;
    let res = c
        .get(s.api("/agent/notifications/stream"))
        .bearer_auth(&token)
        .header("last-event-id", &first)
        .send()
        .await
        .unwrap();
    let evs2 = sse_events(res, 2).await;
    assert_eq!(evs2[0].0, evs[1].0);
    assert!(evs2[1].0.parse::<i64>().unwrap() > evs[1].0.parse::<i64>().unwrap());
    // ?after= 也行
    let res = c
        .get(s.api(&format!("/agent/notifications/stream?after={}", evs[1].0)))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(sse_events(res, 1).await[0].0, evs2[1].0);

    // ack：upto 之前的都处理完；不给 after 时只返回没处理的
    let (_, r) = json(
        c.post(s.api("/agent/notifications/ack"))
            .bearer_auth(&token)
            .json(&json!({ "upto": evs[1].0.parse::<i64>().unwrap() })),
    )
    .await;
    assert_eq!(r["acked"], 2);
    let (_, left) = json(c.get(s.api("/agent/notifications")).bearer_auth(&token)).await;
    assert_eq!(left["notifications"].as_array().unwrap().len(), 1);
    let (_, r) = json(
        c.post(s.api("/agent/notifications/ack"))
            .bearer_auth(&token)
            .json(&json!({ "ids": [evs2[1].0.parse::<i64>().unwrap()] })),
    )
    .await;
    assert_eq!(r["acked"], 1);
    let (_, left) = json(c.get(s.api("/agent/notifications")).bearer_auth(&token)).await;
    assert_eq!(left["notifications"].as_array().unwrap().len(), 0);

    // 长轮询：没有就等着，新通知一来马上返回
    let last = evs2[1].0.clone();
    let started = Instant::now();
    let poll = {
        let c = c.clone();
        let url = s.api(&format!("/agent/notifications?after={last}&wait=10"));
        let token = token.clone();
        tokio::spawn(async move { json(c.get(url).bearer_auth(token)).await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!poll.is_finished());
    poke(&s, &c, "收信", &page).await;
    let (st, v) = poll.await.unwrap();
    assert_eq!(st, 200);
    assert_eq!(v["notifications"].as_array().unwrap().len(), 1);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    // 等到超时：空的
    let t = Instant::now();
    let after = v["notifications"][0]["id"].as_i64().unwrap();
    let (_, v) = json(
        c.get(s.api(&format!("/agent/notifications?after={after}&wait=1")))
            .bearer_auth(&token),
    )
    .await;
    assert_eq!(v["notifications"].as_array().unwrap().len(), 0);
    assert!(t.elapsed() >= Duration::from_millis(900));
    s.stop().await;
}
