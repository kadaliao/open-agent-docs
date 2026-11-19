//! 公开分享的页面：`/s/{token}`、`/s/{token}/{page}`。不用登录、只读，服务器直接把 oad-markdown 的节点树渲染成 HTML：
//! 卡片用链接解析的缓存画成静态卡片，图片用缩略图；一个小脚本负责代码高亮、公式、Mermaid（用到时才从 CDN 加载）、
//! 视频门面（点了才换成播放器）、数据库表格往下翻。不加载编辑器。管理分享的接口在 `/api/v1/pages/{id}/share`。
use crate::api::{ApiResult, Auth};
use crate::i18n::{SERVER, t};
use crate::openapi::ErrorBody;
use crate::{ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use oad_core::i18n::{CORE, DB_ENGINE, Lang, translate};
use oad_core::{Node, Page, Share};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use utoipa::{OpenApi, ToSchema};

/// 数据库表格先出几行（往下翻再取）。
const TABLE_ROWS: usize = 200;

/// `/s` 下面的路由（不在 `/api/v1` 里）。
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/{token}", get(root_page))
        .route("/{token}/{page}", get(child_page))
        .route("/{token}/_/files/{id}/{name}", get(file))
        .route("/{token}/_/media/{hash}", get(media))
        .route("/{token}/_/rows/{db}", get(more_rows))
}

/// `/api/v1` 下管分享的接口。
pub fn api_routes() -> Router<AppState> {
    Router::new().route(
        "/pages/{id}/share",
        get(get_share).post(put_share).delete(del_share),
    )
}

/// 页面的公开分享链接。
#[derive(Serialize, ToSchema)]
pub(crate) struct ShareView {
    /// 公开的网址（`/s/<token>`）
    url: String,
    token: String,
    /// 子页面也公开
    include_children: bool,
    /// 允许搜索引擎收录
    indexable: bool,
    created_at: i64,
}

fn share_json(s: &AppState, sh: &Share) -> ShareView {
    ShareView {
        url: s.core.share_url(&sh.token),
        token: sh.token.clone(),
        include_children: sh.include_children,
        indexable: sh.indexable,
        created_at: sh.created_at,
    }
}

#[utoipa::path(get, path = "/pages/{id}/share", tag = "share", params(("id" = String, Path, description = "页面")),
    responses((status = 200, body = ShareView), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody, description = "页面不存在或没分享")))]
pub(crate) async fn get_share(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<ShareView>> {
    let sh = s.core.get_share(&id, &p).await?;
    Ok(Json(share_json(&s, &sh)))
}

/// 分享设置；不给正文就都是 false。
#[derive(Deserialize, Default, ToSchema)]
pub(crate) struct PutShare {
    /// 子页面也公开
    #[serde(default)]
    include_children: bool,
    /// 允许搜索引擎收录
    #[serde(default)]
    indexable: bool,
}

#[utoipa::path(post, path = "/pages/{id}/share", tag = "share", params(("id" = String, Path, description = "页面")),
    request_body(content = Option<PutShare>, description = "可以不给"),
    description = "打开分享或改设置（已经分享了的，链接不变）。",
    responses((status = 200, body = ShareView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn put_share(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
    body: Option<Json<PutShare>>,
) -> ApiResult<Json<ShareView>> {
    let b = body.map(|Json(b)| b).unwrap_or_default();
    let sh = s
        .core
        .put_share(&id, &p, b.include_children, b.indexable)
        .await?;
    Ok(Json(share_json(&s, &sh)))
}

#[utoipa::path(delete, path = "/pages/{id}/share", tag = "share", params(("id" = String, Path, description = "页面")),
    description = "关掉分享，链接马上失效。",
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn del_share(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    s.core.revoke_share(&id, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- 公开页面 ----------

async fn root_page(State(s): State<AppState>, Path(token): Path<String>, h: HeaderMap) -> Response {
    render(&s, &token, None, &h).await
}

/// 公开页面不用登录：语言按浏览器的。
fn lang_of(h: &HeaderMap) -> Lang {
    h.get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .and_then(Lang::from_accept)
        .unwrap_or_default()
}

async fn child_page(
    State(s): State<AppState>,
    Path((token, page)): Path<(String, String)>,
    h: HeaderMap,
) -> Response {
    render(&s, &token, Some(&page), &h).await
}

async fn render(s: &AppState, token: &str, page: Option<&str>, h: &HeaderMap) -> Response {
    let lang = lang_of(h);
    match build(s, token, page, lang).await {
        Ok(html) => html,
        Err(e) => {
            let (code, msg) = match &e {
                oad_core::Error::NotFound(m) => (
                    StatusCode::NOT_FOUND,
                    translate(&[SERVER, CORE, DB_ENGINE], lang, m),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    t(lang, "服务器出错了", &[]),
                ),
            };
            let body = format!(
                "<!doctype html><html lang=\"{1}\"><meta charset=\"utf-8\"><meta name=\"robots\" content=\"noindex\">\
                 <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{0}</title>\
                 <style>{CSS}</style><main class=\"page\"><p class=\"gone\">{0}</p></main></html>",
                esc(&msg),
                lang.as_str()
            );
            let mut r = (code, body).into_response();
            set_headers(r.headers_mut(), false, None);
            r
        }
    }
}

/// 页面的头：类型、不缓存旧内容、noindex、CSP（脚本只许本页带 nonce 的和 jsDelivr，iframe 只许视频网站）。
fn set_headers(h: &mut HeaderMap, indexable: bool, nonce: Option<&str>) {
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    h.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if !indexable {
        h.insert(
            "x-robots-tag",
            HeaderValue::from_static("noindex, nofollow"),
        );
    }
    let script = match nonce {
        Some(n) => format!("'nonce-{n}' https://cdn.jsdelivr.net"),
        None => "'none'".into(),
    };
    let csp = format!(
        "default-src 'self'; script-src {script}; style-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; \
         img-src * data: blob:; font-src https://cdn.jsdelivr.net data:; media-src *; connect-src 'self'; \
         frame-src https://www.youtube-nocookie.com https://player.bilibili.com; base-uri 'none'; form-action 'none'; frame-ancestors *"
    );
    if let Ok(v) = HeaderValue::from_str(&csp) {
        h.insert(header::CONTENT_SECURITY_POLICY, v);
    }
}

/// 渲染要用到的：分享、能链过去的页面、卡片数据、文件信息、内嵌的库表格。
struct Ctx {
    token: String,
    share: Share,
    root: Page,
    /// 节点里提到的页面 → (标题, 在不在分享里)
    pages: HashMap<String, (String, bool)>,
    /// 网址 → 解析缓存
    cards: HashMap<String, Value>,
    files: HashMap<String, oad_core::FileRow>,
    /// 内嵌的库（在分享里的）→ 表格 HTML
    tables: HashMap<String, String>,
    /// 用到了哪些需要脚本的东西
    needs: HashSet<&'static str>,
    lang: Lang,
}

impl Ctx {
    fn page_href(&self, id: &str) -> String {
        if id == self.root.id {
            format!("/s/{}", self.token)
        } else {
            format!("/s/{}/{id}", self.token)
        }
    }

    fn file_href(&self, f: &oad_core::FileRow) -> String {
        format!("/s/{}/_/files/{}/{}", self.token, f.id, pct(&f.name))
    }

    /// 卡片里的图：转存的图片换成分享下的地址，外面的原样。
    fn img(&self, url: &str) -> String {
        match url.strip_prefix("/api/v1/media/") {
            Some(h) => format!("/s/{}/_/media/{h}", self.token),
            None => url.to_string(),
        }
    }
}

/// 节点树里的网址：(网址, 要不要卡片)。embed 和提到的链接要卡片；图片、普通链接只看是不是本站的。
fn walk<'a>(n: &'a Node, urls: &mut Vec<(&'a str, bool)>) {
    let card = matches!(n.ty.as_str(), "embed" | "linkMention");
    for k in ["url", "src", "href"] {
        if let Some(u) = n.attr_str(k) {
            urls.push((u, card));
        }
    }
    for m in &n.marks {
        if let Some(u) = m.attrs.get("href").and_then(|v| v.as_str()) {
            urls.push((u, false));
        }
    }
    for c in &n.content {
        walk(c, urls);
    }
}

async fn build(
    s: &AppState,
    token: &str,
    page: Option<&str>,
    lang: Lang,
) -> oad_core::Result<Response> {
    let core = &s.core;
    let (share, root, target) = core.open_share(token, page).await?;
    let (doc, _) = core.hub.content(&target.id).await?;
    let mut cx = Ctx {
        token: token.to_string(),
        share: share.clone(),
        root: root.clone(),
        pages: HashMap::new(),
        cards: HashMap::new(),
        files: HashMap::new(),
        tables: HashMap::new(),
        needs: HashSet::new(),
        lang,
    };
    // 先把要查的都查好
    let mut urls = Vec::new();
    walk(&doc, &mut urls);
    let mut seen = HashSet::new();
    for (u, want_card) in urls {
        if !seen.insert(u) {
            continue;
        }
        if let Some(id) = oad_core::media::own_file_id(u) {
            if !cx.files.contains_key(&id)
                && core.storage.page_uses_file(&target.id, &id).await?
                && let Some(f) = core.storage.file(&id).await?
            {
                cx.files.insert(id, f);
            }
            continue;
        }
        if let Some((id, view)) = oad_core::media::own_page_ref(u) {
            if cx.pages.contains_key(&id) {
                continue;
            }
            if let Some(p) = core.storage.page(&id).await?
                && p.deleted_at.is_none()
                && p.workspace_id == root.workspace_id
            {
                let ok = core.in_scope(&share, &root, &id).await?;
                if ok && p.kind == "database" && !cx.tables.contains_key(&id) {
                    let t = table_html(&cx, s, &id, view.as_deref(), 0).await?;
                    cx.tables.insert(id.clone(), t);
                }
                cx.pages.insert(id, (p.title, ok));
            }
            continue;
        }
        if want_card && oad_markdown::is_bare_url(u) && !cx.cards.contains_key(u) {
            let key = oad_unfurl::cache_key(u);
            if let Some(row) = core.storage.unfurl(&key).await?
                && row.ok
                && let Some(d) = row.data
            {
                cx.cards.insert(u.to_string(), d);
            }
        }
    }
    let nonce = oad_core::ids::new_token()[..24].to_string();
    let mut body = String::with_capacity(16 * 1024);
    // 面包屑：分享的页面 → … → 这一页
    if target.id != root.id {
        let anc = core.storage.ancestors(&target.id).await?;
        let mut crumbs = Vec::new();
        for a in anc.iter().rev() {
            if let Some(i) = anc.iter().position(|x| x == &root.id)
                && anc.iter().position(|x| x == a).unwrap_or(0) > i
            {
                continue;
            }
            if let Some(p) = core.storage.page(a).await? {
                crumbs.push(format!(
                    "<a href=\"{}\">{}</a>",
                    cx.page_href(a),
                    esc(&title_of(&p, lang))
                ));
            }
        }
        if !crumbs.is_empty() {
            let _ = write!(body, "<nav class=\"crumbs\">{}</nav>", crumbs.join(" / "));
        }
    }
    let icon = target
        .icon
        .as_deref()
        .map(|i| format!("<span class=\"icon\">{}</span>", esc(i)))
        .unwrap_or_default();
    let _ = write!(
        body,
        "<h1 class=\"title\">{icon}{}</h1>",
        esc(&title_of(&target, lang))
    );
    // 行：先列属性
    if target.kind == "row" {
        let props = core.row_props_public(&target, lang).await?;
        if !props.is_empty() {
            body.push_str("<dl class=\"props\">");
            for (k, v) in props {
                let _ = write!(body, "<dt>{}</dt><dd>{}</dd>", esc(&k), esc(&v));
            }
            body.push_str("</dl>");
        }
    }
    // 库：表格
    if target.kind == "database" {
        let t = table_html(&cx, s, &target.id, None, 0).await?;
        body.push_str(&t);
        cx.needs.insert("table");
    }
    body.push_str("<article class=\"doc\">");
    blocks(&mut body, &doc.content, &mut cx);
    body.push_str("</article>");
    if !cx.tables.is_empty() {
        cx.needs.insert("table");
    }
    // 子页面
    if share.include_children && target.kind != "database" {
        let kids: Vec<Page> = core
            .storage
            .pages(&root.workspace_id)
            .await?
            .into_iter()
            .filter(|p| p.parent_id.as_deref() == Some(target.id.as_str()))
            .collect();
        let mut kids = kids;
        kids.sort_by(|a, b| a.position.cmp(&b.position));
        if !kids.is_empty() {
            body.push_str("<nav class=\"children\"><ul>");
            for k in kids {
                let icon = k.icon.as_deref().map(esc).unwrap_or_else(|| "📄".into());
                let _ = write!(
                    body,
                    "<li><a href=\"{}\"><span>{icon}</span>{}</a></li>",
                    cx.page_href(&k.id),
                    esc(&title_of(&k, lang))
                );
            }
            body.push_str("</ul></nav>");
        }
    }
    let robots = if share.indexable {
        ""
    } else {
        "<meta name=\"robots\" content=\"noindex, nofollow\">"
    };
    let desc: String = doc
        .plain_text()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(160)
        .collect();
    let script = if cx.needs.is_empty() {
        String::new()
    } else {
        let flags: Vec<&str> = cx.needs.iter().copied().collect();
        format!(
            "<script nonce=\"{nonce}\">window.OAD_NEEDS={};{SCRIPT}</script>",
            serde_json::to_string(&flags).unwrap_or_default()
        )
    };
    let html = format!(
        "<!doctype html><html lang=\"{l}\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">{robots}\
         <meta name=\"color-scheme\" content=\"light dark\">\
         <title>{t}</title><meta name=\"description\" content=\"{d}\">\
         <meta property=\"og:title\" content=\"{t}\"><meta property=\"og:description\" content=\"{d}\">\
         <style>{CSS}</style></head><body><main class=\"page\">{body}</main>\
         <footer class=\"foot\">Open Agent Docs</footer>{script}</body></html>",
        t = esc(&title_of(&target, lang)),
        d = esc(&desc),
        l = lang.as_str(),
    );
    let mut r = html.into_response();
    set_headers(r.headers_mut(), share.indexable, Some(&nonce));
    Ok(r)
}

fn title_of(p: &Page, lang: Lang) -> String {
    if p.title.trim().is_empty() {
        t(lang, "无标题", &[])
    } else {
        p.title.clone()
    }
}

/// 库的表格（第一段），后面还有就带上往下翻的地址。
async fn table_html(
    cx: &Ctx,
    s: &AppState,
    db: &str,
    view: Option<&str>,
    offset: usize,
) -> oad_core::Result<String> {
    let (cols, rows, total) = s
        .core
        .db_public_table(db, view, offset, TABLE_ROWS, cx.lang)
        .await?;
    let mut h = String::new();
    let more = if offset + rows.len() < total {
        let v = view
            .map(|v| format!("&view={}", pct(v)))
            .unwrap_or_default();
        format!(
            " data-more=\"/s/{}/_/rows/{db}?offset={}{v}\"",
            cx.token,
            offset + rows.len()
        )
    } else {
        String::new()
    };
    let _ = write!(
        h,
        "<div class=\"dbwrap\"><table class=\"db\"{more}><thead><tr>"
    );
    for c in &cols {
        let _ = write!(h, "<th>{}</th>", esc(c));
    }
    h.push_str("</tr></thead><tbody>");
    rows_html(
        &mut h,
        cx,
        &rows,
        cx.share.include_children || cx.root.id == db,
    );
    let _ = write!(
        h,
        "</tbody></table><p class=\"count\">{}</p></div>",
        t(cx.lang, "{n} 行", &[("n", &total.to_string())])
    );
    Ok(h)
}

fn rows_html(h: &mut String, cx: &Ctx, rows: &[(String, Vec<String>)], link: bool) {
    for (id, cells) in rows {
        h.push_str("<tr>");
        for (i, c) in cells.iter().enumerate() {
            if i == 0 && link {
                let untitled = t(cx.lang, "无标题", &[]);
                let _ = write!(
                    h,
                    "<td><a href=\"/s/{}/{id}\">{}</a></td>",
                    cx.token,
                    esc(if c.is_empty() { &untitled } else { c })
                );
            } else {
                let _ = write!(h, "<td>{}</td>", esc(c));
            }
        }
        h.push_str("</tr>");
    }
}

#[derive(Deserialize)]
struct MoreQ {
    offset: Option<usize>,
    view: Option<String>,
}

/// 表格往下翻：`{ html（几行 <tr>）, next? }`。
async fn more_rows(
    State(s): State<AppState>,
    Path((token, db)): Path<(String, String)>,
    Query(q): Query<MoreQ>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let lang = lang_of(&headers);
    let (share, root, _) = s.core.open_share(&token, None).await?;
    let ok = db == root.id || s.core.in_scope(&share, &root, &db).await?;
    if !ok {
        return Err(ApiError::not_found("分享链接无效或已关闭"));
    }
    let offset = q.offset.unwrap_or(0);
    let (_, rows, total) = s
        .core
        .db_public_table(&db, q.view.as_deref(), offset, TABLE_ROWS, lang)
        .await?;
    let cx = Ctx {
        token: token.clone(),
        share: share.clone(),
        root: root.clone(),
        pages: HashMap::new(),
        cards: HashMap::new(),
        files: HashMap::new(),
        tables: HashMap::new(),
        needs: HashSet::new(),
        lang: Lang::Zh,
    };
    let mut h = String::new();
    rows_html(&mut h, &cx, &rows, share.include_children || root.id == db);
    let end = offset + rows.len();
    let next = (end < total).then(|| {
        let v = q
            .view
            .as_deref()
            .map(|v| format!("&view={}", pct(v)))
            .unwrap_or_default();
        format!("/s/{token}/_/rows/{db}?offset={end}{v}")
    });
    Ok(Json(json!({ "html": h, "next": next })))
}

/// 分享页面里的图片、文件：要是分享范围里的页面用过的。
async fn file(
    State(s): State<AppState>,
    Path((token, id, _name)): Path<(String, String, String)>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let nf = || ApiError::not_found("文件不存在");
    let (share, root, _) = s.core.open_share(&token, None).await.map_err(|_| nf())?;
    let f = s.core.storage.file(&id).await?.ok_or_else(nf)?;
    if f.workspace_id != root.workspace_id {
        return Err(nf());
    }
    // 用它的页面里有一个在分享范围里
    let mut ok = s.core.storage.page_uses_file(&root.id, &id).await?;
    if !ok && share.include_children {
        for p in s.core.storage.pages(&root.workspace_id).await? {
            if s.core.storage.page_uses_file(&p.id, &id).await?
                && s.core.in_scope(&share, &root, &p.id).await?
            {
                ok = true;
                break;
            }
        }
    }
    if !ok {
        return Err(nf());
    }
    crate::files::serve(&s, &f.blob, Some(&f.name), &q, &headers).await
}

async fn media(
    State(s): State<AppState>,
    Path((token, hash)): Path<(String, String)>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    s.core
        .open_share(&token, None)
        .await
        .map_err(|_| ApiError::not_found("没有这张图"))?;
    if !s.core.storage.is_media(&hash).await? {
        return Err(ApiError::not_found("没有这张图"));
    }
    let b = s
        .core
        .storage
        .blob(&hash)
        .await?
        .ok_or_else(|| ApiError::not_found("没有这张图"))?;
    crate::files::serve(&s, &b, None, &q, &headers).await
}

// ---------- 节点 → HTML ----------

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '&' => o.push_str("&amp;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

fn pct(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// 只许 http(s)、mailto、站内地址；别的（javascript: 之类）不当链接。
fn safe_href(u: &str) -> Option<&str> {
    let l = u.trim().to_ascii_lowercase();
    (l.starts_with("http://")
        || l.starts_with("https://")
        || l.starts_with("mailto:")
        || l.starts_with('/')
        || l.starts_with('#'))
    .then_some(u.trim())
}

fn link_href(cx: &Ctx, u: &str) -> Option<String> {
    if let Some((id, _)) = oad_core::media::own_page_ref(u) {
        return match cx.pages.get(&id) {
            Some((_, true)) => Some(cx.page_href(&id)),
            _ => None,
        };
    }
    if let Some(id) = oad_core::media::own_file_id(u) {
        return cx.files.get(&id).map(|f| cx.file_href(f));
    }
    safe_href(u).map(str::to_string)
}

fn blocks(out: &mut String, nodes: &[Node], cx: &mut Ctx) {
    for n in nodes {
        block(out, n, cx);
    }
}

fn block(out: &mut String, n: &Node, cx: &mut Ctx) {
    match n.ty.as_str() {
        "paragraph" => {
            out.push_str("<p>");
            inline(out, &n.content, cx);
            out.push_str("</p>");
        }
        "heading" => {
            let l = n.attr_i64("level").unwrap_or(1).clamp(1, 6) + 1;
            let l = l.min(6);
            let id = n
                .attr_str("id")
                .map(|i| format!(" id=\"{}\"", esc(i)))
                .unwrap_or_default();
            let _ = write!(out, "<h{l}{id}>");
            inline(out, &n.content, cx);
            let _ = write!(out, "</h{l}>");
        }
        "blockquote" => {
            out.push_str("<blockquote>");
            blocks(out, &n.content, cx);
            out.push_str("</blockquote>");
        }
        "callout" => {
            let kind = n.attr_str("kind").unwrap_or("note").to_ascii_lowercase();
            let kind: String = kind.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            let _ = write!(out, "<aside class=\"callout {kind}\">");
            if let Some(t) = n.attr_str("title").filter(|t| !t.is_empty()) {
                let _ = write!(out, "<p class=\"callout-title\">{}</p>", esc(t));
            }
            blocks(out, &n.content, cx);
            out.push_str("</aside>");
        }
        "bulletList" => list(out, n, cx, "ul", ""),
        "orderedList" => {
            let start = n
                .attr_i64("start")
                .filter(|s| *s != 1)
                .map(|s| format!(" start=\"{s}\""))
                .unwrap_or_default();
            list(out, n, cx, "ol", &start)
        }
        "taskList" => list(out, n, cx, "ul", " class=\"tasks\""),
        "listItem" | "taskItem" => {
            out.push_str("<li>");
            blocks(out, &n.content, cx);
            out.push_str("</li>");
        }
        "codeBlock" => {
            let lang = n.attr_str("language").unwrap_or("");
            let lang: String = lang
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '+')
                .collect();
            let text = n.plain_text();
            if lang == "mermaid" {
                cx.needs.insert("mermaid");
                let _ = write!(out, "<pre class=\"mermaid\">{}</pre>", esc(&text));
                return;
            }
            if !lang.is_empty() {
                cx.needs.insert("code");
            }
            let wrap = if n.attr_bool("wrap") { " wrap" } else { "" };
            let class = if lang.is_empty() {
                String::new()
            } else {
                format!(" class=\"language-{lang}\"")
            };
            let _ = write!(
                out,
                "<pre class=\"code{wrap}\"><code{class}>{}</code></pre>",
                esc(&text)
            );
        }
        "mathBlock" => {
            cx.needs.insert("math");
            let l = n.attr_str("latex").unwrap_or("");
            let _ = write!(
                out,
                "<div class=\"math block\" data-latex=\"{}\">{}</div>",
                esc(l),
                esc(l)
            );
        }
        "horizontalRule" => out.push_str("<hr>"),
        "image" => image(out, n, cx),
        "embed" => embed(out, n, cx),
        "table" => {
            out.push_str("<div class=\"tablewrap\"><table>");
            for row in &n.content {
                out.push_str("<tr>");
                for c in &row.content {
                    let tag = if c.ty == "tableHeader" { "th" } else { "td" };
                    let span = |k: &str| {
                        c.attr_i64(k)
                            .filter(|x| *x > 1)
                            .map(|x| format!(" {k}=\"{x}\""))
                            .unwrap_or_default()
                    };
                    let _ = write!(out, "<{tag}{}{}>", span("colspan"), span("rowspan"));
                    blocks(out, &c.content, cx);
                    let _ = write!(out, "</{tag}>");
                }
                out.push_str("</tr>");
            }
            out.push_str("</table></div>");
        }
        "details" => {
            let open = if n.attr_bool("open") { " open" } else { "" };
            let _ = write!(out, "<details{open}>");
            for c in &n.content {
                match c.ty.as_str() {
                    "detailsSummary" => {
                        out.push_str("<summary>");
                        inline(out, &c.content, cx);
                        out.push_str("</summary>");
                    }
                    "detailsContent" => blocks(out, &c.content, cx),
                    _ => block(out, c, cx),
                }
            }
            out.push_str("</details>");
        }
        _ => {
            if n.content.iter().any(|c| !c.is_text())
                && !n.content.iter().all(|c| c.is_text() || c.ty == "hardBreak")
            {
                blocks(out, &n.content, cx);
            } else {
                out.push_str("<p>");
                inline(out, &n.content, cx);
                out.push_str("</p>");
            }
        }
    }
}

fn list(out: &mut String, n: &Node, cx: &mut Ctx, tag: &str, attrs: &str) {
    let _ = write!(out, "<{tag}{attrs}>");
    for item in &n.content {
        if item.ty == "taskItem" {
            let checked = if item.attr_bool("checked") {
                " checked"
            } else {
                ""
            };
            let _ = write!(
                out,
                "<li class=\"task\"><input type=\"checkbox\" disabled{checked}><div>"
            );
            blocks(out, &item.content, cx);
            out.push_str("</div></li>");
        } else {
            block(out, item, cx);
        }
    }
    let _ = write!(out, "</{tag}>");
}

fn image(out: &mut String, n: &Node, cx: &mut Ctx) {
    let src = n.attr_str("src").unwrap_or("");
    let alt = esc(n.attr_str("alt").unwrap_or(""));
    let width = n
        .attr_i64("width")
        .filter(|w| (10..=100).contains(w))
        .map(|w| format!(" style=\"width:{w}%\""))
        .unwrap_or_default();
    let caption = n.attr_str("caption").filter(|c| !c.is_empty());
    let _ = write!(out, "<figure class=\"img\"{width}>");
    match oad_core::media::own_file_id(src).and_then(|id| cx.files.get(&id)) {
        Some(f) => {
            let base = cx.file_href(f);
            let wh = match (f.blob.width, f.blob.height) {
                (Some(w), Some(h)) => format!(" width=\"{w}\" height=\"{h}\""),
                _ => String::new(),
            };
            let animated = f.blob.mime == "image/gif" || f.blob.mime == "image/svg+xml";
            if animated {
                let _ = write!(
                    out,
                    "<img src=\"{base}\" alt=\"{alt}\"{wh} loading=\"lazy\" decoding=\"async\">"
                );
            } else {
                let set: Vec<String> = [640, 960, 1280, 1920]
                    .iter()
                    .filter(|w| f.blob.width.is_none_or(|bw| **w as i64 <= bw.max(640)))
                    .map(|w| format!("{base}?w={w} {w}w"))
                    .collect();
                let _ = write!(
                    out,
                    "<img src=\"{base}?w=1280\" srcset=\"{}\" sizes=\"(max-width: 760px) 100vw, 720px\" alt=\"{alt}\"{wh} loading=\"lazy\" decoding=\"async\">",
                    set.join(", ")
                );
            }
        }
        None => match safe_href(src).filter(|_| oad_core::media::own_file_id(src).is_none()) {
            Some(u) => {
                let _ = write!(
                    out,
                    "<img src=\"{}\" alt=\"{alt}\" loading=\"lazy\" decoding=\"async\">",
                    esc(&cx.img(u))
                );
            }
            None => {
                let _ = write!(out, "<span class=\"missing\">{alt}</span>");
            }
        },
    }
    if let Some(c) = caption {
        let _ = write!(out, "<figcaption>{}</figcaption>", esc(c));
    }
    out.push_str("</figure>");
}

/// 单独一行的网址：本站的库画表格、本站的页面画链接、上传的文件画文件卡片，别的按解析缓存画卡片。
fn embed(out: &mut String, n: &Node, cx: &mut Ctx) {
    let url = n.attr_str("url").unwrap_or("");
    if let Some((id, _)) = oad_core::media::own_page_ref(url) {
        if let Some(t) = cx.tables.get(&id) {
            let title = cx.pages.get(&id).map(|p| p.0.clone()).unwrap_or_default();
            let _ = write!(
                out,
                "<section class=\"embed-db\"><h3><a href=\"{}\">{}</a></h3>{t}</section>",
                cx.page_href(&id),
                esc(&title)
            );
            return;
        }
        match cx.pages.get(&id) {
            Some((title, true)) => {
                let _ = write!(
                    out,
                    "<p><a class=\"pagelink\" href=\"{}\">📄 {}</a></p>",
                    cx.page_href(&id),
                    esc(title)
                );
            }
            _ => {
                let _ = write!(
                    out,
                    "<p class=\"missing\">{}</p>",
                    t(cx.lang, "（没有分享的页面）", &[])
                );
            }
        }
        return;
    }
    if let Some(f) = oad_core::media::own_file_id(url).and_then(|id| cx.files.get(&id)) {
        let size = human_size(f.blob.size);
        let href = cx.file_href(f);
        if f.blob.mime.starts_with("video/") {
            let _ = write!(
                out,
                "<video class=\"video\" controls preload=\"metadata\" src=\"{href}\"></video>"
            );
        } else if f.blob.mime.starts_with("audio/") {
            let _ = write!(
                out,
                "<audio controls preload=\"metadata\" src=\"{href}\"></audio>"
            );
        } else {
            let _ = write!(
                out,
                "<a class=\"card file\" href=\"{href}\"><span class=\"fileicon\">📎</span><span><b>{}</b><small>{} · {size}</small></span></a>",
                esc(&f.name),
                esc(&f.blob.mime)
            );
        }
        return;
    }
    let Some(href) = safe_href(url) else {
        let _ = write!(out, "<p>{}</p>", esc(url));
        return;
    };
    let Some(d) = cx.cards.get(url).cloned() else {
        let _ = write!(
            out,
            "<p><a class=\"bare\" href=\"{0}\" rel=\"noopener nofollow\">{0}</a></p>",
            esc(href)
        );
        return;
    };
    card(out, href, &d, cx, n.attr_str("view") == Some("bookmark"));
}

fn human_size(n: i64) -> String {
    let n = n as f64;
    if n >= 1048576.0 {
        format!("{:.1} MB", n / 1048576.0)
    } else if n >= 1024.0 {
        format!("{:.0} KB", n / 1024.0)
    } else {
        format!("{n} B")
    }
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// 静态卡片。
fn card(out: &mut String, href: &str, d: &Value, cx: &mut Ctx, bookmark: bool) {
    let kind = s(d, "kind");
    let href = esc(href);
    let title = esc(s(d, "title"));
    let desc = esc(s(d, "description"));
    let site = esc(if s(d, "site_name").is_empty() {
        s(d, "provider")
    } else {
        s(d, "site_name")
    });
    let thumb = d["thumbnail"]["url"].as_str().map(|u| esc(&cx.img(u)));
    // 视频：门面（封面 + 播放键），点了才加载播放器
    let player = d["embed"]["embed_url"].as_str().filter(|u| {
        u.starts_with("https://www.youtube-nocookie.com/")
            || u.starts_with("https://player.bilibili.com/")
    });
    if kind == "video"
        && !bookmark
        && let Some(p) = player
    {
        cx.needs.insert("video");
        let poster = thumb
            .clone()
            .map(|t| format!("<img src=\"{t}\" alt=\"\" loading=\"lazy\">"))
            .unwrap_or_default();
        let _ = write!(
            out,
            "<figure class=\"video facade\" data-embed=\"{}\"><button type=\"button\" aria-label=\"{play}\">{poster}<span class=\"play\">▶</span></button>\
             <figcaption><a href=\"{href}\" rel=\"noopener nofollow\">{title}</a></figcaption></figure>",
            esc(p),
            play = t(cx.lang, "播放", &[]),
        );
        return;
    }
    if kind == "image" && !bookmark {
        let src = thumb.unwrap_or_else(|| href.clone());
        let _ = write!(
            out,
            "<figure class=\"img\"><img src=\"{src}\" alt=\"{title}\" loading=\"lazy\"></figure>"
        );
        return;
    }
    if kind == "post" && !bookmark {
        let a = &d["author"];
        let avatar = a["avatar"]
            .as_str()
            .map(|u| {
                format!(
                    "<img class=\"avatar\" src=\"{}\" alt=\"\" loading=\"lazy\">",
                    esc(&cx.img(u))
                )
            })
            .unwrap_or_default();
        let text = d["embed"]["text"]
            .as_str()
            .map(esc)
            .unwrap_or_else(|| desc.clone());
        let media = d["media"][0]["url"]
            .as_str()
            .or_else(|| d["media"][0]["poster"].as_str())
            .map(|u| {
                format!(
                    "<img class=\"media\" src=\"{}\" alt=\"\" loading=\"lazy\">",
                    esc(&cx.img(u))
                )
            })
            .unwrap_or_default();
        let _ = write!(
            out,
            "<a class=\"card post\" href=\"{href}\" rel=\"noopener nofollow\"><span class=\"who\">{avatar}<b>{}</b> <small>@{}</small></span>\
             <span class=\"text\">{text}</span>{media}<small class=\"site\">{site}</small></a>",
            esc(a["name"].as_str().unwrap_or("")),
            esc(a["handle"].as_str().unwrap_or(""))
        );
        return;
    }
    // 书签：标题、说明、网站、封面
    let fav = d["favicon"]
        .as_str()
        .map(|u| {
            format!(
                "<img class=\"fav\" src=\"{}\" alt=\"\" loading=\"lazy\">",
                esc(&cx.img(u))
            )
        })
        .unwrap_or_default();
    let img = thumb
        .map(|t| format!("<img class=\"thumb\" src=\"{t}\" alt=\"\" loading=\"lazy\">"))
        .unwrap_or_default();
    let mut stats = String::new();
    if kind == "repo" {
        let e = &d["embed"];
        if let Some(n) = e["stars"].as_i64().or(e["stargazers_count"].as_i64()) {
            let _ = write!(stats, "★ {n} ");
        }
        if let Some(l) = e["language"].as_str() {
            let _ = write!(stats, "· {} ", esc(l));
        }
    }
    let title = if title.is_empty() {
        href.clone()
    } else {
        title
    };
    let _ = write!(
        out,
        "<a class=\"card bookmark\" href=\"{href}\" rel=\"noopener nofollow\"><span class=\"body\"><b>{title}</b>\
         <small class=\"desc\">{desc}</small><small class=\"site\">{fav}{site} {stats}</small></span>{img}</a>"
    );
}

fn inline(out: &mut String, nodes: &[Node], cx: &mut Ctx) {
    for n in nodes {
        match n.ty.as_str() {
            "text" => {
                let mut open = Vec::new();
                let mut s = String::new();
                for m in &n.marks {
                    let (o, c) = match m.ty.as_str() {
                        "bold" => ("<strong>".to_string(), "</strong>"),
                        "italic" => ("<em>".into(), "</em>"),
                        "strike" => ("<s>".into(), "</s>"),
                        "underline" => ("<u>".into(), "</u>"),
                        "highlight" => ("<mark>".into(), "</mark>"),
                        "code" => ("<code>".into(), "</code>"),
                        "link" => match m
                            .attrs
                            .get("href")
                            .and_then(Value::as_str)
                            .and_then(|h| link_href(cx, h))
                        {
                            Some(h) => {
                                let ext = if h.starts_with('/') {
                                    ""
                                } else {
                                    " rel=\"noopener nofollow\""
                                };
                                (format!("<a href=\"{}\"{ext}>", esc(&h)), "</a>")
                            }
                            None => continue,
                        },
                        _ => continue,
                    };
                    s.push_str(&o);
                    open.push(c);
                }
                out.push_str(&s);
                out.push_str(&esc(n.text.as_deref().unwrap_or("")));
                for c in open.iter().rev() {
                    out.push_str(c);
                }
            }
            "hardBreak" => out.push_str("<br>"),
            "mathInline" => {
                cx.needs.insert("math");
                let l = n.attr_str("latex").unwrap_or("");
                let _ = write!(
                    out,
                    "<span class=\"math\" data-latex=\"{}\">{}</span>",
                    esc(l),
                    esc(l)
                );
            }
            "linkMention" => {
                let url = n.attr_str("url").unwrap_or("");
                match oad_core::media::own_page_ref(url) {
                    Some((id, _)) => match cx.pages.get(&id) {
                        Some((t, true)) => {
                            let _ = write!(
                                out,
                                "<a class=\"mention\" href=\"{}\">📄 {}</a>",
                                cx.page_href(&id),
                                esc(t)
                            );
                        }
                        Some((t, false)) => {
                            let _ = write!(out, "<span class=\"mention\">📄 {}</span>", esc(t));
                        }
                        None => out.push_str("<span class=\"mention\">📄</span>"),
                    },
                    None => {
                        let title = cx
                            .cards
                            .get(url)
                            .map(|d| s(d, "title").to_string())
                            .filter(|t| !t.is_empty());
                        match safe_href(url) {
                            Some(h) => {
                                let _ = write!(
                                    out,
                                    "<a class=\"mention\" href=\"{}\" rel=\"noopener nofollow\">{}</a>",
                                    esc(h),
                                    esc(title.as_deref().unwrap_or(h))
                                );
                            }
                            None => out.push_str(&esc(url)),
                        }
                    }
                }
            }
            _ => {
                if n.content.is_empty() {
                    out.push_str(&esc(&n.plain_text()));
                } else {
                    inline(out, &n.content, cx);
                }
            }
        }
    }
}

const CSS: &str = r#"
:root{--bg:#fff;--fg:#37352f;--muted:#787774;--line:#e9e9e7;--soft:#f7f6f3;--link:#2383e2;--mark:#fbf3db;color-scheme:light dark}
@media (prefers-color-scheme:dark){:root{--bg:#191919;--fg:#e6e6e4;--muted:#9b9a97;--line:#2f2f2f;--soft:#252525;--link:#529cca;--mark:#594a1f}}
*{box-sizing:border-box}html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.65 -apple-system,BlinkMacSystemFont,'Segoe UI Variable Text','Segoe UI','PingFang SC','Hiragino Sans GB','Noto Sans SC','Source Han Sans SC','Microsoft YaHei',system-ui,sans-serif;word-wrap:break-word}
.page{max-width:720px;margin:0 auto;padding:56px 24px 96px}
.title{font-size:40px;line-height:1.2;font-weight:700;margin:0 0 24px}.title .icon{margin-right:.3em}
.crumbs{font-size:14px;color:var(--muted);margin-bottom:24px}.crumbs a{color:inherit}
a{color:var(--link);text-decoration:none}a:hover{text-decoration:underline}
h2{font-size:30px;margin:2em 0 .4em;line-height:1.3}h3{font-size:24px;margin:1.6em 0 .3em}h4,h5,h6{font-size:20px;margin:1.4em 0 .3em}
p{margin:.35em 0}blockquote{margin:.6em 0;padding:0 0 0 14px;border-left:3px solid var(--fg)}
ul,ol{padding-left:1.6em;margin:.3em 0}ul.tasks{list-style:none;padding-left:.2em}li.task{display:flex;gap:.5em}li.task input{margin-top:.4em}
code{font:85% 'SF Mono','JetBrains Mono',ui-monospace,Menlo,Consolas,'PingFang SC',monospace;background:var(--soft);padding:.15em .35em;border-radius:4px}
pre.code .hljs,pre.code code.hljs{background:none;padding:0}pre.code{background:var(--soft);padding:16px;border-radius:6px;overflow:auto;font-size:14px;line-height:1.5}pre.code code{background:none;padding:0;font-size:inherit}pre.wrap{white-space:pre-wrap}
mark{background:var(--mark);color:inherit}hr{border:0;border-top:1px solid var(--line);margin:1.5em 0}
figure{margin:1em 0}figure.img img{max-width:100%;height:auto;border-radius:4px;display:block}figcaption{color:var(--muted);font-size:14px;margin-top:6px}
.tablewrap,.dbwrap{overflow-x:auto;margin:1em 0}table{border-collapse:collapse;font-size:15px}th,td{border:1px solid var(--line);padding:6px 10px;vertical-align:top;text-align:left}th{background:var(--soft);font-weight:600}
table.db{min-width:100%;white-space:nowrap}table.db td{max-width:320px;overflow:hidden;text-overflow:ellipsis}.count{color:var(--muted);font-size:13px}
.callout{display:block;background:var(--soft);border-radius:6px;padding:12px 16px;margin:.8em 0}.callout-title{font-weight:600}
.callout.warning,.callout.caution{background:#fdebec}.callout.tip{background:#edf3ec}@media (prefers-color-scheme:dark){.callout.warning,.callout.caution{background:#3b2525}.callout.tip{background:#243127}}
details{margin:.5em 0}summary{cursor:pointer}
.card{display:flex;gap:12px;border:1px solid var(--line);border-radius:6px;padding:12px 14px;margin:.8em 0;color:inherit;text-decoration:none!important;overflow:hidden}
.card:hover{background:var(--soft)}.card b{display:block}.card small{display:block;color:var(--muted);font-size:13px}.card .desc{display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}
.card .body{flex:1;min-width:0}.card .thumb{width:160px;height:100px;object-fit:cover;border-radius:4px;flex:none}.card .fav{width:14px;height:14px;vertical-align:-2px;margin-right:4px}
.card.post{flex-direction:column}.card.post .who{display:flex;align-items:center;gap:8px}.card.post .who b{display:inline}.card.post .avatar{width:32px;height:32px;border-radius:50%}.card.post .media{max-width:100%;border-radius:6px}.card.post .text{white-space:pre-wrap}
.card.file{align-items:center}.fileicon{font-size:24px}
.video{width:100%;aspect-ratio:16/9;position:relative;background:#000;border-radius:6px;overflow:hidden;margin:1em 0}
.facade button{all:unset;cursor:pointer;display:block;width:100%;height:100%;position:relative}.facade img{width:100%;height:100%;object-fit:cover}
.facade .play{position:absolute;left:50%;top:50%;transform:translate(-50%,-50%);width:68px;height:48px;border-radius:12px;background:rgba(0,0,0,.7);color:#fff;font-size:22px;display:flex;align-items:center;justify-content:center}
.facade figcaption{position:absolute;left:0;right:0;bottom:0;padding:8px 12px;background:linear-gradient(transparent,rgba(0,0,0,.7));margin:0}.facade figcaption a{color:#fff}
.video iframe{width:100%;height:100%;border:0}
.math.block{text-align:center;margin:1em 0;overflow-x:auto}.mention{background:var(--soft);border-radius:4px;padding:0 4px}
.props{display:grid;grid-template-columns:max-content 1fr;gap:6px 24px;margin:0 0 24px;padding-bottom:16px;border-bottom:1px solid var(--line);font-size:15px}.props dt{color:var(--muted)}.props dd{margin:0}
.children ul{list-style:none;padding:0;margin:2em 0 0;border-top:1px solid var(--line)}.children li a{display:flex;gap:8px;padding:6px 4px;color:inherit}.children li a:hover{background:var(--soft)}
.embed-db h3{font-size:18px;margin:1.2em 0 .2em}.missing{color:var(--muted)}.gone{color:var(--muted);text-align:center;margin-top:20vh}
.foot{max-width:720px;margin:0 auto;padding:0 24px 32px;color:var(--muted);font-size:12px}
@media (max-width:600px){.page{padding:32px 16px 64px}.title{font-size:32px}.card .thumb{display:none}}
"#;

/// 页面脚本：按 window.OAD_NEEDS 只做用得到的事。
const SCRIPT: &str = r#"
(function(){var N=window.OAD_NEEDS||[],has=function(k){return N.indexOf(k)>=0};
function js(u){return new Promise(function(ok,no){var s=document.createElement('script');s.src=u;s.onload=ok;s.onerror=no;document.head.appendChild(s)})}
function css(u,m){var l=document.createElement('link');l.rel='stylesheet';l.href=u;if(m)l.media=m;document.head.appendChild(l)}
var CDN='https://cdn.jsdelivr.net/npm/';
if(has('video'))document.querySelectorAll('.facade').forEach(function(f){f.querySelector('button').addEventListener('click',function(){var i=document.createElement('iframe');i.src=f.dataset.embed+(f.dataset.embed.indexOf('?')<0?'?':'&')+'autoplay=1';i.allow='autoplay; encrypted-media; picture-in-picture; fullscreen';i.allowFullscreen=true;f.innerHTML='';f.classList.remove('facade');f.appendChild(i)})});
if(has('code')){css(CDN+'@highlightjs/cdn-assets@11.11.1/styles/github.min.css','(prefers-color-scheme: light)');css(CDN+'@highlightjs/cdn-assets@11.11.1/styles/github-dark.min.css','(prefers-color-scheme: dark)');js(CDN+'@highlightjs/cdn-assets@11.11.1/highlight.min.js').then(function(){document.querySelectorAll('pre.code code[class^=language-]').forEach(function(e){try{hljs.highlightElement(e)}catch(_){}})})}
if(has('math')){css(CDN+'katex@0.16.22/dist/katex.min.css');js(CDN+'katex@0.16.22/dist/katex.min.js').then(function(){document.querySelectorAll('.math').forEach(function(e){try{katex.render(e.dataset.latex,e,{displayMode:e.classList.contains('block'),throwOnError:false})}catch(_){}})})}
if(has('mermaid'))js(CDN+'mermaid@11.4.1/dist/mermaid.min.js').then(function(){mermaid.initialize({startOnLoad:false,theme:matchMedia('(prefers-color-scheme: dark)').matches?'dark':'default'});mermaid.run({querySelector:'pre.mermaid'})});
if(has('table'))document.querySelectorAll('table.db[data-more]').forEach(function(t){var next=t.dataset.more,busy=false,s=document.createElement('div');t.parentNode.appendChild(s);
var io=new IntersectionObserver(function(es){if(!es[0].isIntersecting||busy||!next)return;busy=true;fetch(next).then(function(r){return r.json()}).then(function(d){t.tBodies[0].insertAdjacentHTML('beforeend',d.html);next=d.next;busy=false;if(!next)io.disconnect()}).catch(function(){busy=false})},{rootMargin:'600px'});io.observe(s)});
})();
"#;

/// 这个模块的接口（合进 [`crate::openapi`]）。公开页面（`/s/...`）不在 /api/v1 里，不写。
#[derive(OpenApi)]
#[openapi(paths(get_share, put_share, del_share))]
pub(crate) struct Doc;
