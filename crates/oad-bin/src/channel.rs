//! `open-agent-docs channel`：给 Claude Code 用的 channel（stdio MCP 服务）。
//!
//! Claude Code 只认它自己拉起的 stdio 进程当 channel，远程的 `/mcp` 当不了，所以单独一个小进程：
//! 订阅服务器的 `/api/v1/agent/notifications/stream`，每来一条通知就发一个
//! `notifications/claude/channel`，正在跑的会话就被叫醒了。读写页面、回评论还是走原来的 `/mcp`。
//!
//! 推过去不算处理完：Claude Code 不回执，没开 channel 时会静静丢掉；Agent 处理完自己确认
//! （`my_notifications(after: …)`），没确认的下次连上会再推。
use futures_util::StreamExt;
use rmcp::model::{
    CustomNotification, ExperimentalCapabilities, Implementation, JsonObject, ProtocolVersion,
    ServerCapabilities, ServerConfig, ServerNotification,
};
use rmcp::service::NotificationContext;
use rmcp::{RoleServer, ServerHandler, ServiceExt};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::time::Duration;
use tracing::{info, warn};

const INSTRUCTIONS: &str = "\
Open Agent Docs 的通知（有人在评论里叫你）会以 <channel source=\"…\" notification_id=\"…\" page_id=\"…\" thread_id=\"…\"> 的形式送进来，\
正文写了是谁、在哪一页哪一段、说了什么、该怎么回。用 Open Agent Docs 的 MCP 工具（read_page、comment、edit_page）处理，\
处理完调用 my_notifications(after: <notification_id>) 把它标成已处理；不确认的话下次连上会再送一遍。这个 channel 只送不收，不用回复它。\n\
Open Agent Docs notifications (someone mentioned you in a comment) arrive as <channel source=\"…\" notification_id=\"…\" page_id=\"…\" thread_id=\"…\">; \
the body says who, on which page and passage, what they said and how to reply. Handle them with the Open Agent Docs MCP tools (read_page, comment, edit_page), \
then call my_notifications(after: <notification_id>) to mark it handled; unconfirmed ones are delivered again next time. This channel is one-way: don't reply to it.";

#[derive(Clone)]
pub struct Opts {
    /// 服务器地址（`https://docs.example.com`；带 `/mcp` 也行）
    pub url: String,
    pub token: String,
}

/// 用户给的地址 → API 前缀。
fn api_base(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    let u = u.strip_suffix("/mcp").unwrap_or(u);
    let u = u.strip_suffix("/api/v1").unwrap_or(u);
    format!("{u}/api/v1")
}

struct Channel {
    opts: Opts,
    http: reqwest::Client,
}

impl ServerHandler for Channel {
    fn get_info(&self) -> ServerConfig {
        let mut exp = ExperimentalCapabilities::new();
        exp.insert("claude/channel".into(), JsonObject::new());
        let mut caps = ServerCapabilities::default();
        caps.experimental = Some(exp);
        ServerConfig::new(caps)
            .with_server_info(Implementation::new(
                "open-agent-docs-channel",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS)
    }

    /// 协商到 2026-07-28 的 stdio 服务 Claude Code 不当 channel 注册，停在上一版。
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(ProtocolVersion::known_up_to(&ProtocolVersion::V_2025_11_25))
    }

    async fn on_initialized(&self, ctx: NotificationContext<RoleServer>) {
        let (http, base, token) = (
            self.http.clone(),
            api_base(&self.opts.url),
            self.opts.token.clone(),
        );
        tokio::spawn(pump(http, base, token, ctx.peer));
    }
}

/// 一条通知 → channel 消息。meta 的键只能是字母数字下划线，值都是字符串。
fn to_channel(n: &Value) -> Value {
    let s = |v: &Value| v.as_str().map(str::to_string);
    let mut meta = serde_json::Map::new();
    let mut put = |k: &str, v: Option<String>| {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            meta.insert(k.into(), v.into());
        }
    };
    put("notification_id", n["id"].as_i64().map(|i| i.to_string()));
    put("page_id", s(&n["page"]["id"]));
    put("thread_id", s(&n["thread_id"]));
    put("comment_id", s(&n["comment"]["id"]));
    put("block_id", s(&n["block_id"]));
    put("from", s(&n["comment"]["author"]["name"]));
    put("url", s(&n["page"]["url"]));
    let content = n["prompt"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| n.to_string());
    json!({ "content": content, "meta": meta })
}

/// SSE 的一行一行 → 事件（event 名，data）。只认 `event:` 和 `data:`，空行结束一个事件。
#[derive(Default)]
struct Sse {
    buf: Vec<u8>,
    event: String,
    data: String,
}

impl Sse {
    fn feed(&mut self, chunk: &[u8], out: &mut Vec<(String, String)>) {
        self.buf.extend_from_slice(chunk);
        while let Some(i) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=i).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    out.push((
                        std::mem::take(&mut self.event),
                        std::mem::take(&mut self.data),
                    ));
                }
                self.event.clear();
                continue;
            }
            let (k, v) = line.split_once(':').unwrap_or((line, ""));
            let v = v.strip_prefix(' ').unwrap_or(v);
            match k {
                "event" => self.event = v.to_string(),
                "data" => {
                    if !self.data.is_empty() {
                        self.data.push('\n');
                    }
                    self.data.push_str(v);
                }
                _ => {}
            }
        }
    }
}

enum End {
    /// token 不对、Agent 被撤销了：不再连
    Fatal(String),
    /// 断了，过一会儿再连
    Retry(String),
    /// 会话关了
    Closed,
}

/// 连着通知流往会话里推；断了就退避重连，从推过的最后一条之后续上。
async fn pump(
    http: reqwest::Client,
    base: String,
    token: String,
    peer: rmcp::service::Peer<RoleServer>,
) {
    let mut after: Option<i64> = None;
    let mut backoff = Duration::from_secs(1);
    loop {
        match stream_once(&http, &base, &token, &peer, &mut after, &mut backoff).await {
            End::Closed => return,
            End::Fatal(e) => {
                warn!("不再订阅通知：{e}");
                return;
            }
            End::Retry(e) => {
                warn!("通知流断了（{e}），{}s 后重连", backoff.as_secs());
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
        if peer.is_transport_closed() {
            return;
        }
    }
}

async fn stream_once(
    http: &reqwest::Client,
    base: &str,
    token: &str,
    peer: &rmcp::service::Peer<RoleServer>,
    after: &mut Option<i64>,
    backoff: &mut Duration,
) -> End {
    let mut req = http
        .get(format!("{base}/agent/notifications/stream"))
        .bearer_auth(token)
        .header("accept", "text/event-stream");
    if let Some(a) = *after {
        req = req.query(&[("after", a)]);
    }
    let res = match req.send().await {
        Ok(r) => r,
        Err(e) => return End::Retry(e.to_string()),
    };
    let st = res.status();
    if st == reqwest::StatusCode::UNAUTHORIZED || st == reqwest::StatusCode::FORBIDDEN {
        let body = res.text().await.unwrap_or_default();
        return End::Fatal(format!("{st} {body}"));
    }
    if !st.is_success() {
        return End::Retry(st.to_string());
    }
    info!("已订阅通知：{base}/agent/notifications/stream");
    *backoff = Duration::from_secs(1);
    let mut body = res.bytes_stream();
    let mut sse = Sse::default();
    let mut events = Vec::new();
    // 服务器每 25 秒发一行保活；90 秒什么都没收到就当断了
    loop {
        let chunk = match tokio::time::timeout(Duration::from_secs(90), body.next()).await {
            Err(_) => return End::Retry("90 秒没收到数据".into()),
            Ok(None) => return End::Retry("服务器关了连接".into()),
            Ok(Some(Err(e))) => return End::Retry(e.to_string()),
            Ok(Some(Ok(c))) => c,
        };
        sse.feed(&chunk, &mut events);
        for (event, data) in events.drain(..) {
            if !(event.is_empty() || event == "notification") {
                continue;
            }
            let Ok(n) = serde_json::from_str::<Value>(&data) else {
                warn!("看不懂的通知：{data}");
                continue;
            };
            let id = n["id"].as_i64();
            let note =
                CustomNotification::new("notifications/claude/channel", Some(to_channel(&n)));
            if peer
                .send_notification(ServerNotification::CustomNotification(note))
                .await
                .is_err()
            {
                return End::Closed;
            }
            info!("已推送通知 {}", id.unwrap_or_default());
            if id.is_some() {
                *after = id;
            }
        }
    }
}

/// 跑 channel：先试一下 token（不对就直接退出，Claude Code 里能看到连不上），再在 stdio 上服务。
pub async fn run(opts: Opts) -> Result<(), String> {
    oad_core::notify::install_tls();
    let http = reqwest::Client::builder()
        .user_agent(concat!(
            "open-agent-docs-channel/",
            env!("CARGO_PKG_VERSION")
        ))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let base = api_base(&opts.url);
    match http
        .get(format!("{base}/agent/notifications"))
        .bearer_auth(&opts.token)
        .query(&[("limit", 1)])
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {}
        Ok(r) if matches!(r.status().as_u16(), 401 | 403) => {
            let st = r.status();
            let body = r.text().await.unwrap_or_default();
            return Err(format!(
                "{base} 不认这个 token（{st}）：{body}。要用 Agent 的 token（oad_...）"
            ));
        }
        // 服务器暂时连不上：照样起来，后面会重连
        Ok(r) => warn!("{base} 返回 {}，稍后重连", r.status()),
        Err(e) => warn!("连不上 {base}：{e}，稍后重连"),
    }
    let service = Channel { opts, http }
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| format!("MCP 握手失败：{e}"))?;
    let _ = service.waiting().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base() {
        for u in [
            "https://d.example.com",
            "https://d.example.com/",
            "https://d.example.com/mcp",
            "https://d.example.com/mcp/",
            "https://d.example.com/api/v1",
        ] {
            assert_eq!(api_base(u), "https://d.example.com/api/v1", "{u}");
        }
    }

    #[test]
    fn sse_split_across_chunks() {
        let mut s = Sse::default();
        let mut out = Vec::new();
        s.feed(b": keep-alive\n\nid: 7\nevent: notifica", &mut out);
        assert!(out.is_empty());
        s.feed(
            b"tion\r\ndata: {\"id\":7}\r\n\r\ndata: a\ndata: b\n\n",
            &mut out,
        );
        assert_eq!(
            out,
            vec![
                ("notification".to_string(), "{\"id\":7}".to_string()),
                (String::new(), "a\nb".to_string())
            ]
        );
    }

    #[test]
    fn channel_meta() {
        let n = json!({
            "id": 42, "type": "comment", "thread_id": "t1", "block_id": null,
            "agent": {"id": "a", "name": "Writer"},
            "page": {"id": "p1", "title": "T", "url": "https://d/p/p1"},
            "comment": {"id": "c1", "author": {"name": "小王"}, "body": "hi"},
            "prompt": "你被叫到了"
        });
        let c = to_channel(&n);
        assert_eq!(c["content"], "你被叫到了");
        assert_eq!(c["meta"]["notification_id"], "42");
        assert_eq!(c["meta"]["thread_id"], "t1");
        assert_eq!(c["meta"]["from"], "小王");
        assert!(c["meta"].get("block_id").is_none());
        for (k, v) in c["meta"].as_object().unwrap() {
            assert!(k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
            assert!(v.is_string());
        }
    }
}
