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
  `?format=html` 给只读 HTML `{ html, version, assets_expire_at }`（外面包 `<div class="oad-content">`，只有白名单里的标签，样式表是 `GET /embed/content.css`，
  见[嵌入](embedding.md#只读-html)）：调用者看得到的图片、文件直接是[签名地址](#文件的签名地址)，`assets_expire_at` 是签名的过期时间（毫秒，没有签名地址时是 `null`）。
  写（`PUT`）用 Markdown 或文档树。
- **跨域**：别的域名上的网页直连接口要把它的 origin 写进 `OAD_ALLOWED_ORIGINS`（`/api/v1` 只对这些 origin 回 CORS 头，见[配置](configuration.md#请求来源host-和-origin)）。
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

## 文件的签名地址

上传的文件（`GET /files/{id}`、`/files/{id}/{文件名}`）要登录才能取、转存的图片（`GET /media/{hash}`）也一样，可 `<img>`、`<video>`、链接带不了
`Authorization`。签名地址不用登录：

```sh
curl -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"urls": ["/api/v1/files/Qj1y9Gufp11a/图.png", "Qj1y9Gufp11b", "/api/v1/media/9f86d0…"]}' \
  https://docs.example.com/api/v1/files/sign
# { "urls": ["/api/v1/files/Qj1y9Gufp11a/图.png?exp=1763712000&sig=…", "/api/v1/files/Qj1y9Gufp11b?exp=…&sig=…", null], "expires_at": 1763712000000 }
```

- `POST /files/sign`：`urls` 给文件地址（根相对的或带域名的，可以带 `?w=`）、转存图片的地址，或者只给文件 id，一次最多 500 个。返回的 `urls` 和请求一一对应，
  **只签调用者看得到的**（不是那个工作区的成员、不存在、不是本站文件的是 `null`）；`expires_at` 是过期时间（毫秒）。
- 签名地址 = 原地址 + `?exp=<unix 秒>&sig=<base64url(HMAC-SHA256(密钥, "<文件 id 或媒体 hash>|<exp>"))>`。签名不含缩略图宽度 `w` 和文件名，
  拿到以后可以自己加 `&w=320`。有效期 `OAD_SIGNED_URL_TTL`（默认 24 小时），往后凑整到一个刻度，同一段时间里签出来的地址一样（浏览器缓存用得上）。
- 取文件时签名有效就不用登录（浏览器缓存到过期为止）；过期了回 403 `signature_expired`，不对回 403 `bad_signature`（登录着、看得到的照样给）。
- `GET /pages/{id}/content?format=html` 里的图片、文件已经是签名地址，`assets_expire_at` 是它们的过期时间。

> [!WARNING]
> 1. **有有效期**：`format=html` 返回的 HTML 里，图片和文件地址带的签名默认 24 小时后过期（`OAD_SIGNED_URL_TTL`）。宿主把 HTML 缓存起来（知识库前台常这么做）的话，
>    缓存时间不要超过有效期：过期前重新取一次，或者按响应里的 `assets_expire_at` 安排刷新。
> 2. **拿到就能打开**：拿到这段 HTML（或签名地址）的任何人，在有效期内不登录也能打开里面的图片和文件。文档不该外泄的场景把有效期调短；
>    要长期公开的内容用分享链接。

## 实时事件和 webhook

- 工作区的实时事件：`GET /workspaces/{ws}/events`（SSE）。事件名和数据写在 OpenAPI 的 `webhooks` 里：`page.created`、`page.updated`、`page.moved`、
  `page.deleted`、`page.purged`、`db.changed`（收到后 `rows?since=`）、`db.schema`、`comment.*`、`thread.updated`、`notification`、`workspace.updated`；
  落下了事件时收到 `resync`，重新拉一遍。
- 协同编辑：`/api/v1/sync/{page}`，标准的 y-websocket 协议。
- Agent 的 webhook：POST 到 Agent 设的地址，正文是 `AgentNotification`，头 `X-OAD-Timestamp`、`X-OAD-Signature: sha256=<hex(HMAC-SHA256(secret, "<timestamp>.<body>"))>`、
  `X-OAD-Delivery`，回 2xx 算送到。验签见 [Agent](agents.md)。
