# @open-agent-docs/elements

English | [简体中文](https://github.com/kadaliao/open-agent-docs/blob/main/packages/elements/README.zh-CN.md)

Web Components for Open Agent Docs: `<oad-editor>`, `<oad-viewer>`, `<oad-database>`. The host doesn't need React (React, the editor and Yjs are bundled),
so they work with Vue, Svelte, Angular, plain HTML or server-rendered templates.

## Install

```html
<!-- No bundler: load it from a CDN; the elements are registered on import -->
<script type="module" src="https://cdn.jsdelivr.net/npm/@open-agent-docs/elements@0.4.0/dist/elements.js"></script>

<oad-editor base-url="https://docs.example.com" page-id="Qj1y9Gufp11" token="eyJ…"></oad-editor>
```

```sh
npm i @open-agent-docs/elements
```

```ts
import '@open-agent-docs/elements' // registers the three elements; document.querySelector('oad-editor') is typed
```

`dist/elements.js` is the entry (about 13 KB gzipped). The editor, database view, code highlighting, math and diagrams live in `dist/chunks/` and are
fetched relative to `elements.js` the first time they're needed (works from a CDN too): about 290 KB more once there's an `<oad-editor>` on the page,
about 570 KB for `<oad-database>` (including the ~250 KB WASM query engine; code shared with the editor is downloaded once). A page with only `<oad-viewer>` never downloads the editor.

## Elements

| Element | Attributes | |
| --- | --- | --- |
| `<oad-editor>` | `page-id`, `comments`, `readonly` | Title + collaborative editor (real-time over WebSocket, with other people's cursors). `comments` adds the comments panel (moves below when narrow); `readonly` (automatic for viewers) |
| `<oad-viewer>` | `page-id`, `content-css` | Read-only and lightest: one `GET /pages/{id}/content?format=html`, no WebSocket, no editor code. Stylesheet defaults to `<base-url>/embed/content.css`. On servers without `format=html` it converts Markdown in the browser (`@open-agent-docs/markdown`, loaded on demand) |
| `<oad-database>` | `db-id`, `view-id`, `readonly` | Database view: table, board, calendar, gallery, list. `view-id` shows just that view |

Shared by all three:

| Attribute / property | |
| --- | --- |
| `base-url` / `el.baseUrl` | Server URL; empty means same origin (the host proxies `/api` to Open Agent Docs) |
| `token` / `el.token` | JWT or Agent token: `Authorization: Bearer` for requests, `?token=` for the WebSocket. Without it, cookies are used (same origin, signed in) |
| `el.getToken = async () => …` | Called when there's no token, when the JWT expires in under 60 seconds, or on a 401 (one call at a time); WebSocket reconnects use the new token |
| `theme` | `light` / `dark` / omit to follow the system |
| `lang` | `zh` / `en`: UI language (defaults to the browser's) |
| `el.client` | The `@open-agent-docs/client` client the element uses (same auth), for your own API calls |
| `el.reload()` | Tear down and start again (e.g. to refresh an `<oad-viewer>`) |

Changing `page-id`, `db-id` or `base-url` reconnects; `comments`, `readonly` and `view-id` only re-render. Removing an element closes its WebSocket and
event stream and frees the Y.Doc; moving it within the page (remove + append) doesn't reconnect. Setting `el.token` / `el.getToken` before the script
has loaded is fine too.

## Events

All are `CustomEvent`s that bubble and cross the shadow boundary, with data in `detail`:

| Event | `detail` | When |
| --- | --- | --- |
| `oad-ready` | `{ id, title }` | Content is there (editor: local cache or first server sync; viewer: rendered; database: data loaded) |
| `oad-change` | `{ id, kind: 'title' \| 'content', title, local }` | Title or body changed (body changes are batched per 300 ms). `local`: this user vs. a collaborator or Agent |
| `oad-navigate` | `{ href, pageId? }` | Going to another page (internal link, opening a database row). Call `preventDefault()` to handle it yourself; otherwise the browser navigates |
| `oad-error` | `{ error, status?, code?, message }` | Something failed (401, 403, 404…); a short message is also shown in place |

```js
const el = document.querySelector('oad-editor')
el.getToken = () => fetch('/my-backend/oad-token').then((r) => r.json()).then((j) => j.token)
el.addEventListener('oad-change', (e) => console.log(e.detail.kind, e.detail.title))
el.addEventListener('oad-navigate', (e) => {
  e.preventDefault()
  router.push('/docs/' + e.detail.pageId)
})
```

## Auth

- **JWT** (recommended, `OAD_AUTH=jwt`): your backend signs with `signJwt(secret, userId, { name, email })` from `@open-agent-docs/client/node` and hands it to the element through `getToken`; the element uses it to talk to the server directly (see below).
- **Same origin + proxy**: the host proxies `/api/*` (including the WebSocket) and adds `X-OAD-User-*` (`OAD_AUTH=header`); no token or `base-url` needed.
- **Agent token**: `token="oad_…"`, for internal tools.

**Direct cross-origin** (`base-url` on a different domain): add the host page's origin to the server's `OAD_ALLOWED_ORIGINS` and the API (CORS),
event streams and WebSocket handshakes (Origin check) all let it through:

```sh
OAD_AUTH=jwt OAD_JWT_SECRET='…' OAD_ALLOWED_ORIGINS=https://app.example.com open-agent-docs serve
```

**Images and files**: `<img>`, `<video>` and links can't carry a token. With token sign-in, the element first trades server URLs for signed URLs
via `POST /api/v1/files/sign` (`?exp=…&sig=…`, batched, cached until 60 seconds before expiry, re-signed when expired or when loading fails) and
prefixes `base-url` when cross-origin; the `format=html` that `<oad-viewer>` gets already has signed URLs. Anyone holding a signed URL can open it
until it expires (24 hours by default, `OAD_SIGNED_URL_TTL`); see the [embedding docs](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/embedding.md#read-only-html).

If the browser shouldn't talk to the server directly (it only listens internally), have the host forward `/api/*` (WebSocket included) under
its own domain and leave `base-url` out. See `examples/embed-elements` in the repo (plain HTML + a small Node server that signs JWTs; the elements
connect directly, `PROXY=1` shows the forwarding variant).

## Styling

Styles live inside the Shadow DOM: the host page's CSS doesn't leak in and ours doesn't leak out. Change colors and fonts with CSS variables,
**set on the element** (not on `:root`):

```css
oad-editor, oad-viewer, oad-database {
  --oad-accent: #7c3aed;
  --oad-accent-soft: rgba(124, 58, 237, 0.14);
  --oad-font: 'Inter', -apple-system, sans-serif;
  --oad-fg: #1f1e1c; --oad-bg: #fff; --oad-radius: 8px;
}
```

The variables are the same as in `@open-agent-docs/editor` (`--oad-fg`, `--oad-fg-2`, `--oad-bg-2`, `--oad-line`…). Dark mode: `theme="dark"`, or omit it to
follow the system. `::part(root)` selects the outermost `div` inside the element.

## Differences from the React components

`@open-agent-docs/editor` is the React version, where the host manages React and the stylesheet; this package wraps the same components:

- Everything that pops up (database popovers, the formula editor, the image lightbox) is mounted inside the element's own shadow root, so it gets
  its styles and nothing is inserted into the host page; the code-block language hint list lives in the shadow root too.
- There is one UI language per page: setting `lang` on one element switches every element on the page (and any `@open-agent-docs/editor`
  components the host renders itself), so you can't have one element in Chinese and another in English on the same page.
  Several hundred strings in the editor and database view read the global language, many of them from ProseMirror node views outside React,
  so making it per-element would be a large change; it isn't done for now.
  Setting `lang` also sets the host page's `<html lang>` and stores the choice in localStorage (`oad:locale`).

## Development

```sh
pnpm --filter @open-agent-docs/elements build   # dist/: vite library mode + type declarations (scripts/dts.mjs)
pnpm --filter @open-agent-docs/elements size    # gzipped sizes of each part
pnpm --filter @open-agent-docs/elements test    # jsdom: registration, attribute changes, events, tokens, disconnect
```

At build time each `import './x.css'` becomes `addCss(cssText)` (a plugin in `vite.config.ts`, see `src/styles.ts`), with `:root` rewritten to `:host`,
and is adopted into every element's shadow root.
