//! 回收站：`GET /workspaces/{ws}/trash`、`POST /trash/{id}/restore`、`DELETE /trash/{id}`。
use crate::AppState;
use crate::api::{ApiResult, Auth};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{ws}/trash", get(list))
        .route("/trash/{id}/restore", post(restore))
        .route("/trash/{id}", delete(purge))
}

async fn list(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "items": s.core.list_trash(&ws, &p).await? })))
}

/// → 放回来的那个页面，加上 `restored`（连同放回来的所有页面 id）。
async fn restore(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let pages = s.core.restore_trash(&id, &p).await?;
    let mut v = serde_json::to_value(&pages[0]).unwrap_or_default();
    v["restored"] = json!(pages.iter().map(|p| p.id.clone()).collect::<Vec<_>>());
    Ok(Json(v))
}

async fn purge(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    s.core.purge_trash(&id, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}
