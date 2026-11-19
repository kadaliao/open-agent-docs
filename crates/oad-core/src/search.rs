//! 搜索：用 oad-search 的索引。后台任务收两路消息——同步中心存了增量的页面、工作区事件（页面建 / 改 / 删、库的行和表结构变了），
//! 攒 1 秒再改索引。页面按块增量改（记着每块的哈希，只换变了的）；库按 seq 补变了的行。
//! 启动时对比索引里的标记（每页的 updated_at、每个库的 seq）补漏掉的，不全量重建。
use crate::model::*;
use crate::storage::Storage;
use crate::{Core, Error, Node, PageDoc, Result};
use oad_db::schema::PropType;
use oad_search::{Doc, Index, Kind, Op, SearchQuery};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tracing::warn;

/// 攒多久再改索引。
pub const DEBOUNCE: Duration = Duration::from_secs(1);
/// 一次最多处理几个页面（之后提交一次）。
const BATCH: usize = 200;
/// 攒够这么多条索引改动也提交一次（大页面多的时候不在内存里堆几万段正文）。
const BATCH_OPS: usize = 5_000;

/// 一条搜索结果（GET /workspaces/{ws}/search）。
#[derive(Serialize, Clone, Debug, utoipa::ToSchema)]
pub struct SearchResult {
    pub page_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    pub title: String,
    pub snippet: String,
    /// page / database / row
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub score: f32,
    /// 这个页面命中了几处（标题、正文的段）
    pub blocks: usize,
    pub updated_at: i64,
    /// 行所在的库
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_id: Option<String>,
}

pub struct SearchService {
    pub index: Arc<Index>,
    tx: mpsc::UnboundedSender<Job>,
    /// 页面 → (块 → 内容哈希)：只换变了的块。没有的（刚启动）整页重建一次。
    blocks: Arc<Mutex<HashMap<String, HashMap<String, u64>>>>,
    /// 处理过几次页面（看启动时补了多少）。
    pub processed: std::sync::atomic::AtomicUsize,
}

#[derive(Debug)]
enum Job {
    Page(String),
    Db(String),
    /// 处理完前面所有的再回信（测试用）
    Flush(tokio::sync::oneshot::Sender<()>),
}

fn hash_str(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

impl SearchService {
    /// 打开索引（不给目录放内存里），起后台任务，补启动前漏掉的。
    pub fn start(
        dir: Option<&Path>,
        storage: Arc<dyn Storage>,
        events: broadcast::Receiver<crate::Event>,
        stop: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(Arc<SearchService>, mpsc::UnboundedSender<String>)> {
        let index = Arc::new(Index::open(dir).map_err(|e| Error::Internal(e.to_string()))?);
        let (tx, rx) = mpsc::unbounded_channel();
        let svc = Arc::new(SearchService {
            index,
            tx: tx.clone(),
            blocks: Arc::default(),
            processed: Default::default(),
        });
        // 同步中心那一路：页面 id
        let (ptx, prx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(run(svc.clone(), storage, rx, prx, events, Box::pin(stop)));
        Ok((svc, ptx))
    }

    pub fn touch_page(&self, id: &str) {
        let _ = self.tx.send(Job::Page(id.to_string()));
    }

    pub fn touch_db(&self, id: &str) {
        let _ = self.tx.send(Job::Db(id.to_string()));
    }

    /// 等前面排着的都进了索引（测试、关机前用）。
    pub async fn flush(&self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(Job::Flush(tx)).is_ok() {
            let _ = rx.await;
        }
    }
}

/// 节点里引用的本站上传文件（图片、文件卡片、链接）。
fn collect_files(n: &Node, out: &mut Vec<String>) {
    for k in ["src", "url", "href"] {
        if let Some(u) = n.attr_str(k)
            && let Some(id) = crate::media::own_file_id(u)
            && !out.contains(&id)
        {
            out.push(id);
        }
    }
    for m in &n.marks {
        if let Some(u) = m.attrs.get("href").and_then(|v| v.as_str())
            && let Some(id) = crate::media::own_file_id(u)
            && !out.contains(&id)
        {
            out.push(id);
        }
    }
    for c in &n.content {
        collect_files(c, out);
    }
}

/// 一个块的纯文字（表格、列表里的都算）。
fn block_text(n: &Node) -> String {
    n.plain_text()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 行的属性里能搜的文字：标题、文字、网址、邮箱、电话、选项名。
fn row_text(schema: &oad_db::schema::Schema, row: &DbRow) -> (String, String) {
    let mut title = String::new();
    let mut parts = Vec::new();
    for p in &schema.props {
        let Some(v) = row.props.get(&p.key) else {
            continue;
        };
        match p.kind {
            PropType::Title => title = v.as_str().unwrap_or("").to_string(),
            PropType::Text | PropType::Url | PropType::Email | PropType::Phone => {
                if let Some(s) = v.as_str() {
                    parts.push(s.to_string());
                }
            }
            PropType::Select | PropType::Status | PropType::MultiSelect => {
                let ids: Vec<&str> = match v {
                    serde_json::Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect(),
                    v => v.as_str().into_iter().collect(),
                };
                for id in ids {
                    if let Some(o) = p.options.iter().find(|o| o.id == id) {
                        parts.push(o.name.clone());
                    }
                }
            }
            _ => {}
        }
    }
    (title, parts.join(" · "))
}

struct Ctx {
    svc: Arc<SearchService>,
    storage: Arc<dyn Storage>,
}

impl Ctx {
    /// 一个页面：标题文档（行的话带属性）、正文各段。返回要做的改动和它的 updated_at（标记用）。
    async fn page_ops(&self, id: &str) -> Result<(Vec<Op>, Option<i64>)> {
        let page = match self.storage.page(id).await? {
            Some(p) if p.deleted_at.is_none() => p,
            other => {
                self.svc.blocks.lock().unwrap().remove(id);
                let mut ops = vec![Op::DeletePage(id.to_string())];
                if other.is_some_and(|p| p.kind == "database") {
                    ops.push(Op::DeleteDb(id.to_string()));
                }
                return Ok((ops, None));
            }
        };
        // 先记版本、再读正文：读到的只会更新
        let at = page.updated_at;
        let log = self.storage.load_doc(id).await?;
        let node = tokio::task::spawn_blocking(move || -> Result<Node> {
            let doc = PageDoc::new(oad_doc::server_client_id());
            if let Some(s) = &log.snapshot {
                doc.apply_update(s)?;
            }
            for u in &log.updates {
                doc.apply_update(u)?;
            }
            Ok(doc.to_node())
        })
        .await
        .map_err(|e| Error::Internal(e.to_string()))??;
        // 用了的上传文件（彻底删页面时据此清文件）
        let mut files = Vec::new();
        collect_files(&node, &mut files);
        if !files.is_empty() {
            self.storage.add_file_refs(id, files).await?;
        }
        let kind = if page.kind == "row" {
            Kind::Row
        } else {
            Kind::Page
        };
        let db_id = (page.kind == "row")
            .then(|| page.parent_id.clone())
            .flatten();
        let mut ops = Vec::new();
        // 标题（行：标题 + 属性）
        let (title, text) = match (&db_id, self.storage.db_row(id).await?) {
            (Some(db), Some(row)) => match self.storage.database(db).await? {
                Some(m) => row_text(&crate::dbengine::parse_schema(&m.schema)?, &row),
                None => (page.title.clone(), String::new()),
            },
            _ => (page.title.clone(), String::new()),
        };
        ops.push(Op::Put(Doc {
            page_id: id.into(),
            block_id: None,
            db_id: db_id.clone(),
            workspace_id: page.workspace_id.clone(),
            kind,
            title: if title.is_empty() {
                page.title.clone()
            } else {
                title
            },
            text,
        }));
        // 正文：按块，只换变了的
        let mut now: HashMap<String, u64> = HashMap::new();
        let mut blocks = Vec::new();
        for b in &node.content {
            let Some(bid) = b.attr_str("id") else {
                continue;
            };
            let t = block_text(b);
            if t.is_empty() {
                continue;
            }
            now.insert(bid.to_string(), hash_str(&t));
            blocks.push((bid.to_string(), t));
        }
        let prev = self.svc.blocks.lock().unwrap().remove(id);
        match &prev {
            None => {
                ops.insert(0, Op::DeletePage(id.into()));
            }
            Some(old) => {
                for k in old.keys().filter(|k| !now.contains_key(*k)) {
                    ops.push(Op::DeleteBlock(id.into(), k.clone()));
                }
            }
        }
        for (bid, t) in blocks {
            if prev.as_ref().and_then(|o| o.get(&bid)) == Some(&now[&bid]) {
                continue;
            }
            ops.push(Op::Put(Doc {
                page_id: id.into(),
                block_id: Some(bid),
                db_id: db_id.clone(),
                workspace_id: page.workspace_id.clone(),
                kind,
                title: String::new(),
                text: t,
            }));
        }
        self.svc.blocks.lock().unwrap().insert(id.to_string(), now);
        Ok((ops, Some(at)))
    }

    /// 库里 since 之后变了的行（since 为 None 是全部）：只换标题文档（行的正文由页面那一路管）。
    async fn db_ops(&self, db: &str, all: bool) -> Result<(Vec<Op>, Option<i64>)> {
        let Some(page) = self.storage.page(db).await? else {
            return Ok((vec![Op::DeleteDb(db.into())], None));
        };
        let Some(meta) = self.storage.database(db).await? else {
            return Ok((vec![], None));
        };
        if page.deleted_at.is_some() {
            return Ok((vec![Op::DeleteDb(db.into())], None));
        }
        let since = if all {
            None
        } else {
            self.svc.index.marks().dbs.get(db).copied()
        };
        let (seq, rows, deleted) = self.storage.db_rows(db, since).await?;
        let schema = crate::dbengine::parse_schema(&meta.schema)?;
        let mut ops = Vec::new();
        for r in rows {
            let (title, text) = row_text(&schema, &r);
            ops.push(Op::Put(Doc {
                page_id: r.id.clone(),
                block_id: None,
                db_id: Some(db.into()),
                workspace_id: page.workspace_id.clone(),
                kind: Kind::Row,
                title,
                text,
            }));
        }
        for id in deleted {
            self.svc.blocks.lock().unwrap().remove(&id);
            ops.push(Op::DeletePage(id));
        }
        Ok((ops, Some(seq)))
    }

    /// 处理攒着的页面和库，分批提交。
    async fn process(&self, pages: HashSet<String>, dbs: HashMap<String, bool>) {
        let n = pages.len();
        let mut ops = Vec::new();
        let mut marks: Vec<(String, Option<i64>)> = Vec::new();
        for (i, id) in pages.into_iter().enumerate() {
            self.svc
                .processed
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            match self.page_ops(&id).await {
                Ok((o, m)) => {
                    ops.extend(o);
                    marks.push((id, m));
                }
                Err(e) => warn!(page = %id, "索引页面失败：{e}"),
            }
            if marks.len() >= BATCH || ops.len() >= BATCH_OPS || i + 1 == n {
                let marks = std::mem::take(&mut marks);
                self.commit(std::mem::take(&mut ops), move |m| {
                    for (id, at) in marks {
                        match at {
                            Some(a) => m.pages.insert(id, a),
                            None => m.pages.remove(&id),
                        };
                    }
                })
                .await;
            }
        }
        for (db, all) in dbs {
            match self.db_ops(&db, all).await {
                Ok((ops, seq)) => {
                    self.commit(ops, move |m| {
                        match seq {
                            Some(s) => m.dbs.insert(db, s),
                            None => m.dbs.remove(&db),
                        };
                    })
                    .await
                }
                Err(e) => warn!(db = %db, "索引数据库失败：{e}"),
            }
        }
    }

    async fn commit(
        &self,
        ops: Vec<Op>,
        marks: impl FnOnce(&mut oad_search::Marks) + Send + 'static,
    ) {
        let ix = self.svc.index.clone();
        let r = tokio::task::spawn_blocking(move || ix.apply(ops, marks)).await;
        match r {
            Ok(Ok(())) => crate::mem::release_soon(),
            Ok(Err(e)) => warn!("写搜索索引失败：{e}"),
            Err(e) => warn!("写搜索索引失败：{e}"),
        }
    }

    /// 启动时：和库里对比，找出要补的页面和库。
    async fn catch_up(&self) -> Result<(HashSet<String>, HashMap<String, bool>)> {
        let marks = self.svc.index.marks();
        let mut pages = HashSet::new();
        let mut dbs = HashMap::new();
        let mut live = HashSet::new();
        for p in self.storage.all_pages().await? {
            if p.deleted_at.is_some() {
                if marks.pages.contains_key(&p.id) {
                    pages.insert(p.id.clone());
                }
                if p.kind == "database" && marks.dbs.contains_key(&p.id) {
                    dbs.insert(p.id.clone(), false);
                }
                continue;
            }
            live.insert(p.id.clone());
            if marks.pages.get(&p.id) != Some(&p.updated_at) {
                pages.insert(p.id.clone());
            }
        }
        for (db, seq) in self.storage.db_seqs().await? {
            if marks.dbs.get(&db) != Some(&seq) {
                dbs.insert(db, false);
            }
        }
        // 彻底删掉了的
        for id in marks.pages.keys().filter(|k| !live.contains(*k)) {
            pages.insert(id.clone());
        }
        for id in marks.dbs.keys().filter(|k| !live.contains(*k)) {
            dbs.insert(id.clone(), false);
        }
        Ok((pages, dbs))
    }
}

async fn run(
    svc: Arc<SearchService>,
    storage: Arc<dyn Storage>,
    mut rx: mpsc::UnboundedReceiver<Job>,
    mut prx: mpsc::UnboundedReceiver<String>,
    mut events: broadcast::Receiver<crate::Event>,
    mut stop: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
) {
    let ctx = Ctx { svc, storage };
    let mut pages: HashSet<String> = HashSet::new();
    let mut dbs: HashMap<String, bool> = HashMap::new();
    match ctx.catch_up().await {
        Ok((p, d)) => {
            pages = p;
            dbs = d;
        }
        Err(e) => warn!("搜索索引补漏失败：{e}"),
    }
    let mut due: Option<tokio::time::Instant> =
        (!pages.is_empty() || !dbs.is_empty()).then(tokio::time::Instant::now);
    let mut waiters: Vec<tokio::sync::oneshot::Sender<()>> = Vec::new();
    loop {
        let wake = async {
            match due {
                Some(d) => tokio::time::sleep_until(d).await,
                None => std::future::pending().await,
            }
        };
        let mark = |due: &mut Option<tokio::time::Instant>| {
            due.get_or_insert_with(|| tokio::time::Instant::now() + DEBOUNCE);
        };
        tokio::select! {
            _ = &mut stop => return,
            job = rx.recv() => match job {
                Some(Job::Page(p)) => { pages.insert(p); mark(&mut due); }
                Some(Job::Db(d)) => { dbs.entry(d).or_insert(false); mark(&mut due); }
                Some(Job::Flush(w)) => {
                    // 先把已经到了的都收进来
                    while let Ok(p) = prx.try_recv() {
                        pages.insert(p);
                    }
                    while let Ok(e) = events.try_recv() {
                        on_event(&e, &mut pages, &mut dbs);
                    }
                    waiters.push(w);
                    due = Some(tokio::time::Instant::now());
                }
                None => return,
            },
            p = prx.recv() => match p {
                Some(p) => { pages.insert(p); mark(&mut due); }
                None => return,
            },
            ev = events.recv() => match ev {
                Ok(e) => {
                    if on_event(&e, &mut pages, &mut dbs) {
                        mark(&mut due);
                    }
                }
                // 落下了事件：再对比一遍
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if let Ok((p, d)) = ctx.catch_up().await {
                        pages.extend(p);
                        dbs.extend(d);
                        mark(&mut due);
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return,
            },
            _ = wake => {
                due = None;
                let p = std::mem::take(&mut pages);
                let d = std::mem::take(&mut dbs);
                ctx.process(p, d).await;
                for w in waiters.drain(..) {
                    let _ = w.send(());
                }
            }
        }
    }
}

/// 工作区事件 → 要重新索引的页面、库。返回有没有。
fn on_event(
    e: &crate::Event,
    pages: &mut HashSet<String>,
    dbs: &mut HashMap<String, bool>,
) -> bool {
    let id = e.data["id"].as_str().map(str::to_string);
    let db = e.data["db_id"].as_str().map(str::to_string);
    match (e.name, id, db) {
        (
            "page.created" | "page.updated" | "page.moved" | "page.deleted" | "page.restored",
            Some(id),
            _,
        ) => {
            if e.data["kind"] == "database" {
                dbs.insert(id.clone(), e.name == "page.restored");
            }
            pages.insert(id);
            true
        }
        ("db.changed", _, Some(db)) => {
            dbs.entry(db).or_insert(false);
            true
        }
        ("db.schema", _, Some(db)) => {
            dbs.insert(db, true);
            true
        }
        _ => false,
    }
}

impl Core {
    /// 搜：按工作区；kind 是 page / row / all。标题、更新时间取库里现在的（索引里可能还没跟上的删掉的页面不给）。
    pub async fn search_index(
        &self,
        workspace_id: &str,
        who: &Principal,
        q: &str,
        kind: Option<&str>,
        limit: usize,
        per_page: bool,
    ) -> Result<Vec<SearchResult>> {
        self.require(workspace_id, who, false).await?;
        let q = q.trim();
        if q.is_empty() {
            return Err(Error::BadRequest("要给搜索词".into()));
        }
        let kind = match kind.unwrap_or("all") {
            "all" | "" => None,
            "page" => Some(Kind::Page),
            "row" => Some(Kind::Row),
            k => {
                return Err(Error::BadRequest(format!(
                    "kind 只能是 page、row、all，不是 {k}"
                )));
            }
        };
        let ix = self.search.index.clone();
        let (ws, text) = (workspace_id.to_string(), q.to_string());
        let limit = limit.clamp(1, 100);
        let hits = tokio::task::spawn_blocking(move || {
            ix.search(&SearchQuery {
                workspace_id: &ws,
                text: &text,
                kind,
                limit,
                per_page,
            })
        })
        .await
        .map_err(|e| Error::Internal(e.to_string()))?
        .map_err(|e| Error::Internal(e.to_string()))?;
        let mut out = Vec::with_capacity(hits.len());
        for h in hits {
            let Some(p) = self.storage.page(&h.page_id).await? else {
                continue;
            };
            if p.deleted_at.is_some() || p.workspace_id != workspace_id {
                continue;
            }
            out.push(SearchResult {
                db_id: (p.kind == "row").then(|| p.parent_id.clone()).flatten(),
                page_id: h.page_id,
                block_id: h.block_id,
                title: p.title,
                snippet: h.snippet,
                kind: if p.kind == "database" {
                    "database"
                } else {
                    h.kind.as_str()
                },
                icon: p.icon.clone(),
                score: h.score,
                blocks: h.matches,
                updated_at: p.updated_at,
            });
        }
        Ok(out)
    }
}
