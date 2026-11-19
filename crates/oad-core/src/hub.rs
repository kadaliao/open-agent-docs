//! 同步中心：每个打开的页面一个 actor（tokio 任务），独占这一页的 [`PageDoc`] 和 awareness。
//!
//! - 浏览器的 y-websocket 消息、服务器 / Agent 的按块编辑都在 actor 里按顺序执行，每次都是一个 CRDT 事务；
//!   产生的增量马上广播给其他连接，攒一小会儿（`flush_window`）合并后交给这一页的存盘任务。
//! - 存盘任务按顺序写：追加增量、同步标题到 pages.title（并发 page.updated）、压缩成快照。
//!   因为按顺序，压缩时删掉的增量一定都已经包含在快照里。
//! - 没有连接、空闲 `idle_unload` 后存好、压缩、退出。发命令和退出前检查收件箱都在表锁里做，
//!   所以不会丢命令，也不会同时有两个 actor 管同一页。
//! - 处理命令时 panic 不会让 actor 退出：接住后把已经确认过的改动照常广播、存盘。
//! - 服务器替人 / Agent 写正文时用写入者自己的 client id（每个写入者每次加载一个，直接在主文档上写，
//!   靠 yrs 的小补丁 `TransactionMut::set_client_id`，见 vendor/yrs）。
//!   给了旧的 base 时，如果要改的块在那之后没被别人动过，结果和在当前文档上改一样，也直接写；
//!   动过才“从 base 那一刻复制一份再改”。为此每个顶层块记着最后一次变的序号，发出去的版本记着当时的序号。
//! - 读整页时没变的顶层块用缓存（同样按序号判断），大页面读起来不占着 actor 太久。
//! - 存盘失败会退避重试，数据一直留在存盘任务的队列里，直到写进去。
use crate::events::{EventKind, Events};
use crate::model::{NewVersion, Principal, PrincipalKind, VersionAuthor};
use crate::storage::Storage;
use crate::{Error, Result};
use bytes::Bytes;
use oad_doc::{PageDoc, RenderCache};
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
    /// 编辑停下这么久自动存一版。
    pub version_idle: Duration,
    /// 一直在改：这么久存一版。
    pub version_max_age: Duration,
    /// 一直在改：攒了这么多字节的增量存一版。
    pub version_max_bytes: usize,
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
            version_idle: Duration::from_secs(120),
            version_max_age: Duration::from_secs(600),
            version_max_bytes: 64 * 1024,
        }
    }
}

/// 一次写入会改到哪些块（判断旧的 base 能不能直接在当前文档上改）。
#[derive(Default, Clone, Debug)]
pub struct Targets {
    /// 整页都可能改（整页替换）。
    pub all: bool,
    /// 内容要和读到的比对的块（替换）。
    pub content: Vec<String>,
    /// 只用来定位的块（插在它前后）。
    pub refs: Vec<String>,
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
        who: Principal,
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
    /// 有人写了 embed 块时把网址交给这里（链接解析的后台预取）。
    prefetch: std::sync::OnceLock<mpsc::UnboundedSender<String>>,
    /// 存了增量的页面交给搜索索引（它自己防抖）。
    indexer: std::sync::OnceLock<mpsc::UnboundedSender<String>>,
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
                prefetch: std::sync::OnceLock::new(),
                indexer: std::sync::OnceLock::new(),
            }),
        }
    }

    /// 设预取的去处（只能设一次）。
    pub fn set_prefetch(&self, tx: mpsc::UnboundedSender<String>) {
        let _ = self.inner.prefetch.set(tx);
    }

    /// 页面存了增量后把页面 id 发到这里（搜索索引用）。
    pub fn set_indexer(&self, tx: mpsc::UnboundedSender<String>) {
        let _ = self.inner.indexer.set(tx);
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
    /// 这个连接发来的增量里新出现的 client id 记到 who 名下。
    pub async fn join(&self, page_id: &str, readonly: bool, who: Principal) -> Result<Conn> {
        let conn = self.inner.next.fetch_add(1, Ordering::Relaxed);
        let (out, rx) = mpsc::channel(self.inner.cfg.conn_buffer);
        let (ready, wait) = oneshot::channel();
        let tx = self.dispatch(
            page_id,
            Cmd::Join {
                conn,
                readonly,
                who,
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

    /// 读正文，但页面没打开时不为它起 actor：直接从库里读一份临时的（搜索这种要扫很多页的用）。
    pub async fn peek<R: Send + 'static>(
        &self,
        page_id: &str,
        f: impl FnOnce(&PageDoc) -> R + Send + 'static,
    ) -> Result<R> {
        let loaded = self
            .docs()
            .as_ref()
            .is_some_and(|d| d.contains_key(page_id));
        if loaded {
            return self.read(page_id, f).await;
        }
        let log = self.inner.storage.load_doc(page_id).await?;
        tokio::task::spawn_blocking(move || {
            let d = PageDoc::new(1);
            if let Some(s) = &log.snapshot {
                d.apply_update(s)?;
            }
            for u in &log.updates {
                d.apply_update(u)?;
            }
            Ok(f(&d))
        })
        .await
        .map_err(|e| Error::Internal(e.to_string()))?
    }

    /// 整页的节点树和当前版本。没变的块用缓存；actor 里只取共享的块，拼成整棵树在别的线程上做。
    pub async fn content(&self, page_id: &str) -> Result<(oad_doc::Node, Vec<u8>)> {
        let (blocks, v) = self
            .call(page_id, |a| {
                let v = a.issue_version();
                let (changed, seq) = (&a.changed, a.seq);
                let blocks = a.doc.blocks_cached(
                    &mut a.render,
                    |id| changed.get(id).copied().unwrap_or(0),
                    seq,
                );
                (blocks, v)
            })
            .await?;
        let node = tokio::task::spawn_blocking(move || {
            oad_doc::Node::new("doc").children(blocks.iter().map(|b| (**b).clone()).collect())
        })
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
        Ok((node, v))
    }

    /// 一块和当前版本。
    pub async fn block(
        &self,
        page_id: &str,
        block_id: &str,
    ) -> Result<(Option<oad_doc::Node>, Vec<u8>)> {
        let b = block_id.to_string();
        self.call(page_id, move |a| (a.doc.block(&b), a.issue_version()))
            .await
    }

    /// 当前版本（写的时候当 base 交回来）。
    pub async fn version(&self, page_id: &str) -> Result<Vec<u8>> {
        self.call(page_id, |a| a.issue_version()).await
    }

    /// 读正文。
    pub async fn read<R: Send + 'static>(
        &self,
        page_id: &str,
        f: impl FnOnce(&PageDoc) -> R + Send + 'static,
    ) -> Result<R> {
        self.call(page_id, move |a| f(&a.doc)).await
    }

    /// 服务器 / Agent 改正文，连着的网页马上收到。
    ///
    /// 有写入者时在一份副本上用它自己的 client id 改（记到它名下），改完一次合并回来：f 中途出错就什么都不改。
    /// 给了 base（读的时候拿到的版本）就从那个版本开始改，这期间别人写的不会被当成要删的。
    pub async fn edit<R: Send + 'static>(
        &self,
        page_id: &str,
        writer: Option<Writer>,
        base: Option<Vec<u8>>,
        targets: Targets,
        f: impl FnOnce(&PageDoc) -> Result<R> + Send + 'static,
    ) -> Result<R> {
        self.call(page_id, move |a| {
            let agent = writer
                .as_ref()
                .filter(|w| w.principal.kind == PrincipalKind::Agent);
            // Agent 的每次写入单独成一版：之前别人的改动先存一版
            if agent.is_some() {
                a.cut_version("auto", None, None);
            }
            let r = match &writer {
                Some(w) => {
                    a.presence(w);
                    a.edit_as(&w.principal, base.as_deref(), &targets, f)
                }
                None if base.is_some() => Err(Error::BadRequest("base 要和写入者一起给".into())),
                None => f(&a.doc),
            };
            a.drain();
            if let Some(w) = agent
                && r.is_ok()
            {
                a.cut_version("agent", None, Some(&w.principal));
            }
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

    /// 这次加载里新记的作者（client id → 谁），还没存进库的也在里面。页面没打开就是空的。
    pub async fn recent_owners(&self, page_id: &str) -> HashMap<u64, (PrincipalKind, String)> {
        let loaded = self
            .docs()
            .as_ref()
            .is_some_and(|d| d.contains_key(page_id));
        if !loaded {
            return HashMap::new();
        }
        self.call(page_id, |a| a.owners.clone())
            .await
            .unwrap_or_default()
    }

    /// 存一版（手动：kind = manual，带名字），返回版本号。
    pub async fn save_version(
        &self,
        page_id: &str,
        kind: &str,
        label: Option<String>,
        who: Principal,
    ) -> Result<i64> {
        let kind = kind.to_string();
        let rx = self
            .call(page_id, move |a| {
                a.ver_touch_if_empty(&who);
                a.cut_version(&kind, label, Some(&who))
            })
            .await?;
        match rx {
            Some(rx) => rx.await.map_err(|_| Error::Internal("存版本失败".into())),
            None => Err(Error::Internal("存版本失败".into())),
        }
    }

    /// 恢复到旧内容：先把没存的改动存一版，以 who 的名义整页换成 doc（没变的块沿用编号），再存一版 restore。返回新版本号。
    pub async fn restore(
        &self,
        page_id: &str,
        who: Principal,
        doc: oad_doc::Node,
        label: String,
    ) -> Result<i64> {
        let rx = self
            .call(page_id, move |a| -> Result<_> {
                a.cut_version("auto", None, None);
                a.edit_direct(&who, |d| {
                    d.set_content(&doc);
                    Ok(())
                })?;
                a.drain();
                a.ver_touch_if_empty(&who);
                Ok(a.cut_version("restore", Some(label), Some(&who)))
            })
            .await??;
        match rx {
            Some(rx) => rx.await.map_err(|_| Error::Internal("存版本失败".into())),
            None => Err(Error::Internal("存版本失败".into())),
        }
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

    /// 页面打开着的话，等它排着的都存好（读历史版本前用，刚存的版本也读得到）；没打开就什么也不做。
    pub async fn settle_if_loaded(&self, page_id: &str) {
        let loaded = self
            .docs()
            .as_ref()
            .is_some_and(|d| d.contains_key(page_id));
        if loaded {
            let _ = self.flush(page_id).await;
        }
    }

    /// 关掉一个打开着的页面（存好再退出）；没打开就什么也不做。彻底删页面前用。
    pub async fn unload(&self, page_id: &str) {
        let tx = self
            .docs()
            .as_mut()
            .and_then(|d| d.remove(page_id))
            .map(|e| e.tx);
        if let Some(tx) = tx {
            let (done, rx) = oneshot::channel();
            if tx.send(Cmd::Close(done)).is_ok() {
                let _ = rx.await;
            }
        }
    }

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
    Clients(Vec<(u64, PrincipalKind, String)>),
    Title(String),
    Compact(Vec<u8>),
    Barrier(oneshot::Sender<()>),
    Version(NewVersion, Option<oneshot::Sender<i64>>),
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
                    // 行的标题就是标题那一格（存储里一起改了），告诉看着库的人
                    if page.kind == "row"
                        && let Some(db) = &page.parent_id
                        && let Ok(Some(m)) = st.database(db).await
                    {
                        events.emit(crate::Event {
                            workspace_id: page.workspace_id.clone(),
                            name: "db.changed",
                            data: serde_json::json!({ "db_id": db, "seq": m.seq }),
                            user_id: None,
                        });
                    }
                    events.page(EventKind::Updated, page);
                }
            }
            Persist::Compact(s) => {
                retry(id, "压缩", || st.compact(id, s.clone())).await;
            }
            Persist::Clients(cs) => {
                retry(id, "记作者", || st.add_doc_clients(id, cs.clone())).await;
            }
            Persist::Barrier(done) => {
                let _ = done.send(());
            }
            Persist::Version(v, done) => {
                let r = retry(id, "存版本", || {
                    st.add_version(
                        id,
                        NewVersion {
                            kind: v.kind.clone(),
                            label: v.label.clone(),
                            authors: v.authors.clone(),
                            created_by: v.created_by.clone(),
                            snapshot: v.snapshot.clone(),
                            state_vector: v.state_vector.clone(),
                        },
                    )
                })
                .await;
                if let (Some(vid), Some(done)) = (r, done) {
                    let _ = done.send(vid);
                }
            }
        }
    }
}

struct ConnState {
    out: mpsc::Sender<Bytes>,
    readonly: bool,
    who: Principal,
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
    /// 已经记过作者的 client id。
    known: HashSet<u64>,
    /// 这次加载里新记的作者（存盘任务可能还没写进库）。
    owners: HashMap<u64, (PrincipalKind, String)>,
    /// 每个 HTTP 写入者在这次加载里用的 client id。
    writers: HashMap<String, u64>,
    /// 每条命令改了文档就加一。
    seq: u64,
    /// 顶层块最后一次变时的序号（这次加载以来没变过的不在里面）。
    changed: HashMap<String, u64>,
    /// 提交时记下的、改到的顶层块和 embed 网址。
    touched: Arc<Mutex<Vec<oad_doc::BlockEvent>>>,
    prefetch: Option<mpsc::UnboundedSender<String>>,
    indexer: Option<mpsc::UnboundedSender<String>>,
    /// 这次加载里交给预取过的网址。
    prefetched: HashSet<String>,
    /// 发出去的版本：(版本的哈希, 当时的序号)，最近的一些。
    issued: VecDeque<(u64, u64)>,
    render: RenderCache,
    last_active: Instant,
    cfg: HubConfig,
    /// 上一版之后的改动（自动存版本用）。
    ver: VerState,
}

/// 上一版之后：改过没有、谁改的、攒了多少、什么时候。
#[derive(Default)]
struct VerState {
    dirty: bool,
    authors: Vec<VersionAuthor>,
    bytes: usize,
    first: Option<Instant>,
    last: Option<Instant>,
}

const ISSUED: usize = 256;

fn hash_bytes(b: &[u8]) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
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
        let known = inner
            .storage
            .doc_clients(page_id)
            .await?
            .into_iter()
            .map(|c| c.0)
            .collect();
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
        let touched: Arc<Mutex<Vec<oad_doc::BlockEvent>>> = Arc::default();
        let t = touched.clone();
        doc.observe_blocks(move |e| t.lock().unwrap_or_else(PoisonError::into_inner).push(e));
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
            known,
            owners: HashMap::new(),
            writers: HashMap::new(),
            seq: 0,
            changed: HashMap::new(),
            touched,
            prefetch: inner.prefetch.get().cloned(),
            indexer: inner.indexer.get().cloned(),
            prefetched: HashSet::new(),
            issued: VecDeque::new(),
            render: RenderCache::default(),
            cfg: inner.cfg.clone(),
            ver: VerState::default(),
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

    /// 新出现的 client id 记到 who 名下（已经记过的不改）。
    fn record(&mut self, clients: impl IntoIterator<Item = u64>, who: &Principal) {
        let new: Vec<_> = clients
            .into_iter()
            .filter(|c| self.known.insert(*c))
            .map(|c| (c, who.kind, who.user_id.clone()))
            .collect();
        for (c, k, id) in &new {
            self.owners.insert(*c, (*k, id.clone()));
        }
        if !new.is_empty() {
            let _ = self.persist.send(Persist::Clients(new));
        }
    }

    /// 记下发出去的版本和当时的序号。
    fn issue_version(&mut self) -> Vec<u8> {
        self.after();
        let v = self.doc.version();
        if self.issued.len() >= ISSUED {
            self.issued.pop_front();
        }
        self.issued.push_back((hash_bytes(&v), self.seq));
        v
    }

    /// base 能不能当成当前版本：就是当前版本，或者要比对内容的块在那之后都没变过、定位用的块还在。
    fn base_fresh(&self, base: &[u8], t: &Targets) -> bool {
        let h = hash_bytes(base);
        let Some(&(_, s)) = self.issued.iter().rev().find(|(x, _)| *x == h) else {
            return false;
        };
        if s == self.seq {
            return true;
        }
        if t.all {
            return false;
        }
        t.content
            .iter()
            .all(|id| self.changed.get(id).is_none_or(|&c| c <= s) && self.doc.is_top_level(id))
            && t.refs.iter().all(|id| self.doc.has_block(id))
    }

    /// 从旧版本改用哪个 client id：它在那之后又写过就换一个新的，不然副本会从旧时钟接着编号，和已有的撞上。
    fn stale_writer_client(&mut self, who: &Principal, base: &[u8]) -> u64 {
        let cid = *self
            .writers
            .entry(who.user_id.clone())
            .or_insert_with(oad_doc::server_client_id);
        if oad_doc::version_clock(base, cid).unwrap_or(0) == self.doc.clock(cid) {
            return cid;
        }
        let fresh = oad_doc::server_client_id();
        self.writers.insert(who.user_id.clone(), fresh);
        fresh
    }

    fn edit_as<R>(
        &mut self,
        who: &Principal,
        base: Option<&[u8]>,
        targets: &Targets,
        f: impl FnOnce(&PageDoc) -> Result<R>,
    ) -> Result<R> {
        // 先把之前的改动同步进副本
        self.after();
        match base {
            Some(b) if !self.base_fresh(b, targets) => self.edit_at(who, b, f),
            _ => self.edit_direct(who, f),
        }
    }

    /// 直接在主文档上用写入者自己的 client id 改。
    fn edit_direct<R>(
        &mut self,
        who: &Principal,
        f: impl FnOnce(&PageDoc) -> Result<R>,
    ) -> Result<R> {
        let cid = *self
            .writers
            .entry(who.user_id.clone())
            .or_insert_with(oad_doc::server_client_id);
        let r = self.doc.as_client(cid, f)?;
        self.record([cid], who);
        self.ver_touch(who);
        Ok(r)
    }

    /// 从旧版本 base 复制一份再改（要改的块在那之后被别人动过时）。
    fn edit_at<R>(
        &mut self,
        who: &Principal,
        base: &[u8],
        f: impl FnOnce(&PageDoc) -> Result<R>,
    ) -> Result<R> {
        let cid = self.stale_writer_client(who, base);
        let fork = self
            .doc
            .fork_at(base, cid)
            .map_err(|e| Error::BadRequest(format!("base 版本不对：{e}")))?;
        let sv = fork.state_vector();
        let r = f(&fork)?;
        let diff = fork.encode_diff(&sv)?;
        self.doc.apply_update(&diff)?;
        self.record([cid], who);
        self.ver_touch(who);
        Ok(r)
    }

    fn join(&mut self, conn: u64, readonly: bool, who: Principal, out: mpsc::Sender<Bytes>) {
        self.conns.insert(
            conn,
            ConnState {
                out,
                readonly,
                who,
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
        let Some((readonly, who)) = self.conns.get(&conn).map(|c| (c.readonly, c.who.clone()))
        else {
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
                    // 时钟变大的 client 就是这次写字的人
                    let before = self.doc.ydoc().transact().state_vector();
                    if let Err(e) = self.doc.apply_update_from(&u, conn) {
                        warn!(page = %self.page_id, conn, "客户端增量合并失败：{e}");
                    }
                    let after = self.doc.ydoc().transact().state_vector();
                    let grown: Vec<u64> = after
                        .iter()
                        .filter(|(c, k)| before.get(c) < **k)
                        .map(|(c, _)| c.get())
                        .collect();
                    if !grown.is_empty()
                        || !self
                            .updates
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .is_empty()
                    {
                        self.ver_touch(&who);
                    }
                    self.record(grown, &who);
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

    /// 每条命令之后：把新增量广播出去，排进待存；改得够多了存一版。
    fn after(&mut self) {
        self.drain();
        let now = Instant::now();
        if self.ver.dirty
            && (self.ver.bytes >= self.cfg.version_max_bytes
                || self
                    .ver
                    .first
                    .is_some_and(|f| f + self.cfg.version_max_age <= now))
        {
            self.cut_version("auto", None, None);
        }
    }

    /// 把新增量广播出去，排进待存。
    fn drain(&mut self) {
        let ups = std::mem::take(&mut *self.updates.lock().unwrap_or_else(PoisonError::into_inner));
        let touched =
            std::mem::take(&mut *self.touched.lock().unwrap_or_else(PoisonError::into_inner));
        if ups.is_empty() && touched.is_empty() {
            return;
        }
        self.seq += 1;
        for e in touched {
            match e {
                oad_doc::BlockEvent::Changed(Some(id)) => {
                    self.changed.insert(id, self.seq);
                }
                oad_doc::BlockEvent::Changed(None) => {}
                // 单独一行的网址：后台先解析一次
                oad_doc::BlockEvent::Embed(url) => {
                    if let Some(tx) = &self.prefetch
                        && oad_markdown::is_bare_url(&url)
                        && self.prefetched.len() < 10_000
                        && self.prefetched.insert(url.clone())
                    {
                        let _ = tx.send(url);
                    }
                }
            }
        }
        let now = Instant::now();
        for (origin, u) in ups {
            let msg = encode(&Message::Sync(SyncMessage::Update(u.clone())));
            self.broadcast(&msg, origin);
            self.ver.bytes += u.len();
            self.pending.push(u);
        }
        self.ver.dirty = true;
        self.ver.last = Some(now);
        self.ver.first.get_or_insert(now);
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
            if let Some(tx) = &self.indexer {
                let _ = tx.send(self.page_id.clone());
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

    fn ver_touch_if_empty(&mut self, who: &Principal) {
        if self.ver.authors.is_empty() {
            self.ver_touch(who);
        }
    }

    /// 记下这一版里改过的人。
    fn ver_touch(&mut self, who: &Principal) {
        if !self.ver.authors.iter().any(|a| a.id == who.user_id) {
            self.ver.authors.push(VersionAuthor {
                kind: who.kind,
                id: who.user_id.clone(),
                name: who.name.clone(),
            });
        }
    }

    /// 存一版（上一版之后没改过就不存，除非是手动 / 恢复）。先把增量交给存盘任务，快照才对得上。
    fn cut_version(
        &mut self,
        kind: &str,
        label: Option<String>,
        by: Option<&Principal>,
    ) -> Option<oneshot::Receiver<i64>> {
        if !self.ver.dirty && matches!(kind, "auto" | "agent") {
            return None;
        }
        self.drain();
        self.flush();
        let v = std::mem::take(&mut self.ver);
        let (tx, rx) = oneshot::channel();
        let nv = NewVersion {
            kind: kind.to_string(),
            label,
            authors: v.authors,
            created_by: by.map(|p| p.user_id.clone()),
            snapshot: self.doc.version(),
            state_vector: self.doc.state_vector(),
        };
        let _ = self.persist.send(Persist::Version(nv, Some(tx)));
        Some(rx)
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
        let ver = self
            .ver
            .dirty
            .then(|| self.ver.last.map(|l| l + self.cfg.version_idle))
            .flatten();
        [
            self.flush_at,
            self.presence.values().map(|p| p.clear_at).min(),
            idle,
            ver,
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
                who,
                out,
                ready,
            } => {
                self.join(conn, readonly, who, out);
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
        self.guarded(|a| {
            a.cut_version("auto", None, None);
            a.flush();
        });
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
                    if a.ver.dirty && a.ver.last.is_some_and(|l| l + a.cfg.version_idle <= now) {
                        a.cut_version("auto", None, None);
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
    use yrs::updates::encoder::Encode;

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

    /// 模拟一个浏览器连接：自己的文档 + 把收到的增量合并进来。
    struct Browser {
        doc: PageDoc,
        conn: Conn,
    }

    impl Browser {
        async fn join(core: &Core, page: &str, who: &Principal, client: u64) -> Browser {
            let conn = core.hub.join(page, false, who.clone()).await.unwrap();
            let state = core.hub.read(page, |d| d.encode_state()).await.unwrap();
            Browser {
                doc: PageDoc::from_update(client, &state).unwrap(),
                conn,
            }
        }
        /// 收下服务器发来的增量。
        fn pull(&mut self) {
            while let Ok(m) = self.conn.rx.try_recv() {
                let mut dec = DecoderV1::new(Cursor::new(&m));
                for msg in MessageReader::new(&mut dec).flatten() {
                    if let Message::Sync(SyncMessage::Update(u) | SyncMessage::SyncStep2(u)) = msg {
                        self.doc.apply_update(&u).unwrap();
                    }
                }
            }
        }
        /// 在编号为 id 的块开头打字，发给服务器。
        async fn type_into(&mut self, id: &str, text: &str) {
            self.pull();
            let sv = self.doc.state_vector();
            let b = self.doc.block(id).unwrap();
            let t = format!("{text}{}", b.plain_text());
            self.doc.replace_block(id, &md(&t)).unwrap();
            let u = self.doc.encode_diff(&sv).unwrap();
            let msg = Message::Sync(SyncMessage::Update(u)).encode_v1();
            assert!(self.conn.handle.send(msg).await);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn server_writers_interleave_with_browser() {
        let dir = tempfile::tempdir().unwrap();
        let (storage, core, who) = setup(dir.path(), HubConfig::default()).await;
        let page = new_page(&core, &who, "p").await;
        let text: String = (0..20).map(|i| format!("第 {i} 段\n\n")).collect();
        core.set_content(&page, &who, oad_markdown::parse(&text), None)
            .await
            .unwrap();
        let (a1, _, _) = core
            .create_agent(
                &core.default_workspace.id,
                &who,
                "甲",
                None,
                crate::Scope::Write,
                None,
            )
            .await
            .unwrap();
        let (a2, _, _) = core
            .create_agent(
                &core.default_workspace.id,
                &who,
                "乙",
                None,
                crate::Scope::Write,
                None,
            )
            .await
            .unwrap();
        let p1 = Principal {
            user_id: a1.id.clone(),
            name: a1.name.clone(),
            kind: PrincipalKind::Agent,
        };
        let p2 = Principal {
            user_id: a2.id.clone(),
            name: a2.name.clone(),
            kind: PrincipalKind::Agent,
        };
        let mut b = Browser::join(&core, &page, &who, 7).await;
        let ids: Vec<String> = core
            .content(&page, &who)
            .await
            .unwrap()
            .0
            .content
            .iter()
            .map(|n| n.attr_str("id").unwrap().to_string())
            .collect();
        for round in 0..15 {
            // 两个 Agent 读了版本以后，人在别的块（有时候是同一块）里打字，然后 Agent 带着旧版本改
            let (_, base1) = core.block(&page, &ids[round], &p1).await.unwrap();
            let base2 = core.version(&page, &p2).await.unwrap();
            // 每三轮一次打在甲马上要改的那块里（甲的 base 里没有这些字，得留着）；别的时候打在没人改的块里
            let human_block = if round % 3 == 0 {
                round
            } else {
                15 + round % 5
            };
            b.type_into(&ids[human_block], &format!("人{round}")).await;
            tokio::time::sleep(Duration::from_millis(5)).await;
            core.replace_block(
                &page,
                &ids[round],
                &p1,
                md(&format!("甲第 {round} 次")),
                Some(base1),
            )
            .await
            .unwrap();
            core.edit_blocks(
                &page,
                &p2,
                Some(base2),
                vec![crate::BlockOp::InsertAfter {
                    block: ids[19 - round].clone(),
                    markdown: format!("乙插的 {round}"),
                }],
            )
            .await
            .unwrap();
            // 一步出错：都不改，副本也不能留着脏数据
            let before = core.content(&page, &p1).await.unwrap().0;
            assert!(
                core.edit_blocks(
                    &page,
                    &p1,
                    None,
                    vec![
                        crate::BlockOp::Append {
                            markdown: "不该出现".into()
                        },
                        crate::BlockOp::Delete {
                            block: "nosuch".into()
                        },
                    ]
                )
                .await
                .is_err()
            );
            assert_eq!(core.content(&page, &p1).await.unwrap().0, before);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        b.pull();
        let server = core.content(&page, &who).await.unwrap().0;
        assert_eq!(b.doc.to_node(), server, "浏览器和服务器一致");
        let t = server.plain_text();
        for round in 0..15 {
            assert!(
                t.contains(&format!("甲第 {round} 次")) && t.contains(&format!("乙插的 {round}")),
                "{t}"
            );
            assert!(t.contains(&format!("人{round}")), "人打的字丢了：{t}");
        }
        assert!(!t.contains("不该出现"));
        // 每个 Agent 一直用同一个 client id（除非人改了它要改的那块，退回到从 base 复制）
        let clients = storage.doc_clients(&page).await.unwrap();
        let n2 = clients.iter().filter(|c| c.2 == a2.id).count();
        assert_eq!(n2, 1, "乙只插不改，一直走同步副本：{clients:?}");
        // 存下来的和内存里的一样
        drop(b);
        core.shutdown().await;
        let core = Core::open(storage.clone(), AuthConfig::None, HubConfig::default())
            .await
            .unwrap();
        assert_eq!(core.content(&page, &who).await.unwrap().0, server);
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
