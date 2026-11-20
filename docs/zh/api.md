# HTTP 接口

[English](../en/api.md) · [文档目录](README.md)

完整的参考是 OpenAPI 3.1：运行着的服务器上 `GET /api/v1/openapi.json`（不用登录），仓库里也有一份 [docs/openapi.json](../openapi.json)
（由代码生成，测试保证和代码一致）。拿它生成客户端：`npx openapi-typescript docs/openapi.json -o api.d.ts`，或者直接用 `@open-agent-docs/client`（就是这么生成的）。
可以用 Swagger UI、Scalar、Redocly 之类的工具打开看。

## 基本约定

- 所有接口在 **`/api/v1`** 下，请求和响应都是 JSON（上传文件、导入 CSV 用 multipart；导出 CSV、取文件是二进制）。
- **版本**：`/api/v1` 里只加东西（新接口、新的可选字段、新的事件），不改不删；要改的放进以后的 `/api/v2`，`v1` 继续能用一段时间。
  客户端要能忽略不认识的字段和事件。
- 时间是毫秒时间戳（UTC）；编号都是字符串。
- **压缩**：超过 1KB 的 JSON 响应按 `Accept-Encoding` 压成 br 或 gzip（浏览器、常见 HTTP 库会自动解）；实时事件（SSE）、WebSocket、文件不压。
- **正文的格式**：`GET /pages/{id}/content` 给 `{ markdown, version }`；`?format=json` 给文档树 `{ doc, version }`；
  `?format=html` 给只读 HTML `{ html, version }`（外面包 `<div class="oad-content">`，只有白名单里的标签，样式表是 `GET /embed/content.css`，
  见[嵌入](embedding.md#只读-html)）。写（`PUT`）用 Markdown 或文档树。
- **大页面先拿第一屏**：`GET /pages/{id}/content?limit=N`（哪种 `format` 都行）只给前 N 个顶层块，响应里带 `total`（一共几块）。

## 登录

按优先级认（见[配置](configuration.md#登录)和[嵌入](embedding.md)）：

| 方式 | 怎么带 |
| --- | --- |
| Agent token | `Authorization: Bearer oad_…`（Agent 设置里拿；也可以连 MCP `/mcp`） |
| 会话 | 登录（`POST /auth/login`）后的 cookie `oad_session`，或者 `Authorization: Bearer <会话令牌>` |
| JWT（`OAD_AUTH=jwt`） | `Authorization: Bearer <jwt>`、`?token=<jwt>` 或 cookie `oad_token` |
| header（`OAD_AUTH=header`） | 宿主转发时带 `X-OAD-Secret`、`X-OAD-User-Id` / `-Name` / `-Email` |

WebSocket（`/api/v1/sync/{page}`）在浏览器里带不了请求头，用 cookie 或 `?token=`。

## 错误

出错时是 HTTP 状态码 + `{ "error": { "code": "...", "message": "..." } }`。`message` 按调用者的语言（用户 / Agent 设的语言 > `Accept-Language` > 中文），
程序判断请看 `code`：

| code | 状态 | 什么时候 |
| --- | --- | --- |
| `bad_request` | 400 | 参数不对 |
| `unauthorized` | 401 | 没登录 / token 不对 |
| `forbidden` | 403 | 没权限（只读成员写、不是成员……） |
| `not_found` | 404 | 没有（没权限看的也当没有） |
| `conflict` | 409 | 冲突（名字重复、邮箱注册过） |
| `gone` | 410 | 有过、现在失效了（邀请过期 / 撤销） |
| `too_large` | 413 | 上传太大 |
| `too_many_requests` | 429 | 请求太多（带 `Retry-After`） |
| `bad_gateway` | 502 | 去外部网站取东西失败（链接预览） |
| `internal` | 500 | 服务器出错 |

## 翻页

列表多的接口用游标：`?limit=&cursor=`，响应里有 `next_cursor`，是 `null` 就到头了，不然下次带上它。现在有：页面的版本、数据库的历史、回收站、
Agent 的通知、收件箱。别的列表（页面、成员、评论……）一次全给；数据库的行用 `GET /databases/{id}/rows?since=<seq>` 拿增量，查询用 `offset`。

## 限流

每个调用者（按 token / 会话，没有就按 IP）每分钟最多 `OAD_RATE_LIMIT` 次（默认 1200，`0` 不限）。每个响应都带：

| 头 | 意思 |
| --- | --- |
| `RateLimit-Limit` | 每分钟上限 |
| `RateLimit-Remaining` | 这一分钟还剩几次 |
| `RateLimit-Reset` | 几秒后重置 |

超了回 429 `too_many_requests`，带 `Retry-After`（秒）。登录失败另有限制（同一个 IP + 邮箱 15 分钟最多失败 10 次）。

## 实时事件和 webhook

- 工作区的实时事件：`GET /workspaces/{ws}/events`（SSE）。事件名和数据写在 OpenAPI 的 `webhooks` 里：`page.created`、`page.updated`、`page.moved`、
  `page.deleted`、`page.purged`、`db.changed`（收到后 `rows?since=`）、`db.schema`、`comment.*`、`thread.updated`、`notification`、`workspace.updated`；
  落下了事件时收到 `resync`，重新拉一遍。
- 协同编辑：`/api/v1/sync/{page}`，标准的 y-websocket 协议。
- Agent 的 webhook：POST 到 Agent 设的地址，正文是 `AgentNotification`，头 `X-OAD-Timestamp`、`X-OAD-Signature: sha256=<hex(HMAC-SHA256(secret, "<timestamp>.<body>"))>`、
  `X-OAD-Delivery`，回 2xx 算送到。验签见 [Agent](agents.md)。
