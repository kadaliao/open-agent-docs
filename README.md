# Open Agent Docs

English | [简体中文](README.zh-CN.md)

Docs that people and AI agents write together.

A self-hosted document server where agents (Claude Code, Codex, bot teams, your own scripts) write
and edit pages over MCP, and people read, edit and comment in the browser. It's built for handoffs:
one agent drafts a report, another picks it up, a person leaves a comment on one paragraph, and the
right agent is notified and fixes just that paragraph.

- **Block-level editing.** Pages are Markdown. Each paragraph, list item and table row is a block
  with its own id and author. Agents change only the blocks they mean to (`edit_page`), with
  version checks per block: if someone changed the same block meanwhile, nothing is written and the
  agent gets the current text back. Untouched blocks are never rewritten, byte for byte.
- **A real editor for people.** Block editor (Tiptap) with live updates while agents write, an
  author bar per block, full version history with restore, subpages, trash, share links.
- **Comments that reach agents.** Comment on a block and @mention an agent, or just comment on what
  an agent wrote: that agent gets a notification (webhook, SSE stream or polling) with a ready-made
  prompt. Agents answer in the thread.
- **Databases.** Notion-style tables and boards: select/status/date/person/number/… properties,
  formulas, relations, rollups, files, unique ids, buttons, filters and sorts, schema history with
  undo. Every row is also a page.
- **One small server.** Python + SQLite, one data directory, no other services.

It started as the docs library of [Open Super Agent](https://github.com/kadaliao/open-super-agent)
and was split out so any agent can use it.

## Run it

From source (Python 3.11+, [uv](https://docs.astral.sh/uv/), Node 20+ for the web UI):

```sh
git clone https://github.com/kadaliao/open-agent-docs && cd open-agent-docs
(cd web && npm ci && npm run build)
uv run open-agent-docs serve            # http://127.0.0.1:8790
```

The first start prints a random login password (shown once). Set your own with
`OAD_PASSWORD=...` before the first start, or later with `uv run open-agent-docs password`.

Docker:

```sh
docker build -t open-agent-docs .
docker run -d --name docs -p 8790:8790 -v oad-data:/data \
  -e OAD_PASSWORD='choose-one' -e OAD_PUBLIC_URL=https://docs.example.com open-agent-docs
```

Put it behind HTTPS (Caddy, nginx) before exposing it to the internet, and set
`OAD_SECURE_COOKIE=1` there.

| Setting | Meaning |
| --- | --- |
| `OAD_DATA_DIR` / `--data` | Where the SQLite database lives (default `~/.open-agent-docs`). Back up this one file: `docs.db`. |
| `OAD_PUBLIC_URL` / `--public-url` | The address people open, used in links sent to agents and webhooks. |
| `OAD_PASSWORD` | Login password for the first start. |
| `OAD_LANG` | Default language, `zh` or `en` (changeable in Settings). |
| `OAD_HOST`, `OAD_PORT` / `--host`, `--port` | Where to listen (default `127.0.0.1:8790`). |
| `OAD_SECURE_COOKIE=1` | Send the login cookie over HTTPS only. |

## Connect an agent

In the web UI open **Settings → Agents → New agent**, give it a name (that's how it signs its
edits and how people @mention it) and pick a scope: **read**, **write**, or **admin** (write, plus
managing other agents — for hosts, see below). Copy the token; it's shown once.

Claude Code:

```sh
claude mcp add --transport http docs https://docs.example.com/mcp \
  --header "Authorization: Bearer oad_..."
```

Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.docs]
url = "https://docs.example.com/mcp"
bearer_token_env_var = "OAD_TOKEN"
```

Any MCP client that speaks Streamable HTTP works the same way. From the command line on the server:
`uv run open-agent-docs agent add "Claude Code" --scope write`.

### Tools

| Tool | |
| --- | --- |
| `search_pages`, `list_pages`, `read_page` | Find and read pages. `read_page` shows the version, a block id before each block, and open comments. |
| `create_page`, `edit_page`, `rewrite_page` | Create; change blocks (replace / insert / delete / append, all-or-nothing); replace the whole text while keeping unchanged blocks. |
| `list_comments`, `comment_page` | Read comments; reply, comment on a block, resolve. |
| `upload_image` | Upload an image (base64), get Markdown to put in a page. |
| `db_create`, `db_schema`, `db_query`, `db_add`, `db_update` | Databases: create, change properties, filter/sort, add rows, update properties. |
| `my_notifications` | Comments waiting for this agent, for agents without a host that delivers them. |

Read-only tokens don't see the write tools.

## Agent hosts

Platforms that run many bots (like the [DSH Bots plugin](https://github.com/kadaliao/dsh-plugins/tree/main/plugins/bots))
create one agent per bot with an admin token, and receive comment notifications for all of them over
one SSE stream or per-agent webhooks. See [docs/integration.md](docs/integration.md).

## Development

```sh
uv run pytest -q                 # backend tests
cd web && npm run dev            # UI on :5173, proxies /api and /mcp to :8790
OAD_DATA_DIR=$(mktemp -d) uv run python -m open_agent_docs.i18n   # translation check
```

## License

MIT
