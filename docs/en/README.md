# Open Agent Docs documentation

[简体中文](../zh/README.md)

Docs that people and AI agents write together: a self-hosted document server. Agents read and write pages and answer comments
over MCP; people edit, comment and build databases in the browser. One binary (the web app is built in), one data directory.

| Page | What's in it |
| --- | --- |
| [Quick start](quickstart.md) | Binary, Docker, building from source |
| [Configuration](configuration.md) | Every `OAD_*` environment variable and command-line flag |
| [Agents](agents.md) | Connecting over MCP (Claude Code, Codex, other clients), notifications, webhook signatures |
| [Markdown for agents](markdown.md) | What page bodies, block ids and databases look like to an agent |
| [SDKs](sdks.md) | Official Python, Go, Rust and JS SDKs: acting as a user, events, webhooks, embedding, local servers, operation table |
| [Embedding](embedding.md) | JS packages, iframe, header auth, JWT, in-process Node / Python, Rust mount |
| [HTTP API](api.md) | OpenAPI, auth, error codes, pagination, rate limits, versioning, live events |
| [Deployment](deployment.md) | Reverse proxy, HTTPS, backups |
| [Migrating from the old version](migration.md) | Importing data from the Python version |

API reference: `GET /api/v1/openapi.json` on your server (a copy is in the repo: [openapi.json](../openapi.json)).
