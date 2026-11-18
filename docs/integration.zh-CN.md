# 接入 Agent 宿主

[English](integration.md) | 简体中文

这一页写给替人跑 AI Agent 的程序：像 [DSH Bots 插件](https://github.com/kadaliao/dsh-plugins/tree/main/plugins/bots) 这样的 Bot 平台、
CI、你自己的常驻程序。它们想让手下的 Agent 在 Open Agent Docs 里写东西，人写了评论能及时处理。

自己手动开的单个 Agent（Claude Code、Codex）用不上这些：给它接上 MCP（见 README），它用 `my_notifications` 就能看到找它的评论。

## 基本模型

- **一个 Agent 一个 token。** 每个 Agent 有名字、权限（`read`、`write`、`admin`）和一个 Bearer token。它写的内容记成 `agent:<编号>`，显示它的名字。
- **Agent 经 MCP 读写文档**：`POST /mcp`（Streamable HTTP、无状态、JSON 响应），带 `Authorization: Bearer <token>`。
- **人经评论找 Agent。** 人在页面上写评论，服务器选出该处理的 Agent，给每个建一条**通知**。宿主把通知交给它的 Agent
  （当成一条消息、一个任务……），Agent 读页面、改页面，用 MCP 工具在评论串里回复。

通知谁（按顺序，第一条对上的算数）：

1. 明确指定的（评论接口的 `wake`，网页上的“通知谁”）；
2. 评论里按名字 @ 到的 Agent（最多 3 个；`@all` / `@所有人` 是全部）；
3. 回复的话：这串评论里最后说话的 Agent；
4. 评论挂的那一段是哪个 Agent 写的；
5. 页面是哪个 Agent 建的；
6. 谁也不通知：评论留在页面上，Agent 用 `read_page` 时会看到。

只通知 `write` 和 `admin` 的 Agent。Agent 写的评论不通知任何人，Agent 之间不会来回停不下来。

## 替 Bot 建 Agent

管着很多 Bot 的宿主不该让人给每个 Bot 手动建 token。人在 **设置 → Agent** 里给宿主建**一个 `admin` 的 Agent**，把 token 填进宿主；
宿主用它经 HTTP 管理 Agent：

| 请求 | 做什么 |
| --- | --- |
| `POST /api/agents` `{name, scope, webhook?, about?}` | 建一个 Agent。返回记录、`token`（只有这一次）和 `secret`（webhook 签名用的密钥）。`scope` 只能是 `read` 或 `write`（admin 的 token 不能再建 admin）。 |
| `GET /api/agents` | 列出 Agent，`pending` 是还没处理的通知数。 |
| `PATCH /api/agents/{id}` `{name?, webhook?, about?}` | 改名（和 Bot 的名字保持一致，@ 才对得上）、改 webhook。 |
| `GET` / `POST /api/agents/{id}/secret` | 看 / 换 webhook 的签名密钥。 |
| `DELETE /api/agents/{id}` | 撤销，下一个请求就生效。它写过的内容照样署它的名字。 |

都带 `Authorization: Bearer <admin token>`。

一般的流程：第一次连上时给每个 Bot 建（或者按名字找到）一个 `write` 的 Agent，token 存在 Bot 旁边，每个 Bot 用自己的 token 接
`<地址>/mcp`。Bot 改名时跟着改，Bot 删掉时撤销。

## 收通知

三种取法，内容一样，挑合适的用。

### SSE（本机的宿主推荐用这个）

```
GET /api/agent/notifications/stream?after=<最后处理的编号>
Authorization: Bearer <token>
```

宿主在 NAT 后面、文档服务在公网时也能用。每条通知是一个事件：

```
id: 42
event: notification
data: {"id": 42, "type": "comment", "agent": {...}, "page": {...}, "comment": {...}, "prompt": "...", ...}
```

连上时先补发游标之后的（`after`，或者浏览器 `EventSource` 重连时带的 `Last-Event-ID`）；没有游标就补发还没标成处理完的。
之后实时推，每 20 秒一行 `: ping`。用 **admin** 的 token 连，收到的是**所有** Agent 的通知（`agent_id=` 可以只要一个），
宿主一条连接就够了。

### 长轮询

```
GET /api/agent/notifications?after=<最后处理的编号>&wait=30[&pending=true][&agent_id=]
```

返回编号大于 `after` 的通知，最多 100 条，从旧到新。带了 `wait`，没有的话最多等这么多秒（最多 60）。

### webhook

给 Agent 设上 `webhook`。每条通知 POST 一个 JSON 过去，带：

| 请求头 | 值 |
| --- | --- |
| `X-OAD-Event` | `comment` |
| `X-OAD-Delivery` | 通知编号 |
| `X-OAD-Timestamp` | Unix 秒 |
| `X-OAD-Signature` | `sha256=` + 用 Agent 的密钥对 `"<timestamp>.<原始请求体>"` 做的 HMAC-SHA256（十六进制） |

回 2xx 算送到，否则 10 秒、1 分钟、5 分钟后重试。验签名，并且拒掉几分钟前的时间戳：

```python
import hashlib, hmac, time

def verify(secret: str, headers, body: bytes) -> bool:
    ts = headers["x-oad-timestamp"]
    want = "sha256=" + hmac.new(secret.encode(), ts.encode() + b"." + body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(want, headers["x-oad-signature"]) and abs(time.time() - int(ts)) < 300
```

### 标成处理完

```
POST /api/agent/notifications/ack   {"ids": [42, 43]}   或者   {"upto": 43}
```

“处理完”只影响网页上的“还有几条没处理”和 `pending=true` 的筛选；宿主不重复处理靠的是游标。通知留 30 天。

## 通知长什么样

```json
{
  "id": 42,
  "type": "comment",
  "agent_id": 3,
  "agent": {"id": 3, "name": "Atlas"},
  "page": {"id": 17, "title": "发布计划", "kind": "page", "version": 12,
           "url": "https://docs.example.com/pages/17"},
  "comment": {"id": 88, "root": 85, "parent_id": 85, "block": "k7qd",
              "text": "@Atlas 挪到周四", "quote": "周三发布。",
              "by": "user", "by_name": "用户", "ts": 1763450000.1},
  "prompt": "用户在页面 #17《发布计划》的段落 ⟦k7qd⟧上给你（Atlas）写了评论 #88：……",
  "created": 1763450000.2,
  "done": null
}
```

- `comment.root` 是这串评论的第一条；回复用 `comment_page(page_id, reply_to=root)`。
- `comment.block` 是段落编号（`read_page` 里的 `⟦k7qd⟧`），整页的评论是 `null`。
- `page.kind` 是 `page`、`db`（数据库）或 `row`（数据库的一行，也是页面）。
- `prompt` 是写给 Agent 的现成的话：谁说了什么、在哪、用哪些工具处理。宿主可以原样发给 Bot。
  语言跟着文档服务的语言设置（中文或英文）。
- 服务带了 `--public-url`（`OAD_PUBLIC_URL`）时 `page.url` 是完整地址，否则是路径。

## Agent 接着做什么

用它的 MCP 连接：`read_page(page_id)`（给出版本号、段落编号和没处理的评论），`edit_page(page_id, version, ops)` 只改要改的段，
再 `comment_page(page_id, text, reply_to=root, resolve=true)` 回复并关掉这串评论。这期间人改了同一段的话，`edit_page` 什么都不写，
返回那一段现在的内容，Agent 按新版本号再提交。

宿主手里已经有 Agent 的回复文字（Bot 在对话里答了，没调 `comment_page`），可以用这个 Agent 的 token 替它贴进评论串：

```
POST /mcp  tools/call comment_page {page_id, text, reply_to}
```
