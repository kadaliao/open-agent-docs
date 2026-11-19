// @oad/client：Open Agent Docs 的 JS 客户端。浏览器和 Node 18+ 都能用。
//   const client = createClient({ baseUrl: 'https://docs.example.com', token })
//   await client.pages(ws)                      // 每个接口一个方法
//   client.events(ws).on('page.updated', ...)   // 实时事件（SSE，自动重连）
//   const page = client.openPage(id)            // 协作文档：{ ydoc, provider, awareness, destroy }
//   const db = client.database(id); await db.open(); db.engine.query(view)   // 本地数据库（WASM 引擎）
// 只要 HTTP 接口、想让包小一点：import { createClient } from '@oad/client/core'，用到再从 '@oad/client/page'、'@oad/client/db' 拿来登记。
import { createClient as createCore, type ClientOptions, type ClientPlugins } from './core.ts'
import { databasePlugin } from './db/index.ts'
import { openPage } from './page.ts'

export * from './core.ts'
export { openPage } from './page.ts'
export * from './db/index.ts'
export const VERSION = '0.1.0'

/** 带协作文档和本地数据库的客户端 */
export function createClient(opts: ClientOptions = {}, extra: ClientPlugins = {}) {
  return createCore(opts, { openPage, database: databasePlugin, ...extra })
}
