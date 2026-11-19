//! 页面的历史版本：列表、看某一版、和别的版本 / 现在对比（按块的增删改，谁改的）、恢复（一次新的改动）、手动存一版。
//! 自动存版本的规则在同步中心（hub）里：停下 2 分钟、一直在改 10 分钟或攒了 64KB、Agent 的每次写入、卸载前。
use crate::model::*;
use crate::{Core, Error, Node, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// 对比里的一块。
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct BlockDiff {
    /// added / removed / changed / same
    pub op: &'static str,
    #[serde(rename = "id")]
    pub block_id: String,
    /// 块的类型（paragraph、heading…）
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// 这块里在两版之间写了字的人
    #[serde(rename = "by", skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<VersionAuthor>,
}

/// 按块对比：结果按新的顺序，删掉的块放在它原来前面那块后面。
pub fn diff_blocks(a: &Node, b: &Node) -> Vec<BlockDiff> {
    let md = |n: &Node| oad_markdown::serialize_block(n).trim_end().to_string();
    let ids = |n: &Node| -> Vec<(String, String, String)> {
        n.content
            .iter()
            .filter_map(|x| {
                x.attr_str("id")
                    .map(|i| (i.to_string(), md(x), x.ty.clone()))
            })
            .collect()
    };
    let (av, bv) = (ids(a), ids(b));
    let amap: HashMap<&str, &str> = av
        .iter()
        .map(|(i, m, _)| (i.as_str(), m.as_str()))
        .collect();
    let bset: HashSet<&str> = bv.iter().map(|(i, _, _)| i.as_str()).collect();
    // 删掉的块：挂在 a 里它前面最近的、b 里还在的块后面（None 是最前面）
    let mut removed_after: HashMap<Option<&str>, Vec<&(String, String, String)>> = HashMap::new();
    let mut prev: Option<&str> = None;
    for x in &av {
        if bset.contains(x.0.as_str()) {
            prev = Some(x.0.as_str());
        } else {
            removed_after.entry(prev).or_default().push(x);
        }
    }
    let mut out = Vec::new();
    let push_removed = |out: &mut Vec<BlockDiff>, at: Option<&str>| {
        for (i, m, t) in removed_after.get(&at).into_iter().flatten() {
            out.push(BlockDiff {
                op: "removed",
                block_id: i.clone(),
                ty: t.clone(),
                before: Some(m.clone()),
                after: None,
                authors: vec![],
            });
        }
    };
    push_removed(&mut out, None);
    for (i, m, t) in &bv {
        let (op, before, after) = match amap.get(i.as_str()) {
            None => ("added", None, Some(m.clone())),
            Some(old) if *old != m => ("changed", Some(old.to_string()), Some(m.clone())),
            Some(_) => ("same", None, None),
        };
        out.push(BlockDiff {
            op,
            block_id: i.clone(),
            ty: t.clone(),
            before,
            after,
            authors: vec![],
        });
        push_removed(&mut out, Some(i.as_str()));
    }
    out
}

impl Core {
    pub async fn list_versions(
        &self,
        page_id: &str,
        who: &Principal,
        limit: usize,
    ) -> Result<Vec<VersionMeta>> {
        self.page_for(page_id, who, false).await?;
        self.hub.settle_if_loaded(page_id).await;
        self.storage.versions(page_id, limit.clamp(1, 1000)).await
    }

    async fn version_row(&self, page_id: &str, v: i64) -> Result<(VersionMeta, Vec<u8>)> {
        self.hub.settle_if_loaded(page_id).await;
        self.storage
            .version(page_id, v)
            .await?
            .ok_or_else(|| Error::NotFound(format!("没有第 {v} 版")))
    }

    /// 某一版的内容。
    pub async fn version_content(
        &self,
        page_id: &str,
        v: i64,
        who: &Principal,
    ) -> Result<(VersionMeta, Node)> {
        self.page_for(page_id, who, false).await?;
        let (meta, snap) = self.version_row(page_id, v).await?;
        let node = self
            .hub
            .read(page_id, move |d| d.fork_at(&snap, 0).map(|f| f.to_node()))
            .await?
            .map_err(|e| Error::Internal(format!("读不出这一版：{e}")))?;
        Ok((meta, node))
    }

    /// 第 v 版和 to（另一版；None 是现在）按块对比。改了的块带上在两版之间写过字的人。
    pub async fn version_diff(
        &self,
        page_id: &str,
        v: i64,
        to: Option<i64>,
        who: &Principal,
    ) -> Result<Vec<BlockDiff>> {
        self.page_for(page_id, who, false).await?;
        let (_, from_snap) = self.version_row(page_id, v).await?;
        let to_snap = match to {
            Some(t) => Some(self.version_row(page_id, t).await?.1),
            None => None,
        };
        // 在同步中心里算：两版的内容，和新那版里每块的作者（client id → 字数）
        let (a, b, authors) = self
            .hub
            .read(
                page_id,
                move |d| -> std::result::Result<_, oad_doc::Error> {
                    let a = d.fork_at(&from_snap, 0)?.to_node();
                    let later = match &to_snap {
                        Some(s) => Some(d.fork_at(s, 0)?),
                        None => None,
                    };
                    let bd = later.as_ref().unwrap_or(d);
                    let b = bd.to_node();
                    let mut authors: HashMap<String, Vec<u64>> = HashMap::new();
                    for x in &b.content {
                        let Some(id) = x.attr_str("id") else { continue };
                        if let Some(m) = bd.block_authors(id) {
                            // 只算在旧版之后写过的
                            let grew: Vec<u64> = m
                                .keys()
                                .copied()
                                .filter(|c| {
                                    bd.clock(*c)
                                        > oad_doc::version_clock(&from_snap, *c).unwrap_or(0)
                                })
                                .collect();
                            authors.insert(id.to_string(), grew);
                        }
                    }
                    Ok((a, b, authors))
                },
            )
            .await?
            .map_err(|e| Error::Internal(format!("读不出这一版：{e}")))?;
        let mut clients: HashMap<u64, VersionAuthor> = self
            .storage
            .doc_clients(page_id)
            .await?
            .into_iter()
            .map(|(c, kind, id, name)| (c, VersionAuthor { kind, id, name }))
            .collect();
        // 刚写的还没存进库的
        for (c, (kind, id)) in self.hub.recent_owners(page_id).await {
            if let std::collections::hash_map::Entry::Vacant(e) = clients.entry(c) {
                let name = self
                    .storage
                    .user(&id)
                    .await?
                    .map(|u| u.name)
                    .unwrap_or_default();
                e.insert(VersionAuthor { kind, id, name });
            }
        }
        let mut out = diff_blocks(&a, &b);
        for d in out
            .iter_mut()
            .filter(|d| matches!(d.op, "added" | "changed"))
        {
            for c in authors.get(&d.block_id).into_iter().flatten() {
                if let Some(p) = clients.get(c)
                    && !d.authors.iter().any(|x| x.id == p.id)
                {
                    d.authors.push(p.clone());
                }
            }
        }
        Ok(out)
    }

    /// 恢复到第 v 版：整页换成那一版的内容（一次新的改动，之后的历史都还在），存一版 restore（名字按 lang）。返回新的那一版。
    pub async fn restore_version(
        &self,
        page_id: &str,
        v: i64,
        who: &Principal,
        lang: crate::i18n::Lang,
    ) -> Result<VersionMeta> {
        self.page_for(page_id, who, true).await?;
        let (meta, node) = self.version_content(page_id, v, who).await?;
        let vid = self
            .hub
            .restore(
                page_id,
                who.clone(),
                node,
                self.t(lang, "恢复到第 {v} 版", &[("v", &meta.id.to_string())]),
            )
            .await?;
        self.storage
            .version(page_id, vid)
            .await?
            .map(|x| x.0)
            .ok_or_else(|| Error::Internal("存版本失败".into()))
    }

    /// 手动存一版（带名字）。
    pub async fn save_version(
        &self,
        page_id: &str,
        who: &Principal,
        label: Option<String>,
    ) -> Result<VersionMeta> {
        self.page_for(page_id, who, true).await?;
        let label = label
            .map(|l| l.trim().chars().take(100).collect::<String>())
            .filter(|l| !l.is_empty());
        let vid = self
            .hub
            .save_version(page_id, "manual", label, who.clone())
            .await?;
        self.storage
            .version(page_id, vid)
            .await?
            .map(|x| x.0)
            .ok_or_else(|| Error::Internal("存版本失败".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(md: &str, ids: &[&str]) -> Node {
        let mut n = oad_markdown::parse(md);
        for (b, id) in n.content.iter_mut().zip(ids) {
            b.attrs
                .insert("id".into(), serde_json::Value::String(id.to_string()));
        }
        n
    }

    #[test]
    fn block_diff() {
        let a = doc("一\n\n二\n\n三\n\n四", &["a", "b", "c", "d"]);
        let b = doc("零\n\n一\n\n二改了\n\n四", &["z", "a", "b", "d"]);
        let d: Vec<(&str, String)> = diff_blocks(&a, &b)
            .into_iter()
            .map(|x| (x.op, x.block_id))
            .collect();
        assert_eq!(
            d,
            vec![
                ("added", "z".to_string()),
                ("same", "a".into()),
                ("changed", "b".into()),
                ("removed", "c".into()),
                ("same", "d".into())
            ]
        );
    }
}
