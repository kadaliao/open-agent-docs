//! 评论、@、叫谁，以及“这段是谁写的”。
//!
//! 叫 Agent 的规则（人写的评论才叫；Agent 写的不叫任何 Agent，免得停不下来），按顺序取第一条有结果的：
//! 明确指定的（notify）> 评论里 @ 到的 Agent（最多 3 个）> 回复时这串里最后说话的 Agent > 这段的作者是 Agent > 建页面的是 Agent。
//! 只叫 write / admin、没撤销、同一个工作区的。
//!
//! 人收通知：被 @（mention）、参与的那串有新回复（reply）、参与的那串被解决（resolved）。
use crate::events::Event;
use crate::i18n::Lang;
use crate::ids::{new_id, now_ms};
use crate::model::*;
use crate::{Core, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

/// 评论挂在哪：一段文字（start / end 是网页给的 Yjs RelativePosition，服务器原样存）、整段（只有 block_id）或整页（None）。
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct Anchor {
    pub block_id: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub quote: Option<String>,
}

/// 评论时叫谁。
#[derive(Clone, Debug, Default, PartialEq)]
pub enum NotifySpec {
    /// 按规则。
    #[default]
    Auto,
    /// 谁也不叫。
    Off,
    /// 只叫这些 Agent。
    Only(Vec<String>),
}

impl NotifySpec {
    /// JSON：不给 / null = 按规则，false = 不叫，`["agent:<id>", ...]` = 只叫这些。
    pub fn from_json(v: Option<&Value>) -> Result<NotifySpec> {
        match v {
            None | Some(Value::Null) | Some(Value::Bool(true)) => Ok(NotifySpec::Auto),
            Some(Value::Bool(false)) => Ok(NotifySpec::Off),
            Some(Value::Array(a)) => Ok(NotifySpec::Only(
                a.iter()
                    .filter_map(|x| x.as_str())
                    .filter_map(|s| s.strip_prefix("agent:"))
                    .map(str::to_string)
                    .collect(),
            )),
            _ => Err(Error::BadRequest(
                "notify 只能是 false 或 [\"agent:<id>\", ...]".into(),
            )),
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AuthorRef {
    pub kind: PrincipalKind,
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct CommentView {
    pub id: String,
    pub thread_id: String,
    pub author: AuthorRef,
    /// 删掉的是空字符串。
    pub body: String,
    pub created_at: i64,
    pub edited_at: Option<i64>,
    pub deleted_at: Option<i64>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ThreadView {
    pub id: String,
    pub page_id: String,
    pub anchor: Option<Anchor>,
    pub created_by: AuthorRef,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
    pub resolved_by: Option<AuthorRef>,
    pub comments: Vec<CommentView>,
}

/// 评论里 @ 到的（按出现顺序、去重）：`@[名字](agent:<id>)` / `@[名字](user:<id>)`，以及直接写的 `@名字`（和成员名字对得上的）。
pub fn mentions(
    body: &str,
    members: &[(PrincipalKind, String, String)],
) -> Vec<(PrincipalKind, String)> {
    let mut found: Vec<(usize, PrincipalKind, String)> = Vec::new();
    // 链接写法
    let mut rest = body;
    let mut off = 0;
    while let Some(i) = rest.find("@[") {
        let s = &rest[i..];
        if let Some(close) = s.find("](")
            && let Some(end) = s[close + 2..].find(')')
        {
            let target = &s[close + 2..close + 2 + end];
            let parsed = target
                .strip_prefix("agent:")
                .map(|id| (PrincipalKind::Agent, id))
                .or_else(|| {
                    target
                        .strip_prefix("user:")
                        .map(|id| (PrincipalKind::User, id))
                });
            if let Some((k, id)) = parsed {
                found.push((off + i, k, id.to_string()));
            }
        }
        off += i + 2;
        rest = &rest[i + 2..];
    }
    // 直接写的 @名字：名字后面得是结尾、空白或标点；长名字优先
    let mut names: Vec<&(PrincipalKind, String, String)> = members.iter().collect();
    names.sort_by_key(|m| std::cmp::Reverse(m.2.len()));
    for (pos, _) in body.match_indices('@') {
        let after = &body[pos + 1..];
        // 前面紧挨着字母数字的不算（邮箱）
        if after.starts_with('[')
            || body[..pos]
                .chars()
                .last()
                .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        for (k, id, name) in &names {
            if !name.is_empty() && after.starts_with(name.as_str()) {
                let next = after[name.len()..].chars().next();
                if next.is_none_or(|c| {
                    c.is_whitespace()
                        || (c.is_ascii_punctuation() && c != '_')
                        || "，。！？、；：）".contains(c)
                }) {
                    found.push((pos, *k, id.clone()));
                    break;
                }
            }
        }
    }
    found.sort_by_key(|f| f.0);
    let mut seen = HashSet::new();
    found
        .into_iter()
        .filter(|(_, k, id)| seen.insert((*k, id.clone())))
        .map(|(_, k, id)| (k, id))
        .collect()
}

fn excerpt(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if t.chars().count() < s.chars().count() {
        format!("{t}…")
    } else {
        t
    }
}

impl Core {
    async fn author_ref(&self, cache: &mut HashMap<String, AuthorRef>, id: &str) -> AuthorRef {
        if let Some(a) = cache.get(id) {
            return a.clone();
        }
        let a = match self.storage.user(id).await {
            Ok(Some(u)) => AuthorRef {
                kind: u.kind,
                id: u.id,
                name: u.name,
            },
            _ => AuthorRef {
                kind: PrincipalKind::User,
                id: id.into(),
                name: String::new(),
            },
        };
        cache.insert(id.into(), a.clone());
        a
    }

    async fn thread_view(
        &self,
        t: Thread,
        comments: Vec<Comment>,
        cache: &mut HashMap<String, AuthorRef>,
    ) -> ThreadView {
        let mut cs = Vec::with_capacity(comments.len());
        for c in comments {
            cs.push(CommentView {
                author: self.author_ref(cache, &c.author_id).await,
                body: if c.deleted_at.is_some() {
                    String::new()
                } else {
                    c.body
                },
                id: c.id,
                thread_id: c.thread_id,
                created_at: c.created_at,
                edited_at: c.edited_at,
                deleted_at: c.deleted_at,
            });
        }
        let anchor = (t.block_id.is_some() || t.quote.is_some()).then(|| Anchor {
            block_id: t.block_id.clone(),
            start: t.anchor_start.clone(),
            end: t.anchor_end.clone(),
            quote: t.quote.clone(),
        });
        let resolved_by = match &t.resolved_by {
            Some(r) => Some(self.author_ref(cache, r).await),
            None => None,
        };
        ThreadView {
            created_by: self.author_ref(cache, &t.created_by).await,
            id: t.id,
            page_id: t.page_id,
            anchor,
            created_at: t.created_at,
            resolved_at: t.resolved_at,
            resolved_by,
            comments: cs,
        }
    }

    async fn one_thread(&self, thread_id: &str) -> Result<ThreadView> {
        let t = self
            .storage
            .thread(thread_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("评论串 {thread_id} 不存在")))?;
        let cs = self.storage.thread_comments(thread_id).await?;
        Ok(self.thread_view(t, cs, &mut HashMap::new()).await)
    }

    /// 页面的评论串。resolved：None = 全部。
    pub async fn threads(
        &self,
        page_id: &str,
        who: &Principal,
        resolved: Option<bool>,
    ) -> Result<Vec<ThreadView>> {
        self.page_for(page_id, who, false).await?;
        let mut cache = HashMap::new();
        let mut out = Vec::new();
        for (t, cs) in self.storage.threads(page_id, resolved).await? {
            out.push(self.thread_view(t, cs, &mut cache).await);
        }
        Ok(out)
    }

    /// 评论所在的串和页面；能看页面就能看评论，写评论要有写权限。
    async fn thread_for(
        &self,
        thread_id: &str,
        who: &Principal,
        write: bool,
    ) -> Result<(Thread, Page)> {
        let t = self
            .storage
            .thread(thread_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("评论串 {thread_id} 不存在")))?;
        let (page, _) = self.page_for(&t.page_id, who, write).await?;
        Ok((t, page))
    }

    fn emit_thread(
        &self,
        page: &Page,
        name: &'static str,
        view: &ThreadView,
        comment_id: Option<&str>,
    ) {
        self.events.emit(Event {
            workspace_id: page.workspace_id.clone(),
            name,
            data: json!({ "page_id": page.id, "thread_id": view.id, "comment_id": comment_id, "thread": view }),
            user_id: None,
        });
    }

    /// 新的一串评论。
    pub async fn create_thread(
        &self,
        page_id: &str,
        who: &Principal,
        anchor: Option<Anchor>,
        body: &str,
        notify: NotifySpec,
    ) -> Result<ThreadView> {
        let (page, _) = self.page_for(page_id, who, true).await?;
        let body = check_body(body)?;
        let anchor = anchor.unwrap_or_default();
        let now = now_ms();
        let t = Thread {
            id: new_id(),
            page_id: page.id.clone(),
            block_id: anchor.block_id.filter(|b| !b.is_empty()),
            anchor_start: anchor.start,
            anchor_end: anchor.end,
            quote: anchor.quote,
            created_by: who.user_id.clone(),
            created_at: now,
            resolved_at: None,
            resolved_by: None,
        };
        let c = Comment {
            id: new_id(),
            thread_id: t.id.clone(),
            author_kind: who.kind,
            author_id: who.user_id.clone(),
            body,
            created_at: now,
            edited_at: None,
            deleted_at: None,
        };
        self.storage.create_thread(t.clone(), c.clone()).await?;
        let view = self.one_thread(&t.id).await?;
        self.emit_thread(&page, "comment.created", &view, Some(&c.id));
        self.notify_comment(&page, &t, &c, who, notify, &[]).await?;
        Ok(view)
    }

    /// 回复。
    pub async fn reply(
        &self,
        thread_id: &str,
        who: &Principal,
        body: &str,
        notify: NotifySpec,
    ) -> Result<CommentView> {
        let (t, page) = self.thread_for(thread_id, who, true).await?;
        let body = check_body(body)?;
        let prior = self.storage.thread_comments(thread_id).await?;
        let c = Comment {
            id: new_id(),
            thread_id: t.id.clone(),
            author_kind: who.kind,
            author_id: who.user_id.clone(),
            body,
            created_at: now_ms(),
            edited_at: None,
            deleted_at: None,
        };
        self.storage.add_comment(c.clone()).await?;
        let view = self.one_thread(thread_id).await?;
        self.emit_thread(&page, "comment.created", &view, Some(&c.id));
        self.notify_comment(&page, &t, &c, who, notify, &prior)
            .await?;
        Ok(view
            .comments
            .into_iter()
            .find(|x| x.id == c.id)
            .expect("刚加的评论"))
    }

    async fn own_comment(&self, id: &str, who: &Principal) -> Result<(Comment, Thread, Page)> {
        let c = self
            .storage
            .comment(id)
            .await?
            .filter(|c| c.deleted_at.is_none())
            .ok_or_else(|| Error::NotFound(format!("评论 {id} 不存在")))?;
        let (t, page) = self.thread_for(&c.thread_id, who, false).await?;
        Ok((c, t, page))
    }

    /// 改评论（只能改自己的）。
    pub async fn edit_comment(&self, id: &str, who: &Principal, body: &str) -> Result<ThreadView> {
        let (c, t, page) = self.own_comment(id, who).await?;
        if c.author_id != who.user_id {
            return Err(Error::Forbidden("只能改自己的评论".into()));
        }
        self.storage
            .edit_comment(id, &check_body(body)?, now_ms())
            .await?;
        let view = self.one_thread(&t.id).await?;
        self.emit_thread(&page, "comment.updated", &view, Some(id));
        Ok(view)
    }

    /// 删评论：自己的，或者工作区 owner。
    pub async fn delete_comment(&self, id: &str, who: &Principal) -> Result<ThreadView> {
        let (c, t, page) = self.own_comment(id, who).await?;
        if c.author_id != who.user_id && self.role(&page.workspace_id, who).await? != Role::Owner {
            return Err(Error::Forbidden("只能删自己的评论".into()));
        }
        self.storage.delete_comment(id, now_ms()).await?;
        let view = self.one_thread(&t.id).await?;
        self.emit_thread(&page, "comment.deleted", &view, Some(id));
        Ok(view)
    }

    /// 解决 / 重新打开整串。解决时通知参与过的人。
    pub async fn resolve_thread(
        &self,
        thread_id: &str,
        who: &Principal,
        resolved: bool,
    ) -> Result<ThreadView> {
        let (t, page) = self.thread_for(thread_id, who, true).await?;
        if resolved == t.resolved_at.is_some() {
            return self.one_thread(thread_id).await;
        }
        self.storage
            .resolve_thread(thread_id, resolved.then(|| (now_ms(), who.user_id.clone())))
            .await?;
        let view = self.one_thread(thread_id).await?;
        self.emit_thread(&page, "thread.updated", &view, None);
        if resolved {
            let comments = self.storage.thread_comments(thread_id).await?;
            let people = self.participants(&t, &comments, who).await?;
            let data = json!({
                "page": { "id": page.id, "title": page.title },
                "thread_id": t.id,
                "block_id": t.block_id,
                "quote": t.quote,
                "by": { "kind": who.kind, "id": who.user_id, "name": who.name },
            });
            self.notes
                .push_user(
                    people
                        .into_iter()
                        .map(|u| NewNotification {
                            workspace_id: page.workspace_id.clone(),
                            owner_id: u,
                            kind: "resolved".into(),
                            data: data.clone(),
                            next_attempt_at: None,
                        })
                        .collect(),
                )
                .await?;
        }
        Ok(view)
    }

    /// 这串里说过话的人（不含 Agent、不含 except）。
    async fn participants(
        &self,
        t: &Thread,
        comments: &[Comment],
        except: &Principal,
    ) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let ids = std::iter::once(t.created_by.as_str()).chain(
            comments
                .iter()
                .filter(|c| c.deleted_at.is_none() && c.author_kind == PrincipalKind::User)
                .map(|c| c.author_id.as_str()),
        );
        for id in ids {
            if id == except.user_id || !seen.insert(id.to_string()) {
                continue;
            }
            if matches!(self.storage.user(id).await?, Some(u) if u.kind == PrincipalKind::User) {
                out.push(id.to_string());
            }
        }
        Ok(out)
    }

    /// 能叫的 Agent：同一个工作区、没撤销、能写。
    async fn callable(&self, ws: &str, id: &str) -> Result<Option<Agent>> {
        Ok(self
            .storage
            .agent(id)
            .await?
            .filter(|a| a.workspace_id == ws && a.active() && a.scope.can_write()))
    }

    /// 按规则算这条评论要叫哪些 Agent。
    #[allow(clippy::too_many_arguments)]
    async fn agents_to_call(
        &self,
        page: &Page,
        t: &Thread,
        c: &Comment,
        who: &Principal,
        spec: &NotifySpec,
        prior: &[Comment],
        mentioned: &[(PrincipalKind, String)],
    ) -> Result<Vec<Agent>> {
        if who.kind == PrincipalKind::Agent {
            return Ok(Vec::new());
        }
        let ws = page.workspace_id.as_str();
        let pick = async |ids: Vec<String>| -> Result<Vec<Agent>> {
            let mut out: Vec<Agent> = Vec::new();
            for id in ids {
                if let Some(a) = self.callable(ws, &id).await?
                    && !out.iter().any(|x| x.id == a.id)
                {
                    out.push(a);
                }
            }
            Ok(out)
        };
        match spec {
            NotifySpec::Off => return Ok(Vec::new()),
            NotifySpec::Only(ids) => return pick(ids.clone()).await,
            NotifySpec::Auto => {}
        }
        let at: Vec<String> = mentioned
            .iter()
            .filter(|(k, _)| *k == PrincipalKind::Agent)
            .map(|(_, id)| id.clone())
            .collect();
        let mut got = pick(at).await?;
        got.truncate(3);
        if !got.is_empty() {
            return Ok(got);
        }
        if !prior.is_empty()
            && let Some(last) = prior.iter().rev().find(|x| {
                x.deleted_at.is_none() && x.author_kind == PrincipalKind::Agent && x.id != c.id
            })
        {
            let got = pick(vec![last.author_id.clone()]).await?;
            if !got.is_empty() {
                return Ok(got);
            }
        }
        if let Some(b) = &t.block_id
            && let Some((PrincipalKind::Agent, id)) = self.block_author(&page.id, b).await?
        {
            let got = pick(vec![id]).await?;
            if !got.is_empty() {
                return Ok(got);
            }
        }
        if let Some(creator) = &page.created_by {
            return pick(vec![creator.clone()]).await;
        }
        Ok(Vec::new())
    }

    async fn notify_comment(
        &self,
        page: &Page,
        t: &Thread,
        c: &Comment,
        who: &Principal,
        spec: NotifySpec,
        prior: &[Comment],
    ) -> Result<()> {
        let members: Vec<(PrincipalKind, String, String)> = self
            .storage
            .members(&page.workspace_id)
            .await?
            .into_iter()
            .map(|(u, _)| (u.kind, u.id, u.name))
            .collect();
        let mentioned = mentions(&c.body, &members);
        let author = json!({ "kind": who.kind, "id": who.user_id, "name": who.name });

        // Agent
        let agents = self
            .agents_to_call(page, t, c, who, &spec, prior, &mentioned)
            .await?;
        let url = self.notes.page_url(&page.id);
        let mut rows = Vec::new();
        for a in agents {
            let lang = self.lang_of(&a.id).await;
            let prompt = prompt_for(page, t, c, who, !prior.is_empty(), lang);
            let data = json!({
                "agent": { "id": a.id, "name": a.name },
                "type": "comment",
                "page": { "id": page.id, "title": page.title, "url": url },
                "thread_id": t.id,
                "comment": { "id": c.id, "author": author, "body": c.body },
                "block_id": t.block_id,
                "quote": t.quote,
                "prompt": prompt,
            });
            rows.push(NewNotification {
                workspace_id: page.workspace_id.clone(),
                owner_id: a.id.clone(),
                kind: "comment".into(),
                data,
                next_attempt_at: a.webhook.is_some().then(now_ms),
            });
        }
        self.notes.push_agent(rows).await?;

        // 人
        let data = |kind: &str| {
            json!({
                "type": kind,
                "page": { "id": page.id, "title": page.title },
                "thread_id": t.id,
                "comment": { "id": c.id, "author": author, "body": c.body },
                "block_id": t.block_id,
                "quote": t.quote,
            })
        };
        let mut notified: HashSet<String> = HashSet::from([who.user_id.clone()]);
        let mut rows = Vec::new();
        for (k, id) in &mentioned {
            if *k == PrincipalKind::User
                && members
                    .iter()
                    .any(|m| m.0 == PrincipalKind::User && &m.1 == id)
                && notified.insert(id.clone())
            {
                rows.push(NewNotification {
                    workspace_id: page.workspace_id.clone(),
                    owner_id: id.clone(),
                    kind: "mention".into(),
                    data: data("mention"),
                    next_attempt_at: None,
                });
            }
        }
        if !prior.is_empty() {
            for u in self.participants(t, prior, who).await? {
                if notified.insert(u.clone()) {
                    rows.push(NewNotification {
                        workspace_id: page.workspace_id.clone(),
                        owner_id: u,
                        kind: "reply".into(),
                        data: data("reply"),
                        next_attempt_at: None,
                    });
                }
            }
        }
        self.notes.push_user(rows).await
    }

    /// 每个 client id 是谁写的。
    pub async fn authors(
        &self,
        page_id: &str,
        who: &Principal,
    ) -> Result<BTreeMap<String, AuthorRef>> {
        self.page_for(page_id, who, false).await?;
        Ok(self
            .storage
            .doc_clients(page_id)
            .await?
            .into_iter()
            .map(|(cid, kind, id, name)| (cid.to_string(), AuthorRef { kind, id, name }))
            .collect())
    }

    /// 一块的作者：这块里现存文字最多的那个人（同一个人的几个 client id 合起来算）。
    pub async fn block_author(
        &self,
        page_id: &str,
        block_id: &str,
    ) -> Result<Option<(PrincipalKind, String)>> {
        let b = block_id.to_string();
        let Some(counts) = self.hub.read(page_id, move |d| d.block_authors(&b)).await? else {
            return Ok(None);
        };
        let mut map: HashMap<u64, (PrincipalKind, String)> = self
            .storage
            .doc_clients(page_id)
            .await?
            .into_iter()
            .map(|(cid, kind, id, _)| (cid, (kind, id)))
            .collect();
        // 刚写的还没存进库的作者
        map.extend(self.hub.recent_owners(page_id).await);
        let mut by: HashMap<(PrincipalKind, String), usize> = HashMap::new();
        for (cid, n) in counts {
            if let Some(p) = map.get(&cid) {
                *by.entry(p.clone()).or_default() += n;
            }
        }
        Ok(by
            .into_iter()
            .max_by_key(|(p, n)| (*n, p.1.clone()))
            .map(|(p, _)| p))
    }
}

fn check_body(body: &str) -> Result<String> {
    let b = body.trim();
    if b.is_empty() {
        return Err(Error::BadRequest("评论不能是空的".into()));
    }
    if b.chars().count() > 20_000 {
        return Err(Error::BadRequest("评论太长了".into()));
    }
    Ok(b.to_string())
}

/// 写给 Agent 的一段话：可以直接当消息发给它（按 Agent 的语言）。
fn prompt_for(
    page: &Page,
    t: &Thread,
    c: &Comment,
    who: &Principal,
    is_reply: bool,
    lang: Lang,
) -> String {
    let tr =
        |zh: &str, args: &[(&str, &str)]| crate::i18n::t_in(&[crate::i18n::CORE], lang, zh, args);
    let title = if page.title.is_empty() {
        tr("（无标题）", &[])
    } else {
        page.title.clone()
    };
    let mut s = match (&t.block_id, is_reply) {
        (Some(b), false) => tr(
            "你在 Open Agent Docs 里被叫到了。{who} 在页面《{title}》（page_id: {page}）的块 ⟦{block}⟧ 上写了评论：",
            &[
                ("who", &who.name),
                ("title", &title),
                ("page", &page.id),
                ("block", b),
            ],
        ),
        (Some(b), true) => tr(
            "你在 Open Agent Docs 里被叫到了。{who} 在页面《{title}》（page_id: {page}）的块 ⟦{block}⟧ 上回复了评论：",
            &[
                ("who", &who.name),
                ("title", &title),
                ("page", &page.id),
                ("block", b),
            ],
        ),
        (None, false) => tr(
            "你在 Open Agent Docs 里被叫到了。{who} 在页面《{title}》（page_id: {page}）里写了评论：",
            &[("who", &who.name), ("title", &title), ("page", &page.id)],
        ),
        (None, true) => tr(
            "你在 Open Agent Docs 里被叫到了。{who} 在页面《{title}》（page_id: {page}）里回复了评论：",
            &[("who", &who.name), ("title", &title), ("page", &page.id)],
        ),
    };
    s.push('\n');
    if let Some(q) = t.quote.as_deref().filter(|q| !q.is_empty()) {
        s.push_str(&tr(
            "选中的原文：「{quote}」",
            &[("quote", &excerpt(q, 300))],
        ));
        s.push('\n');
    }
    s.push_str(&tr("评论：{body}", &[("body", &excerpt(&c.body, 2000))]));
    s.push_str("\n\n");
    let zh = if is_reply {
        "先用 read_page(page_id: \"{page}\") 看上下文，用 list_comments 看这串之前的对话。\
回复评论用 comment(thread_id: \"{thread}\", body: \"...\")；要改页面用 edit_page(page_id, base, ops)（base 来自 read_page）；\
处理完用 comment(thread_id: \"{thread}\", resolve: true) 把这串标成已解决。"
    } else {
        "先用 read_page(page_id: \"{page}\") 看上下文。\
回复评论用 comment(thread_id: \"{thread}\", body: \"...\")；要改页面用 edit_page(page_id, base, ops)（base 来自 read_page）；\
处理完用 comment(thread_id: \"{thread}\", resolve: true) 把这串标成已解决。"
    };
    s.push_str(&tr(zh, &[("page", &page.id), ("thread", &t.id)]));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mentions() {
        let m = vec![
            (
                PrincipalKind::Agent,
                "a1".to_string(),
                "写作助手".to_string(),
            ),
            (PrincipalKind::Agent, "a2".to_string(), "写作".to_string()),
            (PrincipalKind::User, "u1".to_string(), "alice".to_string()),
        ];
        let got = mentions(
            "@[张三](user:u9) 请 @写作助手 看看，@alice。@写作x 不算，邮箱 a@alice.com 不算",
            &m,
        );
        assert_eq!(
            got,
            vec![
                (PrincipalKind::User, "u9".into()),
                (PrincipalKind::Agent, "a1".into()),
                (PrincipalKind::User, "u1".into()),
            ]
        );
        assert_eq!(
            NotifySpec::from_json(Some(&json!(["agent:x", "user:y"]))).unwrap(),
            NotifySpec::Only(vec!["x".into()])
        );
        assert_eq!(
            NotifySpec::from_json(Some(&json!(false))).unwrap(),
            NotifySpec::Off
        );
    }
}
