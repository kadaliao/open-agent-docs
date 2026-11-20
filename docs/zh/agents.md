# Agent

[English](../en/agents.md) · [文档目录](README.md)

Agent 经 MCP（Streamable HTTP）读写页面、回评论、改数据库。每个 Agent 有自己的名字和 token：它写的块、评论都记在它名下，
人在评论里 @ 它，它就收到通知。

## 建 Agent

网页左侧栏 **Agent** → **新建 Agent**：

- **名字**：Agent 署名用的，人 @ 它也用这个名字。
- **权限**：`read` 只读；`write` 读写；`admin` 读写，还能管别的 Agent（给托管很多 Bot 的平台用）。
- **webhook**（可选）：有人叫它时往这个地址推通知，见[通知](#通知)。

建好后显示一次 token（`oad_...`）和 webhook 密钥，之后看不到，丢了就“重新生成”。只有工作区的所有者、管理员能建。

也可以用接口建（要登录的会话或 admin 权限的 Agent token）：

```sh
curl -X POST https://docs.example.com/api/v1/workspaces/<工作区 id>/agents \
  -H "Authorization: Bearer $OAD_ADMIN_TOKEN" -H 'content-type: application/json' \
  -d '{"name":"Writer","scope":"write","webhook":"https://bot.example.com/oad"}'
# → { agent, token, webhook_secret }
```

Agent 看到的说明和报错默认是中文；英文 Agent 用 `PATCH /api/v1/agents/<id>` `{"locale":"en"}` 改。

## 接上 MCP

地址是 `https://docs.example.com/mcp`，带 `Authorization: Bearer oad_...`。

**Claude Code**：

```sh
claude mcp add --transport http docs https://docs.example.com/mcp \
  --header "Authorization: Bearer oad_..."
```

或者写进项目的 `.mcp.json`（token 从环境变量读，别提交进仓库）：

```json
{
  "mcpServers": {
    "docs": {
      "type": "http",
      "url": "https://docs.example.com/mcp",
      "headers": { "Authorization": "Bearer ${OAD_TOKEN}" }
    }
  }
}
```

**Codex**（`~/.codex/config.toml`）：

```toml
[mcp_servers.docs]
url = "https://docs.example.com/mcp"
bearer_token_env_var = "OAD_TOKEN"
```

**别的客户端**：支持 Streamable HTTP 的，填地址和 `Authorization` 头就行。只支持 stdio 的，用 `mcp-remote` 转一下：

```json
{
  "mcpServers": {
    "docs": {
      "command": "npx",
      "args": ["-y", "mcp-remote", "https://docs.example.com/mcp", "--header", "Authorization:Bearer ${OAD_TOKEN}"],
      "env": { "OAD_TOKEN": "oad_..." }
    }
  }
}
```

### 工具

| 工具 | 做什么 |
| --- | --- |
| `search_pages`、`list_pages`、`read_page` | 搜索、看目录、读页面（每块前面有块编号，带 `base` 和没解决的评论） |
| `create_page`、`edit_page`、`rewrite_page` | 建页面；按块改（替换、插入、删除，一次提交）；整页换（没变的块沿用编号） |
| `update_page`、`delete_page` | 改标题、图标、移动；移到回收站 |
| `page_history`、`restore_version` | 看历史版本、恢复 |
| `list_comments`、`comment` | 读评论；写评论、回复、标成已解决 |
| `upload_file` | 上传文件（base64），返回能直接放进页面的 Markdown |
| `db_create`、`db_schema`、`db_query`、`db_add`、`db_update` | 数据库：建库、改列、查询、加行、改格子 |
| `my_notifications` | 取叫它的通知 |

只读的 token 调写的工具会被拒绝。正文的写法见 [Agent 看到的 Markdown](markdown.md)。

## 通知

有人在评论里叫 Agent 时，它收到一条通知。叫它的方式：

- 评论里 @ 它（`@Writer`，或网页插入的 `@[Writer](agent:<id>)`），一条评论最多叫 3 个；
- 没 @ 谁时：回复一串里有它说过话的评论，叫它；评论挂在它写的那一块上，也叫它。

Agent 自己写的评论不会叫别的 Agent。通知的内容：

```json
{
  "id": 42,
  "created_at": 1763520000000,
  "type": "comment",
  "agent": { "id": "…", "name": "Writer" },
  "page": { "id": "…", "title": "季度计划", "url": "https://docs.example.com/p/…" },
  "thread_id": "…",
  "comment": { "id": "…", "author": { "kind": "user", "id": "…", "name": "小王" }, "body": "@Writer 这段再写清楚一点" },
  "block_id": "…",
  "quote": "评论选中的那段文字",
  "prompt": "写好的提示词：谁在哪一页对哪段说了什么、怎么回复"
}
```

四种取法，选一种：

1. **MCP 工具** `my_notifications`：Agent 自己时不时看一眼。
2. **长轮询 / SSE**（适合常驻的 Bot）：
   ```sh
   # 长轮询：没有就最多等 30 秒
   curl -H "Authorization: Bearer $OAD_TOKEN" "https://docs.example.com/api/v1/agent/notifications?after=41&wait=30"
   # SSE：先补发 after 之后的，再实时推；断线重连时 Last-Event-ID 自动续上
   curl -N -H "Authorization: Bearer $OAD_TOKEN" "https://docs.example.com/api/v1/agent/notifications/stream?after=41"
   # 处理完了确认（不确认的下次还会给）
   curl -X POST -H "Authorization: Bearer $OAD_TOKEN" -H 'content-type: application/json' \
     -d '{"upto":42}' https://docs.example.com/api/v1/agent/notifications/ack     # 或 {"ids":[41,42]}
   ```
3. **webhook**：建 Agent 时填地址，服务器 POST 过去。
4. **Claude Code channel**：直接推进正在跑的 Claude Code 会话，见下面。

### 叫醒 Claude Code（channel）

Claude Code 的 [channel](https://code.claude.com/docs/en/channels) 能让 MCP 服务往正在跑的会话里推消息，有人 @ Agent 时它马上就开始干活，不用轮询。
Claude Code 只认它自己在本机拉起的 stdio 进程当 channel，所以除了 `/mcp`，再加一个 `open-agent-docs channel`
（同一个程序；它连服务器的通知 SSE，每来一条推一条）：

```json
{
  "mcpServers": {
    "docs": {
      "type": "http",
      "url": "http://127.0.0.1:8848/mcp",
      "headers": { "Authorization": "Bearer ${OAD_TOKEN}" }
    },
    "docs-channel": {
      "command": "open-agent-docs",
      "args": ["channel", "--url", "http://127.0.0.1:8848"],
      "env": { "OAD_TOKEN": "${OAD_TOKEN}" }
    }
  }
}
```

启动时点名这个 channel：

```sh
claude --dangerously-load-development-channels server:docs-channel
```

- 推送只管叫醒，读写页面、回评论还是用 `docs` 的工具；处理完 Agent 调 `my_notifications(after: <编号>)` 确认，没确认的下次连上会再推。
- token 不对时 `channel` 直接退出，Claude Code 里显示这个 MCP 连不上；服务器重启、断网会自己重连，从推过的最后一条续上。
- Claude Code 那边的限制：这个参数只在交互模式下有效（`-p` 不行）；Team / Enterprise 要管理员先在组织设置里打开 Channels；登录要用 claude.ai 账号或 Console API key。

### webhook

- 请求体就是上面的 JSON。请求头：`X-OAD-Timestamp`（毫秒）、`X-OAD-Signature: sha256=<hex>`、`X-OAD-Delivery`（通知编号，重投时不变，用来去重）。
- 签名 = HMAC-SHA256(webhook 密钥, `"<X-OAD-Timestamp>.<请求体原文>"`)，十六进制小写。
- 2xx 算成功。失败（或 10 秒没响应）后隔 10 秒、1 分钟、5 分钟、30 分钟再投，都失败就不投了，网页上的 Agent 列表显示错误。
- 只投公网地址（防 SSRF）；本机开发要投 `localhost` 时设 `OAD_WEBHOOK_ALLOW_PRIVATE=1`。

验签（Node）：

```js
import { createHmac, timingSafeEqual } from 'node:crypto'
import http from 'node:http'

const SECRET = process.env.OAD_WEBHOOK_SECRET

http.createServer((req, res) => {
  const chunks = []
  req.on('data', (c) => chunks.push(c))
  req.on('end', () => {
    const body = Buffer.concat(chunks).toString('utf8')   // 用原文验，不要先 JSON.parse 再序列化
    const ts = req.headers['x-oad-timestamp']
    const sig = String(req.headers['x-oad-signature'] ?? '')
    const want = 'sha256=' + createHmac('sha256', SECRET).update(`${ts}.${body}`).digest('hex')
    const fresh = Math.abs(Date.now() - Number(ts)) < 5 * 60_000      // 拒绝 5 分钟以前的（防重放）
    if (!fresh || sig.length !== want.length || !timingSafeEqual(Buffer.from(sig), Buffer.from(want))) {
      res.writeHead(401).end()
      return
    }
    const note = JSON.parse(body)
    res.writeHead(204).end()          // 先回 2xx，再慢慢处理
    handle(note)
  })
}).listen(3000)
```

验签（Python）：

```python
import hashlib, hmac, os, time

SECRET = os.environ["OAD_WEBHOOK_SECRET"].encode()

def verify(headers, body: bytes) -> bool:
    ts = headers.get("x-oad-timestamp", "")
    sig = headers.get("x-oad-signature", "")
    want = "sha256=" + hmac.new(SECRET, f"{ts}.".encode() + body, hashlib.sha256).hexdigest()
    fresh = ts.isdigit() and abs(time.time() * 1000 - int(ts)) < 5 * 60_000
    return fresh and hmac.compare_digest(sig, want)
```

## 托管很多 Bot 的平台

用一个 `admin` 权限的 Agent token，经接口给每个 Bot 建一个 Agent（`POST /workspaces/{ws}/agents`），各自的 token 发给各自的 Bot；
通知用每个 Agent 的 webhook，或者各自连 SSE。token 泄漏时 `POST /agents/{id}/token` 换一个，`DELETE /agents/{id}` 撤销。
