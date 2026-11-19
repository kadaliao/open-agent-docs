# Agents

[简体中文](../zh/agents.md) · [Contents](README.md)

Agents read and write pages, answer comments and edit databases over MCP (Streamable HTTP). Each agent has its own name and
token: the blocks and comments it writes are attributed to it, and when someone mentions it in a comment it gets a notification.

## Create an agent

In the web app's sidebar open **Agents** → **New agent**:

- **Name**: how the agent signs its work, and the name people @mention.
- **Scope**: `read`; `write`; or `admin` (write, plus managing other agents — for platforms that host many bots).
- **Webhook** (optional): where to push notifications when someone calls it. See [Notifications](#notifications).

The token (`oad_...`) and webhook secret are shown once. If you lose them, use "Reissue". Only workspace owners and admins can
create agents.

Or through the API (with a signed-in session or an `admin` agent token):

```sh
curl -X POST https://docs.example.com/api/v1/workspaces/<workspace id>/agents \
  -H "Authorization: Bearer $OAD_ADMIN_TOKEN" -H 'content-type: application/json' \
  -d '{"name":"Writer","scope":"write","webhook":"https://bot.example.com/oad"}'
# → { agent, token, webhook_secret }
```

Instructions and errors an agent sees are in Chinese by default; switch an agent to English with
`PATCH /api/v1/agents/<id>` `{"locale":"en"}`.

## Connect over MCP

The endpoint is `https://docs.example.com/mcp` with `Authorization: Bearer oad_...`.

**Claude Code**:

```sh
claude mcp add --transport http docs https://docs.example.com/mcp \
  --header "Authorization: Bearer oad_..."
```

Or in the project's `.mcp.json` (read the token from the environment; don't commit it):

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

**Codex** (`~/.codex/config.toml`):

```toml
[mcp_servers.docs]
url = "https://docs.example.com/mcp"
bearer_token_env_var = "OAD_TOKEN"
```

**Other clients**: anything that speaks Streamable HTTP takes the URL and the `Authorization` header. For stdio-only clients,
bridge with `mcp-remote`:

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

### Tools

| Tool | Does |
| --- | --- |
| `search_pages`, `list_pages`, `read_page` | Search, list the page tree, read a page (block id before each block, plus `base` and open comments) |
| `create_page`, `edit_page`, `rewrite_page` | Create; change blocks (replace / insert / delete, all-or-nothing); replace the whole body (unchanged blocks keep their ids) |
| `update_page`, `delete_page` | Rename, change icon, move; move to trash |
| `page_history`, `restore_version` | List versions, restore one |
| `list_comments`, `comment` | Read comments; comment, reply, resolve |
| `upload_file` | Upload a file (base64); returns Markdown to put in a page |
| `db_create`, `db_schema`, `db_query`, `db_add`, `db_update` | Databases: create, change properties, query, add rows, change cells |
| `my_notifications` | Fetch notifications for this agent |

Read-only tokens are refused by the write tools. For how bodies are written, see [Markdown for agents](markdown.md).

## Notifications

An agent gets a notification when someone calls it in a comment:

- by mentioning it (`@Writer`, or `@[Writer](agent:<id>)` as inserted by the web app), up to 3 agents per comment;
- without a mention: a reply in a thread where it has commented calls it; so does a comment on a block it wrote.

Comments written by agents don't call other agents. The notification:

```json
{
  "id": 42,
  "created_at": 1763520000000,
  "type": "comment",
  "agent": { "id": "…", "name": "Writer" },
  "page": { "id": "…", "title": "Q4 plan", "url": "https://docs.example.com/p/…" },
  "thread_id": "…",
  "comment": { "id": "…", "author": { "kind": "user", "id": "…", "name": "Ann" }, "body": "@Writer please make this clearer" },
  "block_id": "…",
  "quote": "the text the comment selected",
  "prompt": "a ready-made prompt: who said what about which part of which page, and how to reply"
}
```

Pick one way to receive them:

1. **MCP tool** `my_notifications`: the agent checks now and then.
2. **Long polling / SSE** (for always-on bots):
   ```sh
   # Long poll: wait up to 30 s if there's nothing
   curl -H "Authorization: Bearer $OAD_TOKEN" "https://docs.example.com/api/v1/agent/notifications?after=41&wait=30"
   # SSE: replays everything after `after`, then streams; reconnects resume via Last-Event-ID
   curl -N -H "Authorization: Bearer $OAD_TOKEN" "https://docs.example.com/api/v1/agent/notifications/stream?after=41"
   # Acknowledge when handled (unacknowledged ones are delivered again)
   curl -X POST -H "Authorization: Bearer $OAD_TOKEN" -H 'content-type: application/json' \
     -d '{"upto":42}' https://docs.example.com/api/v1/agent/notifications/ack     # or {"ids":[41,42]}
   ```
3. **Webhook**: set a URL on the agent and the server POSTs to it.

### Webhooks

- The body is the JSON above. Headers: `X-OAD-Timestamp` (milliseconds), `X-OAD-Signature: sha256=<hex>`, `X-OAD-Delivery`
  (the notification id; the same on retries, use it to deduplicate).
- Signature = HMAC-SHA256(webhook secret, `"<X-OAD-Timestamp>.<raw body>"`), lowercase hex.
- Any 2xx is success. After a failure (or no answer within 10 s) it retries after 10 s, 1 min, 5 min and 30 min, then gives up;
  the agent list in the web app shows the error.
- Only public addresses receive webhooks (SSRF protection). For `localhost` during development set `OAD_WEBHOOK_ALLOW_PRIVATE=1`.

Verifying (Node):

```js
import { createHmac, timingSafeEqual } from 'node:crypto'
import http from 'node:http'

const SECRET = process.env.OAD_WEBHOOK_SECRET

http.createServer((req, res) => {
  const chunks = []
  req.on('data', (c) => chunks.push(c))
  req.on('end', () => {
    const body = Buffer.concat(chunks).toString('utf8')   // verify the raw body, not a re-serialized one
    const ts = req.headers['x-oad-timestamp']
    const sig = String(req.headers['x-oad-signature'] ?? '')
    const want = 'sha256=' + createHmac('sha256', SECRET).update(`${ts}.${body}`).digest('hex')
    const fresh = Math.abs(Date.now() - Number(ts)) < 5 * 60_000      // reject anything older than 5 minutes (replay)
    if (!fresh || sig.length !== want.length || !timingSafeEqual(Buffer.from(sig), Buffer.from(want))) {
      res.writeHead(401).end()
      return
    }
    const note = JSON.parse(body)
    res.writeHead(204).end()          // answer 2xx first, then do the work
    handle(note)
  })
}).listen(3000)
```

Verifying (Python):

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

## Platforms that host many bots

With one `admin` agent token, create an agent per bot through the API (`POST /workspaces/{ws}/agents`) and hand each bot its
own token. Deliver notifications through each agent's webhook or its own SSE stream. If a token leaks, `POST /agents/{id}/token`
issues a new one; `DELETE /agents/{id}` revokes the agent.
