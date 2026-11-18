# 第 1 阶段约定（服务器 ↔ 网页）

服务器（`crates/oad-core`、`crates/oad-server`、`crates/oad-bin`）和网页（`packages/editor`、`apps/web`）并行开发，按这里对接。
改约定先改这份文件。

## 目标（第 1 阶段验收）

1. 两个浏览器同时编辑同一页不冲突，互相看到光标和在线头像；断网编辑、重连后合并。
2. Agent 通过 HTTP 改一段，连着的网页马上看到，人正在打的字不丢。
3. 打开 1000 段的页面：有本地缓存 < 50ms 出内容，没缓存 < 200ms；打字每键 p95 < 16ms。
4. 冷启动 < 50ms，空闲内存 < 30MB，单个二进制（网页打进去）< 30MB。
5. 登录方式可替换：内置账号、宿主可信请求头、不登录（本机单人）。

## 文档结构（Yjs / Yrs）

一页一个 Y.Doc：

- `XmlFragment "default"`：正文，y-prosemirror 的标准结构。节点类型、属性、mark 见 `crates/oad-markdown/src/node.rs` 开头的注释，**网页的 Tiptap schema 必须一字不差**（节点名、属性名、mark 名）。
  属性值为 null 的不写。每个块级节点（除 text、hardBreak、mathInline 外的所有节点）有 `id` 属性：8 位 `[0-9a-zA-Z]` 随机串。
  网页负责给新块分配编号；拆段、粘贴产生重复编号时，后出现的那个换新编号。
- `Text "title"`：页面标题，纯文字。服务器保存时把它同步到 `pages.title`。

网页这边的补充（`packages/editor`，对照测试 `packages/editor/test/schema.test.ts`；它生成的 `test/fixtures/all-nodes.json`、`all-nodes.ydoc.bin` 给服务器的测试用）：

- 有默认值的属性网页会照写进 Yjs：`heading.level`、`orderedList.start`（默认 1）、`taskItem.checked`（默认 false）、
  `details.open`（默认 false）、`callout.kind`（默认 `note`）、`mathBlock.latex` / `mathInline.latex`（默认空串）。服务器读到默认值和没写要当成一样。
- 表格格子没有 `colspan` / `rowspan` / `colwidth`（不用 prosemirror-tables），不能合并单元格，和 Markdown 一致；第一行是 `tableHeader`。
- mark 的顺序（也是嵌套顺序）和 `mark_rank` 一致；行内代码只排斥自己，“链接 + 代码”“粗体 + 代码”都能存。
- 网页用上游的 `y-prosemirror`（Tiptap 自带的 `@tiptap/y-tiptap` 分支在两人同一处打字时恢复光标有错，字会乱序），Yjs 里的结构一样。

位置按 UTF-16 计（Yrs 用 `OffsetKind::Utf16`）。服务器和 Agent 写入用的 client id 在 [2^32, 2^53) 里随机取（`oad_doc::server_client_id`，每次加载、每个写入者各一个），
浏览器用 Yjs 默认的 32 位随机数，两段不重叠。网页不要自己指定 client id。

## 同步：WebSocket

`GET /api/v1/sync/{page_id}`（升级成 WebSocket），**标准 y-websocket 协议**（网页直接用 `y-websocket` 的 `WebsocketProvider`，`serverUrl = ws(s)://host/api/v1/sync`，`roomname = page_id`）：

- 消息类型 0 = sync（step1 / step2 / update），1 = awareness，2 = auth（拒绝时发 `permission-denied` 后关连接，原因是 `unauthorized` / `forbidden` / `not_found`），3 = query awareness。
- 连上后服务器先发 step1 和当前 awareness。
- 认证：同源 cookie；跨源嵌入时用 `?token=...`（值就是会话令牌）。header 模式下宿主转发 WebSocket 时同样带那几个请求头。
- 只读权限的连接，服务器不接受它发来的 update（丢掉，不断开）；它的 awareness 照常转发。
- awareness 消息服务器转发给所有连接（包括发来的那个，y-websocket 靠收到消息判断连接活着）；连接断开时服务器替它清掉它的 awareness。
- 某个连接收得太慢（待发消息积压）会被服务器断开，客户端重连后重新同步。发得太快时服务器暂停读它（背压）。
- step1：同一个连接 2 秒内重复同样的 state vector 不再回；每分钟超过 20 次断开。一条消息最大 16MB。

awareness 里网页放 `{ user: { id, name, color, kind: "user" | "agent" }, cursor }`（`color` 是 `#rrggbb`，`cursor` 是 y-prosemirror 的光标）；通过 HTTP 写正文时服务器替写入者放一份
`{ user: { id, name, color, kind }, block: "<块编号>" | null }`（整页替换时 `block` 为 null），最后一次写入 3 秒后清掉。网页据此显示“Agent 正在写第几段”。

## HTTP 接口

前缀 `/api/v1`，JSON，错误统一 `{ "error": { "code": "not_found", "message": "..." } }`。
`code`：`bad_request`（400，含移动成环、`after` 不是同级、`base` 不是这页出过的版本）、`unauthorized`（401）、`forbidden`（403，不是成员、只读、Host / Origin 不允许）、
`not_found`（404）、`conflict`（409，如邮箱已注册）、`too_many_requests`（429，登录失败太多，带 `Retry-After`）、`internal`（500）。
时间都是毫秒时间戳（整数，UTC）。

登录：

- `GET  /auth/config` → `{ mode: "local" | "header" | "none", ... }`（网页据此决定要不要显示登录页）
- `POST /auth/login` `{ email, password }` → 设 `oad_session` cookie（HttpOnly、SameSite=Lax），返回 `me`
- `POST /auth/logout` → 204，清掉 cookie
- `GET  /me` → `{ user: { id, name, email, kind }, workspaces: [{ id, name, role }] }`，`role` 是 `owner` / `editor` / `viewer`

会话令牌也可以放在 `Authorization: Bearer ...` 里。同时带了几个时依次试 Bearer、`?token=`、cookie，浏览器里过期的 cookie 不会挡住明确给的令牌。
登录限流：同一个 IP + 邮箱 15 分钟内失败 10 次后先拒绝（429），成功一次清零。

页面：

page = `{ id, workspace_id, parent_id, position, title, icon, created_by, created_at, updated_at }`（`updated_at` 正文改了也会变）。

- `GET    /workspaces/{ws}/pages` → `{ pages: [page] }`（不含回收站里的，按 parent_id、position 排，网页自己拼树）
- `POST   /workspaces/{ws}/pages` `{ parent_id?, after?, title?, icon? }` → 201 + page。`after` 是同级里排在它前面的页面 id，不给就放最后。
- `GET    /pages/{id}` → page
- `PATCH  /pages/{id}` `{ title?, icon?, parent_id?, after? }` → page。
  移动：`parent_id`（`null` = 移到根，不给 = 不换父页面）和 `after`（页面 id = 放它后面，`null` = 放最前，不给 = 放最后）。`icon: null` 清掉图标。
  改 `title` 等于改 Y.Text "title"，连着的网页马上看到。
- `DELETE /pages/{id}` → `{ deleted: [id] }`，连子页一起进回收站
- `GET    /pages/{id}/content?format=markdown|json` → `{ markdown, version }` 或 `{ doc, version }`
- `PUT    /pages/{id}/content` `{ markdown, base? }` 或 `{ doc, base? }` → 204，整页替换（对得上的块沿用编号，只改变了的地方）
- `GET    /pages/{id}/blocks/{block_id}` → `{ block, markdown, version }`
- `PUT    /pages/{id}/blocks/{block_id}` `{ markdown, base? }` → `{ ids }`（替换后这几块的编号，第一块一般沿用原编号）
- `POST   /pages/{id}/blocks` `{ markdown, after? }` → `{ ids }`（插入，不给 after 加到最后）
- `DELETE /pages/{id}/blocks/{block_id}` → 204

写正文的接口都通过页面的同步中心写进 CRDT，连着的网页马上收到。

`version` 是读的那一刻的文档版本（Yrs snapshot，base64url）。写的时候把它当 `base` 交回来，服务器就在那个版本上改、再合并进当前文档，
效果和一个从那时起离线编辑的客户端一样：这期间人新打的字不会因为 Markdown 里没有而被删掉。不给 `base` 就按当前内容比对（Markdown 里没有的字会删掉）。

实时事件（侧栏、标题）：`GET /workspaces/{ws}/events`（SSE），事件 `page.created` / `page.updated` / `page.moved` / `page.deleted`，数据是 page。
删页面时子页各发一条 `page.deleted`。网页改标题时 `page.updated` 最多晚几十毫秒。服务器发现这个连接落下了事件时发 `resync`，网页重新拉列表。

`position` 是分数索引字符串，按字典序排；客户端不自己算，移动时给 `after`。

## 登录方式（`AuthProvider`）

服务器启动参数 / 环境变量 `OAD_AUTH`：

- `local`（默认）：内置账号，密码 argon2。没有用户时，首次启动用 `OAD_ADMIN_EMAIL` / `OAD_ADMIN_PASSWORD` 建管理员和默认工作区；也可用 `open-agent-docs user add`。
- `header`：宿主在前面做登录，转发时带 `X-OAD-User-Id` / `X-OAD-User-Name` / `X-OAD-User-Email`，并带 `X-OAD-Secret`（和 `OAD_HEADER_SECRET` 相同才认）。用户第一次出现时自动建，
  加进默认工作区（第一个是 owner，之后的是 editor）。值是 UTF-8，可以百分号编码（中文名字建议这样传）。
- `none`：本机单人，所有请求都是同一个本地用户。

## 请求来源（防 DNS rebinding 和跨站请求）

- **Host**：`none` 模式一直检查；别的模式配了 `OAD_ALLOWED_HOSTS` / `OAD_PUBLIC_URL` 才检查。允许 `localhost`、`127.0.0.1`、`[::1]`、
  `OAD_PUBLIC_URL` 的主机名和 `OAD_ALLOWED_HOSTS`（逗号分隔，不含端口；`*` = 不检查）。不允许的回 403。
- **Origin**：改数据的请求（POST / PUT / PATCH / DELETE）和 WebSocket 握手，带了 Origin 的必须是：同源（和 Host 或 `X-Forwarded-Host` 一致）、
  `OAD_PUBLIC_URL`、`OAD_ALLOWED_ORIGINS`（逗号分隔，跨源嵌入时配），或者服务器在本机时的本机页面（开发时 Vite 在另一个端口）。
  `Origin: null` 拒绝。不带 Origin 的请求（Agent、curl）照常放行。读（GET）不检查 Origin（没有 CORS 头，别的网站读不到结果）。
- 反向代理后面部署时设 `OAD_PUBLIC_URL`。

## 打包

`open-agent-docs serve`：一个二进制，网页（`apps/web/dist`）用 rust-embed 打进去；开发时 `OAD_WEB_DIR` 指向目录直接读，或用 Vite 开发服务器代理 `/api` 到服务器。
默认端口 `8848`（`OAD_PORT`），默认只听 `127.0.0.1`（`OAD_HOST=0.0.0.0` 对外），数据目录 `OAD_DATA_DIR`（默认 `./data`，SQLite 文件 `oad.db`）。
走 HTTPS 时设 `OAD_COOKIE_SECURE=1`。关机（SIGTERM / Ctrl-C）时先结束 SSE 和 WebSocket、存好打开的页面（最多 30 秒），再停 HTTP（最多 10 秒）。没有网页（`apps/web/dist` 没构建）时 `/` 回一个占位页；`/api/...` 以外找不到的路径回 `index.html`（前端路由）。
