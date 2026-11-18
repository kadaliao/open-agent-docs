// ⚠️ 临时：服务器的数据库接口还没好时，用 Playwright 拦请求、在内存里装一个库（照 docs/phase4-contract.md 的 HTTP）。
// 服务器好了以后 phase4 / db-perf 两个测试改成直接打真服务器，这个文件删掉。
import type { BrowserContext, Page, Route } from '@playwright/test'

type Json = Record<string, any>

export interface MockRow {
  id: string
  seq: number
  position: string
  props: Json
  created_at: number
  updated_at: number
  created_by?: string
  unique_id: number
}

export interface MockDb {
  page: Json
  schema: Json
  views: Json[]
  seq: number
  rows: Map<string, MockRow>
  deleted: { id: string; seq: number }[]
}

let n = 0
const rid = () => 'r' + (++n).toString(36).padStart(7, '0')

export class DbMock {
  dbs = new Map<string, MockDb>()
  // 每个请求多等这么久（模拟网络）
  delay = 0
  // 下一个写请求返回 500（测回滚）
  failNext = false
  writes: { method: string; url: string; body: any }[] = []

  constructor(public ws: string) {}

  add(id: string, title: string, schema: Json, views: Json[], rows: Json[]) {
    const db: MockDb = { page: { id, title, kind: 'database', parent_id: null, position: 'a' + id, workspace_id: this.ws, icon: null }, schema, views, seq: 1, rows: new Map(), deleted: [] }
    rows.forEach((r, i) => {
      const row: MockRow = { id: r.id ?? rid(), seq: 1, position: String(100000 + i), props: r.props, created_at: r.created_at ?? Date.now() - i * 60_000, updated_at: Date.now(), unique_id: i + 1 }
      db.rows.set(row.id, row)
    })
    this.dbs.set(id, db)
    return db
  }

  rowOwner(id: string) {
    for (const db of this.dbs.values()) if (db.rows.has(id)) return db
    return null
  }

  /** 别人改了（测增量同步） */
  remoteSet(dbId: string, rowId: string, set: Json) {
    const db = this.dbs.get(dbId)!
    const r = db.rows.get(rowId)!
    db.seq++
    r.props = { ...r.props, ...set }
    r.seq = db.seq
    return db.seq
  }

  async install(target: BrowserContext | Page) {
    await target.route(/\/api\/v1\/workspaces\/[^/]+\/pages(\?.*)?$/, async (route) => {
      if (route.request().method() !== 'GET') return route.fallback()
      const res = await route.fetch()
      const body = await res.json()
      const extra = [...this.dbs.values()].map((d) => d.page)
      const list = Array.isArray(body) ? body : body.pages
      const ids = new Set(extra.map((p) => p.id))
      const merged = [...list.filter((p: Json) => !ids.has(p.id)), ...extra]
      await route.fulfill({ json: Array.isArray(body) ? merged : { ...body, pages: merged } })
    })
    await target.route(/\/api\/v1\/pages\/([^/?]+)(\?.*)?$/, async (route) => {
      const id = /\/pages\/([^/?]+)/.exec(route.request().url())![1]
      const db = this.dbs.get(id)
      if (db && route.request().method() === 'GET') return route.fulfill({ json: db.page })
      if (db && route.request().method() === 'PATCH') {
        Object.assign(db.page, route.request().postDataJSON())
        return route.fulfill({ json: db.page })
      }
      const owner = this.rowOwner(id)
      if (owner && route.request().method() === 'GET') {
        const res = await route.fetch().catch(() => null)
        const base = res && res.ok() ? await res.json() : { id, position: 'a', workspace_id: this.ws, icon: null }
        return route.fulfill({ json: { ...base, title: owner.rows.get(id)!.props.title ?? '', kind: 'row', parent_id: owner.page.id } })
      }
      return route.fallback()
    })
    await target.route(/\/api\/v1\/workspaces\/[^/]+\/databases$/, async (route) => {
      const b = route.request().postDataJSON() ?? {}
      const id = 'db' + Math.random().toString(36).slice(2, 8)
      const db = this.add(id, b.title ?? '', b.schema ?? { props: [{ key: 'title', name: '名称', type: 'title' }], next_id: 1 }, b.views ?? [{ id: 'v1', name: '表格', type: 'table', filter: { op: 'and', items: [] }, sorts: [], hidden: [], order: [], widths: {} }], [])
      await route.fulfill({ status: 201, json: { page: db.page, schema: db.schema, views: db.views, seq: db.seq } })
    })
    await target.route(/\/api\/v1\/databases\/.*/, (route) => this.handle(route))
  }

  private async handle(route: Route) {
    const req = route.request()
    const url = new URL(req.url())
    const parts = url.pathname.replace(/^\/api\/v1\/databases\//, '').split('/')
    const db = this.dbs.get(parts[0])
    if (this.delay) await new Promise((r) => setTimeout(r, this.delay))
    if (!db) return route.fulfill({ status: 404, json: { error: 'not found' } })
    const method = req.method()
    const body = method === 'GET' ? null : req.postDataJSON()
    if (method !== 'GET') {
      this.writes.push({ method, url: url.pathname, body })
      if (this.failNext) {
        this.failNext = false
        return route.fulfill({ status: 500, json: { error: '模拟的服务器错误' } })
      }
    }
    const what = parts.slice(1).join('/')
    const rows = () => [...db.rows.values()].sort((a, b) => (a.position < b.position ? -1 : 1))
    if (what === '' && method === 'GET') return route.fulfill({ json: { page: db.page, schema: db.schema, views: db.views, seq: db.seq } })
    if (what === 'rows' && method === 'GET') {
      const since = url.searchParams.get('since')
      if (since === null) return route.fulfill({ json: { seq: db.seq, rows: rows(), deleted: [] } })
      const s = Number(since)
      return route.fulfill({ json: { seq: db.seq, rows: rows().filter((r) => r.seq > s), deleted: db.deleted.filter((d) => d.seq > s).map((d) => d.id) } })
    }
    if (what === 'rows' && method === 'POST') {
      const ids: string[] = []
      for (const r of body.rows) {
        db.seq++
        const after = r.after ? db.rows.get(r.after)?.position : undefined
        const row: MockRow = { id: rid(), seq: db.seq, position: after ? after + 'm' : String(100000 + db.rows.size) + 'z', props: r.props ?? {}, created_at: Date.now(), updated_at: Date.now(), unique_id: db.rows.size + 1 }
        db.rows.set(row.id, row)
        ids.push(row.id)
      }
      return route.fulfill({ status: 201, json: { ids, seq: db.seq } })
    }
    if (what === 'rows' && method === 'PATCH') {
      db.seq++
      for (const op of body.ops) {
        const r = db.rows.get(op.row_id)
        if (!r) continue
        if (op.set) {
          const props = { ...r.props }
          for (const [k, v] of Object.entries(op.set)) {
            if (v === null || v === '' || (Array.isArray(v) && !v.length)) delete props[k]
            else props[k] = v
          }
          r.props = props
        }
        if ('after' in op) {
          const list = rows().filter((x) => x.id !== r.id)
          const i = op.after ? list.findIndex((x) => x.id === op.after) : -1
          const a = i >= 0 ? list[i].position : ''
          const b = list[i + 1]?.position
          r.position = b ? a + (b > a + '5' ? '5' : a.length < b.length ? '0' : '05') : a + 'z'
        }
        r.seq = db.seq
        r.updated_at = Date.now()
      }
      return route.fulfill({ json: { seq: db.seq } })
    }
    if (what === 'rows' && method === 'DELETE') {
      db.seq++
      for (const id of body.ids) if (db.rows.delete(id)) db.deleted.push({ id, seq: db.seq })
      return route.fulfill({ json: { seq: db.seq } })
    }
    if (what === 'schema' && method === 'PATCH') {
      db.seq++
      for (const raw of body.ops) {
        const props: Json[] = db.schema.props
        const { op: kind, prop: ref, after, ...fields } = raw
        const op = { op: kind, ...fields }
        const p = props.find((x) => x.key === ref || x.name === ref)
        if (op.op === 'add') {
          const i = after ? props.findIndex((x) => x.key === after) + 1 : props.length
          props.splice(i || props.length, 0, { ...fields, key: 'k' + Math.random().toString(36).slice(2, 6) })
        }
        else if (op.op === 'rename' && p) p.name = op.name
        else if (op.op === 'delete') db.schema.props = props.filter((x) => x !== p)
        else if (op.op === 'change_type' && p) Object.assign(p, { ...op, op: undefined, key: p.key })
        else if (op.op === 'set_options' && p) p.options = op.options
        else if (op.op === 'set_formula' && p) p.formula = op.formula
        else if (op.op === 'update' && p) Object.assign(p, { ...op, op: undefined, key: p.key })
      }
      db.schema.props.forEach((p: Json) => delete p.op)
      return route.fulfill({ json: { schema: db.schema, seq: db.seq } })
    }
    if (what === 'views' && method === 'POST') {
      const v = { ...body, id: body.id ?? 'v' + Math.random().toString(36).slice(2, 7) }
      db.views.push(v)
      return route.fulfill({ status: 201, json: v })
    }
    if (parts[1] === 'views' && parts[2] && method === 'PATCH') {
      const v = db.views.find((x) => x.id === parts[2])
      if (v) Object.assign(v, body)
      return route.fulfill({ json: v ?? {} })
    }
    if (parts[1] === 'views' && parts[2] && method === 'DELETE') {
      db.views = db.views.filter((x) => x.id !== parts[2])
      return route.fulfill({ json: {} })
    }
    if (parts[1] === 'button') return route.fulfill({ json: {} })
    return route.fulfill({ status: 404, json: { error: 'mock: ' + what } })
  }
}

// ---------- 样例数据 ----------
export function taskSchema(relDb?: string) {
  const props: Json[] = [
    { key: 'title', name: '名称', type: 'title' },
    {
      key: 'st',
      name: '状态',
      type: 'status',
      options: [
        { id: 's1', name: '未开始', color: 'gray', group: 'todo' },
        { id: 's2', name: '进行中', color: 'blue', group: 'doing' },
        { id: 's3', name: '评审中', color: 'purple', group: 'doing' },
        { id: 's4', name: '已完成', color: 'green', group: 'done' },
      ],
    },
    {
      key: 'pri',
      name: '优先级',
      type: 'select',
      options: [
        { id: 'p1', name: '紧急', color: 'red' },
        { id: 'p2', name: '高', color: 'orange' },
        { id: 'p3', name: '中', color: 'yellow' },
        { id: 'p4', name: '低', color: 'gray' },
      ],
    },
    {
      key: 'tags',
      name: '标签',
      type: 'multi_select',
      options: [
        { id: 't1', name: '前端', color: 'blue' },
        { id: 't2', name: '服务器', color: 'brown' },
        { id: 't3', name: '性能', color: 'orange' },
        { id: 't4', name: '设计', color: 'pink' },
        { id: 't5', name: 'Agent', color: 'purple' },
        { id: 't6', name: '文档', color: 'green' },
      ],
    },
    { key: 'who', name: '负责人', type: 'person' },
    { key: 'due', name: '截止', type: 'date', date_format: 'zh' },
    { key: 'est', name: '预估（小时）', type: 'number' },
    { key: 'cost', name: '预算', type: 'number', format: 'cny' },
    { key: 'ok', name: '已确认', type: 'checkbox' },
    { key: 'link', name: '链接', type: 'url' },
    { key: 'f', name: '人天', type: 'formula', formula: 'round(prop("预估（小时）") / 8 * 10) / 10' },
    { key: 'files', name: '附件', type: 'files' },
    { key: 'ct', name: '创建时间', type: 'created_time', date_format: 'zh' },
  ]
  if (relDb) props.push({ key: 'rel', name: '关联项目', type: 'relation', relation: { db_id: relDb } })
  return { props, next_id: 20 }
}

export function taskViews() {
  return [
    { id: 'vt', name: '全部任务', type: 'table', filter: { op: 'and', items: [] }, sorts: [], hidden: ['files'], order: [], widths: {}, totals: { est: 'sum', cost: 'sum', ok: 'percent_checked', title: 'count' } },
    { id: 'vb', name: '看板', type: 'board', group_by: 'st', filter: { op: 'and', items: [] }, sorts: [], hidden: ['link', 'ct', 'files', 'cost', 'f', 'ok'], order: [] },
    { id: 'vc', name: '日历', type: 'calendar', calendar_by: 'due', filter: { op: 'and', items: [] }, sorts: [], hidden: [], order: [] },
    { id: 'vg', name: '画廊', type: 'gallery', cover: 'files', card_size: 'medium', filter: { op: 'and', items: [] }, sorts: [], hidden: ['link', 'ct', 'files', 'cost', 'f', 'est', 'ok'], order: [] },
    { id: 'vl', name: '列表', type: 'list', filter: { op: 'and', items: [] }, sorts: [], hidden: ['link', 'ct', 'files', 'cost', 'f', 'est', 'tags'], order: [] },
  ]
}

const TITLES = [
  '首页加载提速',
  '数据库表格视图',
  '看板拖动排序',
  '公式补全',
  'Agent 写入回执',
  '离线编辑冲突合并',
  '评论锚点迁移',
  '移动端侧栏',
  '导出 CSV',
  '图片缩略图服务',
  '搜索结果高亮',
  '权限设置页',
  '通知收件箱',
  '日历周视图',
  '富媒体卡片',
  'WebSocket 重连退避',
  '回收站',
  '模板中心',
  '快捷键说明',
  '深色模式细节',
]

export function taskRows(n: number, people: string[], opts: { ids?: string[]; month?: Date; images?: string[] } = {}) {
  let s = 7
  const rnd = () => ((s = (s * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff)
  const pick = <T>(a: T[]) => a[Math.floor(rnd() * a.length)]
  const base = opts.month ?? new Date()
  const y = base.getFullYear()
  const m = base.getMonth()
  const pad = (x: number) => String(x).padStart(2, '0')
  const out: Json[] = []
  for (let i = 0; i < n; i++) {
    const props: Json = { title: n <= TITLES.length ? TITLES[i] : `${pick(TITLES)} #${i + 1}` }
    const r = rnd()
    props.st = r < 0.3 ? 's1' : r < 0.6 ? 's2' : r < 0.72 ? 's3' : 's4'
    if (rnd() < 0.85) props.pri = pick(['p1', 'p2', 'p2', 'p3', 'p3', 'p3', 'p4'])
    const tags = ['t1', 't2', 't3', 't4', 't5', 't6'].filter(() => rnd() < 0.28)
    if (tags.length) props.tags = tags
    if (people.length && rnd() < 0.9) props.who = [pick(people)]
    if (rnd() < 0.8) {
      const d = 1 + Math.floor(rnd() * 28)
      const start = `${y}-${pad(m + 1)}-${pad(d)}`
      props.due = rnd() < 0.15 ? { start, end: `${y}-${pad(m + 1)}-${pad(Math.min(28, d + 1 + Math.floor(rnd() * 3)))}` } : { start }
    }
    if (rnd() < 0.9) props.est = Math.round(rnd() * 40) + 1
    if (rnd() < 0.7) props.cost = Math.round(rnd() * 5000) * 10
    if (rnd() < 0.4) props.ok = true
    if (rnd() < 0.3) props.link = `https://github.com/kadaliao/open-agent-docs/issues/${100 + i}`
    if (opts.images?.length && rnd() < 0.85) props.files = [{ url: pick(opts.images), name: 'cover.svg' }]
    out.push({ id: opts.ids?.[i], props, created_at: Date.now() - i * 3_600_000 })
  }
  return out
}
