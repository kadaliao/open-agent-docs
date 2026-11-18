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

位置按 UTF-16 计（Yrs 用 `OffsetKind::Utf16`）。服务器自己写入时用固定的 client id 段（见 oad-doc），不和浏览器撞。

## 同步：WebSocket

`GET /api/v1/sync/{page_id}`（升级成 WebSocket），**标准 y-websocket 协议**（网页直接用 `y-websocket` 的 `WebsocketProvider`，`serverUrl = ws(s)://host/api/v1/sync`，`roomname = page_id`）：

- 消息类型 0 = sync（step1 / step2 / update），1 = awareness，2 = auth（拒绝时发 `permission-denied` 后关连接），3 = query awareness。
- 连上后服务器先发 step1 和当前 awareness。
- 认证：同源 cookie；跨源嵌入时用 `?token=...`。
- 只读权限的连接，服务器不接受它发来的 update（丢掉，不断开）。

awareness 里网页放 `{ user: { id, name, color, kind: "user" | "agent" } }`；Agent 在写的时候服务器替它放一份，网页据此显示“Agent 正在写第几段”。

## HTTP 接口

前缀 `/api/v1`，JSON，错误统一 `{ "error": { "code": "not_found", "message": "..." } }`。

登录：

- `GET  /auth/config` → `{ mode: "local" | "header" | "none", ... }`（网页据此决定要不要显示登录页）
- `POST /auth/login` `{ email, password }` → 设 `oad_session` cookie（HttpOnly、SameSite=Lax），返回 `me`
- `POST /auth/logout`
- `GET  /me` → `{ user: { id, name, email }, workspaces: [{ id, name, role }] }`

页面：

- `GET    /workspaces/{ws}/pages` → `{ pages: [{ id, parent_id, position, title, icon, updated_at }] }`（不含回收站里的，网页自己拼树）
- `POST   /workspaces/{ws}/pages` `{ parent_id?, after?, title?, icon? }` → page。`after` 是同级里排在它前面的页面 id，不给就放最后。
- `GET    /pages/{id}` → page
- `PATCH  /pages/{id}` `{ title?, icon?, parent_id?, after? }`（移动：给 `parent_id` 和 `after`）
- `DELETE /pages/{id}` → 连子页一起进回收站
- `GET    /pages/{id}/content?format=markdown|json` → `{ markdown }` 或 `{ doc }`
- `PUT    /pages/{id}/content` `{ markdown }` 或 `{ doc }` → 整页替换（对得上的块沿用编号，只改变了的地方）
- `GET    /pages/{id}/blocks/{block_id}` → `{ block, markdown }`
- `PUT    /pages/{id}/blocks/{block_id}` `{ markdown }` → 替换这一块
- `POST   /pages/{id}/blocks` `{ markdown, after? }` → `{ ids }`（插入，不给 after 加到最后）
- `DELETE /pages/{id}/blocks/{block_id}`

写正文的接口都通过页面的同步中心写进 CRDT，连着的网页马上收到。

实时事件（侧栏、标题）：`GET /workspaces/{ws}/events`（SSE），事件 `page.created` / `page.updated` / `page.moved` / `page.deleted`，数据是 page。

`position` 是分数索引字符串，按字典序排；客户端不自己算，移动时给 `after`。

## 登录方式（`AuthProvider`）

服务器启动参数 / 环境变量 `OAD_AUTH`：

- `local`（默认）：内置账号，密码 argon2。没有用户时，首次启动用 `OAD_ADMIN_EMAIL` / `OAD_ADMIN_PASSWORD` 建管理员和默认工作区；也可用 `open-agent-docs user add`。
- `header`：宿主在前面做登录，转发时带 `X-OAD-User-Id` / `X-OAD-User-Name` / `X-OAD-User-Email`，并带 `X-OAD-Secret`（和 `OAD_HEADER_SECRET` 相同才认）。用户第一次出现时自动建。
- `none`：本机单人，所有请求都是同一个本地用户。

## 打包

`open-agent-docs serve`：一个二进制，网页（`apps/web/dist`）用 rust-embed 打进去；开发时 `OAD_WEB_DIR` 指向目录直接读，或用 Vite 开发服务器代理 `/api` 到服务器。
默认端口 `8848`，数据目录 `OAD_DATA_DIR`（默认 `./data`，SQLite 文件 `oad.db`）。
