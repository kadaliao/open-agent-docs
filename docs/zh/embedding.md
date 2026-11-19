# 嵌入到自己的应用

[English](../en/embedding.md) · [文档目录](README.md)

两种做法：

- **iframe**：放一个 iframe 指向 Open Agent Docs 的页面，不用写前端代码。
- **JS 包**：`@oad/client`（接口、实时事件、协作文档、本地数据库）和 `@oad/editor`（React 组件：编辑器、评论栏、数据库视图），
  编辑器直接长在你的页面里，跟着你的样式。

登录有三种接法：同域名下用 Open Agent Docs 自己的账号（cookie）；宿主自己管登录、转发时带用户头（[header 登录](#header-登录)）；
宿主签 JWT（[即将推出](#jwt即将推出)）。

## iframe

```html
<iframe src="https://docs.example.com/p/<页面 id>?embed=1&theme=light&lang=zh" style="width:100%;border:0"></iframe>
```

嵌入模式没有侧栏和顶栏，只有正文（或数据库），里面的链接在 iframe 里跳转。

| 参数 | 说明 |
| --- | --- |
| `embed=1` | 嵌入模式 |
| `theme=light` / `dark` | 主题，不给跟系统 |
| `lang=zh` / `en` | 界面语言，不给跟浏览器和账号（不会存下来） |

和宿主用 `postMessage` 说话：

| 方向 | 消息 |
| --- | --- |
| iframe → 宿主 | `{ type: 'oad:ready', pageId }` 内容出来了 |
| | `{ type: 'oad:navigate', pageId, title }` 在里面换了页 |
| | `{ type: 'oad:height', height }` 内容高度变了（想让 iframe 跟着长高时用） |
| 宿主 → iframe | `{ type: 'oad:navigate', pageId }` 换一页 |
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

iframe 里用的是 Open Agent Docs 自己的登录（cookie）。宿主和它不同域名时浏览器可能拦第三方 cookie：
让两边同域（反向代理到同一个域名下），或者用 header / JWT 登录。完整的例子：[examples/embed-iframe](../../examples/embed-iframe)。

## JS 包

> 还没发布到 npm。现在用仓库里打的包：`pnpm pack:all` 生成 `dist-packs/*.tgz`，在你的项目里 `npm i ./oad-client-x.y.z.tgz` 等。

```sh
npm i @oad/client @oad/editor react react-dom yjs
```

### @oad/client

```ts
import { createClient } from '@oad/client'

const client = createClient({
  baseUrl: 'https://docs.example.com',  // 同源时给 ''
  token: () => getToken(),              // 可选：Agent token、会话令牌或 JWT；不给就用 cookie
  locale: 'zh',                          // 错误信息的语言
})

const me = await client.me()
const ws = me.workspaces[0].id
const pages = await client.pages(ws)

// 实时事件（SSE，断了自动重连，从断的地方续上）
const events = client.events(ws)
events.on('page.updated', (p) => console.log(p.title))

// 协作文档（Yjs + WebSocket；浏览器里带 IndexedDB 缓存）
const conn = client.openPage(pages[0].id)
await conn.ready

// 数据库：本机缓存 + 增量同步，筛选排序分组在 WASM 里算（和服务器同一份代码）
const db = client.database(dbId)
await db.open(events)
const { ids } = db.engine!.query({ sorts: [{ prop: 'n', dir: 'desc' }] })
await db.setCells([{ row_id: ids[0], key: 'n', value: 3 }])   // 先改本地，服务器失败会退回
```

入口：`@oad/client`（全部）、`@oad/client/core`（只有接口和事件，不带 Yjs、WASM，最小）、`@oad/client/page`、`@oad/client/db`。
只用 core 时要显式注册：`registerPlugin('openPage', openPage)`。

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

`react`、`react-dom`、`yjs`、`@oad/client` 是 peer 依赖。组件的样式只在 `.oad-root` 里生效，不会影响宿主；颜色、字体用 CSS 变量换。
界面语言：`import { setLocale } from '@oad/editor/i18n'`。完整的例子（编辑器 + 评论栏 + 数据库视图 + 两个用户）：[examples/embed-react](../../examples/embed-react)。

## header 登录

宿主自己做登录，在服务端把 `/api/*`（包括 WebSocket `/api/v1/sync/*`）转给 Open Agent Docs，转发时带上：

| 请求头 | 说明 |
| --- | --- |
| `X-OAD-User-Id` | 宿主那边的用户编号（必填，第一次出现时自动建用户、加进默认工作区） |
| `X-OAD-User-Name` | 显示的名字；非 ASCII 的用百分号编码（`encodeURIComponent`） |
| `X-OAD-User-Email` | 可选 |
| `X-OAD-Secret` | 和 `OAD_HEADER_SECRET` 一样才认 |

```sh
OAD_AUTH=header OAD_HEADER_SECRET='一串长的随机字符' OAD_HOST=127.0.0.1 open-agent-docs serve
```

- Open Agent Docs 只监听内网，浏览器连不到它，只能经过宿主；密钥只在宿主的服务端。
- 宿主和浏览器看到的域名不一样时，把宿主的地址写进 `OAD_ALLOWED_ORIGINS`（WebSocket 握手要检查 Origin）。
- Agent 照常用自己的 token 连 `/mcp`，不受 header 登录影响。

## JWT（即将推出）

> 服务器这边还在做，下面是计划的用法，发布前可能还会变。

宿主用自己的密钥签一个短期 JWT 交给浏览器，浏览器直接连 Open Agent Docs（不用代理）：

```sh
OAD_AUTH=jwt OAD_JWT_SECRET='…' open-agent-docs serve          # HS256
OAD_AUTH=jwt OAD_JWT_JWKS_URL=https://auth.example.com/.well-known/jwks.json open-agent-docs serve
# 可选：OAD_JWT_ISSUER、OAD_JWT_AUDIENCE、OAD_JWT_LEEWAY（秒，默认 60）
```

- 带法：`Authorization: Bearer <jwt>`、WebSocket 用 `?token=<jwt>`，或 cookie `oad_token`。`@oad/client` 的 `token` 选项都替你处理了。
- claims：`sub`（必填，用户编号）、`exp`（必填）、`name`、`email`、`workspaces`（工作区 id 的数组，或 `[{ "id": "…", "role": "editor" }]`；不给就进默认工作区）。
