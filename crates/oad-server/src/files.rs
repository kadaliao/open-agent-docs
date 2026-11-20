//! 文件、转存的图片和链接解析：`POST /workspaces/{ws}/files`、`GET /files/{id}/{name}`、`GET /media/{hash}`、`GET /unfurl`；
//! 签名地址：`POST /files/sign`，文件和图片的地址带了有效的 `?exp=&sig=` 就不用登录（见 [`oad_core::signed`]）。
use crate::api::{ApiResult, Auth, Body as JsonBody};
use crate::openapi::ErrorBody;
use crate::{ApiError, AppState};
use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use oad_core::media::Uploaded;
use oad_core::signed::{self, Invalid, Subject};
use oad_core::{BlobInfo, Principal};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use utoipa::{OpenApi, ToSchema};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{ws}/files",
            post(upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/files/sign", post(sign_files))
        .route("/files/{id}/{name}", get(get_file))
        .route("/files/{id}", get(get_file_bare))
        .route("/media/{hash}", get(get_media))
        .route("/unfurl", get(unfurl))
}

fn too_large(max: usize) -> ApiError {
    ApiError::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        "too_large",
        format!("文件太大（最多 {} MB）", max >> 20),
    )
}

/// multipart 上传的表单。
#[derive(Serialize, ToSchema)]
#[allow(dead_code)] // 只用来写 OpenAPI
pub(crate) struct UploadForm {
    /// 文件（文件名取这个字段的 filename）
    #[schema(value_type = String, content_media_type = "application/octet-stream")]
    file: Vec<u8>,
}

/// 上传：`multipart/form-data` 的 `file` 字段，或者原始正文 + `X-Filename` 头。边读边数，超过上限就停。
#[utoipa::path(post, path = "/workspaces/{ws}/files", tag = "files",
    description = "上传文件：`multipart/form-data` 的 `file` 字段，或者原始正文 + `X-Filename` 头（文件名按网址规则编码）。\
大小上限见 OAD_MAX_UPLOAD。图片会算宽高和 blurhash，音视频算时长。",
    params(("ws" = String, Path, description = "工作区"), ("X-Filename" = Option<String>, Header, description = "原始正文上传时的文件名（百分号编码），默认 file")),
    request_body(content((UploadForm = "multipart/form-data"), ("application/octet-stream"))),
    responses((status = 201, body = Uploaded), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 413, body = ErrorBody)))]
pub(crate) async fn upload(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    req: Request,
) -> ApiResult<Response> {
    s.core.require(&ws, &p, true).await?;
    let max = s.core.media.max_upload;
    // 声明的长度已经超了就别读了（multipart 的头和分隔符多给一点余量）
    if let Some(n) = req
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        && n > max + 64 * 1024
    {
        return Err(too_large(max));
    }
    let is_multipart = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("multipart/form-data"));
    let (name, data) = if is_multipart {
        let mut mp = Multipart::from_request(req, &s)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        let mut found = None;
        while let Some(mut field) = mp
            .next_field()
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?
        {
            if field.name() != Some("file") {
                continue;
            }
            let name = field.file_name().unwrap_or("file").to_string();
            let mut buf = Vec::new();
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| ApiError::bad_request(e.body_text()))?
            {
                if buf.len() + chunk.len() > max {
                    return Err(too_large(max));
                }
                buf.extend_from_slice(&chunk);
            }
            found = Some((name, buf));
            break;
        }
        found.ok_or_else(|| ApiError::bad_request("要有 file 字段"))?
    } else {
        let name = req
            .headers()
            .get("x-filename")
            .and_then(|v| std::str::from_utf8(v.as_bytes()).ok())
            .map(percent_decode)
            .unwrap_or_else(|| "file".into());
        let body = axum::body::to_bytes(req.into_body(), max)
            .await
            .map_err(|_| too_large(max))?;
        (name, body.to_vec())
    };
    let up = s.core.media.upload(&ws, &p, &name, data).await?;
    Ok((StatusCode::CREATED, Json(up)).into_response())
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(h) = b
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
            && let Ok(x) = u8::from_str_radix(h, 16)
        {
            out.push(x);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[utoipa::path(get, path = "/files/{id}", tag = "files",
    description = "同 `/files/{id}/{name}`，不带文件名。",
    params(("id" = String, Path, description = "文件"), ("w" = Option<u32>, Query, description = "缩略图宽度（图片才有，给 WebP）"),
        ("exp" = Option<i64>, Query, description = "签名地址的过期时间（unix 秒），见 `POST /files/sign`"),
        ("sig" = Option<String>, Query, description = "签名，和 `exp` 一起给；有效就不用登录"),
        ("Range" = Option<String>, Header, description = "`bytes=a-b`，只认一段")),
    responses((status = 200, content_type = "application/octet-stream", description = "文件内容"),
        (status = 206, content_type = "application/octet-stream", description = "Range 的那一段"),
        (status = 304, description = "If-None-Match 对上了"), (status = 416, description = "Range 不对"),
        (status = 401, body = ErrorBody), (status = 403, body = ErrorBody, description = "签名过期了或者不对"), (status = 404, body = ErrorBody)))]
pub(crate) async fn get_file_bare(
    s: State<AppState>,
    parts: Parts,
    Path(id): Path<String>,
    q: Query<HashMap<String, String>>,
    h: HeaderMap,
) -> ApiResult<Response> {
    get_file(s, parts, Path((id, String::new())), q, h).await
}

#[utoipa::path(get, path = "/files/{id}/{name}", tag = "files",
    description = "下载上传的文件：原来的类型；带 ETag、长缓存、Range。`?w=` 给图片的缩略图（WebP）。\
SVG、HTML 之类带 `Content-Security-Policy: sandbox`，SVG 当附件下载。不是这个工作区的成员当不存在（404）。\
带了有效的签名（`?exp=&sig=`，见 `POST /files/sign`）就不用登录；签名过期了或者不对回 403（登录了、看得到的照样给）。",
    params(("id" = String, Path, description = "文件"), ("name" = String, Path, description = "文件名（只为好看，不校验）"),
        ("w" = Option<u32>, Query, description = "缩略图宽度（图片才有，给 WebP）"),
        ("exp" = Option<i64>, Query, description = "签名地址的过期时间（unix 秒），见 `POST /files/sign`"),
        ("sig" = Option<String>, Query, description = "签名，和 `exp` 一起给；有效就不用登录"),
        ("Range" = Option<String>, Header, description = "`bytes=a-b`，只认一段")),
    responses((status = 200, content_type = "application/octet-stream", description = "文件内容"),
        (status = 206, content_type = "application/octet-stream", description = "Range 的那一段"),
        (status = 304, description = "If-None-Match 对上了"), (status = 416, description = "Range 不对"),
        (status = 401, body = ErrorBody), (status = 403, body = ErrorBody, description = "签名过期了或者不对"), (status = 404, body = ErrorBody)))]
pub(crate) async fn get_file(
    State(s): State<AppState>,
    parts: Parts,
    Path((id, _name)): Path<(String, String)>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let f = s
        .core
        .storage
        .file(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("文件不存在"))?;
    let ws = f.workspace_id.clone();
    let signed = access(&s, &parts, &q, &id, |p| {
        let s = s.clone();
        async move {
            // 不是这个工作区的成员：当不存在
            if s.core.require(&ws, &p, false).await.is_err() {
                return Err(ApiError::not_found("文件不存在"));
            }
            Ok(())
        }
    })
    .await?;
    let res = serve(&s, &f.blob, Some(&f.name), &q, &headers).await?;
    Ok(cache_for(res, signed))
}

#[utoipa::path(get, path = "/media/{hash}", tag = "files",
    description = "转存的外部图片（链接卡片的封面、头像之类），按内容的 hash。登录了就能看，带了有效的签名（`?exp=&sig=`）也行。`?w=` 给缩略图（WebP），支持 Range。",
    params(("hash" = String, Path, description = "内容的 hash"), ("w" = Option<u32>, Query, description = "缩略图宽度"),
        ("exp" = Option<i64>, Query, description = "签名地址的过期时间（unix 秒），见 `POST /files/sign`"),
        ("sig" = Option<String>, Query, description = "签名，和 `exp` 一起给；有效就不用登录"),
        ("Range" = Option<String>, Header, description = "`bytes=a-b`，只认一段")),
    responses((status = 200, content_type = "application/octet-stream", description = "图片"),
        (status = 206, content_type = "application/octet-stream", description = "Range 的那一段"),
        (status = 304, description = "If-None-Match 对上了"), (status = 416, description = "Range 不对"),
        (status = 401, body = ErrorBody), (status = 403, body = ErrorBody, description = "签名过期了或者不对"), (status = 404, body = ErrorBody)))]
pub(crate) async fn get_media(
    State(s): State<AppState>,
    parts: Parts,
    Path(hash): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let signed = access(&s, &parts, &q, &hash, |_| async { Ok(()) }).await?;
    if !s.core.storage.is_media(&hash).await? {
        return Err(ApiError::not_found("没有这张图"));
    }
    let b = s
        .core
        .storage
        .blob(&hash)
        .await?
        .ok_or_else(|| ApiError::not_found("没有这张图"))?;
    let res = serve(&s, &b, None, &q, &headers).await?;
    Ok(cache_for(res, signed))
}

/// 能不能取：带了 `exp` + `sig` 的先验签，有效就放行（返回过期时间）；没带的要登录，再过一遍 `check`（工作区成员之类）。
/// 签名过期了或者不对：登录了、`check` 过得去的照样给（同域登录着的人打开旧的 HTML），否则 403。
async fn access<F, Fut>(
    s: &AppState,
    parts: &Parts,
    q: &HashMap<String, String>,
    subject: &str,
    check: F,
) -> ApiResult<Option<i64>>
where
    F: FnOnce(Principal) -> Fut,
    Fut: Future<Output = ApiResult<()>>,
{
    let bad = match (q.get("exp"), q.get("sig")) {
        (Some(exp), Some(sig)) => {
            match s.core.signer.verify(subject, exp, sig, signed::now_secs()) {
                Ok(()) => return Ok(exp.parse().ok()),
                Err(e) => Some(e),
            }
        }
        _ => None,
    };
    let who = crate::api::authenticate(s, parts).await;
    match (who, bad) {
        (Ok(Some(p)), _) => check(p).await.map(|()| None),
        (_, Some(Invalid::Expired)) => Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "signature_expired",
            "签名地址过期了，重新取一个",
        )),
        (_, Some(Invalid::Bad)) => Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "bad_signature",
            "签名不对",
        )),
        (Err(e), None) => Err(e),
        (Ok(None), None) => Err(ApiError::unauthorized()),
    }
}

/// 凭签名取的：浏览器缓存到签名过期为止（不是一年）。
fn cache_for(mut res: Response, signed_exp: Option<i64>) -> Response {
    if let Some(exp) = signed_exp
        && res.headers().contains_key(header::CACHE_CONTROL)
    {
        let left = (exp - signed::now_secs()).max(0);
        if let Ok(v) = HeaderValue::from_str(&format!("private, max-age={left}")) {
            res.headers_mut().insert(header::CACHE_CONTROL, v);
        }
    }
    res
}

/// 签名地址的请求。
#[derive(Deserialize, ToSchema)]
pub(crate) struct SignFileUrls {
    /// 要签的：文件地址 `/api/v1/files/<id>[/<文件名>]`、转存图片的地址 `/api/v1/media/<hash>`（根相对的或者带域名的，可以带 `?w=`），
    /// 或者只给文件 id。最多 500 个
    urls: Vec<String>,
}

/// 签好的地址。
#[derive(Serialize, ToSchema)]
pub(crate) struct SignedFileUrls {
    /// 和请求的 `urls` 一一对应：原来的地址加上 `exp`、`sig`（只给 id 的是 `/api/v1/files/<id>?exp=…&sig=…`）；
    /// 不是本站的文件、不存在、调用者看不到的是 null
    urls: Vec<Option<String>>,
    /// 签名什么时候过期（毫秒时间戳）。有效期是 `OAD_SIGNED_URL_TTL`（默认 24 小时），往后凑整到一个刻度（最多一小时）
    expires_at: i64,
}

#[utoipa::path(post, path = "/files/sign", tag = "files",
    description = "给文件、转存的图片签名，拿到不用登录就能打开的地址（`<img>`、链接带不了 `Authorization`：跨域 iframe、Web Component、宿主自己渲染的 HTML 用）。\
签名不包含 `w` 和文件名，拿到以后可以自己加 `w=`。只签调用者看得到的；拿到地址的人在有效期内不登录也能打开，别外传。",
    request_body = SignFileUrls,
    responses((status = 200, body = SignedFileUrls), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)))]
pub(crate) async fn sign_files(
    State(s): State<AppState>,
    Auth(p): Auth,
    JsonBody(b): JsonBody<SignFileUrls>,
) -> ApiResult<Json<SignedFileUrls>> {
    if b.urls.len() > 500 {
        return Err(ApiError::bad_request("一次最多签 500 个"));
    }
    let wanted: Vec<Option<(String, Subject)>> = b
        .urls
        .iter()
        .map(|u| {
            let u = u.trim();
            // 只给了文件 id
            if u.len() == 12 && u.bytes().all(|c| c.is_ascii_alphanumeric()) {
                return Some((format!("/api/v1/files/{u}"), Subject::File(u.to_string())));
            }
            signed::subject_of(u).map(|sub| (u.to_string(), sub))
        })
        .collect();
    let exp = signed::expiry(s.config.signed_url_ttl, signed::now_secs());
    let subjects: Vec<Subject> = wanted.iter().flatten().map(|w| w.1.clone()).collect();
    let queries = sign_visible(&s, &p, subjects, exp).await?;
    let urls = wanted
        .into_iter()
        .map(|w| {
            let (u, sub) = w?;
            let q = queries.get(sub.key())?;
            Some(signed::append_query(&u, q))
        })
        .collect();
    Ok(Json(SignedFileUrls {
        urls,
        expires_at: exp * 1000,
    }))
}

/// 给调用者看得到的文件、图片签名：文件 id / 媒体 hash → `exp=…&sig=…`。看不到的、不存在的不在结果里。
/// 文件要是所在工作区的成员；转存的图片登录了就能看。
pub(crate) async fn sign_visible(
    s: &AppState,
    p: &Principal,
    subjects: Vec<Subject>,
    exp: i64,
) -> ApiResult<HashMap<String, String>> {
    let mut out = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    let mut ws_ok: HashMap<String, bool> = HashMap::new();
    for sub in subjects {
        if !seen.insert(sub.clone()) {
            continue;
        }
        let ok = match &sub {
            Subject::File(id) => match s.core.storage.file(id).await? {
                Some(f) => match ws_ok.get(&f.workspace_id) {
                    Some(ok) => *ok,
                    None => {
                        let ok = s.core.require(&f.workspace_id, p, false).await.is_ok();
                        ws_ok.insert(f.workspace_id, ok);
                        ok
                    }
                },
                None => false,
            },
            Subject::Media(hash) => s.core.storage.is_media(hash).await?,
        };
        if ok {
            out.insert(sub.key().to_string(), s.core.signer.query(sub.key(), exp));
        }
    }
    Ok(out)
}

/// 链接卡片。除了 kind / provider / source / url 都可能缺；按 kind 还有别的字段
/// （canonical_url、description、site_name、favicon、author、thumbnail、media、published_at、embed；
/// 上传的文件有 name、mime、size、width、height、blurhash、duration_ms；本站的库有 db_id、view_id、icon、views）。
#[derive(Serialize, ToSchema)]
#[allow(dead_code)] // 只用来写 OpenAPI，返回的是 core 拼好的 JSON
pub(crate) struct UnfurlCard {
    /// video / post / repo / issue / pull_request / article / image / file / bookmark / database
    kind: String,
    /// youtube / x / github / bilibili / wechat / generic；上传的文件是 upload，本站的库是 oad
    provider: String,
    /// api / syndication / oembed / html / url_only
    source: String,
    url: String,
    title: Option<String>,
    /// 过期了先给旧的，后台在刷新
    stale: Option<bool>,
}

#[utoipa::path(get, path = "/unfurl", tag = "files",
    description = "解析一个网址成卡片：上传的文件直接从库里给，本站的库给视图列表，别的网址去抓（有缓存，图片转存到 /media）。",
    params(("url" = String, Query, description = "要解析的网址")),
    responses((status = 200, body = UnfurlCard), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 502, body = ErrorBody)))]
pub(crate) async fn unfurl(
    State(s): State<AppState>,
    Auth(p): Auth,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let url = q
        .get("url")
        .ok_or_else(|| ApiError::bad_request("要给 url"))?;
    Ok(Json(s.core.unfurl(url, &p).await?))
}

/// `bytes=a-b` / `bytes=a-` / `bytes=-n`（只认一段）。Err = 范围不对（416）。
fn parse_range(h: &str, len: u64) -> Option<Result<(u64, u64), ()>> {
    let spec = h.trim().strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None; // 多段的不支持，给整份
    }
    let (a, b) = spec.split_once('-')?;
    let r = match (a.trim(), b.trim()) {
        ("", n) => n.parse::<u64>().ok().map(|n| (len.saturating_sub(n), len)),
        (a, "") => a.parse::<u64>().ok().map(|a| (a, len)),
        (a, b) => match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(a), Ok(b)) if b >= a => Some((a, (b + 1).min(len))),
            _ => None,
        },
    };
    Some(match r {
        Some((a, b)) if a < b && a < len => Ok((a, b)),
        _ => Err(()),
    })
}

fn disposition(name: &str, attachment: bool) -> HeaderValue {
    let enc: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let kind = if attachment { "attachment" } else { "inline" };
    HeaderValue::from_str(&format!(
        "{kind}; filename=\"{ascii}\"; filename*=UTF-8''{enc}"
    ))
    .unwrap_or(HeaderValue::from_static("inline"))
}

/// 给出一份内容：缩略图（`?w=`）、Range、ETag、长缓存；SVG 和文字带 `Content-Security-Policy: sandbox`，SVG 当附件。
pub(crate) async fn serve(
    s: &AppState,
    b: &BlobInfo,
    name: Option<&str>,
    q: &HashMap<String, String>,
    headers: &HeaderMap,
) -> ApiResult<Response> {
    let etag = format!("\"{}\"", b.hash);
    let mut thumb = None;
    if let Some(w) = q.get("w").and_then(|w| w.parse::<u32>().ok()) {
        thumb = s.core.media.thumbnail(b, w).await?;
    }
    let etag = match &thumb {
        Some(_) => format!("\"{}-w{}\"", b.hash, q["w"]),
        None => etag,
    };
    let mut h = HeaderMap::new();
    h.insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag))
    {
        return Ok((StatusCode::NOT_MODIFIED, h).into_response());
    }
    if let Some(t) = thumb {
        h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/webp"));
        return Ok((StatusCode::OK, h, t).into_response());
    }
    let mime = if b.mime.starts_with("text/") {
        format!("{}; charset=utf-8", b.mime)
    } else {
        b.mime.clone()
    };
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    let risky = b.mime == "image/svg+xml" || b.mime.starts_with("text/") || b.mime.contains("xml");
    if risky {
        h.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox"),
        );
    }
    if let Some(n) = name.filter(|n| !n.is_empty()) {
        h.insert(
            header::CONTENT_DISPOSITION,
            disposition(n, b.mime == "image/svg+xml"),
        );
    }
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    let len = b.size as u64;
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|r| parse_range(r, len));
    match range {
        Some(Err(())) => {
            h.insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{len}")).unwrap(),
            );
            Ok((StatusCode::RANGE_NOT_SATISFIABLE, h).into_response())
        }
        Some(Ok((a, e))) => {
            let data = s.core.media.read(&b.hash, Some((a, e))).await?;
            h.insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes {a}-{}/{len}", e - 1)).unwrap(),
            );
            Ok((StatusCode::PARTIAL_CONTENT, h, Body::from(data)).into_response())
        }
        None => {
            let data = s.core.media.read(&b.hash, None).await?;
            Ok((StatusCode::OK, h, Body::from(data)).into_response())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_range;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-9", 100), Some(Ok((0, 10))));
        assert_eq!(parse_range("bytes=90-", 100), Some(Ok((90, 100))));
        assert_eq!(parse_range("bytes=-10", 100), Some(Ok((90, 100))));
        assert_eq!(parse_range("bytes=95-200", 100), Some(Ok((95, 100))));
        assert_eq!(parse_range("bytes=100-", 100), Some(Err(())));
        assert_eq!(parse_range("bytes=0-1,5-6", 100), None);
        assert_eq!(parse_range("items=0-1", 100), None);
    }
}

/// 这个模块的接口（合进 [`crate::openapi`]）。
#[derive(OpenApi)]
#[openapi(paths(upload, get_file, get_file_bare, get_media, sign_files, unfurl))]
pub(crate) struct Doc;
