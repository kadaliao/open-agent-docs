# @open-agent-docs/editor

English | [简体中文](https://github.com/kadaliao/open-agent-docs/blob/main/packages/editor/README.zh-CN.md)

The editor and components for [Open Agent Docs](https://github.com/kadaliao/open-agent-docs): a collaborative editor (Tiptap + Yjs), a title,
a comments sidebar, database views, and a `useOadPage` hook. React 19; the styles are a single CSS file.

```sh
npm i @open-agent-docs/editor @open-agent-docs/client react react-dom yjs
```

```tsx
import { createClient } from '@open-agent-docs/client'
import { OadComments, OadDatabaseView, OadEditor, OadProvider, OadTitle, useOadPage } from '@open-agent-docs/editor'
import '@open-agent-docs/editor/style.css'

const client = createClient({ baseUrl: '' }) // same origin (the host forwards /api to Open Agent Docs)

function Doc({ id, me }) {
  const user = { id: me.user.id, name: me.user.name, color: '#3e63dd' }
  const page = useOadPage(client, id, { user })
  if (!page.ydoc) return null
  return (
    <>
      <OadTitle ydoc={page.ydoc} />
      <OadEditor ydoc={page.ydoc} provider={page.provider} user={user} />
    </>
  )
}

<OadProvider client={client}>
  <Doc id={pageId} me={me} />
  <OadDatabaseView dbId={dbId} me={me} ws={ws} canWrite />   {/* table and board code is only downloaded when used */}
</OadProvider>
```

| Component / function | Description |
| --- | --- |
| `OadEditor` | The body editor (`ydoc`, `provider`, `user`, `editable`, `onComment`, `media`…) |
| `OadTitle` | The title (`title` in the `ydoc`) |
| `OadComments` | The comments sidebar; use it with `usePageComments(client, pageId, events)` and `usePeople(...)` |
| `OadDatabaseView` | Databases: table, board, calendar, gallery and list (loaded on demand; `@open-agent-docs/editor/database` has a version that isn't) |
| `useOadPage(client, id)` | Opens a page's collaborative document and closes it on unmount |
| `OadProvider` / `useOadClient` | Puts the client in context so you don't pass `client` to every component |
| `setLocale('zh' \| 'en' \| 'auto')` | UI language (follows the browser by default); takes effect without a reload (`@open-agent-docs/editor/i18n`) |

## Theming

Colors and fonts are CSS variables; override them in the host's own styles. Dark mode follows `prefers-color-scheme`, or set it explicitly with
`<html data-theme="dark">`.

```css
:root {
  --oad-accent: #7c3aed;           /* accent: selection, links, buttons */
  --oad-accent-soft: rgba(124, 58, 237, 0.14);
  --oad-font: 'Inter', -apple-system, 'PingFang SC', sans-serif;
  --oad-fg: #1f1e1c; --oad-fg-2: #5f5e5b; --oad-fg-3: #9b9a97;   /* text: primary / secondary / faint */
  --oad-bg: #fff; --oad-bg-2: #f7f7f5; --oad-bg-3: #efeeeb;       /* backgrounds */
  --oad-line: #e9e8e5; --oad-line-2: #dcdbd7;                     /* lines */
  --oad-radius: 8px;
}
```

The styles of the shared controls inside the components (buttons, avatars, popovers) only apply within `.oad-root`, so they don't affect classes of the same name in the host page.

## More

- [Embedding in your own app](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/embedding.md) ([简体中文](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/embedding.md))
- [`@open-agent-docs/elements`](https://www.npmjs.com/package/@open-agent-docs/elements): the same editor as Web Components, for hosts that don't use React
