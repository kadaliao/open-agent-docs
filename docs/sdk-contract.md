# SDK 和嵌入：契约

目标：别人用主流语言接 Open Agent Docs，装一个包、几行代码就能用，体验一流；网页这边不管用什么前端框架都能把文档嵌进去。
这份契约是各工作包之间的接口：各语言的名字、行为、错误、测试场景以这里为准。改契约先改这里。

## 0. 范围和工作包

| 包 | 内容 | 目录 |
| --- | --- | --- |
| W-PY | Python SDK（同步 + 异步）；进程内服务器改名 | `sdks/python`、`crates/oad-py` |
| W-GO | Go SDK | `sdks/go` |
| W-RS | Rust 客户端 crate | `sdks/rust`（自己一个 cargo workspace） |
| W-JS | JS 客户端补齐（重试、错误、Node 工具、命名对照）；Web Component | `packages/client`、`packages/elements`（新） |
| W-WEB | 服务器 `format=html`；`@open-agent-docs/markdown` 转 HTML；iframe 跨域 JWT 登录 | `crates/oad-markdown`、`crates/oad-server`、`packages/markdown-wasm`、`apps/web` |
| W-REL | 统一测试的跑法、发布流程、文档 | `sdks/conformance`、`.github/workflows/release.yml`、`docs/{zh,en}/sdks.md` |

第一批语言：Python、Go、JS/TS、Rust。Java、C# 以后按同一份契约加。
不做：协同编辑（Yjs）只在 JS 里有；别的语言读写正文走 Markdown（`pages.content`）。

## 1. 包

| 语言 | 包名 | 引入 | 最低版本 | 发到 |
| --- | --- | --- | --- | --- |
| Python | `open-agent-docs` | `import open_agent_docs as oad` | 3.9 | PyPI |
| Python（进程内服务器） | `open-agent-docs-server`（原来的 `crates/oad-py`，模块 `open_agent_docs_server`） | 通过 `pip install "open-agent-docs[server]"` | 3.9 | PyPI（各平台 abi3 wheel） |
| Go | `github.com/kadaliao/open-agent-docs/sdks/go` | `import oad "github.com/kadaliao/open-agent-docs/sdks/go"` | 1.23 | git 标签 `sdks/go/vX.Y.Z` |
| Rust | `open-agent-docs`（lib `open_agent_docs`） | `use open_agent_docs as oad;` | 1.85（edition 2024） | crates.io |
| JS | `@open-agent-docs/client`、`/editor`、`/elements`、`/markdown`、`/server`、`/db` | | Node 18 | npm |

**版本**：所有 SDK 和服务器同一个版本号（`[workspace.package] version`），一起发。SDK 默认连同版本的服务器；服务器 `/api/v1` 只加不删，新 SDK 连旧服务器时，旧服务器没有的接口报 404。

**依赖**：能少就少。Python：`httpx`、`pydantic>=2`（没有别的）。Go：只用标准库。Rust：`reqwest`（rustls）、`serde`、`serde_json`、`tokio`、`futures-util`、`hmac`、`sha2`、`base64`、`thiserror`，下载器另放 feature。

## 2. 唯一的来源

- 接口、模型：`docs/openapi.json`（服务器代码生成，测试保证一致）。
- 名字和返回值整理：`sdks/operations.json`，`node sdks/check-operations.mjs` 检查两者对得上。每个 SDK 的生成器都读这两个文件；**生成的代码提交进仓库**，
  每个 SDK 有一个“生成结果是最新的”检查（像 `packages/client` 的 `gen:types --check`），CI 里跑。
- 生成器用各自语言写（`sdks/python/codegen/`、`sdks/go/internal/codegen/`、`sdks/rust/codegen/`），手写的运行时代码和生成的代码分开放（生成的文件头写“生成的，不要手改”）。

### 命名表的字段

| 字段 | 意思 |
| --- | --- |
| `name` | 点路径：`pages.versions.list`。前面是资源（可以嵌套），最后一段是方法。只有一段的（`search`）挂在客户端上 |
| `unwrap` | 响应是 `{ <字段>: [...] }` 这样只包一层的：直接返回那个字段（`pages.list` 返回页面数组） |
| `paginate` | 有 `next_cursor` 的列表：见 §6 翻页，字段是条目所在的数组 |
| `null_on_404` | 404 不算错，返回空（`pages.share.get` 没分享时返回 None / nil / `Option::None`） |
| `sse` | 事件流（`workspace` 或 `agent`），见 §8 |
| `upload` | 上传：`file`（multipart 的 `file` 字段）或 `csv`（`text/csv` 正文） |
| `download` | 返回二进制（字节，Go 是 `io.ReadCloser`，Rust 是 `bytes::Bytes`） |
| `js` | JS 客户端里这个接口的方法名（JS 保留现有的平铺写法，见 §12） |

### 各语言怎么转名字

| 点路径 | Python | Go | Rust | JS |
| --- | --- | --- | --- | --- |
| `pages.versions.list` | `client.pages.versions.list(...)` | `client.Pages.Versions.List(ctx, ...)` | `client.pages().versions().list(...)` | 看 `js` 字段（`versions`） |
| `search` | `client.search(...)` | `client.Search(ctx, ...)` | `client.search(...)` | `search` |
| 模型 `CreatePage` | `oad.CreatePage` | `oad.CreatePage` | `oad::types::CreatePage` | `Types.CreatePage` |

Go 的缩写按 Go 习惯：`Id`→`ID`、`Url`→`URL`、`Csv`→`CSV`、`Html`→`HTML`、`Ws`→`WS`（`ImportCSV`、`ExportCSV`、`DownloadNamed`）。

## 3. 客户端

```python
client = oad.Client(base_url="https://docs.example.com", token="oad_…")
client = oad.Client()                       # 读环境变量 OAD_BASE_URL、OAD_TOKEN
aclient = oad.AsyncClient(...)              # 一样的方法，全是 async
```

构造参数（各语言同名，按各自大小写习惯）：

| 参数 | 默认 | 说明 |
| --- | --- | --- |
| `base_url` | 环境变量 `OAD_BASE_URL`，再没有就 `http://127.0.0.1:8848` | 服务器地址，末尾的 `/` 去掉；`/api/v1` 由 SDK 拼 |
| `token` | 环境变量 `OAD_TOKEN` | Agent token、会话令牌或 JWT，带成 `Authorization: Bearer` |
| `header_secret` | 无 | `OAD_AUTH=header` 的服务器：配合 `with_user` 用（§4） |
| `jwt_secret` | 无 | `OAD_AUTH=jwt` 的服务器（HS256 共享密钥）：配合 `with_user` 用（§4） |
| `jwt_issuer`、`jwt_audience`、`jwt_ttl` | 无、无、3600 秒 | `with_user` 签 JWT 时写进去 |
| `timeout` | 60 秒 | 单个请求（不含事件流）的超时 |
| `max_retries` | 2 | 见 §5 |
| `locale` | 无 | `zh` / `en`：服务器错误信息的语言（`Accept-Language`） |
| `headers` | 无 | 每个请求额外带的头 |
| 自定义 HTTP 客户端 | | Python `http_client=httpx.Client(...)`，Go `HTTPClient *http.Client`，Rust `reqwest::Client` |

Go 用 `oad.NewClient(oad.WithBaseURL(...), oad.WithToken(...))`（函数式选项）；Rust 用 `oad::Client::builder().base_url(..).token(..).build()?`，`oad::Client::from_env()`。

每个请求带 `User-Agent: open-agent-docs-<语言>/<版本>`（如 `open-agent-docs-python/0.3.0`）。

## 4. 以某个用户的身份调（宿主后端用）

宿主自己做登录，后端替当前用户调 Open Agent Docs：

```python
admin = oad.Client(base_url=..., header_secret=SECRET)   # 或 jwt_secret=...
u = admin.with_user("u-42", name="王小明", email="x@example.com")
u.pages.list(u.default_workspace())
```

- `with_user(user_id, name=None, email=None, workspaces=None)` 返回一个新的客户端（共享连接池、配置），原来的不变。
- 配了 `header_secret`：带 `X-OAD-Secret`、`X-OAD-User-Id`、`X-OAD-User-Name`（UTF-8 百分号编码，等于 JS 的 `encodeURIComponent`）、`X-OAD-User-Email`。`workspaces` 给了报错（header 模式不支持）。
- 配了 `jwt_secret`：用 `sign_jwt`（§10）签一个，缓存到过期前 60 秒再换。
- 两个都配了，或者都没配：调 `with_user` 时报错（配置错误，不是 API 错误）。
- `default_workspace()`：`/me` 里第一个工作区的 id，每个客户端缓存一次。

## 5. 请求的行为

- **超时**：`timeout`，事件流没有总超时（只有连接超时 = `timeout`）。
- **重试**：最多 `max_retries` 次，只在这些情况：429；502、503、504；连不上 / 连接被断（请求还没发出去，或者方法是 GET、PUT、DELETE）。
  等待：有 `Retry-After`（秒）照它，最多 60 秒；没有就 0.5 秒 × 2^n，最多 8 秒，加 0–25% 的随机。POST、PATCH 遇到 5xx 不重试（可能已经生效了）。
- **压缩**：接受 gzip、br（各语言的 HTTP 库能解哪个就声明哪个）。
- **请求体**：值为“没给”的可选字段不发（不是发 `null`）；明确要发 `null` 的（比如 `pages.update` 的 `after: null` = 放最前）各语言要有办法：
  Python 区分 `None`（发 null）和不传（`NOT_GIVEN` 哨兵，默认）；Go 用 `oad.Null[T]()` / 字段类型 `oad.Opt[T]`；Rust 用 `Option<Option<T>>` 并配 `serde_with` 风格的“双重 Option”。

## 6. 方法的形状

- **参数**：路径参数按路径里的顺序做位置参数；查询参数和请求体字段：Python 是关键字参数（keyword-only），Go 是一个 `XxxParams` 结构体（最后一个参数；全是可选的可以传零值），
  Rust 是请求体结构体（实现 `Default`）加查询参数结构体，或者方法上的链式设置（生成器选一种，全 crate 一致）。
  请求体不是对象的（数组、任意 JSON），Python 用 `body=`。
- **返回**：解析好的模型；204 返回 None / 无；`unwrap` 的直接返回数组；`download` 返回字节。
- **翻页**（`paginate`）：
  - Python：`list(...)` 返回 `CursorPage[T]`（异步 `AsyncCursorPage[T]`；不叫 `Page` 是为了不和文档模型 `oad.Page` 重名），可以直接 `for x in page:` 逐条取完所有页（自动带 `cursor` 取下一页），`.items` 是这一页，`.next_cursor`、`.has_next_page()`、`.next_page()`；
    响应里的其它字段（`unread`、`seq`）也能取到。异步版是 `async for`。
  - Go：`List(ctx, ...)` 返回这一页（`*InboxPage` 之类）；`ListAll(ctx, ...)` 返回 `iter.Seq2[T, error]`。
  - Rust：`list(...)` 返回这一页；`list_all(...)` 返回 `impl Stream<Item = Result<T>>`。
  - 停止条件和 JS 的 `paginate` 一样：`next_cursor` 是空、这页没有条目、或者游标没变。
- **上传**：Python `files.upload(ws, file)`，`file` 是 bytes、路径（`pathlib.Path`）或者 `(文件名, bytes | 文件对象, 类型)`；Go `Upload(ctx, ws, name string, r io.Reader)`；
  Rust `upload(ws, name, bytes)`。`databases.import_csv(ws, csv_text_or_bytes, title=..., parent_id=...)`。

## 7. 错误

| 情况 | Python | Go | Rust |
| --- | --- | --- | --- |
| 服务器回了错误 | `oad.APIError(status, code, message, request_id?, retry_after?)` 和子类 `BadRequestError`(400) `AuthenticationError`(401) `PermissionDeniedError`(403) `NotFoundError`(404) `ConflictError`(409) `GoneError`(410) `TooLargeError`(413) `RateLimitError`(429) `BadGatewayError`(502) `InternalServerError`(≥500) | `*oad.APIError{StatusCode, Code, Message, RetryAfter}`；`errors.Is(err, oad.ErrNotFound)` 等哨兵（`ErrBadRequest` `ErrUnauthorized` `ErrForbidden` `ErrNotFound` `ErrConflict` `ErrGone` `ErrTooLarge` `ErrRateLimited` `ErrServer`） | `oad::Error::Api(ApiError { status, code, message, retry_after })`，`err.is_not_found()` 等 |
| 连不上、超时 | `oad.APIConnectionError`、`oad.APITimeoutError` | 包住原错误，`errors.Is(err, context.DeadlineExceeded)` 能用 | `Error::Connection`、`Error::Timeout` |
| 配置错误、webhook 验签失败 | `oad.OADError` 的子类（`WebhookVerificationError(kind)`） | `oad.ErrWebhookSignature` 等 | `Error::Config`、`Error::Webhook(kind)` |

`code` 取响应 `{ "error": { "code", "message" } }`，不是 JSON 时是 `http_<状态码>`。所有错误的基类 Python 是 `oad.OADError`。

## 8. 事件流

```python
with client.workspaces.events(ws) as events:
    for ev in events:                # ev.name、ev.id、ev.data（按事件名解析成模型）
        if ev.name == "page.created": print(ev.data.title)
```

- 解析标准 SSE（`event`、`data` 多行拼接、`id`、注释行是心跳）。
- `data` 按事件名解析成模型：工作区事件的名字和模型在 openapi.json 的 `webhooks` 里（`page.created` → `Page`……），不认识的事件名给原始 JSON。Agent 的流每条是 `AgentNotification`。
- 断了自动重连：1 秒起翻倍、最多 30 秒，带 `Last-Event-ID`；连上后重置。401、403、404 不重连，抛错。
- `resync` 事件照常给出来（意思是“落下了，重新拉一遍”）。
- 关闭：Python 的上下文管理器 / `.close()`；Go 用 `ctx` 取消（`Events(ctx, ws)` 返回 `iter.Seq2[Event, error]`）；Rust drop 掉 Stream。
- Agent 的通知：`agent.notifications.stream(after=None)` 同样的形状；处理完调 `agent.notifications.ack(...)`。文档里给出“取 → 处理 → 确认”的完整循环。

## 9. Webhook 验签

```python
note = oad.verify_webhook(secret, headers, body, tolerance=300)   # 返回 AgentNotification，不对就抛 WebhookVerificationError
```

- `headers`：任意映射，名字大小写不论；`body`：原始字节（或字符串，按 UTF-8）。
- 签名 = `sha256=` + 小写十六进制 HMAC-SHA256(secret, `"<X-OAD-Timestamp>.<body>"`)；用常量时间比较。
- 时间戳是毫秒；和现在差超过 `tolerance` 秒（前后都算）→ `stale`。缺头、时间戳不是整数、签名格式不对 → `malformed`；签名不对 → `signature`。
- 测试时能传“现在”（Python `now=`、Go `oad.VerifyWebhookAt`、Rust `verify_webhook_at`）。用 `sdks/conformance/vectors.json` 的 `webhook` 用例。
- Go 另给 `oad.WebhookHandler(secret, func(ctx, note) error) http.Handler`；Python 给框架无关的函数就够（文档里写 FastAPI、Flask 的例子）。

## 10. 嵌入工具

- `sign_jwt(secret, user_id, name=None, email=None, workspaces=None, ttl=3600, issuer=None, audience=None, now=None) -> str`：HS256，claims `sub`、`name`、`email`、`workspaces`、`iat`、`exp`、`iss`、`aud`（没给的不写）。
  `workspaces` 是工作区 id 的数组，或 `[{ "id", "role" }]`。用 vectors.json 的 `jwt` 用例检查。只做 HS256；要 RS/ES 的用户用自己的 JWT 库，文档写清楚 claims。
- `embed_url(base_url, page_id, theme=None, lang=None, token=None) -> str`：`<base>/p/<page_id 百分号编码>?embed=1[&theme=][&lang=][#token=<token>]`。token 放在 `#` 后面（不进服务器日志、不进 Referer），
  网页读走后从地址栏抹掉（W-WEB）。用 vectors.json 的 `embed_url` 用例检查。

## 11. 本地启动

```python
with oad.start(data_dir="./data", auth="header", header_secret="...") as server:
    client = oad.Client(base_url=server.url, header_secret="...")
```

- 参数：任意 `key=value` 转成环境变量 `OAD_<KEY 大写>`（`data_dir` → `OAD_DATA_DIR`），和命令行一致；`port` 不给时挑一个空端口。另有 `binary=`（二进制路径）。
- 返回 `Server`：`url`、`port`、`close()`（等页面都存好再返回）；支持上下文管理器 / `defer server.Close()` / Rust 的 drop（drop 里尽力关，显式 `shutdown().await` 更好）。
- 怎么跑：
  - Python：装了 `open-agent-docs[server]`（`open_agent_docs_server` 能 import）就在进程内跑；否则走二进制。
  - JS：`@open-agent-docs/server`（进程内，已有）。
  - Go、Rust（feature `launcher`，默认关）：二进制。
- 二进制从哪来，按顺序：`binary=` → 环境变量 `OAD_BINARY` → 缓存 → 下载。下载 `https://github.com/kadaliao/open-agent-docs/releases/download/v<版本>/open-agent-docs-v<版本>-<target>.tar.gz`，
  用同一个 release 的 `SHA256SUMS` 验，解到缓存目录（Python `platformdirs` 风格的用户缓存，Go `os.UserCacheDir()`，Rust `dirs::cache_dir()`）`/open-agent-docs/v<版本>/`。
  target：`aarch64-apple-darwin`、`x86_64-apple-darwin`、`x86_64-unknown-linux-gnu`、`aarch64-unknown-linux-gnu`；别的平台报错并提示用 Docker。
- 子进程：`<binary> serve`，环境变量带上参数，等 `GET /api/v1/auth/config` 回 200（最多 30 秒，期间进程退出就把它的 stderr 末尾放进错误信息）。关：SIGTERM，等 10 秒，再 kill。
- 版本：默认下载和 SDK 同版本的二进制。

## 12. JS（W-JS）

- 保留现有平铺写法（`client.createPage`、`client.pages`……，编辑器和网页都在用）。在 `sdks/operations.json` 的每个接口上填 `js` 字段（JS 里的方法名）；没有方法的接口补上。
  `check-operations.mjs` 加一条：每个接口都有 `js`，且 `createClient()` 返回的对象上真有这个方法（用一个小测试检查）。
- 补：`createClient({ maxRetries, timeout })`（§5 的重试规则）；`ApiError` 增加 `retryAfter`，导出 `isNotFound(e)` 之类的判断或子类；
  `@open-agent-docs/client/node`：`verifyWebhook`、`signJwt`（用 `node:crypto`）；`embedUrl` 放在 core（浏览器也能用）。`withUser` 只在 node 入口里（要密钥）。
- `@open-agent-docs/elements`（新包，W-JS）：Web Components，不要求宿主用 React（React 打进包里）。
  - `<oad-editor page-id base-url token theme lang>`：编辑器 + 标题（可选 `comments` 属性显示评论栏）；`<oad-viewer page-id base-url token>`：只读，用 `format=html` 或本地 WASM 转；`<oad-database db-id …>`：数据库视图。
  - `token` 属性或 `element.token = …`（JWT / Agent token）；`element.getToken = async () => …`（快过期时调它换新的）。
  - 事件：`oad-ready`、`oad-change`（标题、内容变了）、`oad-navigate`、`oad-error`（CustomEvent，`detail` 里给数据）。
  - 样式隔离：Shadow DOM，样式表打进包里；颜色、字体用 CSS 变量从外面改（和 `.oad-root` 的变量同名）。
  - 两种用法：`npm i @open-agent-docs/elements` 后 `import '@open-agent-docs/elements'`；或者 `<script type="module" src="https://cdn.jsdelivr.net/npm/@open-agent-docs/elements@<版本>/dist/elements.js">`（单文件 ESM，自带依赖）。
  - 例子：`examples/embed-elements`（纯 HTML，一个 Vue 例子可选）。

## 13. 网页和服务器（W-WEB）

- **只读 HTML**：`GET /pages/{id}/content?format=html` 返回 `{ html, version }`（`limit` 照常能用）。HTML 由 `oad-markdown` 从文档生成（和 Markdown 导出同一棵树），
  外面包 `<div class="oad-content">`；只出白名单里的标签和属性（不出 `<script>`、事件属性、`javascript:` 链接）；图片、文件链接是服务器上的绝对路径（`/api/v1/files/…`，宿主不同域时用 `OAD_PUBLIC_URL` 拼完整地址）；
  数学公式、图表等需要脚本的块出成带 `data-*` 的占位（文字内容保留）。样式表：服务器 `GET /embed/content.css`（不用登录、长缓存）和 `@open-agent-docs/markdown/content.css` 是同一份。
  OpenAPI 跟着更新（`Content` 加 `html` 字段，`format` 加 `html`），`sdks/operations.json` 不用改。
- **`@open-agent-docs/markdown`**：加 `toHtml(markdownOrDoc)`，和服务器同一份代码（WASM），输出和服务器逐字一样（加一个对照测试）。
- **iframe 跨域登录**：嵌入模式（`?embed=1`）的网页
  - 读地址里的 `#token=<jwt>`，存在内存里（不进 localStorage），马上用 `history.replaceState` 抹掉；之后 API 带 `Authorization: Bearer`，WebSocket 带 `?token=`。
  - 宿主 → iframe：`{ type: 'oad:token', token }` 换新的 token（要检查 origin：`OAD_ALLOWED_ORIGINS` 里的才认，没配就只认同源；这个列表经 `/api/v1/auth/config` 给网页）。
  - iframe → 宿主：收到 401 时发 `{ type: 'oad:auth-required' }`；token 离过期不到 60 秒时也发一次，宿主回 `oad:token`。
  - 文档（docs/zh|en/embedding.md）和 `examples/embed-iframe` 加跨域 + JWT 的用法。

## 14. 统一测试（各 SDK 都要过）

`sdks/conformance/run.sh`：编一次 `target/debug/open-agent-docs`，设 `OAD_BINARY`，依次跑各 SDK 的场景测试。各 SDK 的场景测试：
Python `sdks/python/tests/conformance/`（pytest），Go `go test ./... -run Conformance`，Rust `cargo test --test conformance`，JS `packages/client/test/conformance.test.ts`。
没设 `OAD_BINARY` 时场景测试跳过（单元测试照常跑）。每个场景用自己的临时数据目录。

| 编号 | 场景 | 检查 |
| --- | --- | --- |
| S1 | `start(data_dir=临时, auth="header", header_secret=…)` | `auth.config().mode == "header"`；`close()` 后端口连不上 |
| S2 | `with_user("u1", name="王小明")` | `me.get().user.name == "王小明"`；`default_workspace()` 是 `/me` 的第一个 |
| S3 | `pages.create(ws, title="周报", markdown="# 标题\n\n正文")` | `content.get` 的 markdown 含 `# 标题` 和 `正文`；`pages.list` 里有它；`update(title=)` 生效；`content.replace(markdown=, base=version)` 成功；`delete` 后 `get` 是 NotFound |
| S4 | 错误 | `pages.get("nope")` → NotFound（404、`not_found`、message 非空）；`token="oad_bogus"` 的 `me.get()` → 401 那一类 |
| S5 | `agents.create(ws, name="bot", scope="write")` | 拿它的 token 建客户端，`me.get().user.kind == "agent"` |
| S6 | 翻页：存 5 个版本，`pages.versions.list(id, limit=2)` 逐条取 | 取到 ≥ 5 条，id 不重复，确实翻了页（发了 ≥ 3 个请求） |
| S7 | 事件：开 `workspaces.events(ws)`，再建页面 | 10 秒内收到 `page.created`，`data` 是 Page 模型、id 对得上 |
| S8 | Agent 通知：建 Agent `bot`，用户评论 `@bot 看看`，Agent 客户端开 `agent.notifications.stream()` | 收到一条，`comment.body` 含“看看”；`ack` 后 `list()` 不再有它 |
| S9 | JWT：`start(auth="jwt", jwt_secret=S)`；`Client(jwt_secret=S).with_user("u9", name="小九")` | `me.get()` 成功；用别的密钥签的 → 401 那一类 |
| S10 | webhook | vectors.json 的 `webhook` 全过 |
| S11 | 嵌入工具 | vectors.json 的 `embed_url`、`jwt` 全过 |
| S12 | 分享 | 新页面 `share.get` 是空；`enable()` 的 url 含 `/s/`；`disable()` 后又是空 |
| S13 | HTML | `content.get(id, format="html").html` 含 `<h1`、不含 `<script` |
| S14 | 重试（本地假服务器，不用二进制） | 先回 429 + `Retry-After: 1` 再回 200 → 成功且等了 ≥ 1 秒；`max_retries=0` → RateLimit 错误，`retry_after == 1` |
| S15 | 文件 | `files.upload(ws, b"hello", "a.txt")` 后 `files.download(id)` 字节一样 |

## 15. 发布（W-REL）

推 `v*` 标签时（现有的 release.yml 加任务）：

| 任务 | 怎么发 | 用户要做的（一次） |
| --- | --- | --- |
| PyPI：`open-agent-docs`（纯 Python wheel + sdist） | `pypa/gh-action-pypi-publish`，trusted publishing | 在 PyPI 加 pending publisher：仓库 `kadaliao/open-agent-docs`，workflow `release.yml`，environment `pypi` |
| PyPI：`open-agent-docs-server`（四个平台的 abi3 wheel） | `maturin build` 编，同上发（单独的任务） | 同上，但 environment 用 `pypi-server`（PyPI 不让两个 pending publisher 配成一样的） |
| npm：`@open-agent-docs/*` + 四个 `@open-agent-docs/server-<平台>` | `npm publish --provenance --access public` | 建 npm org `open-agent-docs`；仓库 secret `NPM_TOKEN`（第一次必须用 token：包还不存在时没法配 trusted publishing；之后可以换） |
| crates.io：`open-agent-docs` | `cargo publish` | crates.io 的 API token → secret `CARGO_REGISTRY_TOKEN`（第一次必须用 token，之后可以换 trusted publishing） |
| Go | 推标签 `sdks/go/v<版本>` | 无 |

发版前检查：各 SDK 的版本号和标签一致（`scripts/versions.mjs` 一起改 Python、Rust SDK 的版本）。

## 16. 落地时的调整

做出来以后和上面不一样、已经认可的地方（各 SDK 的 README 里有细节）：

- **事件流**：400、410 也不重连（不只 401、403、404）。Rust 的 `events()` 和 Go 的迭代器在第一次连上之后才开始给数据，第一次就 401 的直接报错。
- **Agent 通知流重连**：服务器先看 `?after=` 再看 `Last-Event-ID`，所以重连时要把 `after` 换成最后收到的编号（Go），或者去掉 `after` 只带头（Python）。两种都对。
- **webhook 用例**：vectors.json 里的正文不是完整的 AgentNotification。只验签名用 Rust 的 `verify_webhook_signature`；Python 签名对但模型不全时返回不校验的模型。
- **重试**：读超时不重试（请求可能已经到了服务器）。
- **Go**：`WithUser` 返回 `(*Client, error)`；查询参数用普通类型，零值不发；只声明 gzip（标准库解不了 br）。
- **Rust**：三态字段是 `Nullable<T>`（只在请求模型里）；响应模型宽松解析（缺字段给默认值，枚举有 `Other`）；上传用原始正文加 `X-Filename`，不用 multipart。
- **Python**：配置错误叫 `ConfigurationError`；`verify_webhook` 的 `now=` 用秒；每个方法另有 `extra_headers`、`extra_query`、`extra_body`、`timeout`。
- **JS**：错误按函数判断（`isNotFound(e)`），不用子类；`@open-agent-docs/elements` 是一个入口文件加按需加载的块（全部打进一个文件会有几 MB）。
- **只读 HTML 和嵌入里的文件地址带签名**（`OAD_SIGNED_URL_TTL`，默认 24 小时）：不登录也能打开。宿主缓存 HTML 不能超过有效期；拿到 HTML 的人在有效期内都能打开文件。文档里要写明。
- **CORS**：`OAD_ALLOWED_ORIGINS`（加上 `OAD_PUBLIC_URL`）里的 origin 可以跨域直接调 `/api`，Web Component 不用宿主转发。

## 17. 文档

- `docs/zh/sdks.md`、`docs/en/sdks.md`：每种语言一节：安装、建客户端、建页面写 Markdown、替用户调、收事件、验 webhook、本地启动；一张接口对照表（点路径 → 各语言写法，由 operations.json 生成）。
- 每个 SDK 目录有 README（中英）；README、docs/README 的目录加链接；embedding.md 加 Web Component 和跨域 iframe。
