//! 回收站：`GET /workspaces/{ws}/trash`、`POST /trash/{id}/restore`、`DELETE /trash/{id}`。
use crate::AppState;
use crate::api::{ApiResult, Auth};
use crate::openapi::ErrorBody;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use oad_core::Page;
use oad_core::trash::TrashItem;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{ws}/trash", get(list))
        .route("/trash/{id}/restore", post(restore))
        .route("/trash/{id}", delete(purge))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct Paging {
    /// 一页几条，默认 200
    limit: Option<usize>,
    /// 上一页给的 `next_cursor`
    cursor: Option<String>,
}

/// 回收站里的页面、库、行（只列单独删的，新删的在前）。
#[derive(Serialize, ToSchema)]
pub(crate) struct TrashList {
    items: Vec<TrashItem>,
    /// 还有下一页时给
    next_cursor: Option<String>,
}

#[utoipa::path(get, path = "/workspaces/{ws}/trash", tag = "trash", params(("ws" = String, Path, description = "工作区"), Paging),
    responses((status = 200, body = TrashList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn list(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    Query(q): Query<Paging>,
) -> ApiResult<Json<TrashList>> {
    let (items, next_cursor) = s
        .core
        .list_trash(&ws, &p, q.limit.unwrap_or(200), q.cursor.as_deref())
        .await?;
    Ok(Json(TrashList { items, next_cursor }))
}

/// 放回来的那个页面，加上 `restored`（连同放回来的所有页面 id）。
#[derive(Serialize, ToSchema)]
pub(crate) struct RestoredPage {
    #[serde(flatten)]
    page: Page,
    /// 放回来的所有页面（这个和一起删的子页面）
    restored: Vec<String>,
}

#[utoipa::path(post, path = "/trash/{id}/restore", tag = "trash", params(("id" = String, Path, description = "回收站里的页面 id")),
    responses((status = 200, body = RestoredPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn restore(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<RestoredPage>> {
    let pages = s.core.restore_trash(&id, &p).await?;
    let restored = pages.iter().map(|p| p.id.clone()).collect();
    let page = pages.into_iter().next().ok_or_else(|| {
        crate::ApiError::from(oad_core::Error::Internal("放回来的页面是空的".into()))
    })?;
    Ok(Json(RestoredPage { page, restored }))
}

#[utoipa::path(delete, path = "/trash/{id}", tag = "trash", params(("id" = String, Path, description = "回收站里的页面 id")),
    responses((status = 204, description = "彻底删了（连子页面、正文、版本、评论）"), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn purge(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    s.core.purge_trash(&id, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 这个模块的接口（合进 [`crate::openapi`]）。
#[derive(utoipa::OpenApi)]
#[openapi(paths(list, restore, purge))]
pub(crate) struct Doc;
