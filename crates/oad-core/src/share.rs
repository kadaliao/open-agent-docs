//! 公开分享：页面生成只读的公开链接（可选带子页面、可选允许搜索引擎），随时能关。渲染在 oad-server。
use crate::model::*;
use crate::{Core, Error, Result};

impl Core {
    /// 分享链接：`{public_url}/s/{token}`。
    pub fn share_url(&self, token: &str) -> String {
        let base = self.notes.page_url("");
        let base = base.trim_end_matches("/p/");
        format!("{base}/s/{token}")
    }

    pub async fn get_share(&self, page_id: &str, who: &Principal) -> Result<Share> {
        self.page_for(page_id, who, false).await?;
        self.storage
            .share_for_page(page_id)
            .await?
            .ok_or_else(|| Error::NotFound("这个页面没有分享".into()))
    }

    /// 分享（已经分享就改设置，链接不变）。
    pub async fn put_share(
        &self,
        page_id: &str,
        who: &Principal,
        include_children: bool,
        indexable: bool,
    ) -> Result<Share> {
        let (page, _) = self.page_for(page_id, who, true).await?;
        if page.kind == "row" {
            return Err(Error::BadRequest(
                "库里的一行不能单独分享，分享整个库".into(),
            ));
        }
        self.storage
            .put_share(
                page_id,
                include_children,
                indexable,
                &crate::dbengine::person_ref(who),
            )
            .await
    }

    pub async fn revoke_share(&self, page_id: &str, who: &Principal) -> Result<()> {
        self.page_for(page_id, who, true).await?;
        if !self.storage.revoke_share(page_id).await? {
            return Err(Error::NotFound("这个页面没有分享".into()));
        }
        Ok(())
    }

    /// 打开分享链接：分享还在、页面还在；给了子页面要在分享范围里（打开了“带子页面”，而且是分享页面的子孙）。
    /// 返回 (分享, 分享的页面, 要看的页面)。
    pub async fn open_share(
        &self,
        token: &str,
        page_id: Option<&str>,
    ) -> Result<(Share, Page, Page)> {
        let gone = || Error::NotFound("分享链接无效或已关闭".into());
        let share = self.storage.share_by_token(token).await?.ok_or_else(gone)?;
        if share.revoked_at.is_some() {
            return Err(gone());
        }
        let root = match self.storage.page(&share.page_id).await? {
            Some(p) if p.deleted_at.is_none() => p,
            _ => return Err(gone()),
        };
        let target = match page_id.filter(|p| *p != root.id) {
            None => root.clone(),
            Some(pid) => {
                let p = match self.storage.page(pid).await? {
                    Some(p) if p.deleted_at.is_none() && p.workspace_id == root.workspace_id => p,
                    _ => return Err(gone()),
                };
                // 库里的行跟着库走；别的要打开了“带子页面”
                let in_db = p.kind == "row" && p.parent_id.as_deref() == Some(root.id.as_str());
                if !(in_db || share.include_children && self.in_scope(&share, &root, &p.id).await?)
                {
                    return Err(gone());
                }
                p
            }
        };
        Ok((share, root, target))
    }

    /// 页面在不在这个分享里（就是它，或者带子页面时是它的子孙，中间没有删掉的）。
    pub async fn in_scope(&self, share: &Share, root: &Page, page_id: &str) -> Result<bool> {
        if page_id == root.id {
            return Ok(true);
        }
        if !share.include_children {
            return Ok(false);
        }
        let anc = self.storage.ancestors(page_id).await?;
        let Some(i) = anc.iter().position(|a| *a == root.id) else {
            return Ok(false);
        };
        for a in &anc[..i] {
            if self
                .storage
                .page(a)
                .await?
                .is_none_or(|p| p.deleted_at.is_some())
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
