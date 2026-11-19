# @oad/client

Open Agent Docs 的 JS 客户端。浏览器和 Node 18+ 都能用。

```sh
pnpm add @oad/client yjs
```

```ts
import { createClient } from '@oad/client'

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

- 每个 HTTP 接口一个方法（`pages`、`createPage`、`threads`、`search`、`versions`、`trash`、`createShare`、`dbRows`、`patchRows`……），
  没包到的用 `client.request(method, path, body)`。出错抛 `ApiError { status, code, message }`。
- 类型（`Page`、`Me`、`Thread`、`Schema`、`View`、`Row`……）现在是照 `docs/phase*-contract.md` 手写的，在 `src/types.ts`；
  服务器出 OpenAPI 以后换成 `openapi-typescript` 生成的，名字不变。
- 只要 HTTP、想让包小一点：`import { createClient } from '@oad/client/core'`（没有 Yjs、没有 WASM）；
  用到协作文档 / 数据库时 `import { openPage } from '@oad/client/page'`、`import { databasePlugin } from '@oad/client/db'`，
  再 `registerPlugin('openPage', openPage)` / `registerPlugin('database', databasePlugin)`。
- Node 18–21 没有全局 WebSocket：`createClient({ WebSocket: (await import('ws')).WebSocket })`。Node 里 WASM 从磁盘读，不用配。
