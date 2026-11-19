//! 第 2 阶段的接口：Agent 管理、作者、评论、通知。见 docs/phase2-contract.md。
use crate::api::{ApiResult, Auth, Body, double};
use crate::openapi::{AgentNotification, ErrorBody};
use crate::{ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use futures_util::{Stream, StreamExt};
use oad_core::agents::AgentView;
use oad_core::comments::{AuthorRef, CommentView, ThreadView};
use oad_core::{Agent, AgentPatch, Anchor, NotifySpec, Role, Scope};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::convert::Infallible;
use std::time::Duration;
use utoipa::{OpenApi, ToSchema};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{ws}/agents",
            get(list_agents).post(create_agent),
        )
        .route("/workspaces/{ws}/members", get(members))
        .route("/agents/{id}", patch(update_agent).delete(revoke_agent))
        .route("/agents/{id}/token", post(rotate_token))
        .route("/agents/{id}/webhook-secret", post(rotate_secret))
        .route("/pages/{id}/authors", get(authors))
        .route(
            "/pages/{id}/comments",
            get(list_comments).post(create_thread),
        )
        .route("/threads/{id}/comments", post(reply))
        .route("/threads/{id}", patch(patch_thread))
        .route("/comments/{id}", patch(edit_comment).delete(delete_comment))
        .route("/agent/notifications", get(poll_notes))
        .route("/agent/notifications/stream", get(stream_notes))
        .route("/agent/notifications/ack", post(ack_notes))
        .route("/me/notifications", get(my_notes))
        .route("/me/notifications/read", post(read_my_notes))
}

fn parse_scope(s: &str) -> ApiResult<Scope> {
    Scope::parse(s).ok_or_else(|| ApiError::bad_request("scope 只能是 read / write / admin"))
}

// ---- Agent ----

#[derive(Serialize, ToSchema)]
pub(crate) struct AgentList {
    agents: Vec<AgentView>,
}

#[utoipa::path(get, path = "/workspaces/{ws}/agents", tag = "agents", params(("ws" = String, Path, description = "工作区")),
    responses((status = 200, body = AgentList), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn list_agents(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
) -> ApiResult<Json<AgentList>> {
    let agents = s.core.list_agents(&ws, &p).await?;
    Ok(Json(AgentList { agents }))
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MemberUser {
    id: String,
    name: String,
    email: Option<String>,
    avatar: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct Member {
    user: MemberUser,
    role: Role,
}

/// 工作区的人（按加入先后）和 Agent。
#[derive(Serialize, ToSchema)]
pub(crate) struct MemberList {
    members: Vec<Member>,
    agents: Vec<AgentView>,
}

#[utoipa::path(get, path = "/workspaces/{ws}/members", tag = "members", params(("ws" = String, Path, description = "工作区")),
    responses((status = 200, body = MemberList), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn members(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
) -> ApiResult<Json<MemberList>> {
    let (people, agents) = s.core.members(&ws, &p).await?;
    let mut members = Vec::with_capacity(people.len());
    for (u, role) in people {
        let avatar = s.core.storage.user_prefs(&u.id).await?.avatar;
        members.push(Member {
            user: MemberUser {
                id: u.id,
                name: u.name,
                email: u.email,
                avatar,
            },
            role,
        });
    }
    Ok(Json(MemberList { members, agents }))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateAgent {
    /// 1 到 40 个字，不能有空白和 @ [ ] ( ) < > 引号 逗号
    name: String,
    /// 介绍（给人和别的 Agent 看）
    about: Option<String>,
    /// read / write / admin
    scope: String,
    /// 收通知的 webhook 地址
    webhook: Option<String>,
}

/// 新建的 Agent；token 和 webhook_secret 只有这一次能看到。
#[derive(Serialize, ToSchema)]
pub(crate) struct CreatedAgent {
    agent: Agent,
    /// `oad_…`，当 Bearer token 用
    token: String,
    /// `whsec_…`，验 webhook 的签名
    webhook_secret: String,
}

#[utoipa::path(post, path = "/workspaces/{ws}/agents", tag = "agents", params(("ws" = String, Path, description = "工作区")), request_body = CreateAgent,
    responses((status = 201, body = CreatedAgent), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub(crate) async fn create_agent(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    Body(b): Body<CreateAgent>,
) -> ApiResult<Response> {
    let scope = parse_scope(&b.scope)?;
    let (agent, token, secret) = s
        .core
        .create_agent(&ws, &p, &b.name, b.about, scope, b.webhook)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedAgent {
            agent,
            token,
            webhook_secret: secret,
        }),
    )
        .into_response())
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct PatchAgent {
    name: Option<String>,
    about: Option<String>,
    /// read / write / admin
    scope: Option<String>,
    /// null = 不用 webhook
    #[serde(default, deserialize_with = "double")]
    webhook: Option<Option<String>>,
    /// 通知里那段话用的语言：zh / en；null = 跟工作区
    #[serde(default, deserialize_with = "double")]
    locale: Option<Option<String>>,
}

#[utoipa::path(patch, path = "/agents/{id}", tag = "agents", params(("id" = String, Path, description = "Agent")), request_body = PatchAgent,
    responses((status = 200, body = Agent), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub(crate) async fn update_agent(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<PatchAgent>,
) -> ApiResult<Json<Agent>> {
    let patch = AgentPatch {
        name: b.name,
        about: b.about,
        scope: b.scope.as_deref().map(parse_scope).transpose()?,
        webhook: b.webhook,
        locale: b.locale,
    };
    Ok(Json(s.core.update_agent(&id, &p, patch).await?))
}

#[utoipa::path(delete, path = "/agents/{id}", tag = "agents", params(("id" = String, Path, description = "Agent")),
    description = "撤销：token 马上失效，返回撤销后的 Agent（带 revoked_at）。",
    responses((status = 200, body = Agent), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn revoke_agent(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<Agent>> {
    Ok(Json(s.core.revoke_agent(&id, &p).await?))
}

#[derive(Serialize, ToSchema)]
pub(crate) struct NewToken {
    /// 新 token（旧的马上失效）
    token: String,
}

#[utoipa::path(post, path = "/agents/{id}/token", tag = "agents", params(("id" = String, Path, description = "Agent")),
    responses((status = 200, body = NewToken), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn rotate_token(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<NewToken>> {
    let token = s.core.rotate_agent_token(&id, &p).await?;
    Ok(Json(NewToken { token }))
}

#[derive(Serialize, ToSchema)]
pub(crate) struct NewSecret {
    /// 新的 webhook 签名密钥
    webhook_secret: String,
}

#[utoipa::path(post, path = "/agents/{id}/webhook-secret", tag = "agents", params(("id" = String, Path, description = "Agent")),
    responses((status = 200, body = NewSecret), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn rotate_secret(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<NewSecret>> {
    let secret = s.core.rotate_webhook_secret(&id, &p).await?;
    Ok(Json(NewSecret {
        webhook_secret: secret,
    }))
}

// ---- 作者 ----

/// 每个 Yjs client id（字符串）是谁写的。
#[derive(Serialize, ToSchema)]
pub(crate) struct Authors {
    clients: BTreeMap<String, AuthorRef>,
}

#[utoipa::path(get, path = "/pages/{id}/authors", tag = "pages", params(("id" = String, Path, description = "页面")),
    responses((status = 200, body = Authors), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn authors(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<Authors>> {
    let clients = s.core.authors(&id, &p).await?;
    Ok(Json(Authors { clients }))
}

// ---- 评论 ----

#[derive(Serialize, ToSchema)]
pub(crate) struct ThreadList {
    threads: Vec<ThreadView>,
}

#[utoipa::path(get, path = "/pages/{id}/comments", tag = "comments",
    params(("id" = String, Path, description = "页面"), ("resolved" = Option<String>, Query, description = "false（默认，没解决的）/ true / all")),
    responses((status = 200, body = ThreadList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn list_comments(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<ThreadList>> {
    let resolved = match q.get("resolved").map(String::as_str) {
        None | Some("false") => Some(false),
        Some("true") => Some(true),
        Some("all") => None,
        Some(x) => {
            return Err(ApiError::bad_request(format!(
                "resolved 只能是 false / true / all，不是 {x}"
            )));
        }
    };
    let threads = s.core.threads(&id, &p, resolved).await?;
    Ok(Json(ThreadList { threads }))
}

/// 评论时叫谁：false = 谁也不叫，`["agent:<id>", ...]` = 只叫这些（不给 / null = 按规则）。
#[derive(Serialize, ToSchema)]
#[serde(untagged)]
#[allow(dead_code)] // 只用来写 OpenAPI，解析在 NotifySpec::from_json
pub(crate) enum Notify {
    Off(bool),
    Only(Vec<String>),
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct NewThread {
    /// 挂在哪；不给 = 整页
    anchor: Option<Anchor>,
    /// Markdown；`@[名字](agent:<id>)` / `@[名字](user:<id>)` 或 `@名字` 叫人
    body: String,
    /// 叫谁：不给 / null = 按规则，false = 谁也不叫，`["agent:<id>", ...]` = 只叫这些
    #[schema(value_type = Option<Notify>)]
    notify: Option<Value>,
}

#[utoipa::path(post, path = "/pages/{id}/comments", tag = "comments", params(("id" = String, Path, description = "页面")), request_body = NewThread,
    responses((status = 201, body = ThreadView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn create_thread(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<NewThread>,
) -> ApiResult<Response> {
    let notify = NotifySpec::from_json(b.notify.as_ref())?;
    let t = s
        .core
        .create_thread(&id, &p, b.anchor, &b.body, notify)
        .await?;
    Ok((StatusCode::CREATED, Json(t)).into_response())
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct Reply {
    /// Markdown
    body: String,
    /// 同 NewThread.notify
    #[schema(value_type = Option<Notify>)]
    notify: Option<Value>,
}

#[utoipa::path(post, path = "/threads/{id}/comments", tag = "comments", params(("id" = String, Path, description = "评论串")), request_body = Reply,
    responses((status = 201, body = CommentView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn reply(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<Reply>,
) -> ApiResult<Response> {
    let notify = NotifySpec::from_json(b.notify.as_ref())?;
    let c = s.core.reply(&id, &p, &b.body, notify).await?;
    Ok((StatusCode::CREATED, Json(c)).into_response())
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct EditComment {
    /// 新的正文（Markdown）
    body: String,
}

#[utoipa::path(patch, path = "/comments/{id}", tag = "comments", params(("id" = String, Path, description = "评论")), request_body = EditComment,
    description = "只有作者能改。",
    responses((status = 200, body = CommentView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn edit_comment(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<EditComment>,
) -> ApiResult<Json<Option<CommentView>>> {
    let t = s.core.edit_comment(&id, &p, &b.body).await?;
    let c = t.comments.into_iter().find(|c| c.id == id);
    Ok(Json(c))
}

#[utoipa::path(delete, path = "/comments/{id}", tag = "comments", params(("id" = String, Path, description = "评论")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn delete_comment(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    s.core.delete_comment(&id, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct PatchThread {
    /// true = 解决，false = 重新打开
    resolved: bool,
}

#[utoipa::path(patch, path = "/threads/{id}", tag = "comments", params(("id" = String, Path, description = "评论串")), request_body = PatchThread,
    responses((status = 200, body = ThreadView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn patch_thread(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<PatchThread>,
) -> ApiResult<Json<ThreadView>> {
    Ok(Json(s.core.resolve_thread(&id, &p, b.resolved).await?))
}

// ---- Agent 的通知 ----

fn num(q: &HashMap<String, String>, k: &str) -> ApiResult<Option<i64>> {
    q.get(k)
        .filter(|v| !v.is_empty())
        .map(|v| {
            v.parse()
                .map_err(|_| ApiError::bad_request(format!("{k} 要是整数")))
        })
        .transpose()
}

/// 游标：`?after=`，`?cursor=` 是同一个意思（统一的翻页写法）。
fn after(q: &HashMap<String, String>) -> ApiResult<Option<i64>> {
    Ok(num(q, "after")?.or(num(q, "cursor")?))
}

/// 取满了一页：最后一条的编号当下一页的游标；没满就是没了。
fn next_cursor(notes: &[Value], limit: i64) -> Option<String> {
    if (notes.len() as i64) < limit.clamp(1, 100) {
        return None;
    }
    notes
        .last()
        .and_then(|v| v["id"].as_i64())
        .map(|i| i.to_string())
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AgentNotes {
    #[schema(value_type = Vec<AgentNotification>)]
    notifications: Vec<Value>,
    /// 取满了一页时是最后一条的编号，带 `?cursor=` 接着取；没有就是取完了
    next_cursor: Option<String>,
}

/// 长轮询：`?after=<id>&wait=<秒，最多 60>&limit=<最多 100>`。
#[utoipa::path(get, path = "/agent/notifications", tag = "notifications",
    description = "Agent 取通知（长轮询）：没有新的就等 wait 秒。不给 after / cursor 就是还没处理（ack）的。只有 Agent 能调。",
    params(
        ("after" = Option<i64>, Query, description = "从这个编号之后取"),
        ("cursor" = Option<String>, Query, description = "同 after（上次的 next_cursor）"),
        ("limit" = Option<i64>, Query, description = "最多几条，默认 50，最多 100"),
        ("wait" = Option<i64>, Query, description = "没有就等几秒，默认 0，最多 60"),
    ),
    responses((status = 200, body = AgentNotes), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub(crate) async fn poll_notes(
    State(s): State<AppState>,
    Auth(p): Auth,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<AgentNotes>> {
    let after = after(&q)?;
    let wait = num(&q, "wait")?.unwrap_or(0).clamp(0, 60) as u64;
    let limit = num(&q, "limit")?.unwrap_or(50);
    let notes = s
        .core
        .wait_agent_notes(&p, after, Duration::from_secs(wait), limit)
        .await?;
    let next_cursor = next_cursor(&notes, limit);
    Ok(Json(AgentNotes {
        notifications: notes,
        next_cursor,
    }))
}

/// SSE：先补发游标之后的（`?after=` 或 `Last-Event-ID`；都没有就是没处理的），再实时推。每 25 秒一个注释行保活。
#[utoipa::path(get, path = "/agent/notifications/stream", tag = "notifications",
    description = "SSE：先补发游标之后的（`?after=` / `?cursor=` 或 `Last-Event-ID` 头；都没有就是没处理的），再实时推。\
每条是 `event: notification`，`id` 是通知编号，`data` 是 AgentNotification。每 25 秒一个注释行保活。只有 Agent 能调。",
    params(
        ("after" = Option<i64>, Query, description = "从这个编号之后"),
        ("cursor" = Option<String>, Query, description = "同 after"),
        ("Last-Event-ID" = Option<String>, Header, description = "断线重连时浏览器自动带的"),
    ),
    responses((status = 200, content_type = "text/event-stream", body = AgentNotification, description = "事件流，每条 data 是一个 AgentNotification"),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub(crate) async fn stream_notes(
    State(s): State<AppState>,
    Auth(p): Auth,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>> {
    let last = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<i64>().ok());
    let cursor = after(&q)?.or(last);
    // 先查一次：不是 Agent 的话这里就报错
    s.core.agent_notes(&p, cursor, 1).await?;
    let rx = s.core.notes.subscribe();
    let core = s.core.clone();
    let state = (core, p, cursor, rx, VecDeque::<Value>::new());
    let stream = futures_util::stream::unfold(
        state,
        |(core, p, mut cursor, mut rx, mut queue)| async move {
            loop {
                if let Some(v) = queue.pop_front() {
                    let id = v["id"].as_i64().unwrap_or_default();
                    let e = SseEvent::default()
                        .id(id.to_string())
                        .event("notification")
                        .json_data(&v)
                        .unwrap_or_default();
                    return Some((Ok(e), (core, p, cursor, rx, queue)));
                }
                rx.borrow_and_update();
                match core.agent_notes(&p, cursor, 100).await {
                    Ok(got) if !got.is_empty() => {
                        cursor = got.last().and_then(|v| v["id"].as_i64()).or(cursor);
                        queue.extend(got);
                        continue;
                    }
                    Ok(_) => {}
                    // Agent 被撤销了之类：结束
                    Err(_) => return None,
                }
                if rx.changed().await.is_err() {
                    return None;
                }
            }
        },
    );
    // 先发一行注释：响应头马上出去
    let hello = futures_util::stream::once(async { Ok(SseEvent::default().comment("ok")) });
    let stream = hello.chain(stream).take_until(s.core.on_shutdown());
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(25))))
}

/// 处理完了：给 ids，或者 upto（编号 ≤ upto 的都算），至少给一个。
#[derive(Deserialize, ToSchema)]
pub(crate) struct Ack {
    /// 这些编号
    #[serde(default)]
    ids: Vec<i64>,
    /// 编号 ≤ 这个的
    upto: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct Acked {
    /// 标了几条
    acked: u64,
}

#[utoipa::path(post, path = "/agent/notifications/ack", tag = "notifications", request_body = Ack,
    responses((status = 200, body = Acked), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub(crate) async fn ack_notes(
    State(s): State<AppState>,
    Auth(p): Auth,
    Body(b): Body<Ack>,
) -> ApiResult<Json<Acked>> {
    if b.ids.is_empty() && b.upto.is_none() {
        return Err(ApiError::bad_request("要给 ids 或 upto"));
    }
    let n = s.core.ack_agent_notes(&p, b.ids, b.upto).await?;
    Ok(Json(Acked { acked: n }))
}

// ---- 人的通知 ----

/// 收件箱里的一条。
#[derive(Serialize, ToSchema)]
#[allow(dead_code)] // 只用来写 OpenAPI，返回的是 core 拼好的 JSON
pub(crate) struct InboxNotification {
    id: i64,
    workspace_id: String,
    /// mention / reply / resolved
    #[serde(rename = "type")]
    kind: String,
    /// 页面、评论串、谁写的之类（按 type 不同）
    #[schema(value_type = Object)]
    data: Value,
    created_at: i64,
    /// 没读是 null
    read_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct Inbox {
    /// 新的在前
    #[schema(value_type = Vec<InboxNotification>)]
    notifications: Vec<Value>,
    /// 没读的总数
    unread: i64,
    /// 取满了一页时是最后一条（最旧的）的编号，带 `?cursor=` 往旧的翻；没有就是到头了
    next_cursor: Option<String>,
}

#[utoipa::path(get, path = "/me/notifications", tag = "notifications",
    params(
        ("after" = Option<i64>, Query, description = "从这个编号往旧的翻"),
        ("cursor" = Option<String>, Query, description = "同 after（上次的 next_cursor）"),
        ("limit" = Option<i64>, Query, description = "最多几条，默认 50，最多 100"),
    ),
    responses((status = 200, body = Inbox), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)))]
pub(crate) async fn my_notes(
    State(s): State<AppState>,
    Auth(p): Auth,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Inbox>> {
    let after = after(&q)?;
    let limit = num(&q, "limit")?.unwrap_or(50);
    let notes = s.core.user_notes(&p, after, limit).await?;
    let unread = s.core.storage.unread_user_notifications(&p.user_id).await?;
    let next_cursor = next_cursor(&notes, limit);
    Ok(Json(Inbox {
        notifications: notes,
        unread,
        next_cursor,
    }))
}

/// 标成已读：给 ids，或者 `all: true`。
#[derive(Deserialize, ToSchema)]
pub(crate) struct ReadNotes {
    /// 这些编号
    ids: Option<Vec<i64>>,
    /// 全部
    #[serde(default)]
    all: bool,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ReadCount {
    /// 标了几条
    read: u64,
}

#[utoipa::path(post, path = "/me/notifications/read", tag = "notifications", request_body = ReadNotes,
    responses((status = 200, body = ReadCount), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)))]
pub(crate) async fn read_my_notes(
    State(s): State<AppState>,
    Auth(p): Auth,
    Body(b): Body<ReadNotes>,
) -> ApiResult<Json<ReadCount>> {
    let ids = match (b.all, b.ids) {
        (true, _) => None,
        (false, Some(ids)) => Some(ids),
        (false, None) => return Err(ApiError::bad_request("要给 ids 或 all: true")),
    };
    let n = s.core.read_user_notes(&p, ids).await?;
    Ok(Json(ReadCount { read: n }))
}

/// 这个模块的接口（合进 [`crate::openapi`]）。
#[derive(OpenApi)]
#[openapi(paths(
    list_agents,
    members,
    create_agent,
    update_agent,
    revoke_agent,
    rotate_token,
    rotate_secret,
    authors,
    list_comments,
    create_thread,
    reply,
    edit_comment,
    delete_comment,
    patch_thread,
    poll_notes,
    stream_notes,
    ack_notes,
    my_notes,
    read_my_notes
))]
pub(crate) struct Doc;
