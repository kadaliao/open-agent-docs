//! 第 5 阶段：页面的历史版本、数据库的历史和撤销。见 docs/phase5-contract.md。
use crate::api::{ApiResult, Auth, Body};
use crate::{ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/pages/{id}/versions", get(list).post(save))
        .route("/pages/{id}/versions/{v}", get(one))
        .route("/pages/{id}/versions/{v}/diff", get(diff))
        .route("/pages/{id}/versions/{v}/restore", post(restore))
        .route("/databases/{id}/history", get(db_history))
        .route("/databases/{id}/history/undo", post(db_undo))
}

#[derive(Deserialize)]
struct Limit {
    limit: Option<usize>,
}

async fn list(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Query(q): Query<Limit>,
) -> ApiResult<Json<Value>> {
    let v = s
        .core
        .list_versions(&id, &p, q.limit.unwrap_or(200))
        .await?;
    Ok(Json(json!({ "versions": v })))
}

#[derive(Deserialize, Default)]
struct SaveBody {
    label: Option<String>,
}

async fn save(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    body: Option<Json<SaveBody>>,
) -> ApiResult<Response> {
    let b = body.map(|Json(b)| b).unwrap_or_default();
    let v = s.core.save_version(&id, &p, b.label).await?;
    Ok((StatusCode::CREATED, Json(v)).into_response())
}

async fn one(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, v)): Path<(String, i64)>,
) -> ApiResult<Json<Value>> {
    let (meta, doc) = s.core.version_content(&id, v, &p).await?;
    let md = oad_markdown::serialize(&doc);
    Ok(Json(json!({ "version": meta, "doc": doc, "markdown": md })))
}

#[derive(Deserialize)]
struct DiffQ {
    to: Option<String>,
}

async fn diff(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, v)): Path<(String, i64)>,
    Query(q): Query<DiffQ>,
) -> ApiResult<Json<Value>> {
    let to = match q.to.as_deref() {
        None | Some("current") | Some("") => None,
        Some(x) => Some(
            x.parse::<i64>()
                .map_err(|_| ApiError::bad_request("to 要是版本号或 current"))?,
        ),
    };
    let blocks = s.core.version_diff(&id, v, to, &p).await?;
    Ok(Json(json!({
        "from": v,
        "to": to.map(|t| json!(t)).unwrap_or(json!("current")),
        "blocks": blocks,
    })))
}

async fn restore(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, v)): Path<(String, i64)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let lang = crate::api::req_lang(&s, Some(&p), &headers).await;
    let meta = s.core.restore_version(&id, v, &p, lang).await?;
    Ok(Json(json!({ "version": meta })))
}

#[derive(Deserialize)]
struct HistQ {
    row: Option<String>,
    prop: Option<String>,
    limit: Option<usize>,
}

async fn db_history(
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
                q.row.as_deref(),
                q.prop.as_deref(),
                q.limit.unwrap_or(200),
                lang,
            )
            .await?,
    ))
}

#[derive(Deserialize)]
struct UndoBody {
    seq: i64,
}

async fn db_undo(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<UndoBody>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.core.undo_db(&id, &p, b.seq).await?))
}
