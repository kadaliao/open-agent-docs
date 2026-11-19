# @oad/editor

Open Agent Docs 的编辑器和组件：协作编辑器（Tiptap + Yjs）、标题、评论栏、数据库视图，加一个 `useOadPage` hook。
React 19，样式是一个单独的 CSS 文件。

```sh
pnpm add @oad/editor @oad/client react react-dom yjs
```

```tsx
import { createClient } from '@oad/client'
import { OadComments, OadDatabaseView, OadEditor, OadProvider, OadTitle, useOadPage } from '@oad/editor'
import '@oad/editor/style.css'

const client = createClient({ baseUrl: '' }) // 同源（宿主把 /api 转给 Open Agent Docs）

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
  <OadDatabaseView dbId={dbId} me={me} ws={ws} canWrite />   {/* 用到时才下载表格、看板的代码 */}
</OadProvider>
```

| 组件 / 函数 | 说明 |
| --- | --- |
| `OadEditor` | 正文编辑器（`ydoc`、`provider`、`user`、`editable`、`onComment`、`media`……） |
| `OadTitle` | 标题（`ydoc` 里的 `title`） |
| `OadComments` | 评论栏；配 `usePageComments(client, pageId, events)`、`usePeople(...)` |
| `OadDatabaseView` | 数据库：表格、看板、日历、画廊、列表（按需加载；`@oad/editor/database` 里有不按需加载的版本） |
| `useOadPage(client, id)` | 打开一页的协作文档，卸载时关掉 |
| `OadProvider` / `useOadClient` | 把客户端放进上下文，组件就不用一个个传 `client` |
| `setLocale('zh' \| 'en' \| 'auto')` | 界面语言（默认跟浏览器），换了不用刷新（`@oad/editor/i18n`） |

## 换主题

颜色、字体都是 CSS 变量，在宿主自己的样式里覆盖就行；深色模式跟着 `prefers-color-scheme`，或者在 `<html data-theme="dark">` 上指定。

```css
:root {
  --oad-accent: #7c3aed;           /* 强调色：选中、链接、按钮 */
  --oad-accent-soft: rgba(124, 58, 237, 0.14);
  --oad-font: 'Inter', -apple-system, 'PingFang SC', sans-serif;
  --oad-fg: #1f1e1c; --oad-fg-2: #5f5e5b; --oad-fg-3: #9b9a97;   /* 文字：主 / 次 / 弱 */
  --oad-bg: #fff; --oad-bg-2: #f7f7f5; --oad-bg-3: #efeeeb;       /* 背景 */
  --oad-line: #e9e8e5; --oad-line-2: #dcdbd7;                     /* 线 */
  --oad-radius: 8px;
}
```

组件里的通用控件（按钮、头像、弹窗）的样式只在 `.oad-root` 里面生效，不会影响宿主页面里同名的 class。
