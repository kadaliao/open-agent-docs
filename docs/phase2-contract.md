# 第 2 阶段约定（Agent、评论、通知）

在第 1 阶段（`docs/phase1-contract.md`）上加。服务器和网页并行做，按这里对接；改约定先改这份文件。

## 验收

1. MCP 端点能被 Claude Code 接上（`claude mcp add --transport http ...`），用自然语言让它读页、按段改、整页重写、建页、回评论，都成功；
   它改的时候网页上看到“某 Agent 正在写这一段”，人同时在打的字不丢。
2. 用户在网页上选中一段文字写评论、@ 某个 Agent，Agent 经 webhook / SSE / 长轮询收到通知（带一段可以直接发给 Agent 的 prompt），
   在评论里回复、改页面、把评论标成已解决，网页实时看到。
3. Agent 管理：网页上建 Agent、只显示一次 token、给出 Claude Code 的接入命令、重发 token、撤销、设 webhook；admin 权限的 Agent 能经接口替别的 Bot 建 Agent。
4. 集成测试全过；MCP 每个工具都有测试。

## Agent

- Agent 属于一个工作区，`scope`：`read` / `write` / `admin`（能写，还能经接口管理本工作区的 Agent、收本工作区所有 Agent 的通知）。
- token `oad_` + 32 字节随机（base62），只在建和重发时显示一次，库里存 SHA-256。请求带 `Authorization: Bearer <token>`。
- 字段：`id`、`name`（工作区内唯一，@ 用它）、`about`（一句话说明，给人看也给别的 Agent 看）、`scope`、`webhook`（可空）、
  `webhook_secret`（服务器生成，建的时候和 token 一起显示一次，可重发）、`created_by`、`created_at`、`last_used_at`、`revoked_at`。
  撤销后名字留着（它写过的、评论过的还要显示是谁），编号不复用。
- HTTP：
  - `GET  /workspaces/{ws}/agents`、`POST /workspaces/{ws}/agents` `{ name, about?, scope, webhook? }` → `{ agent, token, webhook_secret }`
  - `PATCH /agents/{id}` `{ name?, about?, scope?, webhook? }`、`DELETE /agents/{id}`（撤销）
  - `POST /agents/{id}/token` → `{ token }`（旧的立即失效）；`POST /agents/{id}/webhook-secret` → `{ webhook_secret }`
  - 工作区 owner / editor 能管；admin 权限的 Agent 能管本工作区里除自己以外的。
- `Principal` 加 `kind: "agent"`。Agent 能用第 1 阶段所有 HTTP 接口（按 scope：read 只读）。

## 作者（谁写的）

服务器记下每个 Yjs client id 属于谁：`doc_clients(page_id, client_id, principal_kind, principal_id)`。浏览器连接发来的 update 里新出现的
client id 记到这个连接的用户名下；服务器替 Agent 写时用的 client id 记到那个 Agent 名下。

- `GET /pages/{id}/authors` → `{ clients: { "<client_id>": { kind, id, name } } }`（网页据此显示“谁写的这一段”，以后的历史、按人撤销也用它）。
- 一块的“作者”= 这块里现存文字最多的那个人（服务器算，评论通知要用）。

## 评论

- 评论挂在一段文字上：`anchor = { block_id, start, end, quote }`。`start` / `end` 是 Yjs RelativePosition（`Y.encodeRelativePosition` 后 base64），
  文字改了也跟着走；`quote` 是当时选中的原文（锚点失效时显示）。不挂文字的是对整段（只有 block_id）或整页（anchor 为空）的评论。
- 一串评论：第一条是 thread，回复挂在下面。整串一起解决 / 重新打开。
- 评论正文是 Markdown（行内格式、链接、@名字）。@ 用户写 `@名字`，网页里输入 `@` 弹出可选的人和 Agent，存的时候写成 `@[名字](agent:<id>)` / `@[名字](user:<id>)`。
- 表：`comment_threads(id, page_id, block_id, anchor_start, anchor_end, quote, created_by, created_at, resolved_at, resolved_by)`、
  `comments(id, thread_id, author_kind, author_id, body, created_at, edited_at, deleted_at)`。
- HTTP：
  - `GET  /pages/{id}/comments?resolved=false|true|all` → `{ threads: [{ ..., comments: [...] }] }`
  - `POST /pages/{id}/comments` `{ anchor?, body, notify? }` → thread（新的一串）
  - `POST /threads/{id}/comments` `{ body, notify? }` → comment（回复）
  - `PATCH /comments/{id}` `{ body }`（只能改自己的）、`DELETE /comments/{id}`
  - `PATCH /threads/{id}` `{ resolved: bool }`
  - `notify`：不给 = 按下面的规则；`false` = 谁也不叫；`["agent:<id>", ...]` = 只叫这些。
- 实时：工作区 SSE 加事件 `comment.created` / `comment.updated` / `comment.deleted` / `thread.updated`，数据带 `page_id`。

## 通知

**叫谁**（人写的评论才叫 Agent；Agent 写的评论不叫任何 Agent，免得 Agent 之间停不下来）：
明确指定的（`notify`）> 评论里 @ 到的 Agent（最多 3 个）> 回复时，这串里最后说话的 Agent > 这段的作者是 Agent 就叫它 > 建页面的是 Agent 就叫它。
只叫 write / admin 的、没撤销的。都没有就谁也不叫，评论照样留着。

人也收通知（网页右上角的收件箱）：被 @、自己参与的那串有新回复、自己的评论被解决。

**Agent 的通知**存表 `agent_notifications(id, workspace_id, agent_id, kind, data, created_at, delivered_at, acked_at, attempts, error)`，`id` 是游标。
内容：

```json
{ "id": 42, "agent": { "id": "...", "name": "..." }, "type": "comment",
  "page": { "id": "...", "title": "...", "url": "https://.../p/<id>" },
  "thread_id": "...", "comment": { "id": "...", "author": { "kind": "user", "name": "..." }, "body": "..." },
  "block_id": "...", "quote": "...",
  "prompt": "一段写给 Agent 的话：哪个页面、哪一段、原文、评论内容、该用哪个工具回复。宿主可以直接当消息发给 Bot。",
  "created_at": 1763450000000 }
```

三种取法，宿主挑一种：

- **webhook**：`POST` JSON，头 `X-OAD-Timestamp`（毫秒）、`X-OAD-Signature: sha256=<hex>`（用 webhook_secret 对 `"<timestamp>.<body>"` 做 HMAC-SHA256）、
  `X-OAD-Delivery: <id>`。2xx 算送到；失败按 10 秒、1 分钟、5 分钟、30 分钟重试，再失败记 error（网页能看到）。只投公网地址（和链接解析同样的 SSRF 检查），
  `OAD_WEBHOOK_ALLOW_PRIVATE=1` 时允许内网（本机开发）。
- **SSE**：`GET /agent/notifications/stream`（带 Agent token），先补发游标之后的（`?after=` 或 `Last-Event-ID`），再实时推；每 25 秒一个注释行保活。
- **长轮询**：`GET /agent/notifications?after=<id>&wait=<秒，最多 60>&limit=<最多 100>`。
- `POST /agent/notifications/ack` `{ ids }` 或 `{ upto }`：处理完了。webhook 送到也算 ack。
- admin 的 token 收本工作区所有 Agent 的（每条带 `agent`）。
- 保留 30 天。

**人的通知**：`GET /me/notifications?after=&limit=`、`POST /me/notifications/read` `{ ids }` 或 `{ all: true }`；工作区 SSE 推 `notification`（只推给本人）。

## MCP

端点 `/mcp`，Streamable HTTP（rmcp），`Authorization: Bearer <agent token>`。工具描述用中文写清楚何时用、怎么用；返回给 Agent 读的文字（不是 JSON 堆）。
页面里每个顶层块前面一行 `⟦块编号⟧`，Agent 按顶层块改（改一个列表就整块给新的 Markdown，服务器会沿用没变的项的编号）。
读页结果带版本号 `base`（第 1 阶段的 version），写的时候带回来，服务器在那个版本上改再合并（人后来打的字留得住）。

| 工具 | 作用 |
| --- | --- |
| `search_pages(query, limit?)` | 按标题和正文找页面，返回编号、标题、命中的片段（第 2 阶段先用简单的子串匹配，第 5 阶段换 tantivy） |
| `list_pages(parent_id?)` | 页面目录（缩进表示层级），带编号、标题、更新时间 |
| `read_page(page_id, from_block?, max_blocks?)` | 标题 + 正文（每块前 `⟦id⟧`）+ `base` + 没解决的评论概要；页面太长时分段读 |
| `create_page(title, markdown?, parent_id?, after?)` | 建页，返回编号和网页链接 |
| `edit_page(page_id, base, ops)` | 按块改，`ops` 依次执行、一次事务：`replace{block, markdown}`、`insert_after{block, markdown}`、`insert_before{block, markdown}`、`append{markdown}`、`delete{block}` |
| `rewrite_page(page_id, base, markdown, title?)` | 整页换，没变的块沿用编号（评论还挂着） |
| `update_page(page_id, title?, icon?, parent_id?, after?)` | 改标题、图标、移动 |
| `delete_page(page_id)` | 进回收站 |
| `list_comments(page_id, include_resolved?)` | 评论串：挂在哪块（`⟦id⟧` + 原文）、谁说的、回复 |
| `comment(page_id?, thread_id?, body?, block?, resolve?)` | 新评论（给 page_id，可选 block）/ 回复（给 thread_id）/ 标成已解决（`resolve: true`） |
| `my_notifications(after?, wait?)` | 取自己的通知（同长轮询），并 ack 已经读过的 |

Agent 写正文时，服务器在这一页的 awareness 里替它放 `{ user: { id, name, color, kind: "agent" }, block }`，写完 3 秒后清掉（第 1 阶段已有）。

## 网页

- **评论**：选中文字 → 气泡菜单里“评论”；正文里有评论的文字有底色，点开在右侧显示那串；页面右上角“评论”按钮开评论栏（未解决 / 已解决）；
  输入 `@` 弹人和 Agent；回复、编辑、删除、解决、重新打开；实时更新。锚点用 RelativePosition 算出当前位置画底色（ProseMirror decoration，不改文档）。
- **Agent 设置**（工作区设置里）：列表（名字、说明、权限、最后使用、未处理通知数、webhook 状态）；新建 → 弹窗只显示一次 token 和 webhook_secret，
  附上 Claude Code 接入命令 `claude mcp add --transport http oad <服务器地址>/mcp --header "Authorization: Bearer <token>"` 和通用的 MCP JSON；重发 token、撤销、改 webhook。
- **收件箱**：右上角铃铛，未读数；点开列出通知，点一条跳到那页那串评论。
- **作者**：鼠标停在一段左边时显示写这段的人 / Agent（用 `/pages/{id}/authors` + 块里文字的 client id）。可以先做简单版：只显示块的主要作者。
