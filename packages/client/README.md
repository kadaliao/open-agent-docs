# @open-agent-docs/client

English | [简体中文](https://github.com/kadaliao/open-agent-docs/blob/main/packages/client/README.zh-CN.md)

The JS client for [Open Agent Docs](https://github.com/kadaliao/open-agent-docs), docs that people and AI agents write together.
Works in browsers and Node 18+.

```sh
npm i @open-agent-docs/client yjs
```

```ts
import { createClient } from '@open-agent-docs/client'

// Same origin in the browser: no token, the login cookie is enough. Server side / agents: pass a token.
const client = createClient({ baseUrl: 'https://docs.example.com', token: process.env.OAD_TOKEN })

const me = await client.me()
const ws = me.workspaces[0].id
const page = await client.createPage(ws, { title: 'Weekly' })

// Real-time events (SSE; reconnects automatically with Last-Event-ID)
const events = client.events(ws)
events.on('page.updated', (p) => console.log('updated', p.title))

// Collaborative documents: a Y.Doc synced over WebSocket (plus an IndexedDB cache in the browser)
const doc = client.openPage(page.id) // { ydoc, provider, awareness, ready, status(), destroy() }
await doc.ready

// Local database: a local cache, incremental sync and the WASM engine (the same Rust code as the server)
const db = client.database(dbId)
await db.open(events)
const { ids, groups, totals } = db.engine!.query({ sorts: [{ prop: 'due', dir: 'asc' }], group_by: 'status' })
await db.setCells([{ row_id: ids[0], key: 'status', value: 'done' }]) // applied locally first, rolled back if it fails
db.subscribe(() => render()) // called on every change
```

- One method per HTTP endpoint (`pages`, `createPage`, `content`, `threads`, `search`, `versions`, `trash`, `queryDatabase`, `agentNotifications`…).
  Paths, parameters, request bodies and responses are typed from `docs/openapi.json`: `client.call('get', '/pages/{id}', { path: { id } })` can call
  any endpoint in the spec, and a wrong path, method or field is a compile error; `client.request(method, path, body)` is unchecked.
  For what each endpoint is called in the other languages, see the `js` field in
  [`sdks/operations.json`](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/operations.json).
- Paginated lists (`versions`, `dbHistory`, `trash`, `inbox`, `agentNotifications`) return one page (with `next_cursor`).
  To walk all of them use `for await (const v of client.eachVersion(pageId))` (also `eachDbHistory`, `eachTrash`, `eachInbox`, `eachAgentNotification`):
  it follows the cursor page by page and stops fetching when you `break`.
- Types (`Page`, `Me`, `Thread`, `DbRow`, `Version`…) are generated into `src/openapi.ts` (`pnpm --filter @open-agent-docs/client gen:types`;
  `test/openapi-fresh.test.ts` fails when they're stale). Objects the spec only declares as `object` (the table `Schema`, `View`…) are written by hand in `src/types.ts`.
  `OAD_BIN=… pnpm test` also starts a real server and checks the responses of more than 60 endpoints against the spec (`test/contract.test.ts`).
- HTTP only, smaller bundle: `import { createClient } from '@open-agent-docs/client/core'` (no Yjs, no WASM).
  When you need collaborative documents or the database, `import { openPage } from '@open-agent-docs/client/page'` and
  `import { databasePlugin } from '@open-agent-docs/client/db'`, then `registerPlugin('openPage', openPage)` / `registerPlugin('database', databasePlugin)`.
- Node 18–21 has no global WebSocket: `createClient({ WebSocket: (await import('ws')).WebSocket })`. In Node the WASM is read from disk, no setup needed.

## Retries, timeouts, errors

```ts
const client = createClient({ baseUrl, token, maxRetries: 2, timeout: 60_000 }) // these two are the defaults
```

- Retried (up to `maxRetries` times): 429; 502, 503, 504 (not for POST and PATCH, which may already have taken effect); connection failures
  (GET, PUT, DELETE, or when the request certainly wasn't sent). With a `Retry-After` header it waits that long (at most 60 s); otherwise 0.5 s, 1 s, 2 s…
  up to 8 s, with a little jitter.
- `timeout` is per request, in milliseconds, up to the end of the response body; event streams only limit the connection, not the total time.
- A server error throws `ApiError { status, code, message, retryAfter?, requestId? }`; a failed connection throws `ApiConnectionError`, and a timeout
  `ApiTimeoutError` (its subclass). Use the helpers instead of remembering status codes: `isNotFound(e)`, `isUnauthorized`, `isForbidden`, `isConflict`,
  `isGone`, `isBadRequest`, `isTooLarge`, `isRateLimited`, `isServerError`, `isConnectionError`, `isTimeout`.

```ts
import { isNotFound } from '@open-agent-docs/client'
const page = await client.page(id).catch((e) => (isNotFound(e) ? null : Promise.reject(e)))
```

- Event streams (`client.events(ws)`, `client.agentEvents()`) don't reconnect on 401, 403 or 404; `events.onError((e) => …)` receives the error.

## Read-only HTML and embed URLs

```ts
const { html, version } = await client.content(pageId, 'html') // <div class="oad-content">…</div>; the stylesheet is the server's /embed/content.css
import { embedUrl } from '@open-agent-docs/client'
iframe.src = embedUrl('https://docs.example.com', pageId, { theme: 'dark', lang: 'en', token: jwt }) // the token goes after the #
```

## Server side: `@open-agent-docs/client/node`

Everything that needs a secret or `node:crypto` lives in this entry point (don't import it in the browser).

```ts
import { createClient, signJwt, verifyWebhook } from '@open-agent-docs/client/node'

// baseUrl defaults to OAD_BASE_URL (then http://127.0.0.1:8848), token to OAD_TOKEN; no cookies
const admin = createClient({ baseUrl, headerSecret: process.env.OAD_HEADER_SECRET }) // a server with OAD_AUTH=header
// const admin = createClient({ baseUrl, jwtSecret: process.env.OAD_JWT_SECRET })   // a server with OAD_AUTH=jwt (HS256)

const u = admin.withUser('u-42', { name: 'Ming Wang', email: 'x@example.com' }) // a new client; the original is unchanged
await u.pages(await u.defaultWorkspace())

// A JWT for the browser / iframe (claims: sub, name, email, workspaces, iat, exp, iss, aud)
const jwt = signJwt(secret, 'u-42', { name: 'Ming Wang', workspaces: [{ id: ws, role: 'editor' }], ttl: 3600 })

// An agent's webhook: the body must be the raw payload (express.raw(), await req.text()…)
app.post('/oad-webhook', express.raw({ type: '*/*' }), (req, res) => {
  try {
    const note = verifyWebhook(process.env.OAD_WEBHOOK_SECRET!, req.headers, req.body) // AgentNotification
    queue.push(note)
    res.sendStatus(204)
  } catch (e) {
    res.status(400).send((e as Error).message) // WebhookVerificationError, e.kind is malformed / stale / signature
  }
})
```

- `withUser`: header mode sends `X-OAD-Secret`, `X-OAD-User-Id`, `X-OAD-User-Name` (percent-encoded) and `X-OAD-User-Email`; JWT mode signs one and renews it
  60 seconds before it expires. Passing both secrets or neither throws `OadConfigError`; header mode doesn't support `workspaces`.
- `verifyWebhook(secret, headers, body, { tolerance: 300, now })`: `headers` is Node's `req.headers` or a `Headers`; names are case-insensitive.

## Tests

- `pnpm test`: unit tests. `test/conformance.test.ts` is the scenario suite shared by all languages (`docs/sdk-contract.md` §14);
  it runs when there's an in-process server (`packages/server-node`, built) or `OAD_BINARY=target/debug/open-agent-docs` is set, and otherwise skips the cases that need a server.
