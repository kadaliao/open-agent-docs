//! OpenAPI 3.1：从代码里的 `#[utoipa::path]` 和 `ToSchema` 生成，`GET /api/v1/openapi.json` 给出，也写进 `docs/openapi.json`
//! （`cargo test -p oad-server --test openapi` 检查它和代码一致；`OAD_WRITE_OPENAPI=1` 时重写）。
//! 实时事件（SSE）和 Agent 的 webhook 写在 `webhooks` 里（OpenAPI 3.1 自带的字段）。
use serde::Serialize;
use serde_json::{Value, json};
use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};

/// 统一的错误：`{ error: { code, message } }`。message 按调用者的语言（中文 / 英文）。
#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorInfo,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorInfo {
    pub code: ErrorCode,
    /// 给人看的原因
    pub message: String,
}

/// 错误码：not_found 404、bad_request 400、unauthorized 401、forbidden 403、conflict 409、gone 410（邀请过期之类）、
/// too_large 413、too_many_requests 429（带 Retry-After）、bad_gateway 502（去外部网站取东西失败）、internal 500。
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    BadRequest,
    Unauthorized,
    Forbidden,
    Conflict,
    Gone,
    TooLarge,
    TooManyRequests,
    BadGateway,
    Internal,
}

/// 正文的节点树（和编辑器的 ProseMirror JSON 一样）：`{ type, attrs?, content?, text?, marks? }`。
#[derive(Serialize, ToSchema)]
pub struct DocNode {
    #[serde(rename = "type")]
    pub ty: String,
    #[schema(value_type = Object)]
    pub attrs: Option<Value>,
    #[schema(no_recursion)]
    pub content: Option<Vec<DocNode>>,
    pub text: Option<String>,
    pub marks: Option<Vec<DocMark>>,
}

#[derive(Serialize, ToSchema)]
pub struct DocMark {
    #[serde(rename = "type")]
    pub ty: String,
    #[schema(value_type = Object)]
    pub attrs: Option<Value>,
}

struct Security;

impl Modify for Security {
    fn modify(&self, api: &mut utoipa::openapi::OpenApi) {
        let c = api.components.get_or_insert_with(Default::default);
        c.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some(
                        "会话令牌（登录后的 cookie 值）、Agent 的 token（oad_…），或者 OAD_AUTH=jwt 时宿主签的 JWT",
                    ))
                    .build(),
            ),
        );
        c.add_security_scheme(
            "cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(
                oad_core::auth::SESSION_COOKIE,
            ))),
        );
        c.add_security_scheme(
            "header",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "X-OAD-Secret",
                "OAD_AUTH=header：宿主反代带 X-OAD-Secret 和 X-OAD-User-Id / -Name / -Email",
            ))),
        );
        api.security = Some(vec![
            utoipa::openapi::security::SecurityRequirement::new("bearer", Vec::<String>::new()),
            utoipa::openapi::security::SecurityRequirement::new("cookie", Vec::<String>::new()),
            utoipa::openapi::security::SecurityRequirement::new("header", Vec::<String>::new()),
        ]);
    }
}

/// 限流头：每个响应都带 `RateLimit-Limit`、`RateLimit-Remaining`、`RateLimit-Reset`（秒）；超了 429 + `Retry-After`。
struct RateHeaders;

impl Modify for RateHeaders {
    fn modify(&self, api: &mut utoipa::openapi::OpenApi) {
        let mut ext = api.extensions.take().unwrap_or_default();
        ext.merge(
            utoipa::openapi::extensions::ExtensionsBuilder::new()
                .add(
                    "x-rate-limit",
                    json!({
                        "headers": ["RateLimit-Limit", "RateLimit-Remaining", "RateLimit-Reset"],
                        "exceeded": { "status": 429, "code": "too_many_requests", "header": "Retry-After" },
                        "config": "OAD_RATE_LIMIT（每个调用者每分钟多少次，0 不限）",
                    }),
                )
                .add(
                    "x-pagination",
                    json!({
                        "style": "cursor",
                        "request": ["limit", "cursor"],
                        "response": "next_cursor（没有就是最后一页）",
                    }),
                )
                .build(),
        );
        api.extensions = Some(ext);
    }
}

/// 实时事件和 webhook：写进 OpenAPI 3.1 的 `webhooks`（直接写进最后的 JSON：utoipa 读不回任意类型的 schema）。
fn webhooks() -> Value {
    let mut hooks = serde_json::Map::new();
    let mut add = |name: &str, summary: &str, schema: Value| {
        hooks.insert(
            name.to_string(),
            json!({ "post": {
                "summary": summary,
                "requestBody": { "content": { "application/json": { "schema": schema } } },
                "responses": { "200": { "description": "收到" } }
            }}),
        );
    };
    let page = json!({ "$ref": "#/components/schemas/Page" });
    add(
        "agent.notification",
        "Agent 的 webhook：POST 到 Agent 设的地址；头 X-OAD-Timestamp（毫秒）、X-OAD-Signature: sha256=<hex(HMAC-SHA256(webhook_secret, \"<timestamp>.<body>\"))>、X-OAD-Delivery（通知编号）。回 2xx 算送到，否则退避重试",
        json!({ "$ref": "#/components/schemas/AgentNotification" }),
    );
    add(
        "page.created",
        "SSE（GET /workspaces/{ws}/events）：新建 / 从回收站放回的页面",
        page.clone(),
    );
    add(
        "page.updated",
        "SSE：页面改了标题、图标（行的标题改了也有，kind = row）",
        page.clone(),
    );
    add("page.moved", "SSE：页面挪了位置", page.clone());
    add("page.deleted", "SSE：页面进了回收站（行不发）", page);
    add(
        "page.purged",
        "SSE：页面彻底删了",
        json!({ "type": "object", "properties": { "id": { "type": "string" } } }),
    );
    add(
        "db.changed",
        "SSE：库的行变了，拉 rows?since=",
        json!({ "type": "object", "properties": { "db_id": { "type": "string" }, "seq": { "type": "integer" } } }),
    );
    add(
        "db.schema",
        "SSE：库的表结构或视图变了",
        json!({ "type": "object", "properties": { "db_id": { "type": "string" } } }),
    );
    add(
        "workspace.updated",
        "SSE：工作区改了名字、图标",
        json!({ "$ref": "#/components/schemas/Workspace" }),
    );
    add(
        "comment.created",
        "SSE：新评论",
        json!({ "type": "object", "properties": {
        "page_id": { "type": "string" }, "thread_id": { "type": "string" }, "comment_id": { "type": "string" },
        "thread": { "$ref": "#/components/schemas/ThreadView" } } }),
    );
    add(
        "comment.updated",
        "SSE：评论改了",
        json!({ "type": "object" }),
    );
    add(
        "comment.deleted",
        "SSE：评论删了",
        json!({ "type": "object" }),
    );
    add(
        "thread.updated",
        "SSE：评论串解决 / 重新打开",
        json!({ "type": "object" }),
    );
    add(
        "notification",
        "SSE：给本人的通知（收件箱）",
        json!({ "type": "object" }),
    );
    add(
        "resync",
        "SSE：落下了事件，重新拉列表",
        json!({ "type": "object" }),
    );
    Value::Object(hooks)
}

/// Agent 收到的一条通知（webhook 的正文；长轮询 / SSE 取到的也是这样）。
#[derive(Serialize, ToSchema)]
pub struct AgentNotification {
    /// 通知编号（处理完带回来当 after / cursor）
    pub id: i64,
    pub created_at: i64,
    /// comment
    #[serde(rename = "type")]
    pub kind: String,
    pub agent: NoteAgent,
    pub page: NotePage,
    pub thread_id: String,
    pub comment: NoteComment,
    /// 评论挂在哪一块上（对整页的评论是 null）
    pub block_id: Option<String>,
    /// 选中的原文
    pub quote: Option<String>,
    /// 一段可以直接当消息发给 Agent 的话（按 Agent 的语言）
    pub prompt: String,
}

#[derive(Serialize, ToSchema)]
pub struct NoteAgent {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, ToSchema)]
pub struct NotePage {
    pub id: String,
    pub title: String,
    pub url: String,
}

#[derive(Serialize, ToSchema)]
pub struct NoteComment {
    pub id: String,
    /// `{ kind, id, name }`
    #[schema(value_type = Object)]
    pub author: Value,
    pub body: String,
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Open Agent Docs",
        description = "人和 Agent 一起写的文档。接口都在 /api/v1 下；Agent 也可以用 MCP（/mcp）。",
        license(name = "MIT OR Apache-2.0")
    ),
    servers((url = "/api/v1")),
    modifiers(&Security, &RateHeaders),
    components(schemas(ErrorBody, ErrorInfo, ErrorCode, DocNode, DocMark, AgentNotification, NoteAgent, NotePage, NoteComment))
)]
pub struct ApiDoc;

/// 各模块的接口合在一起（每个模块一个 `Doc`）。
pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut api = ApiDoc::openapi();
    for part in [
        crate::api::Doc::openapi(),
        crate::collab::Doc::openapi(),
        crate::files::Doc::openapi(),
        crate::dbs::Doc::openapi(),
        crate::history::Doc::openapi(),
        crate::trash::Doc::openapi(),
        crate::members::Doc::openapi(),
        crate::share::Doc::openapi(),
    ] {
        api.merge(part);
    }
    api
}

/// 整份 OpenAPI（JSON），键按字母排，方便比较。
pub fn spec() -> Value {
    let mut v = serde_json::to_value(openapi()).unwrap_or_default();
    v["webhooks"] = webhooks();
    tidy(&mut v);
    sort(v)
}

/// 补上规范要求的字段：每个响应要有 description、每个操作要有 summary、operationId 不能重复。
fn tidy(v: &mut Value) {
    let reason = |code: &str| match code {
        "101" => "升级成 WebSocket",
        "200" => "成功",
        "201" => "建好了",
        "204" => "成功，没有内容",
        "206" => "一段内容（Range）",
        "304" => "没变（ETag）",
        "400" => "参数不对",
        "401" => "没有登录",
        "403" => "没有权限",
        "404" => "找不到",
        "409" => "冲突",
        "410" => "已失效",
        "413" => "太大",
        "416" => "Range 不对",
        "429" => "请求太多",
        "502" => "外部网站出错",
        _ => "响应",
    };
    let mut seen: std::collections::HashMap<String, usize> = Default::default();
    for section in ["paths", "webhooks"] {
        let Some(paths) = v[section].as_object_mut() else {
            continue;
        };
        for (path, ops) in paths.iter_mut() {
            let Some(ops) = ops.as_object_mut() else {
                continue;
            };
            for (method, op) in ops.iter_mut() {
                if let Some(rs) = op["responses"].as_object_mut() {
                    for (code, r) in rs.iter_mut() {
                        if r.get("description").is_none() {
                            r["description"] = json!(reason(code));
                        }
                    }
                }
                if op.get("summary").is_none() {
                    let s = op
                        .get("description")
                        .and_then(Value::as_str)
                        .and_then(|d| d.lines().next())
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("{} {path}", method.to_uppercase()));
                    op["summary"] = json!(s);
                }
                let id = op
                    .get("operationId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        path.replace(['.', '/', '{', '}'], "_")
                            .trim_matches('_')
                            .to_string()
                    });
                *seen.entry(id).or_default() += 1;
            }
        }
    }
    // 重复的 operationId 前面加上标签
    for section in ["paths", "webhooks"] {
        let Some(paths) = v[section].as_object_mut() else {
            continue;
        };
        for (path, ops) in paths.iter_mut() {
            let Some(ops) = ops.as_object_mut() else {
                continue;
            };
            for (_, op) in ops.iter_mut() {
                let id = op
                    .get("operationId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        path.replace(['.', '/', '{', '}'], "_")
                            .trim_matches('_')
                            .to_string()
                    });
                let id = if seen.get(&id).copied().unwrap_or(0) > 1 {
                    let tag = op["tags"][0].as_str().unwrap_or(section);
                    format!("{tag}_{id}")
                } else {
                    id
                };
                op["operationId"] = json!(id);
            }
        }
    }
    v["info"]["license"]["identifier"] = json!("MIT OR Apache-2.0");
}

fn sort(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<_> = m.into_iter().collect();
            keys.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(keys.into_iter().map(|(k, v)| (k, sort(v))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sort).collect()),
        v => v,
    }
}
