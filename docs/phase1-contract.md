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

位置按 UTF-16 计（Yrs 用 `OffsetKind::Utf16`）。服务器和 Agent 写入用的 client id 在 [2^32, 2^53) 里随机取（`oad_doc::server_client_id`，每次加载、每个写入者各一个），
浏览器用 Yjs 默认的 32 位随机数，两段不重叠。网页不要自己指定 client id。

## 同步：WebSocket

`GET /api/v1/sync/{page_id}`（升级成 WebSocket），**标准 y-websocket 协议**（网页直接用 `y-websocket` 的 `WebsocketProvider`，`serverUrl = ws(s)://host/api/v1/sync`，`roomname = page_id`）：

- 消息类型 0 = sync（step1 / step2 / update），1 = awareness，2 = auth（拒绝时发 `permission-denied` 后关连接，原因是 `unauthorized` / `forbidden` / `not_found`），3 = query awareness。
- 连上后服务器先发 step1 和当前 awareness。
- 认证：同源 cookie；跨源嵌入时用 `?token=...`（值就是会话令牌）。header 模式下宿主转发 WebSocket 时同样带那几个请求头。
- 只读权限的连接，服务器不接受它发来的 update（丢掉，不断开）；它的 awareness 照常转发。
- awareness 消息服务器转发给所有连接（包括发来的那个，y-websocket 靠收到消息判断连接活着）；连接断开时服务器替它清掉它的 awareness。
- 某个连接收得太慢（待发消息积压）会被服务器断开，客户端重连后重新同步。

awareness 里网页放 `{ user: { id, name, color, kind: "user" | "agent" } }`；通过 HTTP 写正文时服务器替写入者放一份
`{ user: { id, name, color, kind }, block: "<块编号>" | null }`（整页替换时 `block` 为 null），最后一次写入 3 秒后清掉。网页据此显示“Agent 正在写第几段”。

## HTTP 接口

前缀 `/api/v1`，JSON，错误统一 `{ "error": { "code": "not_found", "message": "..." } }`。
`code`：`bad_request`（400，含移动成环、`after` 不是同级）、`unauthorized`（401）、`forbidden`（403，不是成员或只读）、`not_found`（404）、`conflict`（409，如邮箱已注册）、`internal`（500）。
时间都是毫秒时间戳（整数，UTC）。

登录：

- `GET  /auth/config` → `{ mode: "local" | "header" | "none", ... }`（网页据此决定要不要显示登录页）
- `POST /auth/login` `{ email, password }` → 设 `oad_session` cookie（HttpOnly、SameSite=Lax），返回 `me`
- `POST /auth/logout` → 204，清掉 cookie
- `GET  /me` → `{ user: { id, name, email, kind }, workspaces: [{ id, name, role }] }`，`role` 是 `owner` / `editor` / `viewer`

会话令牌也可以放在 `Authorization: Bearer ...` 里。

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

## 打包

`open-agent-docs serve`：一个二进制，网页（`apps/web/dist`）用 rust-embed 打进去；开发时 `OAD_WEB_DIR` 指向目录直接读，或用 Vite 开发服务器代理 `/api` 到服务器。
默认端口 `8848`（`OAD_PORT`），默认只听 `127.0.0.1`（`OAD_HOST=0.0.0.0` 对外），数据目录 `OAD_DATA_DIR`（默认 `./data`，SQLite 文件 `oad.db`）。
走 HTTPS 时设 `OAD_COOKIE_SECURE=1`。没有网页（`apps/web/dist` 没构建）时 `/` 回一个占位页；`/api/...` 以外找不到的路径回 `index.html`（前端路由）。
