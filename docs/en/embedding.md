# Embedding

[简体中文](../zh/embedding.md) · [Contents](README.md)

Three ways on the web side:

- **iframe**: point an iframe at an Open Agent Docs page. No front-end code.
- **Web Components**: `<oad-editor>`, `<oad-viewer>`, `<oad-database>` (`@open-agent-docs/elements`), for any framework or plain HTML
  ([Web Components](#web-components)).
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
| | `{ type: 'oad:auth-required', reason }` needs a new token (`reason`: `'unauthorized'` got a 401, `'expiring'` about to expire); see [cross-origin + JWT](#cross-origin--jwt) |
| host → iframe | `{ type: 'oad:navigate', pageId }` open another page |
| | `{ type: 'oad:theme', theme: 'light' \| 'dark' \| 'system' }` |
| | `{ type: 'oad:locale', locale: 'zh' \| 'en' }` |
| | `{ type: 'oad:token', token }` swap in a new token (embed mode only, see below) |

```js
const frame = document.querySelector('iframe')
addEventListener('message', (e) => {
  if (e.origin !== 'https://docs.example.com') return
  if (e.data?.type === 'oad:height') frame.style.height = e.data.height + 'px'
})
frame.contentWindow.postMessage({ type: 'oad:theme', theme: 'dark' }, 'https://docs.example.com')
```

By default the iframe uses Open Agent Docs' own sign-in (cookie). When the host is on another domain, browsers block third-party cookies:
put both under one domain (reverse proxy), or use a JWT as below. Full example: [examples/embed-iframe](../../examples/embed-iframe) (same-domain and cross-origin + JWT).

### Cross-origin + JWT

The host is on `app.example.com`, Open Agent Docs on `docs.example.com`, no cookies: the host's back end signs a short-lived JWT ([JWT](#jwt))
and puts it after `#token=` in the iframe URL.

```sh
OAD_AUTH=jwt OAD_JWT_SECRET='…' OAD_ALLOWED_ORIGINS=https://app.example.com open-agent-docs serve
```

```html
<iframe id="oad" src="https://docs.example.com/p/<page id>?embed=1#token=<JWT>"></iframe>
<script>
  const OAD = 'https://docs.example.com'
  const frame = document.getElementById('oad')
  addEventListener('message', async (e) => {
    if (e.origin !== OAD) return
    // got a 401 or the token is about to expire: get a new one from your back end and send it in
    if (e.data?.type === 'oad:auth-required') {
      const { token } = await (await fetch('/api/oad-token')).json()
      frame.contentWindow.postMessage({ type: 'oad:token', token }, OAD)   // fixed targetOrigin, never '*'
    }
  })
</script>
```

- The token goes after `#`: it is never sent to the server, never lands in access logs or the Referer. The page reads it, keeps it in memory only
  (not in localStorage) and removes it from the address bar right away. From then on API calls send `Authorization: Bearer`, the collaboration
  WebSocket sends `?token=`, and so do live events.
- The host swaps tokens with `oad:token`. Only origins in `OAD_ALLOWED_ORIGINS` (plus `OAD_PUBLIC_URL`'s, and the same origin) are accepted,
  and only from the window that embeds the iframe; the page gets that list from `allowed_origins` in `GET /api/v1/auth/config`.
- On a 401, or when the token's `exp` is less than 60 seconds away, the iframe sends `{ type: 'oad:auth-required', reason }` once
  (once per token and reason); the host answers with `oad:token`. With no token at first, or a bad one, the iframe shows no sign-in form and
  waits for the host's token.
- Outside embed mode (no `?embed=1`) none of this applies; cookies work as usual.
- Uploaded images and files: `<img>`, `<video>` and links can't carry the token, so the iframe first trades their URLs for signed URLs
  via `POST /api/v1/files/sign` (`?exp=…&sig=…`, see [signed URLs](api.md#signed-file-urls)). They load without any cookie.

Sign the JWT on the host's back end with the SDKs' `sign_jwt` / `signJwt`, and build the URL with `embed_url` / `embedUrl` (see [SDKs](sdks.md)).

## Read-only HTML

When you only want to show a page, without the editor (article pages, emails, search results, server-side rendering), ask for HTML:

```sh
curl -H "Authorization: Bearer $TOKEN" 'https://docs.example.com/api/v1/pages/<page id>/content?format=html'
# { "html": "<div class=\"oad-content\"><h1>…</h1>…</div>", "version": "…" }
```

```html
<link rel="stylesheet" href="https://docs.example.com/embed/content.css">
<article>{html}</article>
```

- It is wrapped in `<div class="oad-content">`. The stylesheet `GET /embed/content.css` needs no sign-in, can be fetched cross-origin, has an ETag,
  only applies inside `.oad-content`, and looks like the editor. Change colors and fonts with the `--oad-*` variables (same names as the editor);
  light/dark follows the system, or `data-theme="light|dark"` on an ancestor.
- Safety: only a fixed set of tags and attributes, with text and attribute values escaped; no `<script>`, no event attributes (`onclick`…);
  links only allow http, https, mailto, tel and relative URLs, images only http, https and relative URLs (`javascript:`, `data:` and the like
  lose the link and keep the text); raw HTML written in Markdown shows as text. Safe to set as `innerHTML`.
- Blocks that need scripts become placeholders with `data-*`, text kept: math `<span class="oad-math-inline" data-latex="…">`,
  `<div class="oad-math-block" data-latex="…">`; link cards `<div class="oad-embed" data-url="…" data-view="embed|bookmark">` (a plain link inside);
  uploaded files `<div class="oad-embed oad-file" data-type="file" data-url="…">`. Render math yourself from `data-latex` with KaTeX if you like.
- Images, files and in-app pages are server paths (`/api/v1/files/…`, `/p/…`); with `OAD_PUBLIC_URL` set they are full URLs, so they still
  point to the right place on another domain.
- Images and files the caller can see come as **signed URLs** (`/api/v1/files/<id>/<name>?exp=…&sig=…`): they load in `<img>` without a token,
  whether the host is on another domain or fetched the HTML with a JWT or agent token. `assets_expire_at` in the response (ms timestamp) is when
  those signatures expire, `null` when there are no signed URLs. The signature doesn't cover the thumbnail width: add `&w=640` for a smaller image.
- `limit` works as usual (first N blocks).
- Convert it yourself in the browser or Node: `@open-agent-docs/markdown` (WASM, same code as the server, byte-for-byte the same output);
  the stylesheet ships in the package:

```js
import init, { toHtml } from '@open-agent-docs/markdown'
import '@open-agent-docs/markdown/content.css'

await init()
el.innerHTML = toHtml('# Title\n\nBody')                 // Markdown
el.innerHTML = toHtml(doc, { baseUrl: 'https://docs.example.com' })   // document JSON (from format=json)
```

  HTML you convert yourself has no signatures (signing needs the server's key); cross-origin or with token sign-in, swap the image URLs
  yourself with `client.signFileUrls(urls)` (`POST /files/sign`).

> [!WARNING]
> **Two consequences of signed URLs** (same for `format=html` and `POST /files/sign`):
>
> 1. **They expire**: after 24 hours by default (`OAD_SIGNED_URL_TTL`, see [configuration](configuration.md#server)). If the host caches the HTML
>    (knowledge-base front ends often do), don't cache it longer than that: fetch it again before it expires, or schedule a refresh from
>    `assets_expire_at` in the response. Expired URLs get a 403 and the images break.
> 2. **Whoever has the URL can open it**: anyone who gets this HTML can open its images and files without signing in until they expire.
>    Where documents must not leak, shorten the lifetime; for content meant to stay public, use a share link (`POST /pages/{id}/share`).

## Web Components

When the host doesn't use React (Vue, Svelte, Angular, plain HTML, server templates) but wants the full editor, use `@open-agent-docs/elements`:
three custom elements with React, the editor and Yjs bundled, styles inside Shadow DOM so the host and the elements don't affect each other.
Full reference: [packages/elements/README.en.md](../../packages/elements/README.en.md).

```html
<!-- No bundler: load from a CDN and the elements are registered (or npm i @open-agent-docs/elements and import '@open-agent-docs/elements') -->
<script type="module" src="https://cdn.jsdelivr.net/npm/@open-agent-docs/elements@<version>/dist/elements.js"></script>

<oad-editor base-url="https://docs.example.com" page-id="<page id>" comments></oad-editor>
<oad-viewer base-url="https://docs.example.com" page-id="<page id>"></oad-viewer>
<oad-database base-url="https://docs.example.com" db-id="<database id>"></oad-database>

<script type="module">
  for (const el of document.querySelectorAll('oad-editor, oad-viewer, oad-database')) {
    // a JWT signed by the host's back end; elements ask again when there is none, it's about to expire, or on a 401
    el.getToken = () => fetch('/api/oad-token').then((r) => r.json()).then((j) => j.token)
    el.addEventListener('oad-change', (e) => console.log(e.detail.kind, e.detail.title))
  }
</script>
```

| Element | Attributes | What it is |
| --- | --- | --- |
| `<oad-editor>` | `page-id`, `comments`, `readonly` | Title + collaborative editor (live over WebSocket); `comments` adds the comment panel |
| `<oad-viewer>` | `page-id`, `content-css` | Read-only body via `format=html`; no WebSocket, no editor code — the lightest |
| `<oad-database>` | `db-id`, `view-id`, `readonly` | Database views: table, board, calendar, gallery, list |

- All three take `base-url` (server address, same origin if omitted), `token` (JWT or agent token, or `el.token = …`),
  `el.getToken = async () => …` (called when there's no token, the JWT expires within 60 seconds, or on a 401), `theme` (`light` / `dark`),
  `lang` (`zh` / `en`), plus `el.client` and `el.reload()`.
- Events (`CustomEvent`, bubbling, crossing the Shadow DOM): `oad-ready` (content is in), `oad-change` (title or body changed),
  `oad-navigate` (wants another page; `preventDefault()` to handle it yourself), `oad-error` (401, 403, 404…).
- **Direct cross-origin**: when `base-url` is another domain, the elements call Open Agent Docs' API (CORS) and open WebSockets directly;
  add the host's origin to the server's `OAD_ALLOWED_ORIGINS`:

  ```sh
  OAD_AUTH=jwt OAD_JWT_SECRET='…' OAD_ALLOWED_ORIGINS=https://app.example.com open-agent-docs serve
  ```

  Images and files show through signed URLs (the elements call `POST /files/sign` and cache until 60 seconds before expiry). If the browser
  shouldn't talk to the server directly, have the host forward `/api/*` (WebSocket included) under its own domain and leave `base-url` out.
- Styling: colors and fonts via CSS variables set on the element (`oad-editor { --oad-accent: #7c3aed }`).

Example: [examples/embed-elements](../../examples/embed-elements) (plain HTML plus a small Node service that signs JWTs; the elements connect
directly, `PROXY=1` shows the forwarding variant).

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
  baseUrl: 'https://docs.example.com',  // '' for same origin; another domain needs your page's origin in OAD_ALLOWED_ORIGINS (CORS)
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

Images, videos, uploaded files: nothing to do with same-origin cookie sign-in. When the server is on another domain, or you sign in with a token
(JWT, agent token), `<img>` can't carry the token: give the editor a URL hook that swaps in signed URLs before showing them
(batched `POST /files/sign`, cached until 60 seconds before expiry):

```tsx
import { clientUrlResolver } from '@open-agent-docs/editor'

const resolveUrl = clientUrlResolver(client, { token: true })   // null for same-origin cookie sign-in (nothing to swap)
<OadEditor ydoc={doc.ydoc} provider={doc.provider} user={user} media={{ resolveUrl }} />
<OadDatabaseView client={client} dbId={dbId} media={{ resolveUrl }} … />
```

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
- Browsers on another domain connecting directly: put the host's origin in `OAD_ALLOWED_ORIGINS` (API CORS and the WebSocket Origin check both use it);
  images and files use signed URLs ([read-only HTML](#read-only-html)).
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

**Python** (`pip install "open-agent-docs[server]"`: the Python SDK plus the in-process server `open-agent-docs-server`, a PyO3 abi3 wheel built from `crates/oad-py`):

```python
import open_agent_docs as oad

with oad.start(data_dir="./data", auth="header", header_secret=SECRET) as server:   # closed (pages saved) when the block ends
    admin = oad.Client(base_url=server.url, header_secret=SECRET)
    alice = admin.with_user("alice", name="Alice")                                   # act as the user your host signed in
    alice.pages.create(alice.default_workspace(), title="Weekly", markdown="# This week")
```

With plain `open-agent-docs` (no `[server]`), `oad.start()` downloads the release binary and runs it as a child process.
Full SDK usage: [SDKs](sdks.md). Example: [examples/python-inprocess](../../examples/python-inprocess) (started in FastAPI's lifespan; each request
calls `with_user` for the current user).

## Mounting into a Rust service

A Rust (axum) host depends on `oad-core` and `oad-server` directly:

1. Implement your own `AuthProvider`: recognise the user from the host's session, find or create them by external id
   (`Storage::user_by_external` / `create_user`), and add them to the default workspace with `oad_core::join_default`;
2. `Core::open_full(storage, AuthConfig::Custom(Arc::new(your_provider)), …)`;
3. `merge` `oad_server::router(AppState::new(core, ServerConfig::default()))` with your own routes (API only, no web app:
   `oad_server::api_router`). For multiple tenants use `oad_server::tenant::multi_tenant` (tenant by Host or header, one setup each).

Example: [examples/rust-mount](../../examples/rust-mount).
