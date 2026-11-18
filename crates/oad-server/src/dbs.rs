//! 第 4 阶段：数据库的接口。见 docs/phase4-contract.md。
use crate::api::{ApiResult, Auth, Body};
use crate::{ApiError, AppState};
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use oad_core::CellOp;
use oad_core::db::{NewDb, QueryReq, RowInput};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::io::Write;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{ws}/databases", post(create))
        .route(
            "/workspaces/{ws}/databases/import",
            post(import).layer(DefaultBodyLimit::max(64 << 20)),
        )
        .route("/databases/{id}", get(info))
        .route(
            "/databases/{id}/rows",
            get(rows)
                .post(add_rows)
                .patch(patch_rows)
                .delete(delete_rows),
        )
        .route("/databases/{id}/schema", patch(change_schema))
        .route("/databases/{id}/schema/undo", post(undo_schema))
        .route("/databases/{id}/views", put(put_views).post(add_view))
        .route(
            "/databases/{id}/views/{view}",
            patch(patch_view).delete(delete_view),
        )
        .route("/databases/{id}/query", post(query))
        .route("/databases/{id}/button/{row}/{key}", post(press))
        .route("/databases/{id}/export.csv", get(export))
        .route("/databases/{id}/rows/{row}", delete(delete_one))
}

async fn create(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    Body(b): Body<NewDb>,
) -> ApiResult<Response> {
    let db = s.core.create_database(&ws, &p, b).await?;
    Ok((StatusCode::CREATED, Json(db)).into_response())
}

async fn info(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    Ok(Json(s.core.database_info(&id, &p).await?).into_response())
}

#[derive(Deserialize)]
struct Since {
    since: Option<i64>,
}

/// 行（全量或增量）。按 Accept-Encoding 压缩：br 优先，其次 gzip（1 万行的全量压缩后 < 1MB）。
async fn rows(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Query(q): Query<Since>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let delta = s.core.db_rows(&id, &p, q.since).await?;
    let accept = headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let (enc, body) = tokio::task::spawn_blocking(move || {
        let raw = serde_json::to_vec(&delta).unwrap_or_default();
        compress(&accept, raw)
    })
    .await
    .map_err(|e| ApiError::from(oad_core::Error::Internal(e.to_string())))?;
    let mut res = body.into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    h.insert(header::VARY, HeaderValue::from_static("accept-encoding"));
    if let Some(e) = enc {
        h.insert(header::CONTENT_ENCODING, HeaderValue::from_static(e));
    }
    Ok(res)
}

/// 小的不压；能 br 就 br（质量 5：压得快、比 gzip 小），不然 gzip。
pub(crate) fn compress(accept: &str, raw: Vec<u8>) -> (Option<&'static str>, Vec<u8>) {
    if raw.len() < 1024 {
        return (None, raw);
    }
    let has = |name: &str| {
        accept.split(',').any(|p| {
            let mut it = p.split(';');
            let n = it.next().unwrap_or("").trim();
            let q0 = it.any(|x| x.trim().replace(' ', "") == "q=0");
            n.eq_ignore_ascii_case(name) && !q0
        })
    };
    if has("br") {
        let mut out = Vec::with_capacity(raw.len() / 8);
        {
            let mut w = brotli::CompressorWriter::new(&mut out, 64 * 1024, 5, 22);
            let _ = w.write_all(&raw);
        }
        return (Some("br"), out);
    }
    if has("gzip") {
        let mut w = flate2::write::GzEncoder::new(
            Vec::with_capacity(raw.len() / 6),
            flate2::Compression::new(6),
        );
        let _ = w.write_all(&raw);
        if let Ok(out) = w.finish() {
            return (Some("gzip"), out);
        }
    }
    (None, raw)
}

#[derive(Deserialize)]
struct AddRow {
    #[serde(default)]
    props: Map<String, Value>,
    after: Option<String>,
    body: Option<String>,
}

#[derive(Deserialize)]
struct AddRows {
    rows: Vec<AddRow>,
}

async fn add_rows(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<AddRows>,
) -> ApiResult<Response> {
    let rows = b
        .rows
        .into_iter()
        .map(|r| RowInput {
            props: r.props,
            after: r.after,
            body: r.body,
        })
        .collect();
    let (ids, seq) = s.core.add_db_rows(&id, &p, rows).await?;
    Ok((StatusCode::CREATED, Json(json!({ "ids": ids, "seq": seq }))).into_response())
}

#[derive(Deserialize)]
struct PatchRows {
    ops: Vec<CellOp>,
}

async fn patch_rows(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<PatchRows>,
) -> ApiResult<Json<Value>> {
    let (seq, changes) = s.core.patch_db_rows(&id, &p, b.ops).await?;
    Ok(Json(json!({ "seq": seq, "changes": changes })))
}

#[derive(Deserialize)]
struct Ids {
    ids: Vec<String>,
}

async fn delete_rows(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<Ids>,
) -> ApiResult<Json<Value>> {
    let seq = s.core.delete_db_rows(&id, &p, b.ids).await?;
    Ok(Json(json!({ "seq": seq })))
}

async fn delete_one(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, row)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let seq = s.core.delete_db_rows(&id, &p, vec![row]).await?;
    Ok(Json(json!({ "seq": seq })))
}

#[derive(Deserialize)]
struct SchemaOps {
    ops: Vec<Value>,
}

async fn change_schema(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<SchemaOps>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.core.change_schema(&id, &p, b.ops).await?))
}

#[derive(Deserialize)]
struct Undo {
    seq: i64,
}

async fn undo_schema(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<Undo>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.core.undo_schema(&id, &p, b.seq).await?))
}

/// 整个换掉：`[...]` 或 `{ views: [...] }`。
async fn put_views(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<Value>,
) -> ApiResult<Json<Value>> {
    let views = match b {
        Value::Object(mut o) => o.remove("views").unwrap_or(Value::Null),
        v => v,
    };
    let views = s.core.put_views(&id, &p, views).await?;
    Ok(Json(json!({ "views": views })))
}

async fn add_view(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<Value>,
) -> ApiResult<Response> {
    let v = s.core.add_view(&id, &p, b).await?;
    Ok((StatusCode::CREATED, Json(v)).into_response())
}

async fn patch_view(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, view)): Path<(String, String)>,
    Body(b): Body<Value>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.core.patch_view(&id, &p, &view, b).await?))
}

async fn delete_view(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, view)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    s.core.delete_view(&id, &p, &view).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn query(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<QueryReq>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.core.query_db(&id, &p, b).await?))
}

async fn press(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, row, key)): Path<(String, String, String)>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.core.press_button(&id, &p, &row, &key).await?))
}

async fn export(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let (title, csv) = s.core.export_csv(&id, &p).await?;
    let name = format!(
        "{}.csv",
        if title.is_empty() {
            "数据库"
        } else {
            &title
        }
    );
    let mut res = csv.into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    let cd = format!(
        "attachment; filename=\"export.csv\"; filename*=UTF-8''{}",
        pct(&name)
    );
    if let Ok(v) = HeaderValue::from_str(&cd) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

fn pct(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// 导入 CSV：multipart 的 `file`（和可选的 `title`、`parent_id`），或者原始正文（`?title=`）。
async fn import(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
    req: Request,
) -> ApiResult<Response> {
    let is_multipart = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("multipart/form-data"));
    let mut title = q.get("title").cloned();
    let mut parent = q.get("parent_id").cloned();
    let data: Vec<u8> = if is_multipart {
        let mut mp = Multipart::from_request(req, &s)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        let mut data = None;
        while let Some(f) = mp
            .next_field()
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?
        {
            match f.name() {
                Some("file") => {
                    if title.is_none() {
                        title = f
                            .file_name()
                            .map(|n| n.trim_end_matches(".csv").to_string());
                    }
                    let b = f
                        .bytes()
                        .await
                        .map_err(|e| ApiError::bad_request(e.body_text()))?;
                    data = Some(b.to_vec());
                }
                Some("title") => title = f.text().await.ok(),
                Some("parent_id") => parent = f.text().await.ok().filter(|t| !t.is_empty()),
                _ => {}
            }
        }
        data.ok_or_else(|| ApiError::bad_request("没有 file 字段"))?
    } else {
        axum::body::to_bytes(req.into_body(), 64 << 20)
            .await
            .map_err(|e| ApiError::bad_request(e.to_string()))?
            .to_vec()
    };
    let db = s.core.import_csv(&ws, &p, title, parent, &data).await?;
    Ok((StatusCode::CREATED, Json(db)).into_response())
}

#[cfg(test)]
mod tests {
    use super::compress;

    #[test]
    fn picks_encoding() {
        let raw = vec![b'a'; 4096];
        assert_eq!(compress("br", raw.clone()).0, Some("br"));
        assert_eq!(
            compress("gzip, deflate, br;q=0", raw.clone()).0,
            Some("gzip")
        );
        assert_eq!(compress("gzip;q=0", raw.clone()).0, None);
        assert_eq!(compress("identity", raw.clone()).0, None);
        assert_eq!(compress("br", vec![b'a'; 100]).0, None, "小的不压");
    }
}
