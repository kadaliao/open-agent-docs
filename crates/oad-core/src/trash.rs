//! 回收站：列出单独删的页面、库、行；放回来（连同一起删的子页面）；彻底删（正文、版本、评论、行、索引、没人用的上传文件）；
//! 后台每天清掉放了 30 天以上的。
use crate::events::{Event, EventKind};
use crate::media::THUMB_WIDTHS;
use crate::model::*;
use crate::{Core, Error, Result};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

/// 回收站里放多久（天）。
pub const TRASH_DAYS: i64 = 30;

#[derive(Serialize, Clone, Debug, utoipa::ToSchema)]
pub struct TrashItem {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub kind: String,
    pub deleted_at: i64,
    /// 谁删的；没记是 null
    #[schema(value_type = Option<crate::comments::AuthorRef>)]
    pub deleted_by: Option<serde_json::Value>,
    /// 原来在哪个页面下（行是所在的库）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_title: Option<String>,
    pub parent_id: Option<String>,
}

impl Core {
    /// 人员引用（`user:<id>` / `agent:<id>`）→ `{ kind, id, name }`。
    pub(crate) async fn person_json(&self, r: &str) -> Option<serde_json::Value> {
        let (kind, id) = r.split_once(':')?;
        let name = self
            .storage
            .user(id)
            .await
            .ok()
            .flatten()
            .map(|u| u.name)
            .unwrap_or_default();
        Some(serde_json::json!({ "kind": kind, "id": id, "name": name }))
    }

    /// 回收站（新删的在前），翻页：cursor 是上一页给的 next_cursor。返回 (这一页, 下一页的 cursor)。
    pub async fn list_trash(
        &self,
        workspace_id: &str,
        who: &Principal,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<TrashItem>, Option<String>)> {
        self.require(workspace_id, who, false).await?;
        let skip = crate::parse_cursor(cursor)?.unwrap_or(0).max(0) as usize;
        let limit = limit.clamp(1, 1000);
        let all = self.storage.trash(workspace_id).await?;
        let next = (all.len() > skip + limit).then(|| (skip + limit).to_string());
        let mut out = Vec::new();
        for p in all.into_iter().skip(skip).take(limit) {
            let parent_title = match &p.parent_id {
                Some(pid) => self.storage.page(pid).await?.map(|x| x.title),
                None => None,
            };
            let by = match self.storage.deleted_by(&p.id).await? {
                Some(r) => self.person_json(&r).await,
                None => None,
            };
            out.push(TrashItem {
                id: p.id,
                title: p.title,
                icon: p.icon,
                kind: p.kind,
                deleted_at: p.deleted_at.unwrap_or_default(),
                deleted_by: by,
                parent_title,
                parent_id: p.parent_id,
            });
        }
        Ok((out, next))
    }

    /// 回收站里的页面和权限（要能写那个工作区）。
    async fn trashed(&self, id: &str, who: &Principal) -> Result<Page> {
        let p = self
            .storage
            .page(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("页面 {id} 不存在")))?;
        self.require(&p.workspace_id, who, true).await?;
        if p.deleted_at.is_none() {
            return Err(Error::BadRequest("这个页面不在回收站里".into()));
        }
        Ok(p)
    }

    /// 放回来，返回放回来的页面（第一个是它自己）。侧栏发 page.created，行发 db.changed。
    pub async fn restore_trash(&self, id: &str, who: &Principal) -> Result<Vec<Page>> {
        let root = self.trashed(id, who).await?;
        let back = self.storage.restore_page(id).await?;
        for p in back.iter().filter(|p| p.kind != "row") {
            self.events.page(EventKind::Created, p.clone());
        }
        for p in &back {
            self.search.touch_page(&p.id);
            if p.kind == "database" {
                self.search.touch_db(&p.id);
            }
        }
        if root.kind == "row"
            && let Some(db) = &root.parent_id
            && let Some(m) = self.storage.database(db).await?
        {
            self.events.emit(Event {
                workspace_id: root.workspace_id.clone(),
                name: "db.changed",
                data: serde_json::json!({ "db_id": db, "seq": m.seq }),
                user_id: None,
            });
        }
        Ok(back)
    }

    /// 彻底删。
    pub async fn purge_trash(&self, id: &str, who: &Principal) -> Result<()> {
        let p = self.trashed(id, who).await?;
        self.purge(&p).await
    }

    async fn purge(&self, p: &Page) -> Result<()> {
        // 打开着的先关掉（不然它还会往库里存）
        let (ids, blobs) = {
            self.hub.unload(&p.id).await;
            self.storage.purge_page(&p.id).await?
        };
        for id in &ids {
            self.hub.unload(id).await;
            self.search.touch_page(id);
            self.dbcache.forget(id);
        }
        if p.kind == "database" {
            self.search.touch_db(&p.id);
        }
        for h in blobs {
            let _ = self.media.blobs.delete(&h).await;
            for w in THUMB_WIDTHS {
                let _ = self.media.blobs.delete(&format!("{h}.w{w}.webp")).await;
            }
        }
        self.events.emit(Event {
            workspace_id: p.workspace_id.clone(),
            name: "page.purged",
            data: serde_json::json!({ "id": p.id }),
            user_id: None,
        });
        Ok(())
    }

    /// 清掉回收站里放了 days 天以上的，返回清了几个。
    pub async fn purge_expired(&self, days: i64) -> Result<usize> {
        let before = crate::ids::now_ms() - days * 24 * 3600 * 1000;
        let ids = self.storage.trash_before(before).await?;
        let mut n = 0;
        for id in ids {
            if let Some(p) = self.storage.page(&id).await?
                && p.deleted_at.is_some()
            {
                self.purge(&p).await?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// 后台：起来一分钟后清一次，之后每天一次；关机时停。
    pub fn spawn_trash_purger(self: &Arc<Self>) {
        let core = Arc::downgrade(self);
        let stop = self.on_shutdown();
        tokio::spawn(async move {
            let work = async move {
                tokio::time::sleep(Duration::from_secs(60)).await;
                loop {
                    let Some(c) = core.upgrade() else { return };
                    match c.purge_expired(TRASH_DAYS).await {
                        Ok(0) => {}
                        Ok(n) => {
                            tracing::info!("回收站清掉了 {n} 个放了 {TRASH_DAYS} 天以上的页面")
                        }
                        Err(e) => tracing::warn!("清回收站失败：{e}"),
                    }
                    drop(c);
                    tokio::time::sleep(Duration::from_secs(24 * 3600)).await;
                }
            };
            tokio::select! {
                _ = work => {}
                _ = stop => {}
            }
        });
    }
}
