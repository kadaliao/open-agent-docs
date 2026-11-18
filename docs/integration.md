# Integrating an agent host

English | [简体中文](integration.zh-CN.md)

This page is for programs that run AI agents on someone's behalf — a bot platform like the
[DSH Bots plugin](https://github.com/kadaliao/dsh-plugins/tree/main/plugins/bots), a CI runner, your own
daemon — and want those agents to write in Open Agent Docs and react when a person comments.

A single agent you start by hand (Claude Code, Codex) doesn't need any of this: give it an MCP
connection (see the README) and it can call `my_notifications` to see comments addressed to it.

## The model

- **One agent = one token.** Every agent has a name, a scope (`read`, `write` or `admin`) and a
  Bearer token. Whatever the agent writes is signed `agent:<id>` and shown with its name.
- **Agents talk to docs over MCP** at `POST /mcp` (Streamable HTTP, stateless, JSON responses) with
  `Authorization: Bearer <token>`.
- **People talk to agents through comments.** When a person comments on a page, the server picks the
  agents that should handle it and creates a **notification** for each one. The host delivers it to
  its agent (as a chat message, a task, …); the agent reads the page, edits it and answers in the
  comment thread with MCP tools.

Who gets notified, in order (the first rule that matches wins):

1. an explicit choice (`wake` on the comment API — the web UI's "notify" picker);
2. agents @mentioned in the comment by name (up to 3; `@all` / `@所有人` means every agent);
3. on a reply: the agent that last spoke in that thread;
4. the agent that wrote the block the comment is attached to;
5. the agent that created the page;
6. nobody — the comment waits on the page, and agents see it in `read_page`.

Only `write` and `admin` agents are notified. Comments written by agents never notify anyone, so
agents can't loop each other.

## Provisioning agents

A host that manages many bots should not ask the person to create a token per bot. Instead the
person creates **one `admin` agent** for the host in *Settings → Agents* and pastes its token into the
host. With it the host can manage agents over HTTP:

| Request | What it does |
| --- | --- |
| `POST /api/agents` `{name, scope, webhook?, about?}` | Create an agent. Returns the record plus `token` (shown only here) and `secret` (webhook signing key). `scope` can be `read` or `write` (an admin token can't create another admin). |
| `GET /api/agents` | List agents with `pending` (undelivered notifications). |
| `PATCH /api/agents/{id}` `{name?, webhook?, about?}` | Rename (keep it equal to the bot's name so @mentions work), change the webhook. |
| `GET` / `POST /api/agents/{id}/secret` | Read / rotate the webhook signing secret. |
| `POST /api/agents/{id}/token` | Issue a new token (the old one stops working at once). Use it when the host lost the token it stored: it reconnects to the same agent, whose earlier writing stays its own. |
| `DELETE /api/agents/{id}` | Revoke. Takes effect on the next request. The name stays on what it wrote. |

All with `Authorization: Bearer <admin token>`.

Typical host flow: on first connect, create (or find by name) one `write` agent per bot, store each
token next to the bot, and give each bot an MCP connection to `<base>/mcp` with its own token. Rename
the agent when the bot is renamed; revoke it when the bot is deleted.

## Receiving notifications

Pick whichever transport fits; all three carry the same notification object.

### Server-Sent Events (recommended for local hosts)

```
GET /api/agent/notifications/stream?after=<last id>
Authorization: Bearer <token>
```

Works when the host is behind NAT and the docs server is public. Each notification is one event:

```
id: 42
event: notification
data: {"id": 42, "type": "comment", "agent": {...}, "page": {...}, "comment": {...}, "prompt": "...", ...}
```

On connect it first replays notifications after the cursor (`after`, or the `Last-Event-ID` header the
browser `EventSource` sends on reconnect); without a cursor it replays the ones not yet marked done.
Then it pushes new ones live, with a `: ping` comment every 20 seconds. With the **admin** token the
stream carries notifications for **all** agents (filter with `agent_id=`), so a host needs only one
connection for all its bots.

### Long polling

```
GET /api/agent/notifications?after=<last id>&wait=30[&pending=true][&agent_id=]
```

Returns up to 100 notifications with `id > after`, oldest first. With `wait`, an empty result waits
up to that many seconds (max 60) for a new one.

### Webhook

Set `webhook` on the agent. Each notification is POSTed as JSON with:

| Header | Value |
| --- | --- |
| `X-OAD-Event` | `comment` |
| `X-OAD-Delivery` | notification id |
| `X-OAD-Timestamp` | Unix seconds |
| `X-OAD-Signature` | `sha256=` + hex HMAC-SHA256 of `"<timestamp>.<raw body>"` keyed with the agent's secret |

A 2xx response marks it done. Otherwise it is retried after 10 s, 1 min and 5 min. Verify the
signature and reject timestamps older than a few minutes:

```python
import hashlib, hmac, time

def verify(secret: str, headers, body: bytes) -> bool:
    ts = headers["x-oad-timestamp"]
    want = "sha256=" + hmac.new(secret.encode(), ts.encode() + b"." + body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(want, headers["x-oad-signature"]) and abs(time.time() - int(ts)) < 300
```

### Marking done

```
POST /api/agent/notifications/ack   {"ids": [42, 43]}   or   {"upto": 43}
```

"Done" only feeds the *pending* counts in the web UI and the `pending=true` filter; the cursor is
what keeps a host from processing a notification twice. Notifications are kept for 30 days.

## The notification object

```json
{
  "id": 42,
  "type": "comment",
  "agent_id": 3,
  "agent": {"id": 3, "name": "Atlas"},
  "page": {"id": 17, "title": "Launch plan", "kind": "page", "version": 12,
           "url": "https://docs.example.com/pages/17"},
  "comment": {"id": 88, "root": 85, "parent_id": 85, "block": "k7qd",
              "text": "@Atlas move this to Thursday", "quote": "Ship on Wednesday.",
              "by": "user", "by_name": "User", "ts": 1763450000.1},
  "prompt": "The user wrote comment #88 for you (Atlas) on block ⟦k7qd⟧ of page #17 «Launch plan»: …",
  "created": 1763450000.2,
  "done": null
}
```

- `comment.root` is the first comment of the thread; reply with `comment_page(page_id, reply_to=root)`.
- `comment.block` is the block id (`⟦k7qd⟧` in `read_page`), or `null` for a page-level comment.
- `page.kind` is `page`, `db` (database) or `row` (a database row, which is also a page).
- `prompt` is ready-made text for the agent: what was said, where, and which tools to call. A host
  can post it to the bot's conversation as-is.
- `page.url` is absolute when the server runs with `--public-url` (`OAD_PUBLIC_URL`), otherwise a path.

## What the agent does next

Over its MCP connection: `read_page(page_id)` (shows the version, block ids and open comments),
`edit_page(page_id, version, ops)` to change only the blocks it needs, then
`comment_page(page_id, text, reply_to=root, resolve=true)` to answer and close the thread. If a
person edited the same block in the meantime, `edit_page` writes nothing and returns the current text
of that block; the agent re-submits against the new version.

If the host already has the agent's reply as text (the bot answered in chat instead of calling
`comment_page`), the host can post it for the agent with the agent's token:

```
POST /mcp  tools/call comment_page {page_id, text, reply_to}
```
