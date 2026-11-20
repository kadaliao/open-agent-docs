# @open-agent-docs/client

Open Agent Docs 的 JS 客户端。浏览器和 Node 18+ 都能用。

```sh
pnpm add @open-agent-docs/client yjs
```

```ts
import { createClient } from '@open-agent-docs/client'

// 浏览器里同源：不用 token，靠登录的 cookie；服务端 / Agent：给 token
const client = createClient({ baseUrl: 'https://docs.example.com', token: process.env.OAD_TOKEN })

const me = await client.me()
const ws = me.workspaces[0].id
const page = await client.createPage(ws, { title: '周报' })

// 实时事件（SSE，断了自动重连，带 Last-Event-ID）
const events = client.events(ws)
events.on('page.updated', (p) => console.log('改了', p.title))

// 协作文档：Y.Doc + WebSocket 同步（浏览器里还有 IndexedDB 缓存）
const doc = client.openPage(page.id) // { ydoc, provider, awareness, ready, status(), destroy() }
await doc.ready

// 本地数据库：本机缓存 + 增量同步 + WASM 引擎（和服务器同一份 Rust 代码）
const db = client.database(dbId)
await db.open(events)
const { ids, groups, totals } = db.engine!.query({ sorts: [{ prop: 'due', dir: 'asc' }], group_by: 'status' })
await db.setCells([{ row_id: ids[0], key: 'status', value: 'done' }]) // 先改本地，失败自动改回去
db.subscribe(() => render()) // 有变化
```

- 每个 HTTP 接口一个方法（`pages`、`createPage`、`content`、`threads`、`search`、`versions`、`trash`、`queryDatabase`、`agentNotifications`……）。
  路径、参数、请求体、响应的类型都从 `docs/openapi.json` 来：`client.call('get', '/pages/{id}', { path: { id } })` 能调文档里的任何接口，
  写错路径、方法、字段编译就报错；`client.request(method, path, body)` 不检查。每个接口在别的语言里叫什么，看 `sdks/operations.json` 的 `js` 字段。
- 分页的列表（`versions`、`dbHistory`、`trash`、`inbox`、`agentNotifications`）返回一页（带 `next_cursor`）；
  要全部的用 `for await (const v of client.eachVersion(pageId))`（还有 `eachDbHistory`、`eachTrash`、`eachInbox`、`eachAgentNotification`），
  按游标一页页取，`break` 了就不再取。
- 类型（`Page`、`Me`、`Thread`、`DbRow`、`Version`……）是 `src/openapi.ts` 里生成的（`pnpm --filter @open-agent-docs/client gen:types`；
  过期了 `test/openapi-fresh.test.ts` 会失败）。文档里只写了 `object` 的（表结构 `Schema`、视图 `View` 等）在 `src/types.ts` 手写。
  `OAD_BIN=… pnpm test` 还会起真服务器，把 60 多个接口的响应按文档检查一遍（`test/contract.test.ts`）。
- 只要 HTTP、想让包小一点：`import { createClient } from '@open-agent-docs/client/core'`（没有 Yjs、没有 WASM）；
  用到协作文档 / 数据库时 `import { openPage } from '@open-agent-docs/client/page'`、`import { databasePlugin } from '@open-agent-docs/client/db'`，
  再 `registerPlugin('openPage', openPage)` / `registerPlugin('database', databasePlugin)`。
- Node 18–21 没有全局 WebSocket：`createClient({ WebSocket: (await import('ws')).WebSocket })`。Node 里 WASM 从磁盘读，不用配。

## 重试、超时、错误

```ts
const client = createClient({ baseUrl, token, maxRetries: 2, timeout: 60_000 }) // 这两个是默认值
```

- 重试（最多 `maxRetries` 次）：429；502、503、504（POST、PATCH 除外，可能已经生效了）；连不上（GET、PUT、DELETE，或者确定请求没发出去）。
  有 `Retry-After` 照它等（最多 60 秒），没有就 0.5 秒、1 秒、2 秒……最多 8 秒，加一点随机。
- `timeout` 是单个请求的超时（毫秒，读完响应为止）；事件流只限连接，不限总时长。
- 服务器回了错误抛 `ApiError { status, code, message, retryAfter?, requestId? }`；连不上抛 `ApiConnectionError`，超时抛 `ApiTimeoutError`（它的子类）。
  判断用函数，不用记状态码：`isNotFound(e)`、`isUnauthorized`、`isForbidden`、`isConflict`、`isGone`、`isBadRequest`、`isTooLarge`、`isRateLimited`、`isServerError`、`isConnectionError`、`isTimeout`。

```ts
import { isNotFound } from '@open-agent-docs/client'
const page = await client.page(id).catch((e) => (isNotFound(e) ? null : Promise.reject(e)))
```

- 事件流（`client.events(ws)`、`client.agentEvents()`）遇到 401、403、404 不重连，`events.onError((e) => …)` 拿到错误。

## 只读 HTML、嵌入地址

```ts
const { html, version } = await client.content(pageId, 'html') // <div class="oad-content">…</div>，样式表是服务器的 /embed/content.css
import { embedUrl } from '@open-agent-docs/client'
iframe.src = embedUrl('https://docs.example.com', pageId, { theme: 'dark', lang: 'en', token: jwt }) // token 放在 # 后面
```

## 服务器端：`@open-agent-docs/client/node`

要密钥、要 `node:crypto` 的东西放在这个入口（浏览器里别引）。

```ts
import { createClient, signJwt, verifyWebhook } from '@open-agent-docs/client/node'

// baseUrl 默认 OAD_BASE_URL（再没有 http://127.0.0.1:8848），token 默认 OAD_TOKEN，不带 cookie
const admin = createClient({ baseUrl, headerSecret: process.env.OAD_HEADER_SECRET }) // OAD_AUTH=header 的服务器
// const admin = createClient({ baseUrl, jwtSecret: process.env.OAD_JWT_SECRET })   // OAD_AUTH=jwt 的服务器（HS256）

const u = admin.withUser('u-42', { name: '王小明', email: 'x@example.com' }) // 新的客户端，原来的不变
await u.pages(await u.defaultWorkspace())

// 给浏览器 / iframe 的 JWT（claims：sub、name、email、workspaces、iat、exp、iss、aud）
const jwt = signJwt(secret, 'u-42', { name: '王小明', workspaces: [{ id: ws, role: 'editor' }], ttl: 3600 })

// Agent 的 webhook：body 要原始正文（express.raw()、await req.text()……）
app.post('/oad-webhook', express.raw({ type: '*/*' }), (req, res) => {
  try {
    const note = verifyWebhook(process.env.OAD_WEBHOOK_SECRET!, req.headers, req.body) // AgentNotification
    queue.push(note)
    res.sendStatus(204)
  } catch (e) {
    res.status(400).send((e as Error).message) // WebhookVerificationError，e.kind 是 malformed / stale / signature
  }
})
```

- `withUser`：header 模式带 `X-OAD-Secret`、`X-OAD-User-Id`、`X-OAD-User-Name`（百分号编码）、`X-OAD-User-Email`；JWT 模式签一个，过期前 60 秒换新的。
  两个密钥都给或者都没给，抛 `OadConfigError`；header 模式不支持 `workspaces`。
- `verifyWebhook(secret, headers, body, { tolerance: 300, now })`：`headers` 是 Node 的 `req.headers` 或者 `Headers`，名字大小写不论。

## 测试

- `pnpm test`：单元测试；`test/conformance.test.ts` 是各语言共用的场景（`docs/sdk-contract.md` §14），
  有进程内服务器（`packages/server-node` 编好了）或者设了 `OAD_BINARY=target/debug/open-agent-docs` 就会跑，不然跳过要服务器的那几条。
