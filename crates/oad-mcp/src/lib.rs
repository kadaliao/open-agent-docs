//! MCP 端点（Streamable HTTP，rmcp）：Agent 用自己的 token（`Authorization: Bearer oad_...`）读写页面、回评论、取通知。
//!
//! 返回给 Agent 的都是给模型读的文字：页面里每个顶层块前面一行 `⟦块编号⟧`，按顶层块改；
//! 读页给出 `base`（版本），写的时候带回来，服务器在那个版本上改再合并，人后来打的字留得住。
mod i18n;
mod render;

use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use i18n::{INSTRUCTIONS, tr, tr_msg};
use oad_core::i18n::Lang;
use oad_core::{
    Anchor, AuthRequest, BlockOp, Core, Error as CoreError, NotifySpec, PagePatch, Principal,
    PrincipalKind,
};
use render::*;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CacheScope, CallToolResult, ContentBlock, Implementation, InitializeRequestParams,
    InitializeResult, ListToolsResult, PaginatedRequestParams, ProtocolVersion, ResultType,
    ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

/// 认证过的调用者（认证中间件放进请求的 extensions）。
#[derive(Clone)]
struct Caller(Principal);

pub struct OadMcp {
    core: Arc<Core>,
    tool_router: ToolRouter<Self>,
}

/// `/mcp` 的路由：先认 Agent token（不是 Agent 回 401），再交给 rmcp。关机时结束所有会话。
pub fn router(core: Arc<Core>) -> axum::Router {
    let ct = tokio_util::sync::CancellationToken::new();
    {
        let (ct, stop) = (ct.clone(), core.on_shutdown());
        tokio::spawn(async move {
            stop.await;
            ct.cancel();
        });
    }
    let c = core.clone();
    let config = StreamableHttpServerConfig::default()
        // Host / Origin 由 oad-server 的 guard 统一检查
        .disable_allowed_hosts()
        .with_cancellation_token(ct);
    let service: StreamableHttpService<OadMcp, LocalSessionManager> = StreamableHttpService::new(
        move || Ok(OadMcp::new(c.clone())),
        Default::default(),
        config,
    );
    axum::Router::new()
        .fallback_service(service)
        .layer(axum::middleware::from_fn_with_state(core, auth))
}

async fn auth(State(core): State<Arc<Core>>, mut req: Request, next: Next) -> Response {
    let who = {
        let r = AuthRequest {
            headers: req.headers(),
            query_token: None,
        };
        core.authenticate(&r).await
    };
    match who {
        Ok(Some(p)) if p.kind == PrincipalKind::Agent => {
            req.extensions_mut().insert(Caller(p));
            next.run(req).await
        }
        Ok(_) => {
            // 还不知道是谁：按浏览器 / 客户端给的 Accept-Language
            let lang = req
                .headers()
                .get(header::ACCEPT_LANGUAGE)
                .and_then(|v| v.to_str().ok())
                .and_then(Lang::from_accept)
                .unwrap_or_default();
            let msg = tr!(lang, "要用 Agent 的 token：Authorization: Bearer oad_...");
            let mut res = (
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({
                    "error": { "code": "unauthorized", "message": msg }
                })),
            )
                .into_response();
            res.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer realm=\"open-agent-docs\""),
            );
            res
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

fn ok(text: String) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

/// 工具执行失败：给 Agent 看得懂的原因（按它的语言）。
fn fail(lang: Lang, e: CoreError) -> Result<CallToolResult, ErrorData> {
    let m = |m: &str| tr_msg(lang, m);
    let why = match &e {
        CoreError::NotFound(x) => tr!(lang, "找不到：{m}", m = m(x)),
        CoreError::Forbidden(x) => tr!(lang, "没有权限：{m}", m = m(x)),
        CoreError::BadRequest(x) => tr!(lang, "参数不对：{m}", m = m(x)),
        CoreError::Conflict(x) => tr!(lang, "冲突：{m}", m = m(x)),
        CoreError::Gone(x) => tr!(lang, "已失效：{m}", m = m(x)),
        CoreError::BadGateway(x) => tr!(lang, "去外部网站取东西失败：{m}", m = m(x)),
        CoreError::Unauthorized(x) => tr!(lang, "没有登录：{m}", m = m(x)),
        CoreError::Storage(_) | CoreError::Internal(_) => tr!(lang, "服务器内部错误，稍后再试"),
    };
    Ok(CallToolResult::error(vec![ContentBlock::text(why)]))
}

macro_rules! tri {
    ($lang:expr, $e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return fail($lang, e.into()),
        }
    };
}

/// 调用者和它的语言；认不出调用者时用中文报错。
macro_rules! who {
    ($s:expr, $parts:expr) => {
        match caller(&$parts) {
            Ok(p) => {
                let l = $s.core.lang_of(&p.user_id).await;
                (p, l)
            }
            Err(e) => return fail(Lang::Zh, e),
        }
    };
}

fn caller(parts: &http::request::Parts) -> Result<Principal, CoreError> {
    parts
        .extensions
        .get::<Caller>()
        .map(|c| c.0.clone())
        .ok_or_else(|| CoreError::Unauthorized("没有 Agent token".into()))
}

fn b64(v: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(v)
}

fn unb64(s: Option<&str>) -> Result<Option<Vec<u8>>, CoreError> {
    match s.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => URL_SAFE_NO_PAD
            .decode(s.trim_end_matches('='))
            .map(Some)
            .map_err(|_| CoreError::BadRequest("base 不对：要原样用 read_page 给的 base".into())),
    }
}

/// 去掉块编号外面的 ⟦⟧。
fn bid(s: &str) -> String {
    s.trim()
        .trim_start_matches('⟦')
        .trim_end_matches('⟧')
        .trim()
        .to_string()
}

fn opt_id(s: Option<String>) -> Option<String> {
    s.map(|x| bid(&x)).filter(|x| !x.is_empty())
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// 要找的词，在标题和正文里按子串匹配（不分大小写）。
    pub query: String,
    /// 最多返回几页，默认 10。
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListArgs {
    /// 只列这个页面下面的子页面；不给就列整个工作区。
    pub parent_id: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReadArgs {
    pub page_id: String,
    /// 从这个块开始读（分段读长页面时用上次结果末尾给的编号）。
    pub from_block: Option<String>,
    /// 最多读几块，默认 200。
    pub max_blocks: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CreateArgs {
    pub title: String,
    /// 正文 Markdown（可以不给，建空页）。
    pub markdown: Option<String>,
    /// 父页面；不给就建在最外层。
    pub parent_id: Option<String>,
    /// 放在同级里哪个页面后面；不给就放最后。
    pub after: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    /// 把 block 换成 markdown（可以是多块）
    Replace,
    /// 在 block 后面插入 markdown
    InsertAfter,
    /// 在 block 前面插入 markdown
    InsertBefore,
    /// 在页面最后加上 markdown
    Append,
    /// 删掉 block
    Delete,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct EditOp {
    /// 操作：replace / insert_after / insert_before / append / delete。
    pub op: OpKind,
    /// 块编号（read_page 里 ⟦⟧ 中的那串）；append 不用给。
    pub block: Option<String>,
    /// 新内容（Markdown）；delete 不用给。
    pub markdown: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct EditArgs {
    pub page_id: String,
    /// read_page 给的 base，原样带回来。
    pub base: Option<String>,
    /// 依次执行的操作；一步出错就都不改。
    pub ops: Vec<EditOp>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RewriteArgs {
    pub page_id: String,
    /// read_page 给的 base，原样带回来。
    pub base: Option<String>,
    /// 整页的新正文（Markdown）。
    pub markdown: String,
    /// 顺便改标题。
    pub title: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateArgs {
    pub page_id: String,
    pub title: Option<String>,
    /// 图标（一个 emoji）；给空字符串清掉。
    pub icon: Option<String>,
    /// 移到这个页面下面；给 "root" 移到最外层。
    pub parent_id: Option<String>,
    /// 移动时放在同级里这个页面后面；给 "first" 放最前；不给放最后。
    pub after: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct PageArg {
    pub page_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListCommentsArgs {
    pub page_id: String,
    /// 也列出已解决的，默认只列没解决的。
    pub include_resolved: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CommentArgs {
    /// 新评论：评论哪个页面。
    pub page_id: Option<String>,
    /// 回复 / 解决：哪一串评论。
    pub thread_id: Option<String>,
    /// 评论内容（Markdown）。
    pub body: Option<String>,
    /// 新评论挂在哪一块上（块编号）；不给就是对整页的评论。
    pub block: Option<String>,
    /// true：把这串标成已解决；false：重新打开。
    pub resolve: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UploadArgs {
    /// 文件名（带扩展名，比如 chart.png）。
    pub name: String,
    /// 文件内容，base64 编码。
    pub content_base64: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct HistoryArgs {
    pub page_id: String,
    /// 最多列几版，默认 30。
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RestoreArgs {
    pub page_id: String,
    /// 版本号（page_history 列出来的 #后面的数）。
    pub version: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DbCreateArgs {
    pub title: String,
    /// 属性（列），按顺序：`[{ "name": "状态", "type": "status" }, { "name": "价格", "type": "number", "format": "cny" },
    /// { "name": "标签", "type": "multi_select", "options": ["急", "慢"] }, { "name": "总价", "type": "formula", "formula": "prop(\"价格\") * 2" }]`。
    /// 类型：title、text、number、select、multi_select、status、date、person、checkbox、url、email、phone、files、
    /// created_time、updated_time、created_by、updated_by、relation（给 db_id，twoway: true 是双向）、rollup（给 relation、prop、fn）、formula、unique_id、button。
    /// 标题列自动有（叫“名称”）；想换名字就放一个 type 为 title 的。
    pub props: Option<Vec<serde_json::Value>>,
    /// 建在哪个页面下面；不给就在最外层。
    pub parent_id: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DbSchemaArgs {
    pub db_id: String,
    /// 依次执行的改动，一步出错就都不改：
    /// `{op: "add", name, type, options?, formula?, format?, after?}`、`{op: "rename", prop, name}`、`{op: "change_type", prop, type}`（能转的值转过去）、
    /// `{op: "delete", prop}`、`{op: "set_options", prop, options}`、`{op: "set_formula", prop, formula}`、`{op: "update", prop, ...}`、`{op: "move", prop, after?}`、
    /// `{op: "undo", seq}`（撤销第 seq 次表结构改动，seq 是之前 db_schema 返回的）。prop 写属性名。
    pub ops: Vec<serde_json::Value>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DbQueryArgs {
    pub db_id: String,
    /// 筛选：`{ "op": "and", "items": [{ "prop": "状态", "cond": "eq", "value": "进行中" }, { "prop": "价格", "cond": "gt", "value": 100 }] }`，
    /// 可以嵌套 `{ op: "or", items: [...] }`。cond：eq、neq、contains、not_contains、starts_with、gt、gte、lt、lte、empty、not_empty、is_within（past_week、next_month…）、checked…
    pub filter: Option<serde_json::Value>,
    /// 排序：`[{ "prop": "价格", "dir": "desc" }]`。
    pub sorts: Option<serde_json::Value>,
    /// 从这个视图（名字或 id）的筛选、排序出发。
    pub view: Option<String>,
    /// 搜索：任一格包含这些字。
    pub search: Option<String>,
    /// 最多几行，默认 50（最多 500）。
    pub limit: Option<usize>,
    /// 跳过前几行（翻页）。
    pub offset: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DbAddArgs {
    pub db_id: String,
    /// 每行一个对象：`{ "名称": "写周报", "状态": "进行中", "截止": "2026-11-20", "负责人": "张三" }`；
    /// 可以带 `body`（行的正文，Markdown）和 `after`（排在这一行后面）。
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DbUpdateRow {
    pub row_id: String,
    /// 要改的格子：`{ "状态": "已完成" }`；写 null 清空。
    pub set: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DbUpdateArgs {
    pub rows: Vec<DbUpdateRow>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct NotesArgs {
    /// 上次处理到的通知编号：编号不大于它的会标成已处理，只返回它之后的。不给就返回所有还没处理的。
    pub after: Option<i64>,
    /// 没有新通知时最多等几秒（0 到 60），默认不等。
    pub wait: Option<u32>,
}

impl OadMcp {
    pub fn new(core: Arc<Core>) -> Self {
        OadMcp {
            core,
            tool_router: Self::tool_router(),
        }
    }

    async fn workspace(&self, who: &Principal) -> Result<String, CoreError> {
        self.core
            .agent_of(who)
            .await?
            .filter(|a| a.active())
            .map(|a| a.workspace_id)
            .ok_or_else(|| CoreError::Forbidden("Agent 已撤销".into()))
    }

    async fn new_base(&self, page: &str, who: &Principal) -> String {
        self.core
            .version(page, who)
            .await
            .map(|v| b64(&v))
            .unwrap_or_default()
    }
}

#[tool_router]
impl OadMcp {
    #[tool(
        description = "全文搜索页面和数据库的行：中文按词、英文按词干，标题优先。返回页面编号（行就是行 id）、标题和命中的片段（**加粗**的是命中的词），命中在正文里时还给块编号。不知道页面编号时先用它，或者用 list_pages 看目录。"
    )]
    async fn search_pages(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let ws = tri!(lang, self.workspace(&who).await);
        let limit = a.limit.unwrap_or(10).clamp(1, 50) as usize;
        let hits = tri!(
            lang,
            self.core
                .search_index(&ws, &who, &a.query, None, limit, true)
                .await
        );
        ok(render_search(lang, &a.query, &hits))
    }

    #[tool(description = "页面目录：缩进表示层级，每行是标题、page_id 和更新时间。")]
    async fn list_pages(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<ListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let ws = tri!(lang, self.workspace(&who).await);
        let pages = tri!(lang, self.core.list_pages(&ws, &who).await);
        ok(render_tree(lang, &pages, opt_id(a.parent_id).as_deref()))
    }

    #[tool(
        description = "读一个页面：标题、正文（每个顶层块前面一行 ⟦块编号⟧，块内容是 Markdown）、base（改的时候带回来）和没解决的评论概要。页面很长时分段读：结果末尾会告诉你下一段从哪块开始。"
    )]
    async fn read_page(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<ReadArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let id = bid(&a.page_id);
        let (page, _) = tri!(lang, self.core.page_for(&id, &who, false).await);
        let (doc, version) = tri!(lang, self.core.content(&id, &who).await);
        let threads = tri!(lang, self.core.threads(&id, &who, Some(false)).await);
        let max = a.max_blocks.unwrap_or(200).clamp(1, 2000) as usize;
        let from = opt_id(a.from_block);
        let text = tri!(
            lang,
            render_page(
                lang,
                &page,
                &doc,
                &b64(&version),
                &threads,
                from.as_deref(),
                max
            )
        );
        // 行：先列属性；库：先给表结构
        let head = match page.kind.as_str() {
            "row" => match tri!(lang, self.core.row_props_text(&id, &who).await) {
                Some((db, props)) => render_row_props(lang, &db, &props),
                None => String::new(),
            },
            "database" => {
                let (_, meta, schema) = tri!(lang, self.core.schema_text(&id, &who).await);
                tr!(
                    lang,
                    "这是一个数据库（db_id: {id}，共 {seq} 次改动）。属性：\n{schema}查行用 db_query(db_id: \"{id}\")，加行 db_add，改行 db_update。\n\n",
                    id = id,
                    seq = meta.seq,
                    schema = schema
                )
            }
            _ => String::new(),
        };
        ok(format!("{head}{text}"))
    }

    #[tool(description = "建一个新页面（可以带正文 Markdown），返回 page_id、网页链接和 base。")]
    async fn create_page(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<CreateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let ws = tri!(lang, self.workspace(&who).await);
        let page = tri!(
            lang,
            self.core
                .create_page(
                    &ws,
                    &who,
                    opt_id(a.parent_id),
                    opt_id(a.after),
                    Some(a.title),
                    None
                )
                .await
        );
        if let Some(md) = a.markdown.filter(|m| !m.trim().is_empty()) {
            tri!(
                lang,
                self.core
                    .set_content(&page.id, &who, oad_markdown::parse(&md), None)
                    .await
            );
        }
        let base = self.new_base(&page.id, &who).await;
        ok(tr!(
            lang,
            "已建页面《{title}》\npage_id: {id}\n网页：{url}\nbase: {base}",
            title = page.title,
            id = page.id,
            url = self.core.notes.page_url(&page.id),
            base = base
        ))
    }

    #[tool(
        description = "按块改页面。ops 依次执行、一次提交（一步出错就都不改）：\
replace{block, markdown} 换掉一块（markdown 可以是多块）；insert_after{block, markdown} / insert_before{block, markdown} 插入；\
append{markdown} 加到最后；delete{block} 删掉。block 是 read_page 里 ⟦⟧ 中的编号，只能是顶层块：改列表、表格里的一项就把整个列表 / 表格的新 Markdown 给出来。\
一定带上 read_page 给的 base。返回每一步涉及的块编号和新的 base（接着改就用新的）。"
    )]
    async fn edit_page(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<EditArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let id = bid(&a.page_id);
        let base = tri!(lang, unb64(a.base.as_deref()));
        let mut ops = Vec::new();
        for (i, o) in a.ops.into_iter().enumerate() {
            let need_block = || {
                opt_id(o.block.clone()).ok_or_else(|| {
                    CoreError::BadRequest(tr!(Lang::Zh, "第 {n} 步要给 block", n = i + 1))
                })
            };
            let need_md = || {
                o.markdown.clone().ok_or_else(|| {
                    CoreError::BadRequest(tr!(Lang::Zh, "第 {n} 步要给 markdown", n = i + 1))
                })
            };
            ops.push(match o.op {
                OpKind::Replace => BlockOp::Replace {
                    block: tri!(lang, need_block()),
                    markdown: tri!(lang, need_md()),
                },
                OpKind::InsertAfter => BlockOp::InsertAfter {
                    block: tri!(lang, need_block()),
                    markdown: tri!(lang, need_md()),
                },
                OpKind::InsertBefore => BlockOp::InsertBefore {
                    block: tri!(lang, need_block()),
                    markdown: tri!(lang, need_md()),
                },
                OpKind::Append => BlockOp::Append {
                    markdown: tri!(lang, need_md()),
                },
                OpKind::Delete => BlockOp::Delete {
                    block: tri!(lang, need_block()),
                },
            });
        }
        let kinds: Vec<OpKind> = ops
            .iter()
            .map(|o| match o {
                BlockOp::Replace { .. } => OpKind::Replace,
                BlockOp::InsertAfter { .. } => OpKind::InsertAfter,
                BlockOp::InsertBefore { .. } => OpKind::InsertBefore,
                BlockOp::Append { .. } => OpKind::Append,
                BlockOp::Delete { .. } => OpKind::Delete,
            })
            .collect();
        let refs: Vec<Option<String>> = ops.iter().map(|o| o.block().map(str::to_string)).collect();
        let results = tri!(lang, self.core.edit_blocks(&id, &who, base, ops).await);
        let base = self.new_base(&id, &who).await;
        ok(render_edit(lang, &kinds, &refs, &results, &base))
    }

    #[tool(
        description = "整页换成新的 Markdown（大改时用；小改用 edit_page）。没变的块沿用编号，挂在上面的评论还在。带上 read_page 给的 base。可以顺便改标题。"
    )]
    async fn rewrite_page(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<RewriteArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let id = bid(&a.page_id);
        let base = tri!(lang, unb64(a.base.as_deref()));
        tri!(
            lang,
            self.core
                .set_content(&id, &who, oad_markdown::parse(&a.markdown), base)
                .await
        );
        if let Some(t) = a.title {
            let patch = PagePatch {
                title: Some(t),
                ..Default::default()
            };
            tri!(lang, self.core.update_page(&id, &who, patch).await);
        }
        let (doc, version) = tri!(lang, self.core.content(&id, &who).await);
        ok(tr!(
            lang,
            "已重写，现在共 {n} 块。\nbase: {base}",
            n = doc.content.len(),
            base = b64(&version)
        ))
    }

    #[tool(
        description = "改页面的标题、图标，或者移动页面（parent_id 给 \"root\" 移到最外层；after 给 \"first\" 放同级最前）。"
    )]
    async fn update_page(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<UpdateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let id = bid(&a.page_id);
        let parent = a.parent_id.map(|p| {
            let p = bid(&p);
            (p != "root" && !p.is_empty()).then_some(p)
        });
        let after = a.after.map(|x| {
            let x = bid(&x);
            (x != "first" && !x.is_empty()).then_some(x)
        });
        let moving = parent.is_some() || after.is_some();
        let patch = PagePatch {
            title: a.title,
            icon: a.icon.map(|i| (!i.trim().is_empty()).then_some(i)),
            parent_id: parent,
            after,
        };
        let p = tri!(lang, self.core.update_page(&id, &who, patch).await);
        let mut s = tr!(
            lang,
            "已更新：《{title}》 page_id: {id}",
            title = p.title,
            id = p.id
        );
        if let Some(i) = &p.icon {
            s.push_str(&tr!(lang, "，图标 {icon}", icon = i));
        }
        if moving {
            s.push_str(&match &p.parent_id {
                Some(parent) => tr!(lang, "，现在在 {parent} 下面", parent = parent),
                None => tr!(lang, "，现在在最外层"),
            });
        }
        ok(s)
    }

    #[tool(description = "把页面（连同子页面）移到回收站。")]
    async fn delete_page(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<PageArg>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let gone = tri!(lang, self.core.delete_page(&bid(&a.page_id), &who).await);
        let first = gone.first().map(|p| p.title.clone()).unwrap_or_default();
        ok(if gone.len() > 1 {
            tr!(
                lang,
                "已移到回收站：《{title}》和它的 {n} 个子页面",
                title = first,
                n = gone.len() - 1
            )
        } else {
            tr!(lang, "已移到回收站：《{title}》", title = first)
        })
    }

    #[tool(
        description = "列出页面的评论串：每串挂在哪一块（⟦块编号⟧ 和当时选中的原文）、谁说了什么、回复。默认只列没解决的。"
    )]
    async fn list_comments(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<ListCommentsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let id = bid(&a.page_id);
        let (page, _) = tri!(lang, self.core.page_for(&id, &who, false).await);
        let all = a.include_resolved.unwrap_or(false);
        let threads = tri!(
            lang,
            self.core
                .threads(&id, &who, if all { None } else { Some(false) })
                .await
        );
        ok(render_threads(lang, &page, &threads, all))
    }

    #[tool(
        description = "评论：给 page_id + body 写一条新评论（可以加 block 挂在某一块上）；给 thread_id + body 回复一串；\
给 thread_id + resolve: true 把这串标成已解决（false 重新打开），可以和 body 一起给（先回复再解决）。"
    )]
    async fn comment(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<CommentArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let body = a.body.filter(|b| !b.trim().is_empty());
        if let Some(t) = opt_id(a.thread_id) {
            let mut out = Vec::new();
            if let Some(b) = &body {
                let c = tri!(lang, self.core.reply(&t, &who, b, NotifySpec::Auto).await);
                out.push(tr!(lang, "已回复（comment_id: {id}）", id = c.id));
            }
            if let Some(r) = a.resolve {
                tri!(lang, self.core.resolve_thread(&t, &who, r).await);
                out.push(if r {
                    tr!(lang, "已标成已解决")
                } else {
                    tr!(lang, "已重新打开")
                });
            }
            if out.is_empty() {
                return fail(
                    lang,
                    CoreError::BadRequest("给了 thread_id，还要给 body 或 resolve".into()),
                );
            }
            return ok(tr!(
                lang,
                "{what}（thread_id: {id}）",
                what = out.join(&tr!(lang, "，")),
                id = t
            ));
        }
        let Some(page) = opt_id(a.page_id) else {
            return fail(
                lang,
                CoreError::BadRequest("要给 page_id（新评论）或 thread_id（回复 / 解决）".into()),
            );
        };
        let Some(body) = body else {
            return fail(lang, CoreError::BadRequest("新评论要给 body".into()));
        };
        let anchor = match opt_id(a.block) {
            Some(b) => {
                let (node, _) = tri!(lang, self.core.block(&page, &b, &who).await);
                Some(Anchor {
                    block_id: Some(b),
                    quote: Some(excerpt(&node.plain_text(), 200)),
                    ..Default::default()
                })
            }
            None => None,
        };
        let t = tri!(
            lang,
            self.core
                .create_thread(&page, &who, anchor, &body, NotifySpec::Auto)
                .await
        );
        ok(tr!(lang, "已评论（thread_id: {id}）", id = t.id))
    }

    #[tool(
        description = "上传一个文件（图片、PDF、表格……，内容用 base64），存到这个工作区。返回能直接放进页面的 Markdown：\
图片是 ![](网址)，别的文件是单独一行的网址（网页里显示成文件卡片）。再用 edit_page / create_page 把它写进页面。"
    )]
    async fn upload_file(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<UploadArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use base64::engine::general_purpose::{STANDARD, URL_SAFE};
        let (who, lang) = who!(self, parts);
        let ws = tri!(lang, self.workspace(&who).await);
        tri!(lang, self.core.require(&ws, &who, true).await);
        let raw: String = a
            .content_base64
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let raw = raw
            .split_once("base64,")
            .map(|(_, b)| b.to_string())
            .unwrap_or(raw);
        let data = match STANDARD.decode(&raw).or_else(|_| URL_SAFE.decode(&raw)) {
            Ok(d) => d,
            Err(_) => {
                return fail(
                    lang,
                    CoreError::BadRequest("content_base64 不是合法的 base64".into()),
                );
            }
        };
        let up = tri!(lang, self.core.media.upload(&ws, &who, &a.name, data).await);
        let md = if up.mime.starts_with("image/") {
            format!("![]({})", up.url)
        } else {
            up.url.clone()
        };
        let mut info = tr!(lang, "{mime}，{size} 字节", mime = up.mime, size = up.size);
        if let (Some(w), Some(h)) = (up.width, up.height) {
            info.push_str(&tr!(lang, "，{w}×{h}", w = w, h = h));
        }
        ok(tr!(
            lang,
            "已上传《{name}》（{info}）。放进页面用这段 Markdown：\n{md}",
            name = up.name,
            info = info,
            md = md
        ))
    }

    #[tool(
        description = "页面的历史版本：新的在前，每版有版本号、时间、种类（自动 / Agent 写入 / 恢复 / 手动）和改过的人；\
还列出每一版比上一版改了哪些块。要回到某一版用 restore_version。"
    )]
    async fn page_history(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<HistoryArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let id = bid(&a.page_id);
        let (page, _) = tri!(lang, self.core.page_for(&id, &who, false).await);
        let limit = a.limit.unwrap_or(30).clamp(1, 200) as usize;
        let (versions, _) = tri!(
            lang,
            self.core.list_versions(&id, &who, limit + 1, None).await
        );
        // 每一版和它的上一版比
        let mut changes = Vec::new();
        for w in versions.windows(2).take(limit) {
            let d = self
                .core
                .version_diff(&id, w[1].id, Some(w[0].id), &who)
                .await
                .unwrap_or_default();
            changes.push(Some(d));
        }
        changes.push(None);
        ok(render_history(
            lang,
            &page,
            &versions[..versions.len().min(limit)],
            &changes,
        ))
    }

    #[tool(
        description = "把页面恢复到某一版（version 是 page_history 里的版本号）。恢复是一次新的改动：现在的内容先存成一版，之后的历史都还在，随时能再恢复回来。"
    )]
    async fn restore_version(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<RestoreArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let id = bid(&a.page_id);
        let v = tri!(
            lang,
            self.core.restore_version(&id, a.version, &who, lang).await
        );
        let base = self.new_base(&id, &who).await;
        ok(tr!(
            lang,
            "已恢复到第 {v} 版（新的一版是 #{new}）\nbase: {base}",
            v = a.version,
            new = v.id,
            base = base
        ))
    }

    #[tool(
        description = "建一个数据库（表格）。返回 db_id 和表结构。之后用 db_add 加行、db_query 查。"
    )]
    async fn db_create(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<DbCreateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let ws = tri!(lang, self.workspace(&who).await);
        let db = tri!(
            lang,
            self.core
                .create_database(
                    &ws,
                    &who,
                    oad_core::db::NewDb {
                        title: Some(a.title),
                        parent_id: opt_id(a.parent_id),
                        lang,
                        ..Default::default()
                    }
                )
                .await
        );
        let title_key = db.schema["props"][0]["key"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let mut ops = Vec::new();
        for p in a.props.unwrap_or_default() {
            let mut p = match p {
                serde_json::Value::Object(o) => o,
                _ => {
                    return fail(
                        lang,
                        CoreError::BadRequest("props 里每一项要是 { name, type, ... }".into()),
                    );
                }
            };
            let is_title = p
                .get("type")
                .and_then(|t| t.as_str())
                .is_some_and(oad_core::dbengine::is_title_type);
            if is_title {
                if let Some(n) = p.get("name").cloned() {
                    ops.push(serde_json::json!({ "op": "rename", "prop": title_key, "name": n }));
                }
                continue;
            }
            p.insert("op".into(), "add".into());
            ops.push(serde_json::Value::Object(p));
        }
        let mut seq_note = String::new();
        if !ops.is_empty() {
            match self.core.change_schema(&db.page.id, &who, ops, lang).await {
                Ok(r) => seq_note = warnings_text(lang, &r),
                Err(e) => {
                    // 表结构不对：库不留着
                    let _ = self.core.delete_page(&db.page.id, &who).await;
                    return fail(lang, e);
                }
            }
        }
        let (_, _, text) = tri!(lang, self.core.schema_text(&db.page.id, &who).await);
        ok(tr!(
            lang,
            "已建数据库《{title}》\ndb_id: {id}\n网页：{url}\n属性：\n{schema}{notes}",
            title = db.page.title,
            id = db.page.id,
            url = self.core.notes.page_url(&db.page.id),
            schema = text,
            notes = seq_note
        ))
    }

    #[tool(
        description = "改数据库的表结构（加列、改名、改类型、删列、改选项、改公式），属性写名字。一批改动一起提交。\
返回新的表结构和这次改动的 seq（撤销：db_schema(ops: [{op: \"undo\", seq}])，删掉的列和值都能放回来）。"
    )]
    async fn db_schema(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<DbSchemaArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let id = bid(&a.db_id);
        let undo: Vec<i64> = a
            .ops
            .iter()
            .filter(|o| o["op"] == "undo")
            .filter_map(|o| o["seq"].as_i64())
            .collect();
        let rest: Vec<serde_json::Value> =
            a.ops.into_iter().filter(|o| o["op"] != "undo").collect();
        let mut notes = String::new();
        for s in undo {
            let r = tri!(lang, self.core.undo_schema(&id, &who, s).await);
            notes.push_str(&tr!(
                lang,
                "已撤销第 {s} 次改动（新的 seq: {seq}）\n",
                s = s,
                seq = r["seq"]
            ));
        }
        if !rest.is_empty() {
            let r = tri!(lang, self.core.change_schema(&id, &who, rest, lang).await);
            let changes = r["changes"].as_array().map(|a| a.len()).unwrap_or(0);
            if changes == 0 {
                notes.push_str(&tr!(lang, "没有变化"));
                notes.push('\n');
            } else {
                notes.push_str(&tr!(
                    lang,
                    "已改表结构（{n} 处，seq: {seq}；撤销用这个 seq）\n",
                    n = changes,
                    seq = r["seq"]
                ));
            }
            notes.push_str(&warnings_text(lang, &r));
        }
        let (page, _, text) = tri!(lang, self.core.schema_text(&id, &who).await);
        ok(tr!(
            lang,
            "{notes}《{title}》现在的属性：\n{schema}",
            notes = notes,
            title = page.title,
            schema = text
        ))
    }

    #[tool(
        description = "查数据库的行：按条件筛选、排序、搜索，返回 Markdown 表格（值是名字，不是编号），每行带 row_id，末尾是总数。\
不给条件就是全部（按库里的顺序）。行的正文用 read_page(row_id) 读。"
    )]
    async fn db_query(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<DbQueryArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let id = bid(&a.db_id);
        let view_id = match a.view.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            Some(v) => {
                let info = tri!(lang, self.core.database_info(&id, &who).await);
                let found = info.views.as_array().and_then(|vs| {
                    vs.iter()
                        .find(|x| {
                            x["id"] == v
                                || x["name"]
                                    .as_str()
                                    .is_some_and(|n| n.eq_ignore_ascii_case(v))
                        })
                        .and_then(|x| x["id"].as_str().map(str::to_string))
                });
                match found {
                    Some(f) => Some(f),
                    None => {
                        return fail(
                            lang,
                            CoreError::NotFound(tr!(Lang::Zh, "没有视图「{v}」", v = v)),
                        );
                    }
                }
            }
            None => None,
        };
        let limit = a.limit.unwrap_or(50).clamp(1, 500);
        let q = oad_core::db::QueryReq {
            view_id,
            filter: a.filter,
            sorts: a.sorts,
            search: a.search,
            limit: Some(limit),
            offset: a.offset,
            ..Default::default()
        };
        let (page, shown) = tri!(lang, self.core.query_shown(&id, &who, q).await);
        ok(render_db_table(lang, &page, &shown))
    }

    #[tool(
        description = "往数据库里加行。属性写名字，值也写名字（选项名、人名、行的名称、日期 2026-11-20、勾选 是 / 否）；\
单选、多选里没有的选项会新加上。返回新行的 row_id。"
    )]
    async fn db_add(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<DbAddArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        let id = bid(&a.db_id);
        if a.rows.is_empty() {
            return fail(lang, CoreError::BadRequest("rows 是空的".into()));
        }
        let rows = a
            .rows
            .into_iter()
            .map(|mut r| {
                let body = r
                    .remove("body")
                    .and_then(|b| b.as_str().map(str::to_string));
                let after = r.remove("after").and_then(|b| b.as_str().map(bid));
                oad_core::db::RowInput {
                    props: r,
                    after,
                    body,
                }
            })
            .collect();
        let (ids, _) = tri!(lang, self.core.add_db_rows(&id, &who, rows).await);
        let list: Vec<String> = ids.iter().map(|i| format!("- {i}")).collect();
        ok(tr!(
            lang,
            "已加 {n} 行，row_id：\n{list}",
            n = ids.len(),
            list = list.join("\n")
        ))
    }

    #[tool(
        description = "改数据库里的行：`[{ row_id, set: { \"状态\": \"已完成\", \"价格\": 120 } }]`，属性和值写名字，null 清空。\
只改给了的格子（别人同时改别的格子不受影响）。公式、汇总这些算出来的不能改。"
    )]
    async fn db_update(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<DbUpdateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        // 按库分开
        let mut by_db: Vec<(String, Vec<oad_core::CellOp>)> = Vec::new();
        for r in a.rows {
            let row = bid(&r.row_id);
            let (page, _) = tri!(lang, self.core.page_for(&row, &who, true).await);
            let Some(db) = page.parent_id.filter(|_| page.kind == "row") else {
                return fail(
                    lang,
                    CoreError::BadRequest(tr!(Lang::Zh, "{row} 不是数据库里的行", row = row)),
                );
            };
            let op = oad_core::CellOp {
                row_id: row,
                set: r.set,
                after: None,
            };
            match by_db.iter_mut().find(|(d, _)| *d == db) {
                Some((_, v)) => v.push(op),
                None => by_db.push((db, vec![op])),
            }
        }
        let mut n = 0;
        for (db, ops) in by_db {
            let (_, ch) = tri!(lang, self.core.patch_db_rows(&db, &who, ops).await);
            n += ch.len();
        }
        ok(if n == 0 {
            tr!(lang, "没有变化（值和原来一样）")
        } else {
            tr!(lang, "已改 {n} 格", n = n)
        })
    }

    #[tool(
        description = "取发给你的通知（比如有人在评论里叫你）。每条带一段说明：哪个页面、哪一段、评论内容、该怎么回复。\
处理完后下次调用时把最后一条的编号作为 after 传进来，它和之前的会标成已处理。可以用 wait 等新通知（最多 60 秒）。"
    )]
    async fn my_notifications(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<NotesArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let (who, lang) = who!(self, parts);
        if let Some(after) = a.after {
            tri!(
                lang,
                self.core
                    .ack_agent_notes(&who, Vec::new(), Some(after))
                    .await
            );
        }
        let wait = Duration::from_secs(a.wait.unwrap_or(0).min(60) as u64);
        let notes = tri!(
            lang,
            self.core.wait_agent_notes(&who, a.after, wait, 20).await
        );
        ok(render_notes(lang, &notes))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for OadMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "open-agent-docs",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS)
    }

    /// 握手：英文 Agent 拿英文的 instructions。
    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        context.peer.set_peer_info(request.clone());
        let mut info = self.negotiate_initialize(&request)?;
        let lang = self.lang_of_ctx(&context).await;
        info.instructions = Some(tr!(lang, INSTRUCTIONS));
        Ok(info)
    }

    /// 工具列表：英文 Agent 拿英文的工具说明和参数说明。
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let lang = self.lang_of_ctx(&context).await;
        let mut tools = self.tool_router.list_all();
        if lang != Lang::Zh {
            for t in &mut tools {
                if let Some(d) = &t.description {
                    t.description = Some(tr!(lang, d).into());
                }
                let mut schema = serde_json::Value::Object((*t.input_schema).clone());
                localize_schema(lang, &mut schema);
                if let serde_json::Value::Object(o) = schema {
                    t.input_schema = Arc::new(o);
                }
            }
        }
        let cache = context
            .protocol_version()
            .is_some_and(|v| v >= ProtocolVersion::V_2026_07_28);
        Ok(ListToolsResult {
            result_type: Some(ResultType::COMPLETE),
            tools,
            meta: None,
            next_cursor: None,
            ttl_ms: cache.then_some(0),
            // 说明随调用者的语言变，不能共用缓存
            cache_scope: cache.then_some(CacheScope::Private),
        })
    }
}

impl OadMcp {
    /// 请求的 HTTP 部分里认证中间件放的调用者 → 它的语言；拿不到用中文。
    async fn lang_of_ctx(&self, context: &RequestContext<RoleServer>) -> Lang {
        match context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|p| p.extensions.get::<Caller>())
        {
            Some(c) => self.core.lang_of(&c.0.user_id).await,
            None => Lang::Zh,
        }
    }
}

/// JSON schema 里每个 description 换成对应语言的。
fn localize_schema(lang: Lang, v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                match x {
                    serde_json::Value::String(d) if k == "description" => *d = tr!(lang, d),
                    _ => localize_schema(lang, x),
                }
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(|x| localize_schema(lang, x)),
        _ => {}
    }
}

fn warnings_text(lang: Lang, r: &serde_json::Value) -> String {
    match r["warnings"].as_array() {
        Some(w) if !w.is_empty() => {
            let lines: Vec<String> = w
                .iter()
                .filter_map(|x| x.as_str())
                .map(|x| format!("- {}", tr_msg(lang, x)))
                .collect();
            tr!(
                lang,
                "注意，这些公式 / 汇总现在算不出来：\n{lines}\n",
                lines = lines.join("\n")
            )
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descs(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(o) => {
                for (k, x) in o {
                    match x.as_str() {
                        Some(d) if k == "description" => out.push(d.to_string()),
                        _ => descs(x, out),
                    }
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| descs(x, out)),
            _ => {}
        }
    }

    /// 每个工具说明、参数说明都有英文。
    #[test]
    fn every_description_has_english() {
        let has = |zh: &str| i18n::MCP.iter().any(|(z, _)| *z == zh);
        assert!(has(INSTRUCTIONS));
        for t in OadMcp::tool_router().list_all() {
            let mut all = vec![t.description.clone().unwrap_or_default().to_string()];
            descs(
                &serde_json::Value::Object((*t.input_schema).clone()),
                &mut all,
            );
            for d in all {
                assert!(has(&d), "{} 缺英文：{d:?}", t.name);
            }
        }
    }
}
