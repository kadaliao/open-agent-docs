//! 第 4 阶段：数据库的接口。见 docs/phase4-contract.md。
use crate::api::{ApiResult, Auth, Body};
use crate::openapi::ErrorBody;
use crate::{ApiError, AppState};
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use oad_core::db::{DbInfo, NewDb, QueryReq, RowInput, RowsDelta};
use oad_core::dbdoc::{DbSchema, DbView};
use oad_core::{CellChange, CellOp};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::Write;
use utoipa::{IntoParams, ToSchema};

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

#[utoipa::path(post, path = "/workspaces/{ws}/databases", tag = "databases", params(("ws" = String, Path, description = "工作区")), request_body = NewDb,
    responses((status = 201, body = DbInfo), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn create(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    headers: HeaderMap,
    Body(mut b): Body<NewDb>,
) -> ApiResult<Response> {
    b.lang = crate::api::req_lang(&s, Some(&p), &headers).await;
    let db = s.core.create_database(&ws, &p, b).await?;
    Ok((StatusCode::CREATED, Json(db)).into_response())
}

/// 库的页面、表结构、视图、现在的 seq。
#[utoipa::path(get, path = "/databases/{id}", tag = "databases", params(("id" = String, Path)),
    responses((status = 200, body = DbInfo), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn info(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    Ok(Json(s.core.database_info(&id, &p).await?).into_response())
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct Since {
    /// 只要这个 seq 之后改了的行（和删了的行 id）；不给是全量
    since: Option<i64>,
}

/// 行（全量或增量）。按 Accept-Encoding 压缩：br 优先，其次 gzip（1 万行的全量压缩后 < 1MB）。
#[utoipa::path(get, path = "/databases/{id}/rows", tag = "databases", params(("id" = String, Path), Since),
    responses((status = 200, body = RowsDelta,
        description = "行（全量或 since 之后的增量）。按 Accept-Encoding 压缩：br 优先，其次 gzip；不到 1KB 的不压",
        headers(("Content-Encoding" = String, description = "br / gzip（压了才有）"), ("Vary" = String, description = "accept-encoding"))),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn rows(
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

#[derive(Deserialize, ToSchema)]
pub(crate) struct AddRow {
    /// 属性：键是属性的 key 或名字，值是存的样子（选项也可以写名字）
    #[serde(default)]
    #[schema(value_type = Object)]
    props: Map<String, Value>,
    /// 排在这一行后面（不给放最后）
    after: Option<String>,
    /// 行页面的正文（Markdown）
    body: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct AddRows {
    rows: Vec<AddRow>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AddedRows {
    /// 新行的 id（和给的顺序一样）
    ids: Vec<String>,
    seq: i64,
}

#[utoipa::path(post, path = "/databases/{id}/rows", tag = "databases", params(("id" = String, Path)), request_body = AddRows,
    responses((status = 201, body = AddedRows), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn add_rows(
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
    Ok((StatusCode::CREATED, Json(AddedRows { ids, seq })).into_response())
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct PatchRows {
    ops: Vec<CellOp>,
}

/// 改了的格子和改完的 seq。
#[derive(Serialize, ToSchema)]
pub(crate) struct Patched {
    seq: i64,
    /// 真的变了的格子（值没变的不在里面）
    changes: Vec<CellChange>,
}

#[utoipa::path(patch, path = "/databases/{id}/rows", tag = "databases", params(("id" = String, Path)), request_body = PatchRows,
    responses((status = 200, body = Patched), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn patch_rows(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<PatchRows>,
) -> ApiResult<Json<Patched>> {
    let (seq, changes) = s.core.patch_db_rows(&id, &p, b.ops).await?;
    Ok(Json(Patched { seq, changes }))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct Ids {
    /// 要删的行（进回收站）
    ids: Vec<String>,
}

/// 改完后库的 seq。
#[derive(Serialize, ToSchema)]
pub(crate) struct Seq {
    seq: i64,
}

#[utoipa::path(delete, path = "/databases/{id}/rows", tag = "databases", params(("id" = String, Path)), request_body = Ids,
    responses((status = 200, body = Seq), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn delete_rows(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<Ids>,
) -> ApiResult<Json<Seq>> {
    let seq = s.core.delete_db_rows(&id, &p, b.ids).await?;
    Ok(Json(Seq { seq }))
}

#[utoipa::path(delete, path = "/databases/{id}/rows/{row}", tag = "databases", params(("id" = String, Path), ("row" = String, Path, description = "行的 id")),
    responses((status = 200, body = Seq), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn delete_one(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, row)): Path<(String, String)>,
) -> ApiResult<Json<Seq>> {
    let seq = s.core.delete_db_rows(&id, &p, vec![row]).await?;
    Ok(Json(Seq { seq }))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct SchemaOps {
    /// 依次做的改动：`{ op: "add" | "update" | "delete" | "move" | …, … }`，见 docs/phase4-contract.md
    #[schema(value_type = Vec<Object>)]
    ops: Vec<Value>,
}

// 只用来写文档：正文是核心给的 JSON，形状和这里一样
/// 改完的表结构。
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct SchemaChanged {
    /// 新的表结构
    #[schema(value_type = DbSchema)]
    schema: Value,
    /// 这次改动的 seq（历史里撤销用；什么也没变时是现在的 seq）
    seq: i64,
    /// 动了哪些属性（`{ op, key, name, … }`）
    #[schema(value_type = Vec<Object>)]
    changes: Vec<Value>,
    /// 改了但有问题的地方（比如公式有错）；没有就不给
    #[schema(required = false)]
    warnings: Vec<String>,
}

#[utoipa::path(patch, path = "/databases/{id}/schema", tag = "databases", params(("id" = String, Path)), request_body = SchemaOps,
    responses((status = 200, body = SchemaChanged), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn change_schema(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    headers: HeaderMap,
    Body(b): Body<SchemaOps>,
) -> ApiResult<Json<Value>> {
    let lang = crate::api::req_lang(&s, Some(&p), &headers).await;
    Ok(Json(s.core.change_schema(&id, &p, b.ops, lang).await?))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct Undo {
    /// 要撤销的那次表结构改动
    seq: i64,
}

// 只用来写文档：正文是核心给的 JSON，形状和这里一样
/// 撤销后的表结构和视图。
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct SchemaUndone {
    #[schema(value_type = DbSchema)]
    schema: Value,
    #[schema(value_type = Vec<DbView>)]
    views: Value,
    seq: i64,
}

#[utoipa::path(post, path = "/databases/{id}/schema/undo", tag = "databases", params(("id" = String, Path)), request_body = Undo,
    responses((status = 200, body = SchemaUndone), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn undo_schema(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<Undo>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.core.undo_schema(&id, &p, b.seq).await?))
}

// 只用来写文档：收的时候还是按 JSON 收（`{}` 里没有 views 时报错）
/// 换掉全部视图的正文：视图的数组，或 `{ views: [...] }`。
#[derive(ToSchema)]
#[serde(untagged)]
#[allow(dead_code)]
pub(crate) enum PutViews {
    List(Vec<ViewObj>),
    Wrapped { views: Vec<ViewObj> },
}

/// 一个视图（DbView 的样子；id 不给或重复时分配新的，type 不给是 table，name 不给是“视图”）。
#[derive(ToSchema)]
#[schema(value_type = Object)]
#[allow(dead_code)]
pub(crate) struct ViewObj(Value);

/// 库的全部视图。
#[derive(Serialize, ToSchema)]
pub(crate) struct Views {
    #[schema(value_type = Vec<DbView>)]
    views: Value,
}

/// 整个换掉：`[...]` 或 `{ views: [...] }`。
#[utoipa::path(put, path = "/databases/{id}/views", tag = "databases", params(("id" = String, Path)), request_body = PutViews,
    responses((status = 200, body = Views), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn put_views(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<Value>,
) -> ApiResult<Json<Views>> {
    let views = match b {
        Value::Object(mut o) => o.remove("views").unwrap_or(Value::Null),
        v => v,
    };
    let views = s.core.put_views(&id, &p, views).await?;
    Ok(Json(Views { views }))
}

/// 加一个视图（不给 type 是表格），返回它（带上新的 id）。
#[utoipa::path(post, path = "/databases/{id}/views", tag = "databases", params(("id" = String, Path)), request_body = Object,
    responses((status = 201, body = DbView, description = "加好的视图"), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn add_view(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<Value>,
) -> ApiResult<Response> {
    let v = s.core.add_view(&id, &p, b).await?;
    Ok((StatusCode::CREATED, Json(v)).into_response())
}

/// 改一个视图的几项：给了的字段覆盖，null 去掉。返回改好的视图。
#[utoipa::path(patch, path = "/databases/{id}/views/{view}", tag = "databases", params(("id" = String, Path), ("view" = String, Path, description = "视图的 id")), request_body = Object,
    responses((status = 200, body = DbView, description = "改好的视图"), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn patch_view(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, view)): Path<(String, String)>,
    Body(b): Body<Value>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.core.patch_view(&id, &p, &view, b).await?))
}

/// 删一个视图（最后一个不能删）。
#[utoipa::path(delete, path = "/databases/{id}/views/{view}", tag = "databases", params(("id" = String, Path), ("view" = String, Path, description = "视图的 id")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn delete_view(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, view)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    s.core.delete_view(&id, &p, &view).await?;
    Ok(StatusCode::NO_CONTENT)
}

// 只用来写文档：正文是核心给的 JSON，形状和这里一样
/// 查询结果。
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct QueryResult {
    /// 符合条件的总行数
    total: usize,
    /// 按排序的行 id（给了 limit / offset 是那一段）
    ids: Vec<String>,
    /// 这一段的行，带算出来的值
    rows: Vec<QueryRow>,
    /// 按 group_by 分的组；没分组不给
    #[schema(required = false)]
    groups: Vec<QueryGroup>,
    /// 每列底部的汇总（键是属性的 key）；没要不给
    #[schema(required = false, value_type = Object)]
    totals: Value,
    /// 库现在的 seq
    seq: i64,
}

/// 查出来的一行：存的样子（`props`，键是属性的 key，空值不存），加上 `computed`（公式、汇总、反向关联算出来的值）。
/// 和 `DbRow` 不同：不带 `db_id`、`position`（顺序看 `ids`），时间是毫秒数。
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct QueryRow {
    id: String,
    seq: i64,
    #[schema(value_type = Object)]
    props: Value,
    #[schema(value_type = Object)]
    computed: Value,
    /// 毫秒时间戳
    #[schema(required = false)]
    created_at: f64,
    #[schema(required = false)]
    created_by: String,
    /// 毫秒时间戳
    #[schema(required = false)]
    updated_at: f64,
    #[schema(required = false)]
    updated_by: String,
    /// 自动编号
    #[schema(required = false)]
    unique_id: i64,
}

/// 一个分组。空值的分组 `key` 是 ""，排在最后。
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct QueryGroup {
    key: String,
    name: String,
    #[schema(required = false)]
    color: String,
    ids: Vec<String>,
    count: usize,
    /// 子分组（视图有 sub_group_by 时）
    #[schema(no_recursion, required = false)]
    groups: Vec<QueryGroup>,
    #[schema(required = false, value_type = Object)]
    totals: Value,
}

/// 服务器算的查询：给了 view_id 就从那个视图出发，别的字段覆盖它。
#[utoipa::path(post, path = "/databases/{id}/query", tag = "databases", params(("id" = String, Path)), request_body = QueryReq,
    responses((status = 200, body = QueryResult), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn query(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    Body(b): Body<QueryReq>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.core.query_db(&id, &p, b).await?))
}

// 只用来写文档：正文是核心给的 JSON，形状和这里一样
/// 按按钮的结果。
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct Pressed {
    seq: i64,
    /// 按钮设的属性里真的变了的格子
    changes: Vec<CellChange>,
    /// 按钮带评论时，发在这一行上的评论串
    #[schema(required = false)]
    thread_id: String,
}

/// 按一行上的按钮：按设置改属性，有 `comment` 的在这一行上发一条评论（可以叫 Agent）。
#[utoipa::path(post, path = "/databases/{id}/button/{row}/{key}", tag = "databases",
    params(("id" = String, Path), ("row" = String, Path, description = "行的 id"), ("key" = String, Path, description = "按钮属性的 key 或名字")),
    responses((status = 200, body = Pressed), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn press(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((id, row, key)): Path<(String, String, String)>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.core.press_button(&id, &p, &row, &key).await?))
}

/// 导出 CSV（UTF-8，按第一个视图的列）。
#[utoipa::path(get, path = "/databases/{id}/export.csv", tag = "databases", params(("id" = String, Path)),
    responses((status = 200, body = String, content_type = "text/csv",
        headers(("Content-Disposition" = String, description = "attachment; filename*=UTF-8''<库名>.csv"))),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn export(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let lang = crate::api::req_lang(&s, Some(&p), &headers).await;
    let (title, csv) = s.core.export_csv(&id, &p, lang).await?;
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

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[allow(dead_code)]
pub(crate) struct ImportQ {
    /// 库名（multipart 里的 title 优先；都没有用文件名）
    title: Option<String>,
    /// 放在哪个页面下（不给放在根上）
    parent_id: Option<String>,
}

/// multipart 导入的字段。
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct ImportForm {
    /// CSV 文件（第一行是列名）
    #[schema(value_type = String, content_media_type = "text/csv")]
    file: Vec<u8>,
    /// 库名
    title: Option<String>,
    /// 放在哪个页面下
    parent_id: Option<String>,
}

/// 导入 CSV：multipart 的 `file`（和可选的 `title`、`parent_id`），或者原始正文（`?title=`）。
#[utoipa::path(post, path = "/workspaces/{ws}/databases/import", tag = "databases", params(("ws" = String, Path, description = "工作区"), ImportQ),
    request_body(description = "multipart 的 file 字段，或者原始的 CSV 正文；最大 64MB",
        content((ImportForm = "multipart/form-data"), (String = "text/csv"))),
    responses((status = 201, body = DbInfo), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 413, body = ErrorBody)))]
pub(crate) async fn import(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
    req: Request,
) -> ApiResult<Response> {
    let lang = crate::api::req_lang(&s, Some(&p), req.headers()).await;
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
    let db = s
        .core
        .import_csv(&ws, &p, title, parent, &data, lang)
        .await?;
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

/// 这个模块的接口（合进 [`crate::openapi`]）。
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    create,
    import,
    info,
    rows,
    add_rows,
    patch_rows,
    delete_rows,
    delete_one,
    change_schema,
    undo_schema,
    put_views,
    add_view,
    patch_view,
    delete_view,
    query,
    press,
    export
))]
pub(crate) struct Doc;
