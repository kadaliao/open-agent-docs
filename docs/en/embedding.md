# Embedding

[简体中文](../zh/embedding.md) · [Contents](README.md)

Two ways:

- **iframe**: point an iframe at an Open Agent Docs page. No front-end code.
- **JS packages**: `@oad/client` (API, live events, collaborative documents, local databases) and `@oad/editor` (React components:
  editor, comment panel, database views). The editor lives inside your page and follows your styles.

Three ways to sign people in: Open Agent Docs' own accounts (cookie) on the same domain; the host signs people in and forwards
user headers ([header auth](#header-auth)); or the host signs a JWT ([coming](#jwt-coming)).

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
npm i @oad/client @oad/editor react react-dom yjs
```

### @oad/client

```ts
import { createClient } from '@oad/client'

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

Entry points: `@oad/client` (everything), `@oad/client/core` (API and events only, no Yjs or WASM — the smallest),
`@oad/client/page`, `@oad/client/db`. With core alone, register plugins explicitly: `registerPlugin('openPage', openPage)`.

### @oad/editor

```tsx
import { OadEditor, OadTitle, OadComments, OadDatabaseView, useOadPage, colorFor } from '@oad/editor'
import '@oad/editor/style.css'

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

`react`, `react-dom`, `yjs` and `@oad/client` are peer dependencies. Component styles apply only inside `.oad-root` and don't
leak into the host; change colors and fonts with CSS variables. UI language: `import { setLocale } from '@oad/editor/i18n'`.
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

## JWT (coming)

> The server side is still in progress. This is the planned usage and may change before release.

The host signs a short-lived JWT with its own key and hands it to the browser, which connects to Open Agent Docs directly (no proxy):

```sh
OAD_AUTH=jwt OAD_JWT_SECRET='…' open-agent-docs serve          # HS256
OAD_AUTH=jwt OAD_JWT_JWKS_URL=https://auth.example.com/.well-known/jwks.json open-agent-docs serve
# Optional: OAD_JWT_ISSUER, OAD_JWT_AUDIENCE, OAD_JWT_LEEWAY (seconds, default 60)
```

- Send it as `Authorization: Bearer <jwt>`, as `?token=<jwt>` on WebSockets, or in the `oad_token` cookie. `@oad/client`'s `token` option handles this.
- Claims: `sub` (required, user id), `exp` (required), `name`, `email`, `workspaces` (array of workspace ids, or
  `[{ "id": "…", "role": "editor" }]`; the default workspace if omitted).
