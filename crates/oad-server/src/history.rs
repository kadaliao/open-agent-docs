//! 第 5 阶段：页面的历史版本、数据库的历史和撤销。见 docs/phase5-contract.md。
use crate::api::{ApiResult, Auth, Body};
use crate::openapi::{DocNode, ErrorBody};
use crate::{ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use oad_core::history::BlockDiff;
use oad_core::{Node, VersionMeta};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/pages/{id}/versions", get(list).post(save))
        .route("/pages/{id}/versions/{v}", get(one))
        .route("/pages/{id}/versions/{v}/diff", get(diff))
        .route("/pages/{id}/versions/{v}/restore", post(restore))
        .route("/databases/{id}/history", get(db_history))
        .route("/databases/{id}/history/undo", post(db_undo))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct Limit {
    /// 一页几条，默认 200
    limit: Option<usize>,
    /// 上一页给的 `next_cursor`
    cursor: Option<String>,
}

/// 版本列表，新的在前。
#[derive(Serialize, ToSchema)]
pub(crate) struct VersionList {
    versions: Vec<VersionMeta>,
    /// 还有下一页时给，带上它再取
    next_cursor: Option<String>,
}

#[utoipa::path(get, path = "/pages/{id}/versions", tag = "history", params(("id" = String, Path), Limit),
    responses((status = 200, body = VersionList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn list(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Query(q): Query<Limit>,
) -> ApiResult<Json<VersionList>> {
    let (versions, next_cursor) = s
        .core
        .list_versions(&id, &p, q.limit.unwrap_or(200), q.cursor.as_deref())
        .await?;
    Ok(Json(VersionList {
        versions,
        next_cursor,
    }))
}

#[derive(Deserialize, Default, ToSchema)]
pub(crate) struct SaveBody {
    /// 这一版的名字
    label: Option<String>,
}

#[utoipa::path(post, path = "/pages/{id}/versions", tag = "history", params(("id" = String, Path)),
    request_body(content = Option<SaveBody>, description = "可以不带正文"),
    responses((status = 201, body = VersionMeta, description = "手动存的一版（kind = manual）"), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn save(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    body: Option<Json<SaveBody>>,
) -> ApiResult<Response> {
    let b = body.map(|Json(b)| b).unwrap_or_default();
    let v = s.core.save_version(&id, &p, b.label).await?;
    Ok((StatusCode::CREATED, Json(v)).into_response())
}

/// 某一版的内容。
#[derive(Serialize, ToSchema)]
pub(crate) struct VersionContent {
    version: VersionMeta,
    #[schema(value_type = DocNode)]
    doc: Node,
    markdown: String,
}

#[utoipa::path(get, path = "/pages/{id}/versions/{v}", tag = "history", params(("id" = String, Path), ("v" = i64, Path, description = "版本号")),
    responses((status = 200, body = VersionContent), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn one(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, v)): Path<(String, i64)>,
) -> ApiResult<Json<VersionContent>> {
    let (version, doc) = s.core.version_content(&id, v, &p).await?;
    let markdown = oad_markdown::serialize(&doc);
    Ok(Json(VersionContent {
        version,
        doc,
        markdown,
    }))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct DiffQ {
    /// 和哪一版比：版本号，或 `current`（默认，现在的正文）
    to: Option<String>,
}

/// 对比的另一头：版本号，或 `"current"`。
#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub(crate) enum DiffTo {
    Version(i64),
    Current(&'static str),
}

#[derive(Serialize, ToSchema)]
pub(crate) struct VersionDiff {
    from: i64,
    to: DiffTo,
    /// 按块的增删改，按新的顺序
    blocks: Vec<BlockDiff>,
}

#[utoipa::path(get, path = "/pages/{id}/versions/{v}/diff", tag = "history", params(("id" = String, Path), ("v" = i64, Path, description = "版本号"), DiffQ),
    responses((status = 200, body = VersionDiff), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn diff(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, v)): Path<(String, i64)>,
    Query(q): Query<DiffQ>,
) -> ApiResult<Json<VersionDiff>> {
    let to = match q.to.as_deref() {
        None | Some("current") | Some("") => None,
        Some(x) => Some(
            x.parse::<i64>()
                .map_err(|_| ApiError::bad_request("to 要是版本号或 current"))?,
        ),
    };
    let blocks = s.core.version_diff(&id, v, to, &p).await?;
    Ok(Json(VersionDiff {
        from: v,
        to: to
            .map(DiffTo::Version)
            .unwrap_or(DiffTo::Current("current")),
        blocks,
    }))
}

#[derive(Serialize, ToSchema)]
pub(crate) struct Restored {
    /// 恢复后新存的一版（kind = restore）
    version: VersionMeta,
}

#[utoipa::path(post, path = "/pages/{id}/versions/{v}/restore", tag = "history", params(("id" = String, Path), ("v" = i64, Path, description = "版本号")),
    responses((status = 200, body = Restored), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn restore(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, v)): Path<(String, i64)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Restored>> {
    let lang = crate::api::req_lang(&s, Some(&p), &headers).await;
    let version = s.core.restore_version(&id, v, &p, lang).await?;
    Ok(Json(Restored { version }))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct HistQ {
    /// 只看这一行
    row: Option<String>,
    /// 只看这个属性（key 或名字）
    prop: Option<String>,
    /// 一页几条，默认 200，最多 1000
    limit: Option<usize>,
    /// 上一页给的 `next_cursor`
    cursor: Option<String>,
}

// 只用来写文档：正文是核心给的 JSON，形状和这里一样
/// 库的历史。
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct DbHistory {
    /// 新的在前；同一次改动（同一个 seq）的格子不会分在两页
    items: Vec<DbHistoryItem>,
    /// 库现在的 seq
    seq: i64,
    /// 还有下一页时给
    next_cursor: Option<String>,
}

/// 历史里的一条。`kind`：`cell`（一格改了）、`row`（加 / 删了一行）、`schema`（表结构改了，只在不按行 / 属性筛时有）。
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct DbHistoryItem {
    seq: i64,
    /// cell / row / schema
    kind: String,
    /// 谁改的：`{ kind, id, name }`，认不出的人是原样的引用字符串，没记是 null
    #[schema(value_type = Option<Object>)]
    by: Option<Value>,
    at: i64,
    /// cell / row 有
    row_id: Option<String>,
    /// cell / row 有
    row_title: Option<String>,
    /// row / schema 有：一句话说明
    summary: Option<String>,
    /// cell 有：属性的 key
    prop: Option<String>,
    /// cell 有：属性的名字
    prop_name: Option<String>,
    /// cell 有：改之前存的值（任意 JSON）
    #[schema(value_type = Option<Object>)]
    old: Option<Value>,
    /// cell 有：改之后存的值（任意 JSON）
    #[schema(value_type = Option<Object>)]
    new: Option<Value>,
    /// schema 有：这次动了哪些属性
    #[schema(value_type = Option<Vec<Object>>)]
    changes: Option<Vec<Value>>,
    /// schema 有：顺带改了几格
    cells: Option<usize>,
    /// schema 有：撤销过了
    undone: Option<bool>,
}

#[utoipa::path(get, path = "/databases/{id}/history", tag = "history", params(("id" = String, Path, description = "库的 id"), HistQ),
    responses((status = 200, body = DbHistory), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn db_history(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Query(q): Query<HistQ>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let lang = crate::api::req_lang(&s, Some(&p), &headers).await;
    Ok(Json(
        s.core
            .db_history(
                &id,
                &p,
                oad_core::db::HistoryQuery {
                    row: q.row.as_deref(),
                    prop: q.prop.as_deref(),
                    limit: q.limit.unwrap_or(200),
                    cursor: q.cursor.as_deref(),
                },
                lang,
            )
            .await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct UndoBody {
    /// 要撤销的那次改动（历史里的 seq）
    seq: i64,
}

// 只用来写文档：正文是核心给的 JSON，形状和这里一样
/// 撤销的结果：格子改动只有 `seq`；表结构改动还有新的 `schema`、`views`。
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct DbUndone {
    /// 撤销后库的 seq
    seq: i64,
    #[schema(value_type = Option<Object>)]
    schema: Option<Value>,
    #[schema(value_type = Option<Vec<Object>>)]
    views: Option<Value>,
}

#[utoipa::path(post, path = "/databases/{id}/history/undo", tag = "history", params(("id" = String, Path, description = "库的 id")), request_body = UndoBody,
    responses((status = 200, body = DbUndone), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn db_undo(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<UndoBody>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.core.undo_db(&id, &p, b.seq).await?))
}

/// 这个模块的接口（合进 [`crate::openapi`]）。
#[derive(utoipa::OpenApi)]
#[openapi(paths(list, save, one, diff, restore, db_history, db_undo))]
pub(crate) struct Doc;
