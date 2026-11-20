//! `/api/v1` 下的 JSON 接口。
use crate::openapi::{DocNode, ErrorBody};
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
use oad_core::Page;
use oad_core::auth::SESSION_COOKIE;
use oad_core::{AuthRequest, Node, PagePatch, Principal};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::sync::broadcast::error::RecvError;
use utoipa::{IntoParams, OpenApi, ToSchema};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/openapi.json", get(openapi_json))
        .route("/auth/config", get(auth_config))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/me", get(me).patch(crate::members::update_me))
        .route("/workspaces/{ws}/pages", get(list_pages).post(create_page))
        .route("/workspaces/{ws}/events", get(events))
        .route("/workspaces/{ws}/search", get(search))
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
        .merge(crate::history::routes())
        .merge(crate::trash::routes())
        .merge(crate::share::api_routes())
        .merge(crate::members::routes())
        .fallback(|| async { ApiError::not_found("没有这个接口") })
}

pub(crate) type ApiResult<T> = Result<T, ApiError>;

/// 这个模块的接口（合进 [`crate::openapi`]）。
#[derive(OpenApi)]
#[openapi(paths(
    auth_config,
    login,
    logout,
    me,
    list_pages,
    create_page,
    get_page,
    patch_page,
    delete_page,
    get_content,
    put_content,
    get_block,
    put_block,
    insert_blocks,
    delete_block,
    search,
    events,
    crate::sync::handler
))]
pub(crate) struct Doc;

/// 给这个人看的语言：设过的 > 浏览器的 Accept-Language > 中文。
pub(crate) async fn req_lang(
    s: &AppState,
    who: Option<&Principal>,
    headers: &HeaderMap,
) -> oad_core::i18n::Lang {
    if let Some(p) = who
        && let Some(l) = s.core.locale_of(&p.user_id).await
    {
        return l;
    }
    headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .and_then(oad_core::i18n::Lang::from_accept)
        .unwrap_or_default()
}

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

/// 这份接口的 OpenAPI 3.1（不用登录）。
async fn openapi_json() -> Response {
    let mut r = Json(crate::openapi::spec()).into_response();
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    r
}

// ---- 登录 ----

/// 登录方式和站点设置。
#[derive(Serialize, ToSchema)]
pub(crate) struct AuthConfigBody {
    /// 登录方式：local（账号密码）/ header（宿主反代带身份头）/ none（不登录，单人）/ jwt（宿主签的 JWT）
    mode: oad_core::AuthMode,
    /// 服务器的时区（IANA 名字）
    timezone: String,
    signup: oad_core::members::SignupMode,
    /// 允许跨源嵌入的宿主（`OAD_ALLOWED_ORIGINS`，加上 `OAD_PUBLIC_URL` 的 origin），如 `https://app.example.com`。
    /// iframe 里的网页只认这些 origin（和同源）发来的 `oad:token`。旧版服务器没有这个字段
    #[schema(required = false)]
    allowed_origins: Vec<String>,
}

#[utoipa::path(get, path = "/auth/config", tag = "auth", security(()), responses((status = 200, body = AuthConfigBody)))]
pub(crate) async fn auth_config(State(s): State<AppState>) -> Json<AuthConfigBody> {
    let mut allowed_origins: Vec<String> = Vec::new();
    for o in s
        .config
        .allowed_origins
        .iter()
        .chain(s.config.public_url.as_ref())
        .map(|o| crate::guard::normalize_origin(o))
    {
        if !o.is_empty() && !allowed_origins.contains(&o) {
            allowed_origins.push(o);
        }
    }
    Json(AuthConfigBody {
        mode: s.core.auth.mode(),
        timezone: oad_core::tz::name(&s.core.timezone),
        signup: s.core.signup_mode(),
        allowed_origins,
    })
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct LoginBody {
    email: String,
    password: String,
}

pub(crate) fn session_cookie(s: &AppState, token: &str, max_age: i64) -> HeaderValue {
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

#[utoipa::path(post, path = "/auth/login", tag = "auth", security(()), request_body = LoginBody,
    responses((status = 200, body = Me, description = "登录了，Set-Cookie 带会话"), (status = 401, body = ErrorBody), (status = 429, body = ErrorBody)))]
pub(crate) async fn login(
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

#[utoipa::path(post, path = "/auth/logout", tag = "auth", responses((status = 204)))]
pub(crate) async fn logout(State(s): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
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

/// 我是谁、在哪些工作区。
#[derive(Serialize, ToSchema)]
pub(crate) struct Me {
    user: MeUser,
    workspaces: Vec<MeWorkspace>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MeUser {
    id: String,
    name: String,
    email: Option<String>,
    kind: oad_core::PrincipalKind,
    avatar: Option<String>,
    /// zh / en / null（跟浏览器）
    locale: Option<String>,
    timezone: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MeWorkspace {
    id: String,
    name: String,
    icon: Option<String>,
    role: oad_core::Role,
}

pub(crate) async fn me_json(s: &AppState, p: &Principal) -> ApiResult<Me> {
    let user = s
        .core
        .storage
        .user(&p.user_id)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    let workspaces = s
        .core
        .storage
        .user_workspaces(&user.id)
        .await?
        .into_iter()
        .map(|(w, role)| MeWorkspace {
            id: w.id,
            name: w.name,
            icon: w.icon,
            role,
        })
        .collect();
    let prefs = s.core.storage.user_prefs(&user.id).await?;
    Ok(Me {
        user: MeUser {
            id: user.id,
            name: user.name,
            email: user.email,
            kind: user.kind,
            avatar: prefs.avatar,
            locale: prefs.locale,
            timezone: prefs.timezone,
        },
        workspaces,
    })
}

#[utoipa::path(get, path = "/me", tag = "me", responses((status = 200, body = Me), (status = 401, body = ErrorBody)))]
pub(crate) async fn me(State(s): State<AppState>, Auth(p): Auth) -> ApiResult<Json<Me>> {
    Ok(Json(me_json(&s, &p).await?))
}

// ---- 页面 ----

#[derive(Serialize, ToSchema)]
pub(crate) struct PageList {
    /// 不含回收站里的、库里的行；按 (parent_id, position) 排
    pages: Vec<Page>,
}

#[utoipa::path(get, path = "/workspaces/{ws}/pages", tag = "pages", params(("ws" = String, Path, description = "工作区")),
    responses((status = 200, body = PageList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn list_pages(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
) -> ApiResult<Json<PageList>> {
    let pages = s.core.list_pages(&ws, &p).await?;
    Ok(Json(PageList { pages }))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreatePage {
    parent_id: Option<String>,
    after: Option<String>,
    title: Option<String>,
    icon: Option<String>,
}

#[utoipa::path(post, path = "/workspaces/{ws}/pages", tag = "pages", params(("ws" = String, Path)), request_body = CreatePage,
    responses((status = 201, body = Page), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn create_page(
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

#[utoipa::path(get, path = "/pages/{id}", tag = "pages", params(("id" = String, Path)), responses((status = 200, body = Page), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn get_page(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let (page, _) = s.core.page_for(&id, &p, false).await?;
    Ok(Json(page).into_response())
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct PatchPage {
    title: Option<String>,
    #[serde(default, deserialize_with = "double")]
    icon: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    parent_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    after: Option<Option<String>>,
}

#[utoipa::path(patch, path = "/pages/{id}", tag = "pages", params(("id" = String, Path)), request_body = PatchPage,
    responses((status = 200, body = Page), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn patch_page(
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

#[derive(Serialize, ToSchema)]
pub(crate) struct Deleted {
    /// 进了回收站的页面（连子页面）
    deleted: Vec<String>,
}

#[utoipa::path(delete, path = "/pages/{id}", tag = "pages", params(("id" = String, Path)), responses((status = 200, body = Deleted), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn delete_page(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<Deleted>> {
    let gone = s.core.delete_page(&id, &p).await?;
    Ok(Json(Deleted {
        deleted: gone.into_iter().map(|p| p.id).collect(),
    }))
}

// ---- 正文 ----

/// 正文：`markdown`（默认）、`doc`（`?format=json`）或 `html`（`?format=html`，只读），带当前版本（改的时候作为 base 带回来）。
#[derive(Serialize, ToSchema)]
pub(crate) struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    markdown: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<DocNode>)]
    doc: Option<Node>,
    /// 只读 HTML（`?format=html`）：外面包 `<div class="oad-content">`，配 `/embed/content.css` 显示。
    /// 只有白名单里的标签和属性；公式、链接卡片是带 `data-*` 的占位；设了 `OAD_PUBLIC_URL` 时文件、页面链接是完整地址
    #[serde(skip_serializing_if = "Option::is_none")]
    html: Option<String>,
    version: String,
    /// 顶层块一共几个（给了 `limit` 才有）
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<usize>,
}

#[utoipa::path(get, path = "/pages/{id}/content", tag = "content",
    params(("id" = String, Path), ("format" = Option<String>, Query, description = "markdown（默认）/ json / html（只读 HTML，见 `Content.html`）"),
        ("limit" = Option<usize>, Query, description = "只要前 N 个顶层块（先画第一屏），响应里带 `total`")),
    responses((status = 200, body = Content), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn get_content(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Content>> {
    let limit = match q.get("limit") {
        None => None,
        Some(l) => Some(
            l.parse::<usize>()
                .map_err(|_| ApiError::bad_request(format!("limit 要是非负整数：{l}")))?,
        ),
    };
    let (doc, total, version) = s.core.content_head(&id, &p, limit).await?;
    let version = b64(&version);
    let total = limit.map(|_| total);
    match q.get("format").map(String::as_str) {
        None | Some("markdown") => {
            let md = blocking(move || oad_markdown::serialize(&doc)).await?;
            Ok(Json(Content {
                markdown: Some(md),
                doc: None,
                html: None,
                version,
                total,
            }))
        }
        Some("json") => Ok(Json(Content {
            markdown: None,
            doc: Some(doc),
            html: None,
            version,
            total,
        })),
        Some("html") => {
            // 嵌在别的域名下时，/api/v1/files/… 这种相对地址要拼上对外的地址才打得开
            let opts = oad_markdown::HtmlOptions {
                base_url: s.config.public_url.clone(),
            };
            let html = blocking(move || oad_markdown::to_html(&doc, &opts)).await?;
            Ok(Json(Content {
                markdown: None,
                doc: None,
                html: Some(html),
                version,
                total,
            }))
        }
        Some(f) => Err(ApiError::bad_request(format!("不支持的 format={f}"))),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct PutContent {
    markdown: Option<String>,
    #[schema(value_type = Option<DocNode>)]
    doc: Option<Node>,
    /// 读的时候拿到的版本：在那个版本上改，这期间别人写的不会被覆盖
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

#[utoipa::path(put, path = "/pages/{id}/content", tag = "content", params(("id" = String, Path)), request_body = PutContent,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn put_content(
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

#[derive(Serialize, ToSchema)]
pub(crate) struct BlockContent {
    #[schema(value_type = DocNode)]
    block: Node,
    markdown: String,
    version: String,
}

#[utoipa::path(get, path = "/pages/{id}/blocks/{block_id}", tag = "content", params(("id" = String, Path), ("block_id" = String, Path)),
    responses((status = 200, body = BlockContent), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn get_block(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, block_id)): Path<(String, String)>,
) -> ApiResult<Json<BlockContent>> {
    let (block, version) = s.core.block(&id, &block_id, &p).await?;
    let markdown = oad_markdown::serialize_block(&block);
    Ok(Json(BlockContent {
        block,
        markdown,
        version: b64(&version),
    }))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct BlockBody {
    markdown: String,
    after: Option<String>,
    base: Option<String>,
}

fn parse_blocks(md: &str) -> Vec<Node> {
    oad_markdown::parse(md).content
}

#[derive(Serialize, ToSchema)]
pub(crate) struct BlockIds {
    ids: Vec<String>,
}

#[utoipa::path(put, path = "/pages/{id}/blocks/{block_id}", tag = "content", params(("id" = String, Path), ("block_id" = String, Path)),
    request_body = BlockBody, responses((status = 200, body = BlockIds), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn put_block(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, block_id)): Path<(String, String)>,
    Body(b): Body<BlockBody>,
) -> ApiResult<Json<BlockIds>> {
    let nodes = parse_blocks(&b.markdown);
    let ids = s
        .core
        .replace_block(&id, &block_id, &p, nodes, unb64(b.base)?)
        .await?;
    Ok(Json(BlockIds { ids }))
}

#[utoipa::path(post, path = "/pages/{id}/blocks", tag = "content", params(("id" = String, Path)),
    request_body = BlockBody, responses((status = 200, body = BlockIds), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn insert_blocks(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<BlockBody>,
) -> ApiResult<Json<BlockIds>> {
    let nodes = parse_blocks(&b.markdown);
    let ids = s.core.insert_blocks(&id, b.after, &p, nodes).await?;
    Ok(Json(BlockIds { ids }))
}

#[utoipa::path(delete, path = "/pages/{id}/blocks/{block_id}", tag = "content", params(("id" = String, Path), ("block_id" = String, Path)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn delete_block(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, block_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    s.core.delete_block(&id, &block_id, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- 实时事件 ----

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct SearchParams {
    /// 搜索词
    q: String,
    /// 最多几条，默认 20，最多 100
    limit: Option<usize>,
    /// page / row / all（默认）
    kind: Option<String>,
    /// `block`：每块一条（同一页最多两条）；默认每页一条
    per: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct SearchResults {
    results: Vec<oad_core::search::SearchResult>,
}

#[utoipa::path(get, path = "/workspaces/{ws}/search", tag = "search", params(("ws" = String, Path), SearchParams),
    responses((status = 200, body = SearchResults), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn search(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    Query(q): Query<SearchParams>,
) -> ApiResult<Json<SearchResults>> {
    let results = s
        .core
        .search_index(
            &ws,
            &p,
            &q.q,
            q.kind.as_deref(),
            q.limit.unwrap_or(20),
            q.per.as_deref() != Some("block"),
        )
        .await?;
    Ok(Json(SearchResults { results }))
}

#[utoipa::path(get, path = "/workspaces/{ws}/events", tag = "events", params(("ws" = String, Path)),
    description = "SSE：工作区的实时事件，事件名和数据见 webhooks（page.created、db.changed……）。慢了会收到 resync。",
    responses((status = 200, content_type = "text/event-stream", description = "事件流"), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn events(
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
    // 先发一行注释：响应头马上出去（不然要等第一个事件，经代理时客户端一直等着）
    let hello = futures_util::stream::once(async { Ok(SseEvent::default().comment("ok")) });
    let stream = hello.chain(stream).take_until(s.core.on_shutdown());
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
