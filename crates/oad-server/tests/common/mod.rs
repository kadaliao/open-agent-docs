//! 测试用：在随机端口起服务器；用 yrs 写的 y-websocket 客户端（和浏览器里的 WebsocketProvider 行为一样）。
#![allow(dead_code)]
use futures_util::{SinkExt, StreamExt};
use oad_core::storage::sqlite::Sqlite;
use oad_core::{AuthConfig, Core, HubConfig, Node, PageDoc};
use oad_server::{AppState, ServerConfig};
use serde_json::Value;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use yrs::encoding::read::Cursor;
use yrs::sync::{Message, MessageReader, SyncMessage};
use yrs::updates::decoder::DecoderV1;
use yrs::updates::encoder::Encode;
use yrs::{Any, Out, ReadTxn, Text, Transact, Xml, XmlFragment, XmlFragmentRef, XmlOut};

pub struct Server {
    pub addr: SocketAddr,
    pub core: Arc<Core>,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

pub fn fast_hub() -> HubConfig {
    HubConfig {
        flush_window: Duration::from_millis(5),
        ..HubConfig::default()
    }
}

impl Server {
    pub async fn start(dir: &Path, auth: AuthConfig, hub: HubConfig) -> Server {
        let storage = Arc::new(Sqlite::open(dir.join("oad.db")).unwrap());
        let core = Core::open(storage, auth, hub).await.unwrap();
        let app = oad_server::router(AppState::new(core.clone(), ServerConfig::default()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (stop, rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });
        Server {
            addr,
            core,
            stop: Some(stop),
            task,
        }
    }

    pub fn api(&self, path: &str) -> String {
        format!("http://{}/api/v1{}", self.addr, path)
    }

    pub fn ws(&self, page: &str) -> String {
        format!("ws://{}/api/v1/sync/{}", self.addr, page)
    }

    pub fn ws_id(&self) -> String {
        self.core.default_workspace.id.clone()
    }

    /// 像关机一样：先存好所有页面，再停 HTTP。
    pub async fn stop(mut self) {
        self.core.shutdown().await;
        let _ = self.stop.take().unwrap().send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), self.task).await;
    }

    pub async fn server_doc(&self, page: &str) -> Node {
        self.core.hub.read(page, |d| d.to_node()).await.unwrap()
    }
}

pub fn http() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

pub async fn json(r: reqwest::RequestBuilder) -> (u16, Value) {
    let res = r.send().await.unwrap();
    let status = res.status().as_u16();
    let text = res.text().await.unwrap();
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// 建一页并写入 Markdown，返回页面编号。
pub async fn page_with(s: &Server, c: &reqwest::Client, title: &str, md: &str) -> String {
    let (st, p) = json(
        c.post(s.api(&format!("/workspaces/{}/pages", s.ws_id())))
            .json(&serde_json::json!({ "title": title })),
    )
    .await;
    assert_eq!(st, 201, "{p}");
    let id = p["id"].as_str().unwrap().to_string();
    let r = c
        .put(s.api(&format!("/pages/{id}/content")))
        .json(&serde_json::json!({ "markdown": md }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    id
}

/// 等条件成立，最多 5 秒。
pub async fn eventually(what: &str, mut f: impl AsyncFnMut() -> bool) {
    for _ in 0..500 {
        if f().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("等不到：{what}");
}

#[derive(Default)]
pub struct ClientState {
    /// 收到并合并了几条别人的增量。
    pub applied: usize,
    pub denied: Option<String>,
    /// 收到的 awareness 里各个状态（JSON 字符串）。
    pub awareness: Vec<String>,
    pub synced: bool,
}

/// y-websocket 客户端。
pub struct YClient {
    pub doc: Arc<PageDoc>,
    frag: XmlFragmentRef,
    out: Arc<Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>>,
    pub state: Arc<Mutex<ClientState>>,
    tasks: Vec<JoinHandle<()>>,
}

const REMOTE: &str = "remote";

fn is_remote(txn: &yrs::TransactionMut) -> bool {
    txn.origin()
        .is_some_and(|o| o.as_ref() == REMOTE.as_bytes())
}

impl YClient {
    /// client 用 32 位以内的数，和浏览器的 Yjs 一样。
    pub fn new(client: u32) -> YClient {
        let doc = Arc::new(PageDoc::new(client as u64));
        let frag = doc.ydoc().get_or_insert_xml_fragment("default");
        let out: Arc<Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>> = Arc::default();
        let o = out.clone();
        doc.ydoc()
            .observe_update_v1("client", move |txn, e| {
                if is_remote(txn) {
                    return;
                }
                if let Some(tx) = &*o.lock().unwrap() {
                    let _ =
                        tx.send(Message::Sync(SyncMessage::Update(e.update.clone())).encode_v1());
                }
            })
            .unwrap();
        YClient {
            doc,
            frag,
            out,
            state: Arc::default(),
            tasks: Vec::new(),
        }
    }

    pub async fn connect(&mut self, url: &str, headers: &[(&str, &str)]) {
        let mut req = url.into_client_request().unwrap();
        for (k, v) in headers {
            let name =
                tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(k.as_bytes()).unwrap();
            req.headers_mut().insert(name, v.parse().unwrap());
        }
        let (ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
        let (mut sink, mut stream) = ws.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        self.state.lock().unwrap().synced = false;
        let sv = self.doc.ydoc().transact().state_vector();
        tx.send(Message::Sync(SyncMessage::SyncStep1(sv)).encode_v1())
            .unwrap();
        *self.out.lock().unwrap() = Some(tx.clone());
        self.tasks.push(tokio::spawn(async move {
            while let Some(b) = rx.recv().await {
                if sink.send(WsMsg::Binary(b.into())).await.is_err() {
                    break;
                }
            }
        }));
        let (doc, state) = (self.doc.clone(), self.state.clone());
        self.tasks.push(tokio::spawn(async move {
            while let Some(Ok(m)) = stream.next().await {
                let WsMsg::Binary(b) = m else {
                    if m.is_close() {
                        break;
                    }
                    continue;
                };
                let mut dec = DecoderV1::new(Cursor::new(&b));
                let msgs: Vec<_> = MessageReader::new(&mut dec).collect();
                for msg in msgs {
                    match msg.unwrap() {
                        Message::Sync(SyncMessage::SyncStep1(sv)) => {
                            let diff = doc.ydoc().transact().encode_diff_v1(&sv);
                            let _ =
                                tx.send(Message::Sync(SyncMessage::SyncStep2(diff)).encode_v1());
                        }
                        Message::Sync(SyncMessage::SyncStep2(u)) => {
                            doc.apply_update_from(&u, REMOTE.to_string()).unwrap();
                            state.lock().unwrap().synced = true;
                        }
                        Message::Sync(SyncMessage::Update(u)) => {
                            doc.apply_update_from(&u, REMOTE.to_string()).unwrap();
                            state.lock().unwrap().applied += 1;
                        }
                        Message::Awareness(u) => {
                            let mut st = state.lock().unwrap();
                            for e in u.clients.values() {
                                st.awareness.push(e.json.to_string());
                            }
                        }
                        Message::Auth(Some(reason)) => state.lock().unwrap().denied = Some(reason),
                        _ => {}
                    }
                }
            }
        }));
    }

    /// 发一条原始消息（比如 awareness）。
    pub fn send_raw(&self, msg: Vec<u8>) {
        if let Some(tx) = &*self.out.lock().unwrap() {
            let _ = tx.send(msg);
        }
    }

    pub fn disconnect(&mut self) {
        *self.out.lock().unwrap() = None;
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }

    pub fn synced(&self) -> bool {
        self.state.lock().unwrap().synced
    }

    pub fn node(&self) -> Node {
        self.doc.to_node()
    }

    /// 在编号为 block 的段落里第 offset 个字（UTF-16）处打字，和浏览器里一样是一个本地事务。
    pub fn type_at(&self, block: &str, offset: u32, text: &str) {
        let mut txn = self.doc.ydoc().transact_mut();
        let target = self.frag.children(&txn).find_map(|c| match c {
            XmlOut::Element(el)
                if matches!(el.get_attribute(&txn, "id"), Some(Out::Any(Any::String(s))) if &*s == block) =>
            {
                match el.get(&txn, 0) {
                    Some(XmlOut::Text(t)) => Some(t),
                    _ => None,
                }
            }
            _ => None,
        });
        let t = target.expect("找不到这一段");
        let len = t.len(&txn);
        t.insert(&mut txn, offset.min(len), text);
    }
}

impl Drop for YClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// 第 i 个顶层块的编号。
pub fn block_id(doc: &Node, i: usize) -> String {
    doc.content[i].attr_str("id").unwrap().to_string()
}
