// @open-agent-docs/client/db：本地数据库（本机缓存 + 增量同步 + WASM 引擎）。import 进来以后 client.database(id) 就能用。
export { DbStore, type EventSource_ as EventsLike, type Status as DbStatus } from './store.ts'
export { createEngine, preloadEngine, safeQuery, tryNormalize } from './engine.ts'
import { DbStore as Store } from './store.ts'
import type { OadClient } from '../core.ts'
/** 给 createClient(opts, { database }) 或 registerPlugin('database', …) 用 */
export const databasePlugin = (c: OadClient, id: string) => new Store(c, id)
