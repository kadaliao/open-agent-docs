//! 事件流（SSE，契约 §8）：工作区的实时事件、Agent 的通知。
//!
//! `.await` 连上以后是一个 `Stream<Item = Result<Event<T>>>`；断了自动重连（1 秒起翻倍、最多 30 秒，连上后重置），
//! 带 `Last-Event-ID` 接着收。401、403、404 不重连：给出错误然后结束。drop 掉就断开。
//!
//! ```no_run
//! # use open_agent_docs as oad;
//! use futures_util::TryStreamExt;
//! use oad::types::WorkspaceEvent;
//!
//! # async fn run(client: oad::Client, ws: &str) -> oad::Result<()> {
//! let mut events = client.workspaces().events(ws).await?;
//! while let Some(ev) = events.try_next().await? {
//!     match ev.data {
//!         WorkspaceEvent::PageCreated(page) => println!("新页面：{}", page.title),
//!         WorkspaceEvent::Resync(_) => println!("落下了事件，重新拉一遍列表"),
//!         other => println!("{}：{other:?}", ev.name),
//!     }
//! }
//! # Ok(())
//! # }
//! ```
use crate::client::Client;
use crate::error::{Error, Result};
use crate::request::Spec;
use crate::types::{AgentNotification, WorkspaceEvent};
use bytes::Bytes;
use futures_util::stream::{BoxStream, Stream, StreamExt};
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

/// 按“上次收到的事件 id”拼请求（重连时用）
pub(crate) type SpecFn = Arc<dyn Fn(Option<&str>) -> Result<Spec> + Send + Sync>;

/// 服务器每 15–25 秒发一行保活；这么久什么都没收到就当断了
const IDLE: Duration = Duration::from_secs(90);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// 事件流里的一条。
#[derive(Clone, Debug, PartialEq)]
pub struct Event<T> {
    /// 事件名：`page.created`、`notification`……
    pub name: String,
    /// 事件 id（Agent 的通知是通知编号；工作区事件没有）
    pub id: Option<String>,
    /// 按事件名解析好的数据
    pub data: T,
}

/// 事件的数据怎么从 SSE 的 `data` 解析出来。
pub trait EventData: Sized + Send + 'static {
    /// 返回 `None` 是跳过这条（不是这个流关心的事件）
    fn parse(name: &str, data: &str) -> Option<Self>;
}

impl EventData for WorkspaceEvent {
    fn parse(name: &str, data: &str) -> Option<Self> {
        let v = serde_json::from_str(data)
            .unwrap_or_else(|_| serde_json::Value::String(data.to_string()));
        Some(WorkspaceEvent::parse(name, v))
    }
}

impl EventData for AgentNotification {
    fn parse(name: &str, data: &str) -> Option<Self> {
        if !(name == "notification" || name == "message") {
            return None;
        }
        serde_json::from_str(data).ok()
    }
}

impl EventData for serde_json::Value {
    fn parse(_: &str, data: &str) -> Option<Self> {
        Some(
            serde_json::from_str(data)
                .unwrap_or_else(|_| serde_json::Value::String(data.to_string())),
        )
    }
}

/// 解析出来的一条原始事件
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawEvent {
    pub name: String,
    pub data: String,
    pub id: Option<String>,
}

/// SSE 解析：一块一块喂进来，凑够一行处理一行。`event`、`data`（多行用 `\n` 拼）、`id`；冒号开头的是注释（心跳）。
#[derive(Default, Debug)]
pub(crate) struct Parser {
    buf: Vec<u8>,
    name: String,
    data: String,
    has_data: bool,
    id: Option<String>,
    /// 最后收到的事件 id（重连时带上）
    pub last_id: Option<String>,
    started: bool,
}

impl Parser {
    pub fn feed(&mut self, chunk: &[u8], out: &mut VecDeque<RawEvent>) {
        self.buf.extend_from_slice(chunk);
        let mut start = 0;
        let mut i = 0;
        while i < self.buf.len() {
            let b = self.buf[i];
            if b == b'\n' || b == b'\r' {
                // \r 在最后一个字节：等下一块，看看后面是不是 \n
                if b == b'\r' && i + 1 == self.buf.len() {
                    break;
                }
                let line = String::from_utf8_lossy(&self.buf[start..i]).into_owned();
                self.line(&line, out);
                i += if b == b'\r' && self.buf.get(i + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                start = i;
            } else {
                i += 1;
            }
        }
        self.buf.drain(..start);
    }

    fn line(&mut self, line: &str, out: &mut VecDeque<RawEvent>) {
        let line = if !self.started {
            self.started = true;
            line.strip_prefix('\u{feff}').unwrap_or(line)
        } else {
            line
        };
        if line.is_empty() {
            if let Some(id) = self.id.take() {
                self.last_id = Some(id.clone());
                if self.has_data {
                    out.push_back(RawEvent {
                        name: self.event_name(),
                        data: std::mem::take(&mut self.data),
                        id: Some(id),
                    });
                }
            } else if self.has_data {
                out.push_back(RawEvent {
                    name: self.event_name(),
                    data: std::mem::take(&mut self.data),
                    id: None,
                });
            }
            self.name.clear();
            self.data.clear();
            self.has_data = false;
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (k, v) = line.split_once(':').unwrap_or((line, ""));
        let v = v.strip_prefix(' ').unwrap_or(v);
        match k {
            "event" => self.name = v.to_string(),
            "data" => {
                if self.has_data {
                    self.data.push('\n');
                }
                self.data.push_str(v);
                self.has_data = true;
            }
            "id" if !v.contains('\0') => self.id = Some(v.to_string()),
            _ => {}
        }
    }

    fn event_name(&mut self) -> String {
        let n = std::mem::take(&mut self.name);
        if n.is_empty() { "message".into() } else { n }
    }

    /// 连接断了：丢掉没凑完的半条（last_id 留着）
    pub fn reset(&mut self) {
        let last = self.last_id.take();
        *self = Parser {
            last_id: last,
            ..Default::default()
        };
    }
}

/// 事件流：`Stream<Item = Result<Event<T>>>`。drop 掉就断开。
#[must_use = "Stream 要取才有事件"]
pub struct EventStream<T> {
    inner: BoxStream<'static, Result<Event<T>>>,
}

impl<T> std::fmt::Debug for EventStream<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventStream").finish_non_exhaustive()
    }
}

struct State {
    client: Client,
    make: SpecFn,
    body: Option<BoxStream<'static, reqwest::Result<Bytes>>>,
    parser: Parser,
    queue: VecDeque<RawEvent>,
    backoff: Duration,
    done: bool,
}

fn fatal(e: &Error) -> bool {
    matches!(e.status(), Some(401 | 403 | 404)) || matches!(e, Error::Config(_))
}

impl<T: EventData> EventStream<T> {
    pub(crate) fn new(client: Client, make: SpecFn, first: reqwest::Response) -> EventStream<T> {
        let backoff = client.reconnect_delay();
        let state = State {
            client,
            make,
            body: Some(first.bytes_stream().boxed()),
            parser: Parser::default(),
            queue: VecDeque::new(),
            backoff,
            done: false,
        };
        let inner = futures_util::stream::unfold(state, |mut s| async move {
            loop {
                if let Some(raw) = s.queue.pop_front() {
                    if let Some(data) = T::parse(&raw.name, &raw.data) {
                        let ev = Event {
                            name: raw.name,
                            id: raw.id,
                            data,
                        };
                        return Some((Ok(ev), s));
                    }
                    continue;
                }
                if s.done {
                    return None;
                }
                let Some(body) = s.body.as_mut() else {
                    // 断了：等一会儿再连
                    tokio::time::sleep(s.backoff).await;
                    let spec = match (s.make)(s.parser.last_id.as_deref()) {
                        Ok(spec) => spec,
                        Err(e) => {
                            s.done = true;
                            return Some((Err(e), s));
                        }
                    };
                    match s.client.execute(&spec, true).await {
                        Ok(r) => {
                            s.body = Some(r.bytes_stream().boxed());
                            s.backoff = s.client.reconnect_delay();
                        }
                        Err(e) if fatal(&e) => {
                            s.done = true;
                            return Some((Err(e), s));
                        }
                        Err(_) => s.backoff = (s.backoff * 2).min(MAX_BACKOFF),
                    }
                    continue;
                };
                match tokio::time::timeout(IDLE, body.next()).await {
                    Ok(Some(Ok(chunk))) => s.parser.feed(&chunk, &mut s.queue),
                    // 服务器关了、读出错、太久没动静：重连
                    _ => {
                        s.body = None;
                        s.parser.reset();
                    }
                }
            }
        })
        .boxed();
        EventStream { inner }
    }
}

impl<T> Stream for EventStream<T> {
    type Item = Result<Event<T>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(chunks: &[&[u8]]) -> (Vec<RawEvent>, Option<String>) {
        let mut p = Parser::default();
        let mut q = VecDeque::new();
        for c in chunks {
            p.feed(c, &mut q);
        }
        (q.into_iter().collect(), p.last_id)
    }

    fn ev(name: &str, data: &str, id: Option<&str>) -> RawEvent {
        RawEvent {
            name: name.into(),
            data: data.into(),
            id: id.map(String::from),
        }
    }

    #[test]
    fn split_across_chunks() {
        let (out, last) = feed_all(&[
            b": ok\n\nid: 7\nevent: notifica",
            b"tion\r\ndata: {\"id\":7}\r\n\r\ndata: a\ndata: b\n\n",
        ]);
        assert_eq!(
            out,
            vec![
                ev("notification", "{\"id\":7}", Some("7")),
                ev("message", "a\nb", None)
            ]
        );
        assert_eq!(last.as_deref(), Some("7"));
    }

    #[test]
    fn cr_only_and_cr_at_chunk_end() {
        // \r\n 被拆在两块里不能当成两个换行
        let (out, _) = feed_all(&[b"event: x\r", b"\ndata: 1\r", b"\n\r", b"\n"]);
        assert_eq!(out, vec![ev("x", "1", None)]);
        // 只用 \r 换行的（最后一个 \r 要等下一块才知道后面不是 \n）
        let (out, _) = feed_all(&[b"data: a\rdata: b\r\r", b": ping\n"]);
        assert_eq!(out, vec![ev("message", "a\nb", None)]);
    }

    #[test]
    fn fields_without_space_and_empty_data() {
        let (out, _) = feed_all(&[b"event:page.created\ndata:{}\n\ndata\n\n"]);
        assert_eq!(
            out,
            vec![ev("page.created", "{}", None), ev("message", "", None)]
        );
    }

    #[test]
    fn id_without_data_updates_last_id() {
        let (out, last) = feed_all(&[b"id: 9\n\n: keep-alive\n\n"]);
        assert!(out.is_empty());
        assert_eq!(last.as_deref(), Some("9"));
    }

    #[test]
    fn workspace_event_parsed_by_name() {
        let page = r#"{"id":"p1","workspace_id":"w","position":"a","title":"周报","created_at":1,"updated_at":1}"#;
        match <WorkspaceEvent as EventData>::parse("page.created", page) {
            Some(WorkspaceEvent::PageCreated(p)) => assert_eq!(p.title, "周报"),
            other => panic!("{other:?}"),
        }
        match <WorkspaceEvent as EventData>::parse("something.new", r#"{"a":1}"#) {
            Some(WorkspaceEvent::Other(v)) => assert_eq!(v["a"], 1),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            <WorkspaceEvent as EventData>::parse("resync", "{}"),
            Some(WorkspaceEvent::Resync(_))
        ));
    }
}
