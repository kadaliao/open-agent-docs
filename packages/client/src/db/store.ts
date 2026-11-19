// 一个数据库在网页里的数据：先从 IndexedDB 出（离线、再打开很快），再向服务器要增量（rows?since=）；
// 本地改动先显示（乐观），服务器拒了就回滚；别人改了经工作区 SSE 的 db.changed 通知再拉增量。
import { registerPlugin, type OadClient } from '../core.ts'
import type { DbEngine, DbMeta, Prop, Row, Schema, View } from '../types.ts'
import { createEngine, preloadEngine } from './engine.ts'

/** 能订阅工作区事件的东西：client.events(ws)，或者网页自己的页面列表 */
export interface EventSource_ {
  on(name: string, f: (data: any) => void): () => void
}

// 浏览器和 Node 都能跑：没有 requestAnimationFrame 就用 setTimeout
const nextFrame = (f: () => void) => (typeof requestAnimationFrame === 'function' ? requestAnimationFrame(f) : setTimeout(f, 16))

type Listener = () => void

// ---------- IndexedDB ----------
let dbp: Promise<IDBDatabase | null> | null = null
// 打开以后留一份，关页面前能同步地开事务
let opened: IDBDatabase | null = null
function idb() {
  dbp ??= new Promise((resolve) => {
    try {
      const r = indexedDB.open('oad:db', 1)
      r.onupgradeneeded = () => r.result.createObjectStore('d')
      r.onsuccess = () => resolve((opened = r.result))
      r.onerror = () => resolve(null)
    } catch {
      resolve(null)
    }
  })
  return dbp
}
// 行存成一个 JSON 字符串：读出来只拷一个字符串（不是 1 万个对象），引擎也直接吃字符串，省一次转换
interface Cached {
  meta: DbMeta
  rows?: Row[]
  json?: string
  seq: number
}
async function readCache(id: string): Promise<Cached | null> {
  const d = await idb()
  if (!d) return null
  return new Promise((resolve) => {
    try {
      const r = d.transaction('d').objectStore('d').get(id)
      r.onsuccess = () => resolve(r.result ?? null)
      r.onerror = () => resolve(null)
    } catch {
      resolve(null)
    }
  })
}
async function writeCache(id: string, v: Cached) {
  const d = opened ?? (await idb())
  try {
    d?.transaction('d', 'readwrite').objectStore('d').put(v, id)
  } catch {
    /* 存不下就算了 */
  }
}

let tmp = 0
const tempId = () => `tmp_${Date.now().toString(36)}_${++tmp}`
const randKey = () => Math.random().toString(36).slice(2, 8)

const byPosition = (a: Row, b: Row) => {
  const x = a.position ?? '\uffff'
  const y = b.position ?? '\uffff'
  return x < y ? -1 : x > y ? 1 : 0
}

export type Status = 'loading' | 'cache' | 'ready' | 'error'

const all = new Set<DbStore>()
let hooked = false

/** 一个数据库在本机的样子。用 client.database(id) 拿（同一个 id 是同一个对象） */
export class DbStore {

  meta: DbMeta | null = null
  rows = new Map<string, Row>()
  seq = 0
  engine: DbEngine | null = null
  status: Status = 'loading'
  error: string | null = null
  version = 0
  // 打开花了多久（性能测试用）
  timing: Record<string, number> = {}
  private listeners = new Set<Listener>()
  private opening: Promise<void> | null = null
  private syncing: Promise<void> | null = null
  private saveTimer: ReturnType<typeof setTimeout> | undefined = undefined
  private dirty = false
  private offs: (() => void)[] = []
  private related = new Set<string>()
  private events?: EventSource_
  private listening = false

  constructor(
    public client: OadClient,
    public id: string,
  ) {
    all.add(this)
    // 关页面前把还没写进缓存的写进去
    if (!hooked && typeof window !== 'undefined' && typeof window.addEventListener === 'function') {
      hooked = true
      window.addEventListener('pagehide', () => all.forEach((s) => s.flush()))
    }
  }

  subscribe = (l: Listener) => {
    this.listeners.add(l)
    return () => this.listeners.delete(l)
  }
  getVersion = () => this.version

  private emit() {
    this.version++
    this.listeners.forEach((l) => l())
  }

  private save() {
    clearTimeout(this.saveTimer)
    this.dirty = true
    this.saveTimer = setTimeout(() => this.flush(), 300)
  }

  /** 马上写进 IndexedDB（关页面前也写一次） */
  flush() {
    clearTimeout(this.saveTimer)
    if (!this.dirty || !this.meta) return
    this.dirty = false
    writeCache(this.id, { meta: this.meta, json: JSON.stringify([...this.rows.values()].filter((r) => !r.id.startsWith('tmp_'))), seq: this.seq })
  }

  get schema(): Schema {
    return this.meta?.schema ?? { props: [] }
  }
  get views(): View[] {
    return this.meta?.views ?? []
  }
  prop(key: string): Prop | undefined {
    return this.meta?.schema.props.find((p) => p.key === key)
  }

  /** 打开：缓存先出，再和服务器同步。可以多次调用 */
  open(events?: EventSource_) {
    if (events) this.events = events
    if (events && !this.listening) {
      this.listening = true
      this.offs.push(
        events.on('db.changed', (d) => d?.db_id === this.id && (d.seq ?? Infinity) > this.seq && this.sync()),
        events.on('db.schema', (d) => d?.db_id === this.id && this.refreshMeta()),
        events.on('resync', () => this.sync()),
      )
    }
    this.opening ??= this.doOpen()
    return this.opening
  }

  private async doOpen() {
    const t0 = performance.now()
    this.timing.start = t0
    // 引擎（WASM）和数据一起下载
    preloadEngine()
    const cached = await readCache(this.id)
    this.timing.idb = performance.now() - t0
    if (cached && !this.meta) {
      // 先让首屏快照画出来（引擎装行、画表格要占主线程几十毫秒）
      await new Promise((r) => nextFrame(() => setTimeout(r, 0)))
    }
    const t1 = performance.now()
    if (cached && !this.meta) {
      this.meta = cached.meta
      const list: Row[] = cached.rows ?? JSON.parse(cached.json ?? '[]')
      this.rows = new Map(list.map((r) => [r.id, r]))
      this.seq = cached.seq
      this.engine = await createEngine(cached.meta.schema, this.id)
      this.timing.engineReady = performance.now() - t0
      this.engine.loadRows(cached.json ?? list)
      this.timing.loadRows = performance.now() - t1
      this.status = 'cache'
      this.timing.cache = performance.now() - t0
      this.emit()
    }
    try {
      const [meta, rows] = await Promise.all([this.client.getDatabase(this.id), this.client.dbRows(this.id, cached ? cached.seq : undefined)])
      this.timing.fetch = performance.now() - t0
      this.setMeta(meta)
      if (!cached || !this.engine) {
        this.rows = new Map(rows.rows.map((r) => [r.id, r]))
        this.engine ??= await createEngine(meta.schema, this.id)
        this.timing.engine = performance.now() - t0
        this.engine.setSchema(meta.schema)
        this.engine.loadRows(rows.rows)
      } else this.merge(rows.rows, rows.deleted ?? [])
      this.seq = Math.max(this.seq, rows.seq)
      this.status = 'ready'
      this.error = null
      this.timing.ready = performance.now() - t0
      this.emit()
      // 第一次拿到全量：画出来以后马上写缓存，别等
      this.dirty = true
      nextFrame(() => setTimeout(() => this.flush(), 0))
      this.loadRelated()
    } catch (e) {
      if (this.status !== 'cache') this.status = 'error'
      this.error = e instanceof Error ? e.message : String(e)
      this.emit()
    }
  }

  private setMeta(meta: DbMeta) {
    const schemaChanged = JSON.stringify(meta.schema) !== JSON.stringify(this.meta?.schema)
    this.meta = meta
    if (schemaChanged && this.engine) this.engine.setSchema(meta.schema)
  }

  async refreshMeta() {
    try {
      this.setMeta(await this.client.getDatabase(this.id))
      this.emit()
      this.save()
      this.loadRelated()
    } catch {
      /* 下次再说 */
    }
  }

  /** 关联、汇总要用到的别的库 */
  private loadRelated() {
    for (const p of this.schema.props) {
      const other = p.relation?.db_id
      if (!other || other === this.id || this.related.has(other)) continue
      this.related.add(other)
      const s = this.client.database(other)
      // 引擎不会自己知道对方库变了：对方一变（它自己收 db.changed 增量）就把它的行重新给引擎；一帧里只做一次
      let queued = false
      const feed = () => {
        if (queued) return
        queued = true
        nextFrame(() => {
          queued = false
          if (s.meta && this.engine) {
            this.engine.loadRelated(other, s.meta.schema, [...s.rows.values()])
            this.emit()
          }
        })
      }
      this.offs.push(s.subscribe(feed))
      s.open(this.events).then(feed)
    }
  }

  private merge(rows: Row[], deleted: string[]) {
    const changes: { row_id: string; row?: Row; deleted?: boolean }[] = []
    // 位置变了（别人拖动排序）或者新行不在最后：整个按位置重排
    let reorder = false
    const last = [...this.rows.values()].pop()?.position
    for (const r of rows) {
      const old = this.rows.get(r.id)
      if (old ? old.position !== r.position : last !== undefined && r.position !== undefined && r.position < last) reorder = true
      this.rows.set(r.id, r)
      changes.push({ row_id: r.id, row: r })
    }
    for (const id of deleted) {
      this.rows.delete(id)
      changes.push({ row_id: id, deleted: true })
    }
    if (reorder) {
      this.rows = new Map([...this.rows.entries()].sort(([, a], [, b]) => byPosition(a, b)))
      this.engine?.loadRows([...this.rows.values()])
    } else if (changes.length) this.engine?.apply(changes)
  }

  /** 拉增量 */
  sync() {
    if (this.syncing) return this.syncing
    this.syncing = (async () => {
      try {
        const r = await this.client.dbRows(this.id, this.seq)
        this.merge(r.rows, r.deleted ?? [])
        this.seq = Math.max(this.seq, r.seq)
        this.emit()
        this.save()
      } catch {
        /* 断网：等下次 */
      } finally {
        this.syncing = null
      }
    })()
    return this.syncing
  }

  // ---------- 写（乐观） ----------
  private fail(e: unknown) {
    this.error = e instanceof Error ? e.message : String(e)
    this.emit()
    setTimeout(() => {
      this.error = null
      this.emit()
    }, 4000)
  }

  /** 改格子：先改本地，服务器拒了改回去 */
  async setCells(ops: { row_id: string; key: string; value: unknown }[]) {
    if (!ops.length) return
    const before = new Map<string, Record<string, unknown>>()
    const byRow = new Map<string, Record<string, unknown>>()
    for (const o of ops) {
      const r = this.rows.get(o.row_id)
      if (!r) continue
      if (!before.has(o.row_id)) before.set(o.row_id, { ...r.props })
      const set = byRow.get(o.row_id) ?? {}
      set[o.key] = o.value === undefined ? null : o.value
      byRow.set(o.row_id, set)
    }
    const apply = (rowId: string, set: Record<string, unknown>) => {
      const r = this.rows.get(rowId)!
      const props = { ...r.props }
      for (const [k, v] of Object.entries(set)) {
        if (v === null || v === '' || (Array.isArray(v) && !v.length)) delete props[k]
        else props[k] = v
      }
      this.rows.set(rowId, { ...r, props, updated_at: Date.now() })
    }
    byRow.forEach((set, id) => apply(id, set))
    this.engine?.apply([...byRow.entries()].map(([row_id, props]) => ({ row_id, props })))
    this.emit()
    const real = [...byRow.entries()].filter(([id]) => !id.startsWith('tmp_'))
    if (!real.length) return
    try {
      const r = await this.client.patchRows(
        this.id,
        real.map(([row_id, set]) => ({ row_id, set })),
      )
      this.seq = Math.max(this.seq, r.seq ?? this.seq)
      this.save()
    } catch (e) {
      // 回滚
      before.forEach((props, id) => {
        const r = this.rows.get(id)
        if (r) this.rows.set(id, { ...r, props })
      })
      this.engine?.apply([...before.entries()].map(([row_id, props]) => ({ row_id, row: this.rows.get(row_id)! })))
      this.fail(e)
    }
  }

  /** 加一行，返回本地编号（服务器给了真编号以后换掉，onId 告诉调用方） */
  addRow(props: Record<string, unknown> = {}, after?: string | null, onId?: (id: string) => void) {
    const id = tempId()
    const row: Row = { id, props, created_at: Date.now(), updated_at: Date.now() }
    this.rows.set(id, row)
    this.engine?.apply([{ row_id: id, row }])
    this.emit()
    this.client.addRows(this.id, [{ props, after }]).then(
      (r) => {
        const real = r.ids[0]
        // 服务器只回编号（{ ids, seq }）：它存的就是发过去的那些属性
        const got: Row = { ...this.rows.get(id)!, id: real, props: { ...props } }
        // 等服务器的这段时间里本地又改了的格子保留
        const cur = this.rows.get(id)
        this.rows.delete(id)
        const merged: Row = { ...got, id: real, props: { ...got.props, ...(cur?.props ?? {}) } }
        this.rows.set(real, merged)
        this.engine?.apply([
          { row_id: id, deleted: true },
          { row_id: real, row: merged },
        ])
        this.seq = Math.max(this.seq, r.seq ?? this.seq)
        // 本地改过的属性补发给服务器
        const extra = Object.fromEntries(Object.entries(cur?.props ?? {}).filter(([k, v]) => JSON.stringify(got.props[k]) !== JSON.stringify(v)))
        if (Object.keys(extra).length) this.client.patchRows(this.id, [{ row_id: real, set: extra }]).catch(() => {})
        this.emit()
        this.save()
        onId?.(real)
      },
      (e) => {
        this.rows.delete(id)
        this.engine?.apply([{ row_id: id, deleted: true }])
        this.fail(e)
      },
    )
    return id
  }

  /** 行页面里改了标题（标题在行页面的协作文档里，服务器同步到 props.title）：只改本地显示，不再发一遍 */
  localTitle(id: string, title: string) {
    const r = this.rows.get(id)
    if (!r || (r.props.title ?? '') === title) return
    const props = { ...r.props }
    if (title) props.title = title
    else delete props.title
    this.rows.set(id, { ...r, props })
    this.engine?.apply([{ row_id: id, props: { title: title || null } }])
    this.emit()
  }

  /** 手动排序（看板列内拖动）：放到 after 后面，null 是最前面。约定外加的：PATCH rows 的 op 带 after */
  async moveRow(id: string, after: string | null) {
    const order = [...this.rows.keys()].filter((x) => x !== id)
    const old = [...this.rows.keys()]
    const at = after ? order.indexOf(after) + 1 : 0
    order.splice(at, 0, id)
    const reorder = (ids: string[]) => {
      const m = new Map<string, Row>()
      for (const k of ids) m.set(k, this.rows.get(k)!)
      this.rows = m
      this.engine?.loadRows([...m.values()])
      this.emit()
    }
    reorder(order)
    if (id.startsWith('tmp_')) return
    try {
      const r = await this.client.patchRows(this.id, [{ row_id: id, after }])
      this.seq = Math.max(this.seq, r.seq ?? this.seq)
      this.save()
    } catch (e) {
      reorder(old)
      this.fail(e)
    }
  }

  async deleteRows(ids: string[]) {
    const gone = ids.map((id) => this.rows.get(id)).filter(Boolean) as Row[]
    for (const r of gone) this.rows.delete(r.id)
    this.engine?.apply(gone.map((r) => ({ row_id: r.id, deleted: true })))
    this.emit()
    try {
      await this.client.deleteRows(
        this.id,
        gone.map((r) => r.id).filter((x) => !x.startsWith('tmp_')),
      )
      this.save()
    } catch (e) {
      for (const r of gone) this.rows.set(r.id, r)
      this.engine?.apply(gone.map((r) => ({ row_id: r.id, row: r })))
      this.fail(e)
    }
  }

  // ---------- 表结构 ----------
  async schemaOps(ops: Record<string, unknown>[], optimistic?: (s: Schema) => Schema) {
    const old = this.meta?.schema
    if (optimistic && this.meta && old) {
      this.meta = { ...this.meta, schema: optimistic(structuredClone(old)) }
      this.engine?.setSchema(this.meta.schema)
      this.emit()
    }
    try {
      const r = await this.client.patchSchema(this.id, ops)
      if (this.meta) this.meta = { ...this.meta, schema: r.schema }
      this.engine?.setSchema(r.schema)
      this.seq = Math.max(this.seq, r.seq ?? this.seq)
      this.emit()
      this.save()
      this.loadRelated()
      return r.schema
    } catch (e) {
      if (old && this.meta) {
        this.meta = { ...this.meta, schema: old }
        this.engine?.setSchema(old)
      }
      this.fail(e)
      return null
    }
  }

  /** 加属性；key 由服务器发，返回它（失败是 null） */
  async addProp(p: Omit<Prop, 'key'> & { key?: string }, after?: string) {
    const tmpKey = p.key ?? 'tmp' + randKey()
    const { key: _k, ...fields } = p as Prop
    const before = new Set(this.schema.props.map((x) => x.key))
    const schema = await this.schemaOps([{ op: 'add', ...fields, ...(after ? { after } : {}) }], (s) => {
      const i = after ? s.props.findIndex((x) => x.key === after) + 1 : s.props.length
      s.props.splice(i || s.props.length, 0, { ...fields, key: tmpKey } as Prop)
      return s
    })
    return schema?.props.find((x) => !before.has(x.key) && x.name === p.name)?.key ?? schema?.props.find((x) => !before.has(x.key))?.key ?? null
  }

  renameProp(key: string, name: string) {
    return this.schemaOps([{ op: 'rename', prop: key, name }], (s) => {
      const p = s.props.find((x) => x.key === key)
      if (p) p.name = name
      return s
    })
  }

  deleteProp(key: string) {
    return this.schemaOps([{ op: 'delete', prop: key }], (s) => ({ ...s, props: s.props.filter((x) => x.key !== key) }))
  }

  changeType(key: string, type: Prop['type'], extra: Partial<Prop> = {}) {
    return this.schemaOps([{ op: 'change_type', prop: key, type, ...extra }], (s) => {
      const p = s.props.find((x) => x.key === key)
      if (p) Object.assign(p, { type }, extra)
      return s
    })
  }

  setOptions(key: string, options: NonNullable<Prop['options']>) {
    return this.schemaOps([{ op: 'set_options', prop: key, options }], (s) => {
      const p = s.props.find((x) => x.key === key)
      if (p) p.options = options
      return s
    })
  }

  setFormula(key: string, formula: string) {
    return this.schemaOps([{ op: 'set_formula', prop: key, formula }], (s) => {
      const p = s.props.find((x) => x.key === key)
      if (p) p.formula = formula
      return s
    })
  }

  updateProp(key: string, patch: Partial<Prop>) {
    return this.schemaOps([{ op: 'update', prop: key, ...patch }], (s) => {
      const p = s.props.find((x) => x.key === key)
      if (p) Object.assign(p, patch)
      return s
    })
  }

  // ---------- 视图 ----------
  async updateView(viewId: string, patch: Partial<View>) {
    if (!this.meta) return
    const old = this.meta.views
    this.meta = { ...this.meta, views: old.map((v) => (v.id === viewId ? { ...v, ...patch } : v)) }
    this.emit()
    try {
      await this.client.patchView(this.id, viewId, patch)
      this.save()
    } catch (e) {
      if (this.meta) this.meta = { ...this.meta, views: old }
      this.fail(e)
    }
  }

  async addView(view: Omit<View, 'id'>) {
    if (!this.meta) return null
    const local: View = { ...view, id: 'v' + randKey() }
    const old = this.meta.views
    this.meta = { ...this.meta, views: [...old, local] }
    this.emit()
    try {
      const v = await this.client.addView(this.id, local)
      if (this.meta) this.meta = { ...this.meta, views: this.meta.views.map((x) => (x.id === local.id ? { ...local, ...v } : x)) }
      this.emit()
      this.save()
      return v?.id ?? local.id
    } catch (e) {
      if (this.meta) this.meta = { ...this.meta, views: old }
      this.fail(e)
      return null
    }
  }

  async deleteView(viewId: string) {
    if (!this.meta || this.meta.views.length <= 1) return
    const old = this.meta.views
    this.meta = { ...this.meta, views: old.filter((v) => v.id !== viewId) }
    this.emit()
    try {
      await this.client.deleteView(this.id, viewId)
      this.save()
    } catch (e) {
      if (this.meta) this.meta = { ...this.meta, views: old }
      this.fail(e)
    }
  }

  close() {
    this.offs.forEach((f) => f())
    this.offs = []
    this.related.clear()
    this.listening = false
  }
}

registerPlugin('database', (c, id) => new DbStore(c, id))
