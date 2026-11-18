//! MCP 端点（Streamable HTTP，rmcp）：Agent 用自己的 token（`Authorization: Bearer oad_...`）读写页面、回评论、取通知。
//!
//! 返回给 Agent 的都是给模型读的文字：页面里每个顶层块前面一行 `⟦块编号⟧`，按顶层块改；
//! 读页给出 `base`（版本），写的时候带回来，服务器在那个版本上改再合并，人后来打的字留得住。
mod render;

use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use oad_core::{
    Anchor, AuthRequest, BlockOp, Core, Error as CoreError, NotifySpec, PagePatch, Principal,
    PrincipalKind,
};
use render::*;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

/// 认证过的调用者（认证中间件放进请求的 extensions）。
#[derive(Clone)]
struct Caller(Principal);

const INSTRUCTIONS: &str = "这是 Open Agent Docs：人和 Agent 一起写的文档。\n\
- 页面正文按“块”组织（段落、标题、列表、表格……）。read_page 返回的正文里，每个顶层块前面有一行 ⟦块编号⟧，改的时候用这个编号指定块。\n\
- 改页面前先 read_page，拿到 base；edit_page / rewrite_page 时把 base 带回来。服务器在那个版本上改再合并，你读完之后别人新打的字不会被你覆盖。\n\
- 小改用 edit_page（按块替换、插入、删除，几步一起执行）；大改才用 rewrite_page（整页换，没变的块沿用编号，评论还挂着）。\n\
- 改一个列表、表格时，把整块的新 Markdown 给出来，服务器会沿用没变的项的编号。\n\
- 有人在评论里叫你时，用 my_notifications 取通知；回复和标成已解决用 comment。\n\
- 数据库（表格）：db_create 建库，db_schema 改列，db_add 加行，db_update 改格子，db_query 按条件查。属性和值都写名字（“状态”: “进行中”），不用写编号。\n\
  库里的每一行也是一个页面：行的正文照样用 read_page / edit_page（page_id 就是行 id），read_page 读行时先列属性。\n\
- 写给人看的内容用页面原来的语言。";

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
            let mut res = (
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({
                    "error": { "code": "unauthorized", "message": "要用 Agent 的 token：Authorization: Bearer oad_..." }
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

/// 工具执行失败：给 Agent 看得懂的原因。
fn fail(e: CoreError) -> Result<CallToolResult, ErrorData> {
    let why = match &e {
        CoreError::NotFound(m) => format!("找不到：{m}"),
        CoreError::Forbidden(m) => format!("没有权限：{m}"),
        CoreError::BadRequest(m) => format!("参数不对：{m}"),
        CoreError::Conflict(m) => format!("冲突：{m}"),
        CoreError::BadGateway(m) => format!("去外部网站取东西失败：{m}"),
        CoreError::Unauthorized(m) => format!("没有登录：{m}"),
        CoreError::Storage(_) | CoreError::Internal(_) => "服务器内部错误，稍后再试".to_string(),
    };
    Ok(CallToolResult::error(vec![ContentBlock::text(why)]))
}

macro_rules! tri {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return fail(e.into()),
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
        description = "按标题和正文找页面（子串匹配）。返回页面编号、标题和命中的片段。不知道页面编号时先用它，或者用 list_pages 看目录。"
    )]
    async fn search_pages(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let who = tri!(caller(&parts));
        let ws = tri!(self.workspace(&who).await);
        let limit = a.limit.unwrap_or(10).clamp(1, 50) as usize;
        let hits = tri!(self.core.search(&ws, &who, &a.query, limit).await);
        ok(render_search(&a.query, &hits))
    }

    #[tool(description = "页面目录：缩进表示层级，每行是标题、page_id 和更新时间。")]
    async fn list_pages(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<ListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let who = tri!(caller(&parts));
        let ws = tri!(self.workspace(&who).await);
        let pages = tri!(self.core.list_pages(&ws, &who).await);
        ok(render_tree(&pages, opt_id(a.parent_id).as_deref()))
    }

    #[tool(
        description = "读一个页面：标题、正文（每个顶层块前面一行 ⟦块编号⟧，块内容是 Markdown）、base（改的时候带回来）和没解决的评论概要。页面很长时分段读：结果末尾会告诉你下一段从哪块开始。"
    )]
    async fn read_page(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<ReadArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let who = tri!(caller(&parts));
        let id = bid(&a.page_id);
        let (page, _) = tri!(self.core.page_for(&id, &who, false).await);
        let (doc, version) = tri!(self.core.content(&id, &who).await);
        let threads = tri!(self.core.threads(&id, &who, Some(false)).await);
        let max = a.max_blocks.unwrap_or(200).clamp(1, 2000) as usize;
        let from = opt_id(a.from_block);
        let text = tri!(render_page(
            &page,
            &doc,
            &b64(&version),
            &threads,
            from.as_deref(),
            max
        ));
        // 行：先列属性；库：先给表结构
        let head = match page.kind.as_str() {
            "row" => match tri!(self.core.row_props_text(&id, &who).await) {
                Some((db, props)) => render_row_props(&db, &props),
                None => String::new(),
            },
            "database" => {
                let (_, meta, schema) = tri!(self.core.schema_text(&id, &who).await);
                format!(
                    "这是一个数据库（db_id: {id}，共 {} 次改动）。属性：\n{schema}查行用 db_query(db_id: \"{id}\")，加行 db_add，改行 db_update。\n\n",
                    meta.seq
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
        let who = tri!(caller(&parts));
        let ws = tri!(self.workspace(&who).await);
        let page = tri!(
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
                self.core
                    .set_content(&page.id, &who, oad_markdown::parse(&md), None)
                    .await
            );
        }
        let base = self.new_base(&page.id, &who).await;
        ok(format!(
            "已建页面《{}》\npage_id: {}\n网页：{}\nbase: {base}",
            page.title,
            page.id,
            self.core.notes.page_url(&page.id)
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
        let who = tri!(caller(&parts));
        let id = bid(&a.page_id);
        let base = tri!(unb64(a.base.as_deref()));
        let mut ops = Vec::new();
        for (i, o) in a.ops.into_iter().enumerate() {
            let need_block = || {
                opt_id(o.block.clone())
                    .ok_or_else(|| CoreError::BadRequest(format!("第 {} 步要给 block", i + 1)))
            };
            let need_md = || {
                o.markdown
                    .clone()
                    .ok_or_else(|| CoreError::BadRequest(format!("第 {} 步要给 markdown", i + 1)))
            };
            ops.push(match o.op {
                OpKind::Replace => BlockOp::Replace {
                    block: tri!(need_block()),
                    markdown: tri!(need_md()),
                },
                OpKind::InsertAfter => BlockOp::InsertAfter {
                    block: tri!(need_block()),
                    markdown: tri!(need_md()),
                },
                OpKind::InsertBefore => BlockOp::InsertBefore {
                    block: tri!(need_block()),
                    markdown: tri!(need_md()),
                },
                OpKind::Append => BlockOp::Append {
                    markdown: tri!(need_md()),
                },
                OpKind::Delete => BlockOp::Delete {
                    block: tri!(need_block()),
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
        let results = tri!(self.core.edit_blocks(&id, &who, base, ops).await);
        let base = self.new_base(&id, &who).await;
        ok(render_edit(&kinds, &refs, &results, &base))
    }

    #[tool(
        description = "整页换成新的 Markdown（大改时用；小改用 edit_page）。没变的块沿用编号，挂在上面的评论还在。带上 read_page 给的 base。可以顺便改标题。"
    )]
    async fn rewrite_page(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(a): Parameters<RewriteArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let who = tri!(caller(&parts));
        let id = bid(&a.page_id);
        let base = tri!(unb64(a.base.as_deref()));
        tri!(
            self.core
                .set_content(&id, &who, oad_markdown::parse(&a.markdown), base)
                .await
        );
        if let Some(t) = a.title {
            let patch = PagePatch {
                title: Some(t),
                ..Default::default()
            };
            tri!(self.core.update_page(&id, &who, patch).await);
        }
        let (doc, version) = tri!(self.core.content(&id, &who).await);
        ok(format!(
            "已重写，现在共 {} 块。\nbase: {}",
            doc.content.len(),
            b64(&version)
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
        let who = tri!(caller(&parts));
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
        let p = tri!(self.core.update_page(&id, &who, patch).await);
        let mut s = format!("已更新：《{}》 page_id: {}", p.title, p.id);
        if let Some(i) = &p.icon {
            s.push_str(&format!("，图标 {i}"));
        }
        if moving {
            s.push_str(&match &p.parent_id {
                Some(parent) => format!("，现在在 {parent} 下面"),
                None => "，现在在最外层".into(),
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
        let who = tri!(caller(&parts));
        let gone = tri!(self.core.delete_page(&bid(&a.page_id), &who).await);
        let first = gone.first().map(|p| p.title.clone()).unwrap_or_default();
        ok(if gone.len() > 1 {
            format!(
                "已移到回收站：《{first}》和它的 {} 个子页面",
                gone.len() - 1
            )
        } else {
            format!("已移到回收站：《{first}》")
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
        let who = tri!(caller(&parts));
        let id = bid(&a.page_id);
        let (page, _) = tri!(self.core.page_for(&id, &who, false).await);
        let all = a.include_resolved.unwrap_or(false);
        let threads = tri!(
            self.core
                .threads(&id, &who, if all { None } else { Some(false) })
                .await
        );
        ok(render_threads(&page, &threads, all))
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
        let who = tri!(caller(&parts));
        let body = a.body.filter(|b| !b.trim().is_empty());
        if let Some(t) = opt_id(a.thread_id) {
            let mut out = Vec::new();
            if let Some(b) = &body {
                let c = tri!(self.core.reply(&t, &who, b, NotifySpec::Auto).await);
                out.push(format!("已回复（comment_id: {}）", c.id));
            }
            if let Some(r) = a.resolve {
                tri!(self.core.resolve_thread(&t, &who, r).await);
                out.push(if r {
                    "已标成已解决".into()
                } else {
                    "已重新打开".into()
                });
            }
            if out.is_empty() {
                return fail(CoreError::BadRequest(
                    "给了 thread_id，还要给 body 或 resolve".into(),
                ));
            }
            return ok(format!("{}（thread_id: {t}）", out.join("，")));
        }
        let Some(page) = opt_id(a.page_id) else {
            return fail(CoreError::BadRequest(
                "要给 page_id（新评论）或 thread_id（回复 / 解决）".into(),
            ));
        };
        let Some(body) = body else {
            return fail(CoreError::BadRequest("新评论要给 body".into()));
        };
        let anchor = match opt_id(a.block) {
            Some(b) => {
                let (node, _) = tri!(self.core.block(&page, &b, &who).await);
                Some(Anchor {
                    block_id: Some(b),
                    quote: Some(excerpt(&node.plain_text(), 200)),
                    ..Default::default()
                })
            }
            None => None,
        };
        let t = tri!(
            self.core
                .create_thread(&page, &who, anchor, &body, NotifySpec::Auto)
                .await
        );
        ok(format!("已评论（thread_id: {}）", t.id))
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
        let who = tri!(caller(&parts));
        let ws = tri!(self.workspace(&who).await);
        tri!(self.core.require(&ws, &who, true).await);
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
                return fail(CoreError::BadRequest(
                    "content_base64 不是合法的 base64".into(),
                ));
            }
        };
        let up = tri!(self.core.media.upload(&ws, &who, &a.name, data).await);
        let md = if up.mime.starts_with("image/") {
            format!("![]({})", up.url)
        } else {
            up.url.clone()
        };
        let mut info = format!("{}，{} 字节", up.mime, up.size);
        if let (Some(w), Some(h)) = (up.width, up.height) {
            info.push_str(&format!("，{w}×{h}"));
        }
        ok(format!(
            "已上传《{}》（{info}）。放进页面用这段 Markdown：\n{md}",
            up.name
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
        let who = tri!(caller(&parts));
        let ws = tri!(self.workspace(&who).await);
        let db = tri!(
            self.core
                .create_database(
                    &ws,
                    &who,
                    oad_core::db::NewDb {
                        title: Some(a.title),
                        parent_id: opt_id(a.parent_id),
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
                    return fail(CoreError::BadRequest(
                        "props 里每一项要是 { name, type, ... }".into(),
                    ));
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
            match self.core.change_schema(&db.page.id, &who, ops).await {
                Ok(r) => seq_note = warnings_text(&r),
                Err(e) => {
                    // 表结构不对：库不留着
                    let _ = self.core.delete_page(&db.page.id, &who).await;
                    return fail(e);
                }
            }
        }
        let (_, _, text) = tri!(self.core.schema_text(&db.page.id, &who).await);
        ok(format!(
            "已建数据库《{}》\ndb_id: {}\n网页：{}\n属性：\n{text}{seq_note}",
            db.page.title,
            db.page.id,
            self.core.notes.page_url(&db.page.id)
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
        let who = tri!(caller(&parts));
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
            let r = tri!(self.core.undo_schema(&id, &who, s).await);
            notes.push_str(&format!("已撤销第 {s} 次改动（新的 seq: {}）\n", r["seq"]));
        }
        if !rest.is_empty() {
            let r = tri!(self.core.change_schema(&id, &who, rest).await);
            let changes = r["changes"].as_array().map(|a| a.len()).unwrap_or(0);
            if changes == 0 {
                notes.push_str("没有变化\n");
            } else {
                notes.push_str(&format!(
                    "已改表结构（{changes} 处，seq: {}；撤销用这个 seq）\n",
                    r["seq"]
                ));
            }
            notes.push_str(&warnings_text(&r));
        }
        let (page, _, text) = tri!(self.core.schema_text(&id, &who).await);
        ok(format!("{notes}《{}》现在的属性：\n{text}", page.title))
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
        let who = tri!(caller(&parts));
        let id = bid(&a.db_id);
        let view_id = match a.view.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            Some(v) => {
                let info = tri!(self.core.database_info(&id, &who).await);
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
                    None => return fail(CoreError::NotFound(format!("没有视图「{v}」"))),
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
        let (page, shown) = tri!(self.core.query_shown(&id, &who, q).await);
        ok(render_db_table(&page, &shown))
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
        let who = tri!(caller(&parts));
        let id = bid(&a.db_id);
        if a.rows.is_empty() {
            return fail(CoreError::BadRequest("rows 是空的".into()));
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
        let (ids, _) = tri!(self.core.add_db_rows(&id, &who, rows).await);
        let list: Vec<String> = ids.iter().map(|i| format!("- {i}")).collect();
        ok(format!(
            "已加 {} 行，row_id：\n{}",
            ids.len(),
            list.join("\n")
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
        let who = tri!(caller(&parts));
        // 按库分开
        let mut by_db: Vec<(String, Vec<oad_core::CellOp>)> = Vec::new();
        for r in a.rows {
            let row = bid(&r.row_id);
            let (page, _) = tri!(self.core.page_for(&row, &who, true).await);
            let Some(db) = page.parent_id.filter(|_| page.kind == "row") else {
                return fail(CoreError::BadRequest(format!("{row} 不是数据库里的行")));
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
            let (_, ch) = tri!(self.core.patch_db_rows(&db, &who, ops).await);
            n += ch.len();
        }
        ok(if n == 0 {
            "没有变化（值和原来一样）".into()
        } else {
            format!("已改 {n} 格")
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
        let who = tri!(caller(&parts));
        if let Some(after) = a.after {
            tri!(
                self.core
                    .ack_agent_notes(&who, Vec::new(), Some(after))
                    .await
            );
        }
        let wait = Duration::from_secs(a.wait.unwrap_or(0).min(60) as u64);
        let notes = tri!(self.core.wait_agent_notes(&who, a.after, wait, 20).await);
        ok(render_notes(&notes))
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
}

fn warnings_text(r: &serde_json::Value) -> String {
    match r["warnings"].as_array() {
        Some(w) if !w.is_empty() => {
            let lines: Vec<String> = w
                .iter()
                .filter_map(|x| x.as_str())
                .map(|x| format!("- {x}"))
                .collect();
            format!(
                "注意，这些公式 / 汇总现在算不出来：\n{}\n",
                lines.join("\n")
            )
        }
        _ => String::new(),
    }
}
