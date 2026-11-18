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

use std::collections::HashSet;

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
use yrs::updates::encoder::Encode;
use yrs::{
    ClientID, Doc, OffsetKind, Options, ReadTxn, StateVector, Transact, Update, XmlFragmentRef,
};

pub const FIELD: &str = "default";

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

pub struct PageDoc {
    doc: Doc,
    frag: XmlFragmentRef,
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
        PageDoc { doc, frag }
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

    pub fn to_node(&self) -> Node {
        let txn = self.doc.transact();
        Node::new("doc").children(convert::read_children(&txn, &self.frag))
    }

    /// 编号为 id 的块。
    pub fn block(&self, id: &str) -> Option<Node> {
        let txn = self.doc.transact();
        edit::find(&txn, &self.frag, id).map(|(_, _, el)| convert::read_element(&txn, &el))
    }

    /// 文档里已有的所有编号（要读整页）。
    pub fn ids(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        ids::collect(&self.to_node(), &mut set);
        set
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

    pub fn apply_update(&self, update: &[u8]) -> Result<()> {
        let u = Update::decode_v1(update).map_err(|e| Error::Decode(e.to_string()))?;
        self.doc
            .transact_mut()
            .apply_update(u)
            .map_err(|e| Error::Apply(e.to_string()))
    }

    /// 把编号为 id 的块换成 nodes（Agent 改一段）。和原来对得上的节点就地改：沿用编号、只动变了的字。
    /// 新节点没有编号的会自动分配。
    pub fn replace_block(&self, id: &str, nodes: &[Node]) -> Result<()> {
        let mut txn = self.doc.transact_mut();
        let (parent, idx, el) =
            edit::find(&txn, &self.frag, id).ok_or_else(|| Error::NoBlock(id.into()))?;
        // 只认被替换的这一块里原有的编号，别的编号去掉重新分配（避免和页面别处重复，又不用扫整页）
        let mut existing = HashSet::new();
        ids::collect(&convert::read_element(&txn, &el), &mut existing);
        let nodes: Vec<Node> = nodes.iter().map(|n| ids::keep_only(n, &existing)).collect();
        edit::reconcile_range(&mut txn, &parent, idx, 1, &nodes, &mut existing);
        Ok(())
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
