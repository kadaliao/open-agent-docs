# HTTP API

[简体中文](../zh/api.md) · [Contents](README.md)

The full reference is OpenAPI 3.1: `GET /api/v1/openapi.json` on a running server (no sign-in needed), with a copy in the repo at
[docs/openapi.json](../openapi.json) (generated from the code; a test keeps it in sync). Generate a client from it with
`npx openapi-typescript docs/openapi.json -o api.d.ts`, or use `@oad/client` (generated the same way). Swagger UI, Scalar, Redocly and similar tools can display it.

## Basics

- Every endpoint is under **`/api/v1`**; requests and responses are JSON (multipart for uploads and CSV import; binary for CSV export and files).
- **Versioning**: `/api/v1` only grows (new endpoints, new optional fields, new events) — nothing is changed or removed. Breaking changes go to a
  future `/api/v2`, with `v1` kept for a while. Clients should ignore unknown fields and events.
- Times are millisecond timestamps (UTC); ids are strings.

## Authentication

Tried in this order (see [Configuration](configuration.md#sign-in) and [Embedding](embedding.md)):

| Method | How |
| --- | --- |
| Agent token | `Authorization: Bearer oad_…` (from the Agent settings; agents can also use MCP at `/mcp`) |
| Session | The `oad_session` cookie after `POST /auth/login`, or `Authorization: Bearer <session token>` |
| JWT (`OAD_AUTH=jwt`) | `Authorization: Bearer <jwt>`, `?token=<jwt>` or the `oad_token` cookie |
| Header (`OAD_AUTH=header`) | The host forwards `X-OAD-Secret` and `X-OAD-User-Id` / `-Name` / `-Email` |

WebSockets (`/api/v1/sync/{page}`) can't carry headers in browsers; use the cookie or `?token=`.

## Errors

Errors are an HTTP status plus `{ "error": { "code": "...", "message": "..." } }`. `message` follows the caller's language (the user's / agent's
setting > `Accept-Language` > Chinese); programs should look at `code`:

| code | Status | When |
| --- | --- | --- |
| `bad_request` | 400 | Invalid parameters |
| `unauthorized` | 401 | Not signed in / bad token |
| `forbidden` | 403 | Not allowed (read-only member writing, not a member…) |
| `not_found` | 404 | Doesn't exist (or you can't see it) |
| `conflict` | 409 | Conflict (duplicate name, email already registered) |
| `gone` | 410 | Existed but no longer valid (expired / revoked invite) |
| `too_large` | 413 | Upload too large |
| `too_many_requests` | 429 | Rate limited (with `Retry-After`) |
| `bad_gateway` | 502 | Fetching from another site failed (link previews) |
| `internal` | 500 | Server error |

## Pagination

Long lists use cursors: `?limit=&cursor=`; the response has `next_cursor`, `null` on the last page, otherwise pass it back. Currently: page versions,
database history, trash, agent notifications, the inbox. Other lists (pages, members, comments…) come in full; database rows use
`GET /databases/{id}/rows?since=<seq>` for increments, and queries use `offset`.

## Rate limits

Each caller (by token / session, otherwise by IP) gets `OAD_RATE_LIMIT` requests per minute (default 1200, `0` = unlimited). Every response carries:

| Header | Meaning |
| --- | --- |
| `RateLimit-Limit` | Per-minute limit |
| `RateLimit-Remaining` | Requests left this minute |
| `RateLimit-Reset` | Seconds until reset |

Over the limit you get 429 `too_many_requests` with `Retry-After` (seconds). Failed sign-ins have their own limit (10 failures per IP + email per 15 minutes).

## Live events and webhooks

- Workspace events: `GET /workspaces/{ws}/events` (SSE). Event names and payloads are in OpenAPI's `webhooks`: `page.created`, `page.updated`,
  `page.moved`, `page.deleted`, `page.purged`, `db.changed` (then fetch `rows?since=`), `db.schema`, `comment.*`, `thread.updated`, `notification`,
  `workspace.updated`; `resync` means events were missed — reload.
- Collaborative editing: `/api/v1/sync/{page}`, the standard y-websocket protocol.
- Agent webhooks: POST to the agent's URL with an `AgentNotification` body and headers `X-OAD-Timestamp`,
  `X-OAD-Signature: sha256=<hex(HMAC-SHA256(secret, "<timestamp>.<body>"))>`, `X-OAD-Delivery`; any 2xx counts as delivered. See [Agents](agents.md) for verification.
