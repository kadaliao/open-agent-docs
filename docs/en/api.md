# HTTP API

[简体中文](../zh/api.md) · [Contents](README.md)

The full reference is OpenAPI 3.1: `GET /api/v1/openapi.json` on a running server (no sign-in needed), with a copy in the repo at
[docs/openapi.json](../openapi.json) (generated from the code; a test keeps it in sync). Generate a client from it with
`npx openapi-typescript docs/openapi.json -o api.d.ts`, or use `@open-agent-docs/client` (generated the same way). Swagger UI, Scalar, Redocly and similar tools can display it.

## Basics

- Every endpoint is under **`/api/v1`**; requests and responses are JSON (multipart for uploads and CSV import; binary for CSV export and files).
- **Versioning**: `/api/v1` only grows (new endpoints, new optional fields, new events) — nothing is changed or removed. Breaking changes go to a
  future `/api/v2`, with `v1` kept for a while. Clients should ignore unknown fields and events.
- Times are millisecond timestamps (UTC); ids are strings.
- **Compression**: JSON responses over 1KB are compressed with br or gzip according to `Accept-Encoding` (browsers and common HTTP libraries
  decode it for you). Live events (SSE), WebSocket and files are not compressed.
- **Page content formats**: `GET /pages/{id}/content` returns `{ markdown, version }`; `?format=json` returns the document tree `{ doc, version }`;
  `?format=html` returns read-only HTML `{ html, version, assets_expire_at }` (wrapped in `<div class="oad-content">`, whitelisted tags only, styled by
  `GET /embed/content.css`; see [Embedding](embedding.md#read-only-html)): images and files the caller can see are [signed URLs](#signed-file-urls),
  and `assets_expire_at` is when those signatures expire (ms; `null` when there are none). Writing (`PUT`) takes Markdown or the document tree.
- **Cross-origin**: pages on another domain calling the API directly need their origin in `OAD_ALLOWED_ORIGINS` (`/api/v1` sends CORS headers
  only to those; see [Configuration](configuration.md#request-origin-host-and-origin)).
- **First screen of a big page**: `GET /pages/{id}/content?limit=N` (any `format`) returns only the first N top-level blocks,
  plus `total` (how many there are).

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

## Signed file URLs

Uploaded files (`GET /files/{id}`, `/files/{id}/{name}`) and rehosted images (`GET /media/{hash}`) need sign-in, but `<img>`, `<video>` and links
can't send `Authorization`. Signed URLs work without signing in:

```sh
curl -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"urls": ["/api/v1/files/Qj1y9Gufp11a/pic.png", "Qj1y9Gufp11b", "/api/v1/media/9f86d0…"]}' \
  https://docs.example.com/api/v1/files/sign
# { "urls": ["/api/v1/files/Qj1y9Gufp11a/pic.png?exp=1763712000&sig=…", "/api/v1/files/Qj1y9Gufp11b?exp=…&sig=…", null], "expires_at": 1763712000000 }
```

- `POST /files/sign`: `urls` takes file URLs (root-relative or absolute, `?w=` allowed), rehosted image URLs, or bare file ids, up to 500 per call.
  The returned `urls` match the request one to one and **only cover what the caller can see** (`null` for files in workspaces you're not in,
  missing files, or URLs that aren't this server's files); `expires_at` is the expiry (ms).
- A signed URL is the original URL plus `?exp=<unix seconds>&sig=<base64url(HMAC-SHA256(key, "<file id or media hash>|<exp>"))>`. The signature
  doesn't cover the thumbnail width `w` or the file name, so add `&w=320` yourself. Lifetime: `OAD_SIGNED_URL_TTL` (24 hours by default),
  rounded up to a step so URLs signed close together are identical (browser caches can reuse them).
- With a valid signature the file is served without sign-in (browsers cache it until expiry); expired gives 403 `signature_expired`,
  wrong gives 403 `bad_signature` (still served if you're signed in and can see it).
- Images and files in `GET /pages/{id}/content?format=html` are already signed; `assets_expire_at` is their expiry.

> [!WARNING]
> 1. **They expire**: signatures on image and file URLs in the `format=html` output expire after 24 hours by default (`OAD_SIGNED_URL_TTL`).
>    If the host caches the HTML (knowledge-base front ends often do), don't cache it longer than that: fetch it again before it expires,
>    or schedule a refresh from `assets_expire_at` in the response.
> 2. **Whoever has it can open it**: anyone who gets this HTML (or a signed URL) can open its images and files without signing in until they
>    expire. Where documents must not leak, shorten the lifetime; for content meant to stay public, use a share link.

## Live events and webhooks

- Workspace events: `GET /workspaces/{ws}/events` (SSE). Event names and payloads are in OpenAPI's `webhooks`: `page.created`, `page.updated`,
  `page.moved`, `page.deleted`, `page.purged`, `db.changed` (then fetch `rows?since=`), `db.schema`, `comment.*`, `thread.updated`, `notification`,
  `workspace.updated`; `resync` means events were missed — reload.
- Collaborative editing: `/api/v1/sync/{page}`, the standard y-websocket protocol.
- Agent webhooks: POST to the agent's URL with an `AgentNotification` body and headers `X-OAD-Timestamp`,
  `X-OAD-Signature: sha256=<hex(HMAC-SHA256(secret, "<timestamp>.<body>"))>`, `X-OAD-Delivery`; any 2xx counts as delivered. See [Agents](agents.md) for verification.
