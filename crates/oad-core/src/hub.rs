//! 同步中心：每个打开的页面一个 actor（tokio 任务），独占这一页的 [`PageDoc`] 和 awareness。
//!
//! - 浏览器的 y-websocket 消息、服务器 / Agent 的按块编辑都在 actor 里按顺序执行，每次都是一个 CRDT 事务；
//!   产生的增量马上广播给其他连接，攒一小会儿（`flush_window`）合并后交给这一页的存盘任务。
//! - 存盘任务按顺序写：追加增量、同步标题到 pages.title（并发 page.updated）、压缩成快照。
//!   因为按顺序，压缩时删掉的增量一定都已经包含在快照里。
//! - 没有连接、空闲 `idle_unload` 后存好、压缩、退出。发命令和退出前检查收件箱都在表锁里做，
//!   所以不会丢命令，也不会同时有两个 actor 管同一页。
use crate::events::{EventKind, Events};
use crate::model::Principal;
use crate::storage::Storage;
use crate::{Error, Result};
use bytes::Bytes;
use oad_doc::PageDoc;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tracing::{debug, warn};
use yrs::encoding::read::Cursor;
use yrs::sync::awareness::AwarenessUpdateEntry;
use yrs::sync::{Awareness, AwarenessUpdate, Message, MessageReader, SyncMessage};
use yrs::updates::decoder::DecoderV1;
use yrs::updates::encoder::Encode;
use yrs::{ClientID, ReadTxn, Transact};

#[derive(Clone, Debug)]
pub struct HubConfig {
    /// 增量攒多久合并存一次。
    pub flush_window: Duration,
    /// 快照之后攒了这么多条增量就压缩。
    pub compact_after: usize,
    /// 没有连接多久后卸载。
    pub idle_unload: Duration,
    /// HTTP 写入后，写入者的在线状态保留多久。
    pub writer_presence: Duration,
    /// 每个连接的待发消息上限，满了说明对方太慢，断开它（重连后会重新同步）。
    pub conn_buffer: usize,
}

impl Default for HubConfig {
    fn default() -> Self {
        HubConfig {
            flush_window: Duration::from_millis(20),
            compact_after: 200,
            idle_unload: Duration::from_secs(30),
            writer_presence: Duration::from_secs(3),
            conn_buffer: 512,
        }
    }
}

/// 通过 HTTP 写入的是谁、在写哪一块（替它在 awareness 里放一份）。
#[derive(Clone, Debug)]
pub struct Writer {
    pub principal: Principal,
    pub block: Option<String>,
}

type Job = Box<dyn FnOnce(&mut Actor) + Send>;

enum Cmd {
    Join {
        conn: u64,
        readonly: bool,
        out: mpsc::Sender<Bytes>,
        ready: oneshot::Sender<()>,
    },
    Leave {
        conn: u64,
    },
    Message {
        conn: u64,
        data: Vec<u8>,
    },
    Run(Job),
    /// 存好、压缩、退出。
    Close(oneshot::Sender<()>),
}

struct Entry {
    generation: u64,
    tx: mpsc::UnboundedSender<Cmd>,
}

struct Inner {
    storage: Arc<dyn Storage>,
    events: Events,
    cfg: HubConfig,
    docs: Mutex<HashMap<String, Entry>>,
    next: AtomicU64,
    closed: AtomicBool,
}

#[derive(Clone)]
pub struct DocHub {
    inner: Arc<Inner>,
}

/// 一个同步连接。收到的消息交给 [`ConnHandle::send`]，要发出去的从 `rx` 里取。
pub struct Conn {
    pub handle: ConnHandle,
    pub rx: mpsc::Receiver<Bytes>,
}

pub struct ConnHandle {
    id: u64,
    tx: mpsc::UnboundedSender<Cmd>,
}

impl ConnHandle {
    /// 转交客户端发来的一条（或几条拼在一起的）y-protocol 消息。actor 已经没了返回 false。
    pub fn send(&self, data: Vec<u8>) -> bool {
        self.tx
            .send(Cmd::Message {
                conn: self.id,
                data,
            })
            .is_ok()
    }
}

impl Drop for ConnHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Leave { conn: self.id });
    }
}

impl DocHub {
    pub fn new(storage: Arc<dyn Storage>, events: Events, cfg: HubConfig) -> Self {
        DocHub {
            inner: Arc::new(Inner {
                storage,
                events,
                cfg,
                docs: Mutex::new(HashMap::new()),
                next: AtomicU64::new(1),
                closed: AtomicBool::new(false),
            }),
        }
    }

    pub fn config(&self) -> &HubConfig {
        &self.inner.cfg
    }

    /// 现在打开着的页面数。
    pub fn loaded(&self) -> usize {
        self.inner.docs.lock().unwrap().len()
    }

    /// 发一条命令（在表锁里发，见模块说明）；页面没打开就先起一个 actor。
    fn dispatch(&self, page_id: &str, cmd: Cmd) -> Result<mpsc::UnboundedSender<Cmd>> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(Error::Internal("服务器正在关闭".into()));
        }
        let mut docs = self.inner.docs.lock().unwrap();
        let cmd = match docs.get(page_id) {
            Some(e) => match e.tx.send(cmd) {
                Ok(()) => return Ok(e.tx.clone()),
                Err(mpsc::error::SendError(c)) => c,
            },
            None => cmd,
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let generation = self.inner.next.fetch_add(1, Ordering::Relaxed);
        let _ = tx.send(cmd);
        docs.insert(
            page_id.to_string(),
            Entry {
                generation,
                tx: tx.clone(),
            },
        );
        tokio::spawn(run_actor(
            self.inner.clone(),
            page_id.to_string(),
            generation,
            rx,
        ));
        Ok(tx)
    }

    async fn call<R: Send + 'static>(
        &self,
        page_id: &str,
        f: impl FnOnce(&mut Actor) -> R + Send + 'static,
    ) -> Result<R> {
        let (tx, rx) = oneshot::channel();
        self.dispatch(
            page_id,
            Cmd::Run(Box::new(move |a| {
                let _ = tx.send(f(a));
            })),
        )?;
        rx.await
            .map_err(|_| Error::Internal(format!("页面 {page_id} 加载失败")))
    }

    /// 加入同步。连上后 actor 先发 step1 和当前 awareness。调用方要先确认页面存在、有权限。
    pub async fn join(&self, page_id: &str, readonly: bool) -> Result<Conn> {
        let conn = self.inner.next.fetch_add(1, Ordering::Relaxed);
        let (out, rx) = mpsc::channel(self.inner.cfg.conn_buffer);
        let (ready, wait) = oneshot::channel();
        let tx = self.dispatch(
            page_id,
            Cmd::Join {
                conn,
                readonly,
                out,
                ready,
            },
        )?;
        // 等它真的加载好（加载失败时这里报错）
        wait.await
            .map_err(|_| Error::Internal(format!("页面 {page_id} 加载失败")))?;
        Ok(Conn {
            handle: ConnHandle { id: conn, tx },
            rx,
        })
    }

    /// 读正文。
    pub async fn read<R: Send + 'static>(
        &self,
        page_id: &str,
        f: impl FnOnce(&PageDoc) -> R + Send + 'static,
    ) -> Result<R> {
        self.call(page_id, move |a| f(&a.doc)).await
    }

    /// 服务器 / Agent 改正文：一次 CRDT 事务，连着的网页马上收到。
    pub async fn edit<R: Send + 'static>(
        &self,
        page_id: &str,
        writer: Option<Writer>,
        f: impl FnOnce(&PageDoc) -> Result<R> + Send + 'static,
    ) -> Result<R> {
        self.call(page_id, move |a| {
            if let Some(w) = &writer {
                a.presence(w);
            }
            let r = f(&a.doc);
            a.after();
            r
        })
        .await?
    }

    /// 改标题，等写进 pages.title（并发出 page.updated）后返回。
    pub async fn set_title(&self, page_id: &str, title: String) -> Result<()> {
        let done = self
            .call(page_id, move |a| {
                a.doc.set_title(&title);
                a.after();
                a.flush();
                a.barrier()
            })
            .await?;
        let _ = done.await;
        Ok(())
    }

    /// 把攒着的增量存掉（测试、关机前用）。
    pub async fn flush(&self, page_id: &str) -> Result<()> {
        let done = self
            .call(page_id, |a| {
                a.flush();
                a.barrier()
            })
            .await?;
        let _ = done.await;
        Ok(())
    }

    /// 关闭：所有页面存好、压缩后退出，之后不再接新的。
    pub async fn shutdown(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        let txs: Vec<_> = self
            .inner
            .docs
            .lock()
            .unwrap()
            .values()
            .map(|e| e.tx.clone())
            .collect();
        let mut waits = Vec::new();
        for tx in txs {
            let (done, rx) = oneshot::channel();
            if tx.send(Cmd::Close(done)).is_ok() {
                waits.push(rx);
            }
        }
        for w in waits {
            let _ = w.await;
        }
    }
}

enum Persist {
    Append(Vec<u8>),
    Title(String),
    Compact(Vec<u8>),
    Barrier(oneshot::Sender<()>),
}

/// 一页的存盘任务：按顺序写。
async fn persister(
    storage: Arc<dyn Storage>,
    events: Events,
    page_id: String,
    mut rx: mpsc::UnboundedReceiver<Persist>,
) {
    while let Some(p) = rx.recv().await {
        match p {
            Persist::Append(u) => {
                if let Err(e) = storage.append_update(&page_id, u).await {
                    warn!(page = %page_id, "存增量失败：{e}");
                }
            }
            Persist::Title(t) => match storage.update_page(&page_id, Some(t), None).await {
                Ok(page) => events.emit(EventKind::Updated, page),
                Err(e) => warn!(page = %page_id, "同步标题失败：{e}"),
            },
            Persist::Compact(s) => {
                if let Err(e) = storage.compact(&page_id, s).await {
                    warn!(page = %page_id, "压缩失败：{e}");
                }
            }
            Persist::Barrier(done) => {
                let _ = done.send(());
            }
        }
    }
}

struct ConnState {
    out: mpsc::Sender<Bytes>,
    readonly: bool,
    /// 这个连接带来的 awareness client id，断开时替它清掉。
    clients: HashSet<u64>,
}

/// 替 HTTP 写入者放的在线状态。
struct Presence {
    client: u64,
    clock: u32,
    clear_at: Instant,
}

type UpdateBuf = Arc<Mutex<Vec<(Option<u64>, Vec<u8>)>>>;

pub struct Actor {
    page_id: String,
    doc: PageDoc,
    awareness: Awareness,
    conns: HashMap<u64, ConnState>,
    /// 事务提交时收集的增量：(来源连接, 增量)。
    updates: UpdateBuf,
    pending: Vec<Vec<u8>>,
    flush_at: Option<Instant>,
    log_len: usize,
    title: String,
    persist: mpsc::UnboundedSender<Persist>,
    presence: HashMap<String, Presence>,
    last_active: Instant,
    cfg: HubConfig,
}

fn encode(m: &Message) -> Bytes {
    Bytes::from(m.encode_v1())
}

fn origin_conn(o: &yrs::Origin) -> Option<u64> {
    let b: [u8; 8] = o.as_ref().try_into().ok()?;
    Some(u64::from_be_bytes(b))
}

/// 按用户编号给个固定的颜色。
fn color_for(id: &str) -> &'static str {
    const COLORS: &[&str] = &[
        "#e5484d", "#f76b15", "#ffc53d", "#46a758", "#12a594", "#0090ff", "#8e4ec6", "#d6409f",
    ];
    let h = id
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    COLORS[h as usize % COLORS.len()]
}

impl Actor {
    async fn load(
        inner: &Inner,
        page_id: &str,
        persist: mpsc::UnboundedSender<Persist>,
    ) -> Result<Actor> {
        if inner.storage.page(page_id).await?.is_none() {
            return Err(Error::NotFound(format!("页面 {page_id} 不存在")));
        }
        let log = inner.storage.load_doc(page_id).await?;
        let doc = PageDoc::new(oad_doc::server_client_id());
        if let Some(s) = &log.snapshot {
            doc.apply_update(s)?;
        }
        for u in &log.updates {
            doc.apply_update(u)?;
        }
        // 加载完再订阅，加载本身不算新增量
        let updates: UpdateBuf = Arc::default();
        let buf = updates.clone();
        doc.ydoc()
            .observe_update_v1("hub", move |txn, e| {
                let origin = txn.origin().and_then(origin_conn);
                buf.lock().unwrap().push((origin, e.update.clone()));
            })
            .map_err(|e| Error::Internal(e.to_string()))?;
        let awareness = Awareness::new(doc.ydoc().clone());
        Ok(Actor {
            page_id: page_id.to_string(),
            title: doc.title(),
            doc,
            awareness,
            conns: HashMap::new(),
            updates,
            pending: Vec::new(),
            flush_at: None,
            log_len: log.updates.len(),
            persist,
            presence: HashMap::new(),
            last_active: Instant::now(),
            cfg: inner.cfg.clone(),
        })
    }

    fn send_to(&mut self, conn: u64, msg: Bytes) {
        let ok = self
            .conns
            .get(&conn)
            .is_some_and(|c| c.out.try_send(msg).is_ok());
        if !ok {
            self.drop_conn(conn);
        }
    }

    /// 发给所有连接，except 除外。
    fn broadcast(&mut self, msg: &Bytes, except: Option<u64>) {
        let mut slow = Vec::new();
        for (id, c) in &self.conns {
            if Some(*id) != except && c.out.try_send(msg.clone()).is_err() {
                slow.push(*id);
            }
        }
        for id in slow {
            debug!(page = %self.page_id, conn = id, "连接太慢或已断开，移除");
            self.drop_conn(id);
        }
    }

    fn drop_conn(&mut self, conn: u64) {
        let Some(c) = self.conns.remove(&conn) else {
            return;
        };
        // 替它清掉 awareness，告诉其他人它走了
        let ids: Vec<ClientID> = c.clients.iter().map(|&id| ClientID::new(id)).collect();
        if ids.is_empty() {
            return;
        }
        for id in &ids {
            self.awareness.remove_state(*id);
        }
        if let Ok(u) = self.awareness.update_with_clients(ids) {
            self.broadcast(&encode(&Message::Awareness(u)), None);
        }
    }

    fn join(&mut self, conn: u64, readonly: bool, out: mpsc::Sender<Bytes>) {
        self.conns.insert(
            conn,
            ConnState {
                out,
                readonly,
                clients: HashSet::new(),
            },
        );
        let sv = self.doc.ydoc().transact().state_vector();
        self.send_to(conn, encode(&Message::Sync(SyncMessage::SyncStep1(sv))));
        if let Ok(u) = self.awareness.update()
            && !u.clients.is_empty()
        {
            self.send_to(conn, encode(&Message::Awareness(u)));
        }
    }

    fn on_message(&mut self, conn: u64, data: &[u8]) {
        let Some(readonly) = self.conns.get(&conn).map(|c| c.readonly) else {
            return;
        };
        let mut dec = DecoderV1::new(Cursor::new(data));
        let msgs: Vec<_> = MessageReader::new(&mut dec).collect();
        for m in msgs {
            match m {
                Ok(Message::Sync(SyncMessage::SyncStep1(sv))) => {
                    let diff = self.doc.ydoc().transact().encode_diff_v1(&sv);
                    self.send_to(conn, encode(&Message::Sync(SyncMessage::SyncStep2(diff))));
                }
                Ok(Message::Sync(SyncMessage::SyncStep2(u) | SyncMessage::Update(u))) => {
                    if readonly {
                        continue; // 只读：丢掉，不断开
                    }
                    if let Err(e) = self.doc.apply_update_from(&u, conn) {
                        warn!(page = %self.page_id, conn, "客户端增量合并失败：{e}");
                    }
                }
                Ok(Message::Awareness(u)) => {
                    if let Some(c) = self.conns.get_mut(&conn) {
                        for (id, e) in &u.clients {
                            if &*e.json == "null" {
                                c.clients.remove(&id.get());
                            } else {
                                c.clients.insert(id.get());
                            }
                        }
                    }
                    match self.awareness.apply_update_summary(u.clone()) {
                        // 发给所有人，包括它自己：y-websocket 靠收到消息判断连接还活着
                        Ok(Some(_)) => self.broadcast(&encode(&Message::Awareness(u)), None),
                        Ok(None) => {}
                        Err(e) => warn!(page = %self.page_id, conn, "awareness 出错：{e}"),
                    }
                }
                Ok(Message::AwarenessQuery) => {
                    if let Ok(u) = self.awareness.update() {
                        self.send_to(conn, encode(&Message::Awareness(u)));
                    }
                }
                Ok(Message::Auth(_) | Message::Custom(..)) => {}
                Err(e) => {
                    warn!(page = %self.page_id, conn, "消息格式不对：{e}");
                    break;
                }
            }
        }
        self.after();
    }

    /// 每条命令之后：把新增量广播出去，排进待存。
    fn after(&mut self) {
        let ups = std::mem::take(&mut *self.updates.lock().unwrap());
        if ups.is_empty() {
            return;
        }
        for (origin, u) in ups {
            let msg = encode(&Message::Sync(SyncMessage::Update(u.clone())));
            self.broadcast(&msg, origin);
            self.pending.push(u);
        }
        if self.flush_at.is_none() {
            self.flush_at = Some(Instant::now() + self.cfg.flush_window);
        }
        if self.pending.len() >= 200 {
            self.flush();
        }
    }

    /// 把攒着的增量合并成一条交给存盘任务；标题变了顺带同步；增量够多了压缩。
    fn flush(&mut self) {
        self.flush_at = None;
        if !self.pending.is_empty() {
            let ups = std::mem::take(&mut self.pending);
            let merged = if ups.len() == 1 {
                ups.into_iter().next().unwrap()
            } else {
                match yrs::merge_updates_v1(&ups) {
                    Ok(m) => m,
                    Err(e) => {
                        warn!(page = %self.page_id, "合并增量失败：{e}");
                        for u in ups {
                            let _ = self.persist.send(Persist::Append(u));
                            self.log_len += 1;
                        }
                        Vec::new()
                    }
                }
            };
            if !merged.is_empty() {
                let _ = self.persist.send(Persist::Append(merged));
                self.log_len += 1;
            }
        }
        let t = self.doc.title();
        if t != self.title {
            self.title = t.clone();
            let _ = self.persist.send(Persist::Title(t));
        }
        if self.log_len >= self.cfg.compact_after {
            self.compact();
        }
    }

    fn compact(&mut self) {
        let _ = self.persist.send(Persist::Compact(self.doc.encode_state()));
        self.log_len = 0;
    }

    /// 前面排着的都存好后完成。
    fn barrier(&mut self) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let _ = self.persist.send(Persist::Barrier(tx));
        rx
    }

    fn presence(&mut self, w: &Writer) {
        let p = self
            .presence
            .entry(w.principal.user_id.clone())
            .or_insert_with(|| Presence {
                client: oad_doc::server_client_id(),
                clock: 0,
                clear_at: Instant::now(),
            });
        p.clock += 1;
        p.clear_at = Instant::now() + self.cfg.writer_presence;
        let state = serde_json::json!({
            "user": {
                "id": w.principal.user_id,
                "name": w.principal.name,
                "color": color_for(&w.principal.user_id),
                "kind": w.principal.kind.as_str(),
            },
            "block": w.block,
        });
        let (client, clock) = (p.client, p.clock);
        self.set_awareness(client, clock, state.to_string());
    }

    fn set_awareness(&mut self, client: u64, clock: u32, json: String) {
        let u = AwarenessUpdate {
            clients: HashMap::from([(
                ClientID::new(client),
                AwarenessUpdateEntry {
                    clock,
                    json: json.into(),
                },
            )]),
        };
        if self.awareness.apply_update(u.clone()).is_ok() {
            self.broadcast(&encode(&Message::Awareness(u)), None);
        }
    }

    fn clear_presence(&mut self, now: Instant) {
        let expired: Vec<String> = self
            .presence
            .iter()
            .filter(|(_, p)| p.clear_at <= now)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired {
            if let Some(p) = self.presence.remove(&k) {
                self.set_awareness(p.client, p.clock + 1, "null".into());
            }
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        let idle = self
            .conns
            .is_empty()
            .then(|| self.last_active + self.cfg.idle_unload);
        [
            self.flush_at,
            self.presence.values().map(|p| p.clear_at).min(),
            idle,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn handle(&mut self, cmd: Cmd) -> Option<oneshot::Sender<()>> {
        self.last_active = Instant::now();
        match cmd {
            Cmd::Join {
                conn,
                readonly,
                out,
                ready,
            } => {
                self.join(conn, readonly, out);
                let _ = ready.send(());
            }
            Cmd::Leave { conn } => self.drop_conn(conn),
            Cmd::Message { conn, data } => self.on_message(conn, &data),
            Cmd::Run(f) => f(self),
            Cmd::Close(done) => return Some(done),
        }
        None
    }

    /// 存好、压缩（有新增量时），等存盘任务写完。
    async fn settle(&mut self) {
        self.flush();
        if self.log_len > 0 {
            self.compact();
        }
        let _ = self.barrier().await;
    }
}

async fn run_actor(
    inner: Arc<Inner>,
    page_id: String,
    generation: u64,
    mut rx: mpsc::UnboundedReceiver<Cmd>,
) {
    let (ptx, prx) = mpsc::unbounded_channel();
    let persist = tokio::spawn(persister(
        inner.storage.clone(),
        inner.events.clone(),
        page_id.clone(),
        prx,
    ));
    let mut actor = match Actor::load(&inner, &page_id, ptx).await {
        Ok(a) => a,
        Err(e) => {
            warn!(page = %page_id, "加载页面失败：{e}");
            // 收件箱里的命令直接丢掉，调用方会收到错误
            let mut docs = inner.docs.lock().unwrap();
            if docs
                .get(&page_id)
                .is_some_and(|e| e.generation == generation)
            {
                docs.remove(&page_id);
            }
            rx.close();
            return;
        }
    };
    debug!(page = %page_id, "页面已加载");
    let mut closing: Option<oneshot::Sender<()>> = None;
    loop {
        let deadline = actor.next_deadline();
        tokio::select! {
            cmd = rx.recv() => match cmd {
                Some(c) => {
                    if let Some(done) = actor.handle(c) {
                        closing = Some(done);
                        break;
                    }
                }
                None => {
                    actor.settle().await;
                    break;
                }
            },
            _ = async { tokio::time::sleep_until(deadline.unwrap()).await }, if deadline.is_some() => {
                let now = Instant::now();
                if actor.flush_at.is_some_and(|t| t <= now) {
                    actor.flush();
                }
                actor.clear_presence(now);
                if actor.conns.is_empty() && actor.last_active + actor.cfg.idle_unload <= now {
                    actor.settle().await;
                    // 在表锁里确认收件箱是空的再关；不空就接着干
                    let quit = {
                        let mut docs = inner.docs.lock().unwrap();
                        let quit = rx.is_empty();
                        if quit {
                            if docs.get(&page_id).is_some_and(|e| e.generation == generation) {
                                docs.remove(&page_id);
                            }
                            rx.close();
                        }
                        quit
                    };
                    if quit {
                        debug!(page = %page_id, "空闲，卸载");
                        break;
                    }
                }
            }
        }
    }
    if closing.is_some() {
        actor.settle().await;
        remove_entry(&inner, &page_id, generation);
    }
    drop(actor);
    let _ = persist.await;
    if let Some(done) = closing {
        let _ = done.send(());
    }
}

fn remove_entry(inner: &Inner, page_id: &str, generation: u64) {
    let mut docs = inner.docs.lock().unwrap();
    if docs
        .get(page_id)
        .is_some_and(|e| e.generation == generation)
    {
        docs.remove(page_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_roundtrip() {
        let o = yrs::Origin::from(42u64);
        assert_eq!(origin_conn(&o), Some(42));
    }
}
