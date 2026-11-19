//! 第 2 阶段的接口：Agent 管理、作者、评论、通知。见 docs/phase2-contract.md。
use crate::api::{ApiResult, Auth, Body, double};
use crate::{ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use futures_util::{Stream, StreamExt};
use oad_core::{AgentPatch, Anchor, NotifySpec, Scope};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::time::Duration;

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

async fn list_agents(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
) -> ApiResult<Json<Value>> {
    let agents = s.core.list_agents(&ws, &p).await?;
    Ok(Json(json!({ "agents": agents })))
}

async fn members(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
) -> ApiResult<Json<Value>> {
    let (people, agents) = s.core.members(&ws, &p).await?;
    let mut members: Vec<Value> = Vec::with_capacity(people.len());
    for (u, role) in people {
        let avatar = s.core.storage.user_prefs(&u.id).await?.avatar;
        members.push(json!({ "user": { "id": u.id, "name": u.name, "email": u.email, "avatar": avatar }, "role": role }));
    }
    Ok(Json(json!({ "members": members, "agents": agents })))
}

#[derive(Deserialize)]
struct CreateAgent {
    name: String,
    about: Option<String>,
    scope: String,
    webhook: Option<String>,
}

async fn create_agent(
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
        Json(json!({ "agent": agent, "token": token, "webhook_secret": secret })),
    )
        .into_response())
}

#[derive(Deserialize)]
struct PatchAgent {
    name: Option<String>,
    about: Option<String>,
    scope: Option<String>,
    #[serde(default, deserialize_with = "double")]
    webhook: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    locale: Option<Option<String>>,
}

async fn update_agent(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<PatchAgent>,
) -> ApiResult<Response> {
    let patch = AgentPatch {
        name: b.name,
        about: b.about,
        scope: b.scope.as_deref().map(parse_scope).transpose()?,
        webhook: b.webhook,
        locale: b.locale,
    };
    Ok(Json(s.core.update_agent(&id, &p, patch).await?).into_response())
}

async fn revoke_agent(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    Ok(Json(s.core.revoke_agent(&id, &p).await?).into_response())
}

async fn rotate_token(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let token = s.core.rotate_agent_token(&id, &p).await?;
    Ok(Json(json!({ "token": token })))
}

async fn rotate_secret(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let secret = s.core.rotate_webhook_secret(&id, &p).await?;
    Ok(Json(json!({ "webhook_secret": secret })))
}

// ---- 作者 ----

async fn authors(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let clients = s.core.authors(&id, &p).await?;
    Ok(Json(json!({ "clients": clients })))
}

// ---- 评论 ----

async fn list_comments(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
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
    Ok(Json(json!({ "threads": threads })))
}

#[derive(Deserialize)]
struct NewThread {
    anchor: Option<Anchor>,
    body: String,
    notify: Option<Value>,
}

async fn create_thread(
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

#[derive(Deserialize)]
struct Reply {
    body: String,
    notify: Option<Value>,
}

async fn reply(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<Reply>,
) -> ApiResult<Response> {
    let notify = NotifySpec::from_json(b.notify.as_ref())?;
    let c = s.core.reply(&id, &p, &b.body, notify).await?;
    Ok((StatusCode::CREATED, Json(c)).into_response())
}

#[derive(Deserialize)]
struct EditComment {
    body: String,
}

async fn edit_comment(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<EditComment>,
) -> ApiResult<Response> {
    let t = s.core.edit_comment(&id, &p, &b.body).await?;
    let c = t.comments.into_iter().find(|c| c.id == id);
    Ok(Json(c).into_response())
}

async fn delete_comment(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    s.core.delete_comment(&id, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct PatchThread {
    resolved: bool,
}

async fn patch_thread(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<PatchThread>,
) -> ApiResult<Response> {
    Ok(Json(s.core.resolve_thread(&id, &p, b.resolved).await?).into_response())
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

/// 长轮询：`?after=<id>&wait=<秒，最多 60>&limit=<最多 100>`。
async fn poll_notes(
    State(s): State<AppState>,
    Auth(p): Auth,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let after = num(&q, "after")?;
    let wait = num(&q, "wait")?.unwrap_or(0).clamp(0, 60) as u64;
    let limit = num(&q, "limit")?.unwrap_or(50);
    let notes = s
        .core
        .wait_agent_notes(&p, after, Duration::from_secs(wait), limit)
        .await?;
    Ok(Json(json!({ "notifications": notes })))
}

/// SSE：先补发游标之后的（`?after=` 或 `Last-Event-ID`；都没有就是没处理的），再实时推。每 25 秒一个注释行保活。
async fn stream_notes(
    State(s): State<AppState>,
    Auth(p): Auth,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>> {
    let last = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<i64>().ok());
    let cursor = num(&q, "after")?.or(last);
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
    let stream = stream.take_until(s.core.on_shutdown());
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(25))))
}

#[derive(Deserialize)]
struct Ack {
    #[serde(default)]
    ids: Vec<i64>,
    upto: Option<i64>,
}

async fn ack_notes(
    State(s): State<AppState>,
    Auth(p): Auth,
    Body(b): Body<Ack>,
) -> ApiResult<Json<Value>> {
    if b.ids.is_empty() && b.upto.is_none() {
        return Err(ApiError::bad_request("要给 ids 或 upto"));
    }
    let n = s.core.ack_agent_notes(&p, b.ids, b.upto).await?;
    Ok(Json(json!({ "acked": n })))
}

// ---- 人的通知 ----

async fn my_notes(
    State(s): State<AppState>,
    Auth(p): Auth,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let after = num(&q, "after")?;
    let limit = num(&q, "limit")?.unwrap_or(50);
    let notes = s.core.user_notes(&p, after, limit).await?;
    let unread = s.core.storage.unread_user_notifications(&p.user_id).await?;
    Ok(Json(json!({ "notifications": notes, "unread": unread })))
}

#[derive(Deserialize)]
struct ReadNotes {
    ids: Option<Vec<i64>>,
    #[serde(default)]
    all: bool,
}

async fn read_my_notes(
    State(s): State<AppState>,
    Auth(p): Auth,
    Body(b): Body<ReadNotes>,
) -> ApiResult<Json<Value>> {
    let ids = match (b.all, b.ids) {
        (true, _) => None,
        (false, Some(ids)) => Some(ids),
        (false, None) => return Err(ApiError::bad_request("要给 ids 或 all: true")),
    };
    let n = s.core.read_user_notes(&p, ids).await?;
    Ok(Json(json!({ "read": n })))
}
