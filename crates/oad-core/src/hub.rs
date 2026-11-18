//! 同步中心：每个打开的页面一个 actor（tokio 任务），独占这一页的 [`PageDoc`] 和 awareness。
//!
//! - 浏览器的 y-websocket 消息、服务器 / Agent 的按块编辑都在 actor 里按顺序执行，每次都是一个 CRDT 事务；
//!   产生的增量马上广播给其他连接，攒一小会儿（`flush_window`）合并后交给这一页的存盘任务。
//! - 存盘任务按顺序写：追加增量、同步标题到 pages.title（并发 page.updated）、压缩成快照。
//!   因为按顺序，压缩时删掉的增量一定都已经包含在快照里。
//! - 没有连接、空闲 `idle_unload` 后存好、压缩、退出。发命令和退出前检查收件箱都在表锁里做，
//!   所以不会丢命令，也不会同时有两个 actor 管同一页。
//! - 处理命令时 panic 不会让 actor 退出：接住后把已经确认过的改动照常广播、存盘。
//! - 存盘失败会退避重试，数据一直留在存盘任务的队列里，直到写进去。
use crate::events::{EventKind, Events};
use crate::model::Principal;
use crate::storage::Storage;
use crate::{Error, Result};
use bytes::Bytes;
use oad_doc::PageDoc;
use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::time::Instant;
use tracing::{debug, error, warn};
use yrs::encoding::read::Cursor;
use yrs::sync::awareness::AwarenessUpdateEntry;
use yrs::sync::{Awareness, AwarenessUpdate, Message, MessageReader, SyncMessage};
use yrs::updates::decoder::DecoderV1;
use yrs::updates::encoder::Encode;
use yrs::{ClientID, ReadTxn, StateVector, Transact};

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
    /// 每个连接还没处理完的收到消息上限，满了就先不读这个连接（背压到 WebSocket）。
    pub conn_inbound: usize,
    /// 每个连接每分钟最多回几次 step1（每次都要算一份差量），超过就断开。
    pub step1_per_minute: usize,
}

impl Default for HubConfig {
    fn default() -> Self {
        HubConfig {
            flush_window: Duration::from_millis(20),
            compact_after: 200,
            idle_unload: Duration::from_secs(30),
            writer_presence: Duration::from_secs(3),
            conn_buffer: 512,
            conn_inbound: 64,
            step1_per_minute: 20,
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
        /// 处理完才释放，给连接的读取做背压。
        _permit: OwnedSemaphorePermit,
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
    /// 打开着的页面；None 表示已经关闭，不再接新的。
    docs: Mutex<Option<HashMap<String, Entry>>>,
    next: AtomicU64,
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
    inbound: Arc<Semaphore>,
}

impl ConnHandle {
    /// 转交客户端发来的一条（或几条拼在一起的）y-protocol 消息。积压太多时等前面的处理完。actor 已经没了返回 false。
    pub async fn send(&self, data: Vec<u8>) -> bool {
        let Ok(permit) = self.inbound.clone().acquire_owned().await else {
            return false;
        };
        self.tx
            .send(Cmd::Message {
                conn: self.id,
                data,
                _permit: permit,
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
                docs: Mutex::new(Some(HashMap::new())),
                next: AtomicU64::new(1),
            }),
        }
    }

    pub fn config(&self) -> &HubConfig {
        &self.inner.cfg
    }

    /// 现在打开着的页面数。
    pub fn loaded(&self) -> usize {
        self.docs().as_ref().map_or(0, HashMap::len)
    }

    fn docs(&self) -> std::sync::MutexGuard<'_, Option<HashMap<String, Entry>>> {
        self.inner
            .docs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// 发一条命令（在表锁里发，见模块说明）；页面没打开就先起一个 actor。关闭后（也在表锁里判断）报错。
    fn dispatch(&self, page_id: &str, cmd: Cmd) -> Result<mpsc::UnboundedSender<Cmd>> {
        let mut guard = self.docs();
        let Some(docs) = guard.as_mut() else {
            return Err(Error::Internal("服务器正在关闭".into()));
        };
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
            .map_err(|_| Error::Internal(format!("页面 {page_id} 加载或处理失败")))
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
            handle: ConnHandle {
                id: conn,
                tx,
                inbound: Arc::new(Semaphore::new(self.inner.cfg.conn_inbound.max(1))),
            },
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
        // 在表锁里取走全部并标记关闭：之后 dispatch 不会再起新的 actor
        let txs: Vec<_> = self
            .docs()
            .take()
            .map(|docs| docs.into_values().map(|e| e.tx).collect())
            .unwrap_or_default();
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

/// 写到成功为止：存储出错（库忙、磁盘满之类）退避重试；别的错（比如页面没了）重试也没用，放弃。
async fn retry<T, F, Fut>(page: &str, what: &str, mut f: F) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut wait = Duration::from_millis(50);
    loop {
        match f().await {
            Ok(v) => return Some(v),
            Err(e @ (Error::Storage(_) | Error::Internal(_))) => {
                warn!(page, "{what}失败，{wait:?} 后重试：{e}");
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(Duration::from_secs(5));
            }
            Err(e) => {
                warn!(page, "{what}失败，放弃：{e}");
                return None;
            }
        }
    }
}

/// 一页的存盘任务：按顺序写。
async fn persister(
    storage: Arc<dyn Storage>,
    events: Events,
    page_id: String,
    mut rx: mpsc::UnboundedReceiver<Persist>,
) {
    let st = &storage;
    let id = page_id.as_str();
    while let Some(p) = rx.recv().await {
        match p {
            Persist::Append(u) => {
                retry(id, "存增量", || st.append_update(id, u.clone())).await;
            }
            Persist::Title(t) => {
                let r = retry(id, "同步标题", || {
                    st.update_page(id, Some(t.clone()), None)
                })
                .await;
                if let Some(page) = r {
                    events.emit(EventKind::Updated, page);
                }
            }
            Persist::Compact(s) => {
                retry(id, "压缩", || st.compact(id, s.clone())).await;
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
    /// 最近一分钟回过的 step1，和上一次的 state vector（重复的不再算差量）。
    step1: VecDeque<Instant>,
    last_sv: Option<(StateVector, Instant)>,
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

enum Step1 {
    Answer,
    Skip,
    Kick,
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
        let Some(page) = inner.storage.page(page_id).await? else {
            return Err(Error::NotFound(format!("页面 {page_id} 不存在")));
        };
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
                buf.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((origin, e.update.clone()));
            })
            .map_err(|e| Error::Internal(e.to_string()))?;
        let awareness = Awareness::new(doc.ydoc().clone());
        Ok(Actor {
            page_id: page_id.to_string(),
            // 用库里的标题：上次存了增量、还没来得及同步标题就崩了的话，第一次 flush 会补上
            title: page.title,
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
                step1: VecDeque::new(),
                last_sv: None,
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
                    match self.allow_step1(conn, &sv) {
                        Step1::Answer => {
                            let diff = self.doc.ydoc().transact().encode_diff_v1(&sv);
                            self.send_to(
                                conn,
                                encode(&Message::Sync(SyncMessage::SyncStep2(diff))),
                            );
                        }
                        Step1::Skip => {}
                        Step1::Kick => {
                            warn!(page = %self.page_id, conn, "step1 太频繁，断开");
                            self.drop_conn(conn);
                            return;
                        }
                    }
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
        let ups = std::mem::take(&mut *self.updates.lock().unwrap_or_else(PoisonError::into_inner));
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
            Cmd::Message { conn, data, .. } => self.on_message(conn, &data),
            Cmd::Run(f) => f(self),
            Cmd::Close(done) => return Some(done),
        }
        None
    }

    /// 同一个连接 2 秒内发来一样的 state vector 不再回（差量不会变）；一分钟内太多次就断开。
    fn allow_step1(&mut self, conn: u64, sv: &StateVector) -> Step1 {
        let limit = self.cfg.step1_per_minute;
        let Some(c) = self.conns.get_mut(&conn) else {
            return Step1::Skip;
        };
        let now = Instant::now();
        if let Some((last, at)) = &c.last_sv
            && last == sv
            && now.duration_since(*at) < Duration::from_secs(2)
        {
            return Step1::Skip;
        }
        while c
            .step1
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60))
        {
            c.step1.pop_front();
        }
        if c.step1.len() >= limit {
            return Step1::Kick;
        }
        c.step1.push_back(now);
        c.last_sv = Some((sv.clone(), now));
        Step1::Answer
    }

    /// 执行一段处理，panic 了也不退出：记日志，把已经生效的改动照常广播、排进存盘。
    fn guarded<R>(&mut self, f: impl FnOnce(&mut Actor) -> R) -> Option<R> {
        match catch_unwind(AssertUnwindSafe(|| f(self))) {
            Ok(r) => Some(r),
            Err(_) => {
                error!(page = %self.page_id, "处理命令时 panic，已接住");
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    self.after();
                    self.flush();
                }));
                None
            }
        }
    }

    /// 存好、压缩（有新增量时），等存盘任务写完。
    async fn settle(&mut self) {
        self.guarded(|a| a.flush());
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
            remove_entry(&inner, &page_id, generation);
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
                    if let Some(Some(done)) = actor.guarded(|a| a.handle(c)) {
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
                actor.guarded(|a| {
                    if a.flush_at.is_some_and(|t| t <= now) {
                        a.flush();
                    }
                    a.clear_presence(now);
                });
                if actor.conns.is_empty() && actor.last_active + actor.cfg.idle_unload <= now {
                    actor.settle().await;
                    // 在表锁里确认收件箱是空的再关；不空就接着干
                    let quit = {
                        let mut docs = inner.docs.lock().unwrap_or_else(PoisonError::into_inner);
                        let quit = rx.is_empty();
                        if quit {
                            if let Some(d) = docs.as_mut()
                                && d.get(&page_id).is_some_and(|e| e.generation == generation)
                            {
                                d.remove(&page_id);
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
    let mut docs = inner.docs.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(d) = docs.as_mut()
        && d.get(page_id).is_some_and(|e| e.generation == generation)
    {
        d.remove(page_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::Sqlite;
    use crate::{AuthConfig, AuthRequest, Core};
    use std::sync::atomic::AtomicUsize;

    async fn setup(dir: &std::path::Path, cfg: HubConfig) -> (Arc<Sqlite>, Arc<Core>, Principal) {
        let storage = Arc::new(Sqlite::open(dir.join("oad.db")).unwrap());
        let core = Core::open(storage.clone(), AuthConfig::None, cfg)
            .await
            .unwrap();
        let headers = http::HeaderMap::new();
        let who = core
            .auth
            .authenticate(&AuthRequest {
                headers: &headers,
                query_token: None,
            })
            .await
            .unwrap()
            .unwrap();
        (storage, core, who)
    }

    async fn new_page(core: &Core, who: &Principal, title: &str) -> String {
        let ws = core.default_workspace.id.clone();
        core.create_page(&ws, who, None, None, Some(title.into()), None)
            .await
            .unwrap()
            .id
    }

    fn md(s: &str) -> Vec<oad_doc::Node> {
        oad_markdown::parse(s).content
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panic_in_command_keeps_actor_and_acked_edits() {
        let dir = tempfile::tempdir().unwrap();
        // 攒很久才存：确认过的改动还在 pending 里时 panic
        let cfg = HubConfig {
            flush_window: Duration::from_secs(60),
            ..HubConfig::default()
        };
        let (storage, core, who) = setup(dir.path(), cfg).await;
        let page = new_page(&core, &who, "p").await;
        core.insert_blocks(&page, None, &who, md("确认过的改动"))
            .await
            .unwrap();
        let r = core
            .hub
            .call(&page, |a| {
                a.doc.append(&md("panic 前写进去的"));
                panic!("测试用的 panic");
            })
            .await;
        assert!(r.is_err());
        // actor 还活着
        let t = core
            .hub
            .read(&page, |d| d.to_node().plain_text())
            .await
            .unwrap();
        assert!(
            t.contains("确认过的改动") && t.contains("panic 前写进去的"),
            "{t}"
        );
        // panic 后马上存了（不用等 60 秒）
        for _ in 0..200 {
            if !storage.load_doc(&page).await.unwrap().updates.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let log = storage.load_doc(&page).await.unwrap();
        let d = PageDoc::new(1);
        d.apply_update(log.snapshot.as_ref().unwrap()).unwrap();
        for u in &log.updates {
            d.apply_update(u).unwrap();
        }
        assert!(d.to_node().plain_text().contains("确认过的改动"));
        core.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn acked_edits_survive_shutdown_race() {
        let dir = tempfile::tempdir().unwrap();
        // 很快卸载，让关机时一直有新 actor 在起
        let cfg = HubConfig {
            flush_window: Duration::from_millis(50),
            idle_unload: Duration::from_millis(1),
            ..HubConfig::default()
        };
        let (storage, core, who) = setup(dir.path(), cfg).await;
        let mut pages = Vec::new();
        for i in 0..20 {
            pages.push(new_page(&core, &who, &format!("p{i}")).await);
        }
        let mut tasks = Vec::new();
        for (i, page) in pages.iter().enumerate() {
            let (core, who, page) = (core.clone(), who.clone(), page.clone());
            tasks.push(tokio::spawn(async move {
                let mut acked = Vec::new();
                for n in 0.. {
                    let text = format!("改动-{i}-{n}");
                    match core.insert_blocks(&page, None, &who, md(&text)).await {
                        Ok(_) => acked.push(text),
                        Err(_) => break,
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                (page, acked)
            }));
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        core.shutdown().await;
        let mut results = Vec::new();
        for t in tasks {
            results.push(t.await.unwrap());
        }
        let core2 = Core::open(storage.clone(), AuthConfig::None, HubConfig::default())
            .await
            .unwrap();
        let mut total = 0;
        for (page, acked) in results {
            let text = core2
                .hub
                .read(&page, |d| d.to_node().plain_text())
                .await
                .unwrap();
            for a in &acked {
                assert!(text.contains(a.as_str()), "确认过的 {a} 丢了");
            }
            total += acked.len();
        }
        assert!(total > 20);
        core2.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stale_title_is_reconciled_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let (storage, core, who) = setup(dir.path(), HubConfig::default()).await;
        let page = new_page(&core, &who, "旧标题").await;
        core.shutdown().await;
        // 模拟崩溃：标题的增量存进去了，pages.title 还没来得及改
        let log = storage.load_doc(&page).await.unwrap();
        let d = PageDoc::new(oad_doc::server_client_id());
        d.apply_update(log.snapshot.as_ref().unwrap()).unwrap();
        let sv = d.state_vector();
        d.set_title("新标题");
        storage
            .append_update(&page, d.encode_diff(&sv).unwrap())
            .await
            .unwrap();
        assert_eq!(storage.page(&page).await.unwrap().unwrap().title, "旧标题");
        let core = Core::open(storage.clone(), AuthConfig::None, HubConfig::default())
            .await
            .unwrap();
        core.hub.flush(&page).await.unwrap();
        assert_eq!(storage.page(&page).await.unwrap().unwrap().title, "新标题");
        core.shutdown().await;
    }

    #[tokio::test]
    async fn persist_retries_until_it_lands() {
        let tries = AtomicUsize::new(0);
        let r = retry("p", "存增量", || async {
            if tries.fetch_add(1, Ordering::SeqCst) < 2 {
                Err(Error::Storage("database is locked".into()))
            } else {
                Ok(7)
            }
        })
        .await;
        assert_eq!(r, Some(7));
        assert_eq!(tries.load(Ordering::SeqCst), 3);
        // 重试没用的错误直接放弃
        let r: Option<()> = retry("p", "同步标题", || async {
            Err(Error::NotFound("页面没了".into()))
        })
        .await;
        assert_eq!(r, None);
    }

    #[test]
    fn origin_roundtrip() {
        let o = yrs::Origin::from(42u64);
        assert_eq!(origin_conn(&o), Some(42));
    }
}
