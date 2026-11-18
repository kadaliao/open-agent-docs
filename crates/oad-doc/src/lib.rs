//! 一个页面的正文：Yrs 文档，结构和浏览器里 y-prosemirror（Tiptap Collaboration）一样，
//! 根是 XmlFragment "default"，节点是 XmlElement（标签 = 节点类型，属性 = attrs），一串文字是 XmlText（格式 = mark 名 → attrs）。
//! 位置按 UTF-16 计，和浏览器一致。
//!
//! 这里提供：和 [`Node`] 之间转换、按块编号读写（Agent 改一段只动变了的字，人同时在改也不冲突）、增量的编码和合并。
//! 不保存、不联网，那些在上层。
mod convert;
mod edit;
mod ids;

pub use ids::{assign_ids, is_block, new_id};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// 新插入的节点：去掉带来的编号，全部重新分配。编号 8 位随机（62^8 ≈ 2×10^14 种），不和整页比对。
fn fresh(nodes: &[Node]) -> Vec<Node> {
    let mut seen = HashSet::new();
    nodes
        .iter()
        .map(|n| {
            let mut n = ids::keep_only(n, &HashSet::new());
            assign_ids(&mut n, &mut seen);
            n
        })
        .collect()
}
pub use oad_markdown::{Mark, Node};

use thiserror::Error;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::{Encode, Encoder, EncoderV1};
use yrs::{
    ClientID, Doc, GetString, OffsetKind, Options, Origin, ReadTxn, Snapshot, StateVector, Text,
    TextRef, Transact, Update, XmlFragmentRef,
};

pub const FIELD: &str = "default";
/// 页面标题（纯文字），和正文在同一个文档里。
pub const TITLE: &str = "title";

/// 服务器和 Agent 写入用的 client id：在 [2^32, 2^53) 里随机取。
/// 浏览器的 Yjs 用 32 位随机数（`random.uint32()`），两段不重叠；每次加载换一个，不会和以前的自己撞上。
pub fn server_client_id() -> u64 {
    const LO: u64 = 1 << 32;
    const HI: u64 = 1 << 53;
    LO + rand::random::<u64>() % (HI - LO)
}

/// 版本（[`PageDoc::version`]）里某个 client 的时钟；版本格式不对返回 None。
pub fn version_clock(version: &[u8], client: u64) -> Option<u32> {
    let snap = Snapshot::decode_v1(version).ok()?;
    Some(snap.state_map.get(&ClientID::new(client)))
}

/// 是不是服务器段的 client id。
pub fn is_server_client(id: u64) -> bool {
    id >= 1 << 32
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("增量格式不对：{0}")]
    Decode(String),
    #[error("合并增量失败：{0}")]
    Apply(String),
    #[error("找不到块 {0}")]
    NoBlock(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// 版本得是这份文档自己出过的：每个 client 的时钟在 [1, 本地时钟] 里，删除集不超出版本里的时钟。
/// 不检查的话，Yrs 遇到时钟 0 会溢出 panic。
fn check_snapshot(snap: &Snapshot, local: &StateVector) -> Result<()> {
    let bad = |m: &str| Err(Error::Decode(format!("版本不对：{m}")));
    for (client, &clock) in snap.state_map.iter() {
        if clock == 0 || !local.contains_client(client) || clock > local.get(client) {
            return bad("时钟超出范围");
        }
    }
    for (client, ranges) in snap.delete_set.iter() {
        let max = snap.state_map.get(client);
        if ranges.iter().any(|r| r.start >= r.end || r.end > max) {
            return bad("删除集超出范围");
        }
    }
    Ok(())
}

/// [`PageDoc::observe_blocks`] 报告的事。
#[derive(Clone, Debug, PartialEq)]
pub enum BlockEvent {
    /// 这个顶层块变了（`None`：顶层有删除之类，说不清是哪块）。
    Changed(Option<String>),
    /// 顶层的 embed 块（新写的或改过的）的网址。
    Embed(String),
}

/// [`PageDoc::blocks_cached`] 用的缓存：元素的 Yrs ID → (读出来时的序号, 节点)。
#[derive(Default)]
pub struct RenderCache(HashMap<(u64, u32), (u64, Arc<Node>)>);

pub struct PageDoc {
    doc: Doc,
    frag: XmlFragmentRef,
    title: TextRef,
}

impl PageDoc {
    /// 空文档。`client` 是这一方的编号（服务器、每个 Agent 各用各的，别和浏览器的随机编号撞上）。
    /// 不回收删掉的内容（skip_gc），这样能按快照回到旧版本。
    pub fn new(client: u64) -> Self {
        let doc = Doc::with_options(Options {
            client_id: ClientID::new(client),
            offset_kind: OffsetKind::Utf16,
            skip_gc: true,
            ..Default::default()
        });
        let frag = doc.get_or_insert_xml_fragment(FIELD);
        let title = doc.get_or_insert_text(TITLE);
        PageDoc { doc, frag, title }
    }

    pub fn from_update(client: u64, update: &[u8]) -> Result<Self> {
        let d = PageDoc::new(client);
        d.apply_update(update)?;
        Ok(d)
    }

    /// 用一棵节点树建文档（节点要先有编号，见 [`assign_ids`]）。
    pub fn from_node(client: u64, doc: &Node) -> Self {
        let d = PageDoc::new(client);
        {
            let mut txn = d.doc.transact_mut();
            convert::write_children(&mut txn, &d.frag, 0, &doc.content);
        }
        d
    }

    pub fn ydoc(&self) -> &Doc {
        &self.doc
    }

    pub fn client_id(&self) -> u64 {
        self.doc.client_id().get()
    }

    pub fn title(&self) -> String {
        self.title.get_string(&self.doc.transact())
    }

    /// 改标题：只插删变了的字（别人同时在改标题也不丢字）。
    pub fn set_title(&self, title: &str) {
        let old: Vec<char> = self.title().chars().collect();
        let new: Vec<char> = title.chars().collect();
        if old == new {
            return;
        }
        let pre = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suf = old[pre..]
            .iter()
            .rev()
            .zip(new[pre..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let u16 = |cs: &[char]| cs.iter().map(|c| c.len_utf16() as u32).sum::<u32>();
        let at = u16(&old[..pre]);
        let mut txn = self.doc.transact_mut();
        let del = u16(&old[pre..old.len() - suf]);
        if del > 0 {
            self.title.remove_range(&mut txn, at, del);
        }
        let ins: String = new[pre..new.len() - suf].iter().collect();
        if !ins.is_empty() {
            self.title.insert(&mut txn, at, &ins);
        }
    }

    pub fn to_node(&self) -> Node {
        let txn = self.doc.transact();
        Node::new("doc").children(convert::read_children(&txn, &self.frag))
    }

    /// 订阅每次提交改到了哪些顶层块（新插进来的也算），以及顶层的 embed 块现在的网址。在提交时同步调用。
    pub fn observe_blocks(&self, f: impl Fn(BlockEvent) + Send + Sync + 'static) {
        use yrs::DeepObservable;
        use yrs::types::{Change, Event, PathSegment};
        fn report(txn: &yrs::TransactionMut, el: &yrs::XmlElementRef, f: &dyn Fn(BlockEvent)) {
            f(BlockEvent::Changed(edit::id_of(txn, el)));
            if el.tag().as_ref() == "embed"
                && let Some(yrs::Out::Any(yrs::Any::String(u))) =
                    yrs::Xml::get_attribute(el, txn, "url")
            {
                f(BlockEvent::Embed(u.to_string()));
            }
        }
        let frag = self.frag.clone();
        self.frag.observe_deep("blocks", move |txn, events| {
            for e in events.iter() {
                match e.path().front() {
                    Some(PathSegment::Index(i)) => {
                        if let Some(yrs::XmlOut::Element(el)) =
                            yrs::XmlFragment::get(&frag, txn, *i)
                        {
                            report(txn, &el, &f);
                        }
                    }
                    _ => {
                        if let Event::XmlFragment(xe) = e {
                            for ch in xe.delta(txn) {
                                if let Change::Added(outs) = ch {
                                    for o in outs {
                                        if let yrs::Out::YXmlElement(el) = o {
                                            report(txn, el, &f);
                                        }
                                    }
                                }
                            }
                        }
                        f(BlockEvent::Changed(None));
                    }
                }
            }
        });
    }

    /// 整页的顶层块，没变的直接用缓存里的（共享，不复制）。`changed(编号)` 是这块最后一次变时的序号，
    /// `now` 是现在的序号；缓存按元素本身（Yrs 的 ID）认，块被换成新元素时不会用错。
    pub fn blocks_cached(
        &self,
        cache: &mut RenderCache,
        changed: impl Fn(&str) -> u64,
        now: u64,
    ) -> Vec<Arc<Node>> {
        let txn = self.doc.transact();
        let mut next = HashMap::with_capacity(cache.0.len());
        let mut out = Vec::new();
        for c in yrs::XmlFragment::children(&self.frag, &txn) {
            match c {
                yrs::XmlOut::Element(el) => {
                    let branch: &yrs::branch::Branch = el.as_ref();
                    let key = match branch.id() {
                        yrs::branch::BranchID::Nested(id) => (id.client.get(), id.clock),
                        yrs::branch::BranchID::Root(_) => (0, 0),
                    };
                    let since = match yrs::Xml::get_attribute(&el, &txn, "id") {
                        Some(yrs::Out::Any(yrs::Any::String(id))) => changed(&id),
                        _ => u64::MAX,
                    };
                    let node = match cache.0.remove(&key) {
                        Some((at, n)) if since <= at => {
                            next.insert(key, (at, n.clone()));
                            n
                        }
                        _ => {
                            let n = Arc::new(convert::read_element(&txn, &el));
                            next.insert(key, (now, n.clone()));
                            n
                        }
                    };
                    out.push(node);
                }
                yrs::XmlOut::Text(t) => {
                    out.extend(convert::read_text(&txn, &t).into_iter().map(Arc::new))
                }
                yrs::XmlOut::Fragment(_) => {}
            }
        }
        cache.0 = next;
        out
    }

    /// 有没有编号为 id 的块（任何一层）。
    pub fn has_block(&self, id: &str) -> bool {
        edit::find(&self.doc.transact(), &self.frag, id).is_some()
    }

    /// 编号为 id 的块是不是顶层块。
    pub fn is_top_level(&self, id: &str) -> bool {
        let txn = self.doc.transact();
        yrs::XmlFragment::children(&self.frag, &txn).any(|c| match c {
            yrs::XmlOut::Element(el) => edit::id_of(&txn, &el).as_deref() == Some(id),
            _ => false,
        })
    }

    /// 编号为 id 的块。
    pub fn block(&self, id: &str) -> Option<Node> {
        let txn = self.doc.transact();
        edit::find(&txn, &self.frag, id).map(|(_, _, el)| convert::read_element(&txn, &el))
    }

    /// 文档里已有的所有编号（只读属性，不读文字）。
    pub fn ids(&self) -> HashSet<String> {
        let txn = self.doc.transact();
        yrs::XmlFragment::successors(&self.frag, &txn)
            .filter_map(|n| match n {
                yrs::XmlOut::Element(el) => edit::id_of(&txn, &el),
                _ => None,
            })
            .collect()
    }

    pub fn state_vector(&self) -> Vec<u8> {
        self.doc.transact().state_vector().encode_v1()
    }

    /// 整份状态（新客户端第一次同步、存库用）。
    pub fn encode_state(&self) -> Vec<u8> {
        self.doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default())
    }

    /// 对方缺的那部分（sv 是对方的 state vector）。
    pub fn encode_diff(&self, sv: &[u8]) -> Result<Vec<u8>> {
        let sv = StateVector::decode_v1(sv).map_err(|e| Error::Decode(e.to_string()))?;
        Ok(self.doc.transact().encode_diff_v1(&sv))
    }

    /// 当前版本（Yrs snapshot：state vector + delete set）。Agent 读的时候带上它，改的时候交回来，
    /// 服务器就能在这个版本上改（见 [`PageDoc::fork_at`]），期间别人打的字不会被当成要删的。
    pub fn version(&self) -> Vec<u8> {
        self.doc.transact().snapshot().encode_v1()
    }

    /// 回到 version 那一刻的一份副本，用新的 client id 写。在副本上改完，把 `encode_diff(副本改之前的 sv)`
    /// 合并回来，效果和一个从那个版本开始离线编辑的客户端一样。
    pub fn fork_at(&self, version: &[u8], client: u64) -> Result<PageDoc> {
        let snap = Snapshot::decode_v1(version).map_err(|e| Error::Decode(e.to_string()))?;
        let txn = self.doc.transact();
        check_snapshot(&snap, &txn.state_vector())?;
        let mut enc = EncoderV1::new();
        txn.encode_state_from_snapshot(&snap, &mut enc)
            .map_err(|e| Error::Decode(e.to_string()))?;
        drop(txn);
        PageDoc::from_update(client, &enc.to_vec())
    }

    pub fn apply_update(&self, update: &[u8]) -> Result<()> {
        let u = Update::decode_v1(update).map_err(|e| Error::Decode(e.to_string()))?;
        self.doc
            .transact_mut()
            .apply_update(u)
            .map_err(|e| Error::Apply(e.to_string()))
    }

    /// 合并增量并标上来源（同步中心据此不把增量发回给发来的那个连接）。
    pub fn apply_update_from(&self, update: &[u8], origin: impl Into<Origin>) -> Result<()> {
        let u = Update::decode_v1(update).map_err(|e| Error::Decode(e.to_string()))?;
        self.doc
            .transact_mut_with(origin)
            .apply_update(u)
            .map_err(|e| Error::Apply(e.to_string()))
    }

    /// 把编号为 id 的块换成 nodes（Agent 改一段）。和原来对得上的节点就地改：沿用编号、只动变了的字。
    /// 新节点没有编号的会自动分配。返回替换后这几块的编号。
    pub fn replace_block(&self, id: &str, nodes: &[Node]) -> Result<Vec<String>> {
        let mut txn = self.doc.transact_mut();
        let (parent, idx, el) =
            edit::find(&txn, &self.frag, id).ok_or_else(|| Error::NoBlock(id.into()))?;
        // 只认被替换的这一块里原有的编号，别的编号去掉重新分配（避免和页面别处重复，又不用扫整页）
        let mut existing = HashSet::new();
        ids::collect(&convert::read_element(&txn, &el), &mut existing);
        let nodes: Vec<Node> = nodes.iter().map(|n| ids::keep_only(n, &existing)).collect();
        edit::reconcile_range(&mut txn, &parent, idx, 1, &nodes, &mut existing);
        // 块级节点不会被合并，替换后的就是 [idx, idx + n)
        let n = convert::group(&nodes).len() as u32;
        Ok((idx..idx + n)
            .filter_map(|i| parent.element(&txn, i))
            .filter_map(|el| edit::id_of(&txn, &el))
            .collect())
    }

    /// 在编号为 after 的块后面插入；after 为 None 时插到最前面。
    pub fn insert_after(&self, after: Option<&str>, nodes: &[Node]) -> Result<Vec<String>> {
        let nodes = fresh(nodes);
        let mut txn = self.doc.transact_mut();
        let (parent, idx) = match after {
            None => (edit::Parent::Root(self.frag.clone()), 0),
            Some(a) => {
                let (p, i, _) =
                    edit::find(&txn, &self.frag, a).ok_or_else(|| Error::NoBlock(a.into()))?;
                (p, i + 1)
            }
        };
        parent.write(&mut txn, idx, &nodes);
        Ok(nodes
            .iter()
            .filter_map(|n| n.attr_str("id").map(str::to_string))
            .collect())
    }

    /// 在编号为 before 的块前面插入（同一层）。
    pub fn insert_before(&self, before: &str, nodes: &[Node]) -> Result<Vec<String>> {
        let nodes = fresh(nodes);
        let mut txn = self.doc.transact_mut();
        let (parent, idx, _) =
            edit::find(&txn, &self.frag, before).ok_or_else(|| Error::NoBlock(before.into()))?;
        parent.write(&mut txn, idx, &nodes);
        Ok(nodes
            .iter()
            .filter_map(|n| n.attr_str("id").map(str::to_string))
            .collect())
    }

    /// 顶层块的编号，按顺序。
    pub fn top_ids(&self) -> Vec<String> {
        let txn = self.doc.transact();
        yrs::XmlFragment::children(&self.frag, &txn)
            .filter_map(|c| match c {
                yrs::XmlOut::Element(el) => edit::id_of(&txn, &el),
                _ => None,
            })
            .collect()
    }

    /// 编号为 id 的块里现存的文字，按写它的 client id 数字数（算“这段是谁写的”）。
    pub fn block_authors(&self, id: &str) -> Option<HashMap<u64, usize>> {
        let mut txn = self.doc.transact_mut();
        let (_, _, el) = edit::find(&txn, &self.frag, id)?;
        let mut texts = Vec::new();
        fn walk<T: ReadTxn, F: yrs::XmlFragment>(txn: &T, p: &F, out: &mut Vec<yrs::XmlTextRef>) {
            for c in p.children(txn) {
                match c {
                    yrs::XmlOut::Element(el) => walk(txn, &el, out),
                    yrs::XmlOut::Text(t) => out.push(t),
                    yrs::XmlOut::Fragment(_) => {}
                }
            }
        }
        walk(&txn, &el, &mut texts);
        // 和空快照比：现存的每段字都是“新增”，带着写它的 id
        let now = txn.snapshot();
        let empty = Snapshot::default();
        let mut count: HashMap<u64, usize> = HashMap::new();
        for t in texts {
            for d in t.diff_range(&mut txn, Some(&now), Some(&empty), |c| c.id.client.get()) {
                if let (Some(client), yrs::Out::Any(yrs::Any::String(s))) = (d.ychange, &d.insert) {
                    *count.entry(client).or_default() += s.chars().count();
                }
            }
        }
        Some(count)
    }

    /// 这份文档里某个 client 的时钟（写过多少个单位）。
    pub fn clock(&self, client: u64) -> u32 {
        self.doc
            .transact()
            .state_vector()
            .get(&ClientID::new(client))
    }

    /// 加到最后。
    pub fn append(&self, nodes: &[Node]) -> Vec<String> {
        let nodes = fresh(nodes);
        let mut txn = self.doc.transact_mut();
        let len = yrs::XmlFragment::len(&self.frag, &txn);
        convert::write_children(&mut txn, &self.frag, len, &nodes);
        nodes
            .iter()
            .filter_map(|n| n.attr_str("id").map(str::to_string))
            .collect()
    }

    pub fn delete_block(&self, id: &str) -> Result<()> {
        let mut txn = self.doc.transact_mut();
        let (parent, idx, _) =
            edit::find(&txn, &self.frag, id).ok_or_else(|| Error::NoBlock(id.into()))?;
        parent.remove(&mut txn, idx, 1);
        Ok(())
    }

    /// 整页换成 doc（Agent 重写整页）：对得上的块沿用编号、只改变了的地方。
    pub fn set_content(&self, doc: &Node) {
        let mut existing = self.ids();
        let mut txn = self.doc.transact_mut();
        let len = yrs::XmlFragment::len(&self.frag, &txn);
        edit::reconcile_range(
            &mut txn,
            &edit::Parent::Root(self.frag.clone()),
            0,
            len,
            &doc.content,
            &mut existing,
        );
    }
}
