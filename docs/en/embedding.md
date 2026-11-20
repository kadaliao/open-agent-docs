# Embedding

[简体中文](../zh/embedding.md) · [Contents](README.md)

Two ways on the web side:

- **iframe**: point an iframe at an Open Agent Docs page. No front-end code.
- **JS packages**: `@open-agent-docs/client` (API, live events, collaborative documents, local databases) and `@open-agent-docs/editor` (React components:
  editor, comment panel, database views). The editor lives inside your page and follows your styles.

Three ways to run the server: a separate `open-agent-docs serve` next to the host (behind its reverse proxy); inside the host's
Node / Python process ([in-process](#in-process-node--python)); or mounted into a Rust service's axum app ([Rust mount](#mounting-into-a-rust-service)).

Four ways to sign people in: Open Agent Docs' own accounts (cookie) on the same domain; the host signs people in and forwards
user headers ([header auth](#header-auth)); the host signs a JWT ([JWT](#jwt)); or a Rust host implements `AuthProvider`
([Rust mount](#mounting-into-a-rust-service)).

## iframe

```html
<iframe src="https://docs.example.com/p/<page id>?embed=1&theme=light&lang=en" style="width:100%;border:0"></iframe>
```

Embed mode has no sidebar or top bar, only the page (or database); links navigate inside the iframe.

| Parameter | Meaning |
| --- | --- |
| `embed=1` | Embed mode |
| `theme=light` / `dark` | Theme; follows the system if omitted |
| `lang=zh` / `en` | UI language; follows the browser and account if omitted (not saved) |

It talks to the host page with `postMessage`:

| Direction | Message |
| --- | --- |
| iframe → host | `{ type: 'oad:ready', pageId }` content is showing |
| | `{ type: 'oad:navigate', pageId, title }` moved to another page inside |
| | `{ type: 'oad:height', height }` content height changed (to size the iframe to its content) |
| host → iframe | `{ type: 'oad:navigate', pageId }` open another page |
| | `{ type: 'oad:theme', theme: 'light' \| 'dark' \| 'system' }` |
| | `{ type: 'oad:locale', locale: 'zh' \| 'en' }` |

```js
const frame = document.querySelector('iframe')
addEventListener('message', (e) => {
  if (e.origin !== 'https://docs.example.com') return
  if (e.data?.type === 'oad:height') frame.style.height = e.data.height + 'px'
})
frame.contentWindow.postMessage({ type: 'oad:theme', theme: 'dark' }, 'https://docs.example.com')
```

The iframe uses Open Agent Docs' own sign-in (cookie). When the host is on another domain, browsers may block third-party cookies:
put both under one domain (reverse proxy), or use header / JWT auth. Full example: [examples/embed-iframe](../../examples/embed-iframe).

## JS packages

> Not on npm yet. Build the packages from the repo: `pnpm pack:all` writes `dist-packs/*.tgz`; install them with
> `npm i ./oad-client-x.y.z.tgz` and so on.

```sh
npm i @open-agent-docs/client @open-agent-docs/editor react react-dom yjs
```

### @open-agent-docs/client

```ts
import { createClient } from '@open-agent-docs/client'

const client = createClient({
  baseUrl: 'https://docs.example.com',  // '' for same origin
  token: () => getToken(),              // optional: agent token, session token or JWT; cookies otherwise
  locale: 'en',                          // language of error messages
})

const me = await client.me()
const ws = me.workspaces[0].id
const pages = await client.pages(ws)

// Live events (SSE; reconnects and resumes where it left off)
const events = client.events(ws)
events.on('page.updated', (p) => console.log(p.title))

// Collaborative document (Yjs + WebSocket; IndexedDB cache in browsers)
const conn = client.openPage(pages[0].id)
await conn.ready

// Database: local cache + incremental sync; filter/sort/group run in WASM (same code as the server)
const db = client.database(dbId)
await db.open(events)
const { ids } = db.engine!.query({ sorts: [{ prop: 'n', dir: 'desc' }] })
await db.setCells([{ row_id: ids[0], key: 'n', value: 3 }])   // applied locally first, rolled back if the server refuses
```

Entry points: `@open-agent-docs/client` (everything), `@open-agent-docs/client/core` (API and events only, no Yjs or WASM — the smallest),
`@open-agent-docs/client/page`, `@open-agent-docs/client/db`. With core alone, register plugins explicitly: `registerPlugin('openPage', openPage)`.

### @open-agent-docs/editor

```tsx
import { OadEditor, OadTitle, OadComments, OadDatabaseView, useOadPage, colorFor } from '@open-agent-docs/editor'
import '@open-agent-docs/editor/style.css'

function Doc({ pageId, me }) {
  const user = { id: me.user.id, name: me.user.name, color: colorFor(me.user.id) }
  const doc = useOadPage(client, pageId, { user })
  if (!doc.ydoc) return null
  return (
    <>
      <OadTitle ydoc={doc.ydoc} />
      <OadEditor ydoc={doc.ydoc} provider={doc.provider} user={user} />
    </>
  )
}
```

`react`, `react-dom`, `yjs` and `@open-agent-docs/client` are peer dependencies. Component styles apply only inside `.oad-root` and don't
leak into the host; change colors and fonts with CSS variables. UI language: `import { setLocale } from '@open-agent-docs/editor/i18n'`.
Full example (editor + comments + database views + two users): [examples/embed-react](../../examples/embed-react).

## Header auth

The host signs people in and, on its server, forwards `/api/*` (including the WebSocket at `/api/v1/sync/*`) to Open Agent Docs with:

| Header | Meaning |
| --- | --- |
| `X-OAD-User-Id` | The host's user id (required; the user is created and added to the default workspace on first sight) |
| `X-OAD-User-Name` | Display name; percent-encode non-ASCII (`encodeURIComponent`) |
| `X-OAD-User-Email` | Optional |
| `X-OAD-Secret` | Must equal `OAD_HEADER_SECRET` |

```sh
OAD_AUTH=header OAD_HEADER_SECRET='a long random string' OAD_HOST=127.0.0.1 open-agent-docs serve
```

- Open Agent Docs listens only on a private address; browsers reach it only through the host, and the secret stays on the host's server.
- If the browser sees a different domain than the server, add the host's address to `OAD_ALLOWED_ORIGINS` (WebSocket handshakes check Origin).
- Agents still connect to `/mcp` with their own tokens; header auth doesn't affect them.

## JWT

The host signs a short-lived JWT with its own key and hands it to the browser, which connects to Open Agent Docs directly (no proxy):

```sh
OAD_AUTH=jwt OAD_JWT_SECRET='…' open-agent-docs serve          # HS256 / HS384 / HS512
OAD_AUTH=jwt OAD_JWT_JWKS_URL=https://auth.example.com/.well-known/jwks.json open-agent-docs serve   # RS* / ES256 / ES384 / EdDSA
# Optional: OAD_JWT_ISSUER, OAD_JWT_AUDIENCE, OAD_JWT_LEEWAY (seconds, default 60)
```

- Send it as `Authorization: Bearer <jwt>`, as `?token=<jwt>` on WebSockets, or in the `oad_token` cookie. `@open-agent-docs/client`'s `token` option handles this.
- Claims: `sub` (required, user id), `exp` (required), `name`, `email`, `workspaces` (array of workspace ids, or
  `[{ "id": "…", "role": "editor" }]`; the default workspace if omitted).
- Users are matched by `sub` and created on first sight; the name follows the claims; a role given in the claims wins, new members default to editor.
- JWKS keys are cached by `kid` for 10 minutes and refetched when the host rotates keys (a new kid); the token's algorithm must match the key's.
- Agents keep using their own tokens.

## In-process (Node / Python)

If you'd rather not run a separate process, start the server inside the host's own process. It runs on its own thread and tokio
runtime, so it doesn't block Node's event loop or hold Python's GIL. Options are exactly the command-line ones (`OAD_*`, see
[Configuration](configuration.md)), also writable as `dataDir` / `data_dir`, with the environment as fallback; without a port it picks a free one.

**Node** (`@open-agent-docs/server`, napi-rs, `packages/server-node`):

```js
import { start } from '@open-agent-docs/server'

const oad = await start({ dataDir: './data', auth: 'header', headerSecret: process.env.OAD_HEADER_SECRET })
console.log(oad.url)               // http://127.0.0.1:<port>
app.use(oad.handler)               // Express / Node http: forwards requests (streaming, SSE works)
server.on('upgrade', oad.upgrade)  // collaborative-editing WebSocket
await oad.close()                  // resolves once open pages are saved
```

`handler` forwards the request to the in-process server rather than calling Rust directly from Node: the server is async with its
own runtime, a per-request FFI call would copy request and response bodies, and long-lived SSE / WebSocket connections wouldn't fit;
forwarding over loopback is cheap. The web app's links assume the root path, so either mount at `/` or forward only `/api` and `/s`.
Example: [examples/node-inprocess](../../examples/node-inprocess) (Express, header auth).

**Python** (`open-agent-docs`, a PyO3 abi3 wheel, `crates/oad-py`, built with maturin):

```python
import open_agent_docs as oad

with oad.start(data_dir="./data", auth="header", header_secret=SECRET) as server:
    print(server.url)              # closed (pages saved) when the with block ends
# or: server = oad.start(...); ...; server.close()
```

Example: [examples/python-inprocess](../../examples/python-inprocess) (started in FastAPI's lifespan; the host calls the API as a user via httpx).

## Mounting into a Rust service

A Rust (axum) host depends on `oad-core` and `oad-server` directly:

1. Implement your own `AuthProvider`: recognise the user from the host's session, find or create them by external id
   (`Storage::user_by_external` / `create_user`), and add them to the default workspace with `oad_core::join_default`;
2. `Core::open_full(storage, AuthConfig::Custom(Arc::new(your_provider)), …)`;
3. `merge` `oad_server::router(AppState::new(core, ServerConfig::default()))` with your own routes (API only, no web app:
   `oad_server::api_router`). For multiple tenants use `oad_server::tenant::multi_tenant` (tenant by Host or header, one setup each).

Example: [examples/rust-mount](../../examples/rust-mount).
