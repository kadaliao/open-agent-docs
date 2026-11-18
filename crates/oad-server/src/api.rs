//! `/api/v1` 下的 JSON 接口。
use crate::{ApiError, AppState, sync};
use axum::extract::{ConnectInfo, FromRequest, FromRequestParts, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use futures_util::{Stream, StreamExt};
use oad_core::auth::SESSION_COOKIE;
use oad_core::{AuthRequest, Node, PagePatch, Principal};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::sync::broadcast::error::RecvError;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/config", get(auth_config))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/me", get(me))
        .route("/workspaces/{ws}/pages", get(list_pages).post(create_page))
        .route("/workspaces/{ws}/events", get(events))
        .route(
            "/pages/{id}",
            get(get_page).patch(patch_page).delete(delete_page),
        )
        .route("/pages/{id}/content", get(get_content).put(put_content))
        .route("/pages/{id}/blocks", post(insert_blocks))
        .route(
            "/pages/{id}/blocks/{block_id}",
            get(get_block).put(put_block).delete(delete_block),
        )
        .route("/sync/{page_id}", get(sync::handler))
        .merge(crate::collab::routes())
        .merge(crate::files::routes())
        .merge(crate::dbs::routes())
        .fallback(|| async { ApiError::not_found("没有这个接口") })
}

pub(crate) type ApiResult<T> = Result<T, ApiError>;

/// `?token=` 的值。
pub fn query_token(query: Option<&str>) -> Option<&str> {
    query?
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == "token")
        .map(|(_, v)| v)
}

pub async fn authenticate(state: &AppState, parts: &Parts) -> ApiResult<Option<Principal>> {
    let req = AuthRequest {
        headers: &parts.headers,
        query_token: query_token(parts.uri.query()),
    };
    Ok(state.core.authenticate(&req).await?)
}

/// 登录了的调用者。
pub struct Auth(pub Principal);

impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> ApiResult<Self> {
        authenticate(state, parts)
            .await?
            .map(Auth)
            .ok_or_else(ApiError::unauthorized)
    }
}

/// JSON 请求体；格式不对时按统一的错误格式回。
pub struct Body<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for Body<T> {
    type Rejection = ApiError;
    async fn from_request(req: Request, state: &S) -> ApiResult<Self> {
        Json::<T>::from_request(req, state)
            .await
            .map(|Json(v)| Body(v))
            .map_err(|e| ApiError::bad_request(e.body_text()))
    }
}

/// 区分“没给”（None）和“给了 null”（Some(None)）。
pub(crate) fn double<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// CPU 重的活（大文档的 Markdown 转换）放到 blocking 线程。
async fn blocking<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> ApiResult<R> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::from(oad_core::Error::Internal(e.to_string())))
}

// ---- 登录 ----

async fn auth_config(State(s): State<AppState>) -> Json<Value> {
    Json(json!({ "mode": s.core.auth.mode() }))
}

#[derive(Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

fn session_cookie(s: &AppState, token: &str, max_age: i64) -> HeaderValue {
    let secure = if s.config.cookie_secure {
        "; Secure"
    } else {
        ""
    };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .unwrap()
}

async fn login(
    State(s): State<AppState>,
    parts: Parts,
    Body(b): Body<LoginBody>,
) -> ApiResult<Response> {
    let ip = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_default();
    let key = format!("{ip}|{}", b.email.trim().to_lowercase());
    if let Some(wait) = s.login_limit.check(&key) {
        let mut res = ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_requests",
            "失败次数太多，稍后再试",
        )
        .into_response();
        res.headers_mut().insert(
            header::RETRY_AFTER,
            HeaderValue::from(wait.as_secs().max(1)),
        );
        return Ok(res);
    }
    let (token, user) = match s.core.auth.login(&b.email, &b.password).await {
        Ok(v) => {
            s.login_limit.success(&key);
            v
        }
        Err(e) => {
            if matches!(e, oad_core::Error::Unauthorized(_)) {
                s.login_limit.fail(&key);
            }
            return Err(e.into());
        }
    };
    let me = me_json(&s, &Principal::from(&user)).await?;
    let max_age = oad_core::auth::SESSION_DAYS * 24 * 3600;
    let mut res = Json(me).into_response();
    res.headers_mut()
        .insert(header::SET_COOKIE, session_cookie(&s, &token, max_age));
    Ok(res)
}

async fn logout(State(s): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    let req = AuthRequest {
        headers: &headers,
        query_token: None,
    };
    s.core.auth.logout(&req).await?;
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut()
        .insert(header::SET_COOKIE, session_cookie(&s, "", 0));
    Ok(res)
}

async fn me_json(s: &AppState, p: &Principal) -> ApiResult<Value> {
    let user = s
        .core
        .storage
        .user(&p.user_id)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    let ws: Vec<Value> = s
        .core
        .storage
        .user_workspaces(&user.id)
        .await?
        .into_iter()
        .map(|(w, role)| json!({ "id": w.id, "name": w.name, "role": role }))
        .collect();
    Ok(json!({
        "user": { "id": user.id, "name": user.name, "email": user.email, "kind": user.kind },
        "workspaces": ws,
    }))
}

async fn me(State(s): State<AppState>, Auth(p): Auth) -> ApiResult<Json<Value>> {
    Ok(Json(me_json(&s, &p).await?))
}

// ---- 页面 ----

async fn list_pages(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
) -> ApiResult<Json<Value>> {
    let pages = s.core.list_pages(&ws, &p).await?;
    Ok(Json(json!({ "pages": pages })))
}

#[derive(Deserialize)]
struct CreatePage {
    parent_id: Option<String>,
    after: Option<String>,
    title: Option<String>,
    icon: Option<String>,
}

async fn create_page(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    Body(b): Body<CreatePage>,
) -> ApiResult<Response> {
    let page = s
        .core
        .create_page(&ws, &p, b.parent_id, b.after, b.title, b.icon)
        .await?;
    Ok((StatusCode::CREATED, Json(page)).into_response())
}

async fn get_page(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let (page, _) = s.core.page_for(&id, &p, false).await?;
    Ok(Json(page).into_response())
}

#[derive(Deserialize)]
struct PatchPage {
    title: Option<String>,
    #[serde(default, deserialize_with = "double")]
    icon: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    parent_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    after: Option<Option<String>>,
}

async fn patch_page(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<PatchPage>,
) -> ApiResult<Response> {
    let patch = PagePatch {
        title: b.title,
        icon: b.icon,
        parent_id: b.parent_id,
        after: b.after,
    };
    let page = s.core.update_page(&id, &p, patch).await?;
    Ok(Json(page).into_response())
}

async fn delete_page(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let gone = s.core.delete_page(&id, &p).await?;
    let ids: Vec<&str> = gone.iter().map(|p| p.id.as_str()).collect();
    Ok(Json(json!({ "deleted": ids })))
}

// ---- 正文 ----

async fn get_content(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let (doc, version) = s.core.content(&id, &p).await?;
    let version = b64(&version);
    match q.get("format").map(String::as_str) {
        None | Some("markdown") => {
            let md = blocking(move || oad_markdown::serialize(&doc)).await?;
            Ok(Json(json!({ "markdown": md, "version": version })))
        }
        Some("json") => Ok(Json(json!({ "doc": doc, "version": version }))),
        Some(f) => Err(ApiError::bad_request(format!("不支持的 format={f}"))),
    }
}

#[derive(Deserialize)]
struct PutContent {
    markdown: Option<String>,
    doc: Option<Node>,
    base: Option<String>,
}

fn b64(v: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(v)
}

fn unb64(v: Option<String>) -> ApiResult<Option<Vec<u8>>> {
    v.map(|s| {
        URL_SAFE_NO_PAD
            .decode(s.trim_end_matches('='))
            .map_err(|_| ApiError::bad_request("base 不是合法的版本"))
    })
    .transpose()
}

async fn put_content(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<PutContent>,
) -> ApiResult<StatusCode> {
    let doc = match (b.markdown, b.doc) {
        (Some(md), None) => blocking(move || oad_markdown::parse(&md)).await?,
        (None, Some(doc)) if doc.ty == "doc" => doc,
        (None, Some(_)) => return Err(ApiError::bad_request("doc 的 type 要是 \"doc\"")),
        _ => return Err(ApiError::bad_request("要给 markdown 或 doc 其中一个")),
    };
    s.core.set_content(&id, &p, doc, unb64(b.base)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_block(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, block_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let (block, version) = s.core.block(&id, &block_id, &p).await?;
    let md = oad_markdown::serialize_block(&block);
    Ok(Json(
        json!({ "block": block, "markdown": md, "version": b64(&version) }),
    ))
}

#[derive(Deserialize)]
struct BlockBody {
    markdown: String,
    after: Option<String>,
    base: Option<String>,
}

fn parse_blocks(md: &str) -> Vec<Node> {
    oad_markdown::parse(md).content
}

async fn put_block(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, block_id)): Path<(String, String)>,
    Body(b): Body<BlockBody>,
) -> ApiResult<Json<Value>> {
    let nodes = parse_blocks(&b.markdown);
    let ids = s
        .core
        .replace_block(&id, &block_id, &p, nodes, unb64(b.base)?)
        .await?;
    Ok(Json(json!({ "ids": ids })))
}

async fn insert_blocks(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<BlockBody>,
) -> ApiResult<Json<Value>> {
    let nodes = parse_blocks(&b.markdown);
    let ids = s.core.insert_blocks(&id, b.after, &p, nodes).await?;
    Ok(Json(json!({ "ids": ids })))
}

async fn delete_block(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, block_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    s.core.delete_block(&id, &block_id, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- 实时事件 ----

async fn events(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
) -> ApiResult<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>> {
    s.core.require(&ws, &p, false).await?;
    let rx = s.core.events.subscribe();
    let me = p.user_id.clone();
    let stream = futures_util::stream::unfold((rx, ws, me), |(mut rx, ws, me)| async move {
        loop {
            match rx.recv().await {
                // 人的通知只推给本人
                Ok(ev) if ev.workspace_id == ws && ev.user_id.as_ref().is_none_or(|u| *u == me) => {
                    let e = SseEvent::default()
                        .event(ev.name)
                        .json_data(&ev.data)
                        .unwrap_or_default();
                    return Some((Ok(e), (rx, ws, me)));
                }
                Ok(_) => continue,
                // 落下了事件：让网页重新拉列表
                Err(RecvError::Lagged(_)) => {
                    let e = SseEvent::default().event("resync").data("{}");
                    return Some((Ok(e), (rx, ws, me)));
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });
    // 关机时结束，不然 HTTP 服务停不下来
    let stream = stream.take_until(s.core.on_shutdown());
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
