// 计算引擎：@oad/db（packages/db-wasm，和服务器同一份 Rust 代码，按需下载）。
import type { DbEngine, QueryResult, Schema, View } from './types'

type Mod = { default?: unknown; DbEngine: { create(schema: Schema, opts?: { id?: string }): Promise<DbEngine> } }
let mod: Promise<Mod> | null = null

/** 引擎先下载好（打开库页面之前就可以开始） */
export const preloadEngine = () =>
  (mod ??= import('@oad/db').then(async (m) => {
    const x = m as unknown as Mod
    // wasm-bindgen 打的包要先 init
    if (typeof x.default === 'function') await (x.default as () => Promise<unknown>)()
    return x
  }))

export async function createEngine(schema: Schema, id?: string): Promise<DbEngine> {
  const m = await preloadEngine()
  return m.DbEngine.create(schema, { id })
}

/** 引擎认不出的输入会抛错（中文说明）；这里接住 */
export function tryNormalize(eng: DbEngine | null, key: string, input: unknown): { value: unknown } | { error: string } {
  if (!eng) return { value: input === '' ? null : input }
  try {
    return { value: eng.normalize(key, input) }
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) }
  }
}

/** 查询出错（条件不完整、属性删了）：先去掉筛选再查，错误交给界面显示 */
export function safeQuery(eng: DbEngine, view: Partial<View> & { search?: string }): QueryResult & { error?: string } {
  try {
    return eng.query(cleanView(view))
  } catch (e) {
    const error = e instanceof Error ? e.message : String(e)
    try {
      return { ...eng.query({ ...cleanView(view), filter: null }), error }
    } catch {
      return { ids: [], error }
    }
  }
}

// 还没填值的条件（刚加上的）先不算
function cleanView(view: Partial<View> & { search?: string }) {
  if (!view.filter) return view
  const NO_VALUE = new Set(['empty', 'not_empty', 'checked', 'unchecked'])
  type F = NonNullable<View['filter']>
  const clean = (f: F): F | null => {
    const items = f.items
      .map((x) => ('items' in x ? clean(x as F) : NO_VALUE.has(x.cond) || (x.value !== undefined && x.value !== '' && x.value !== null) ? x : null))
      .filter(Boolean) as F['items']
    return items.length ? { ...f, items } : null
  }
  return { ...view, filter: clean(view.filter) }
}
