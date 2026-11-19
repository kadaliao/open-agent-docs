// client.database(id)：真的 WASM 引擎（Node 里从磁盘读），假的服务器（fetch）。
// 打开、查询（筛选 / 排序 / 分组 / 汇总）、改格子（先改本地，失败改回去）、加行（临时编号换成真的）、删行、增量同步、事件触发同步、表结构改动失败回滚、手动排序。
import { beforeEach, describe, expect, it } from 'vitest'
import { createClient, type Row, type Schema } from '../src/index'
import { fakeFetch, json } from './fake-fetch'

const schema: Schema = {
  props: [
    { key: 'title', name: '名称', type: 'title' },
    { key: 'st', name: '状态', type: 'select', options: [{ id: 'a', name: '进行中', color: 'blue' }, { id: 'b', name: '已完成', color: 'green' }] },
    { key: 'n', name: '数', type: 'number' },
  ],
}

function fakeServer() {
  let seq = 3
  let rows: Row[] = [
    { id: 'r1', seq: 1, position: 'a', props: { title: '一', st: 'a', n: 5 } },
    { id: 'r2', seq: 2, position: 'b', props: { title: '二', st: 'b', n: 1 } },
    { id: 'r3', seq: 3, position: 'c', props: { title: '三', st: 'a', n: 9 } },
  ]
  const deleted: { id: string; seq: number }[] = []
  let fail = false
  let nextId = 10
  const ff = fakeFetch((c) => {
    const u = new URL(c.url, 'http://x')
    const path = u.pathname.replace('/api/v1', '')
    if (fail && c.method !== 'GET') {
      fail = false
      return json({ error: { code: 'internal', message: '模拟的错误' } }, 500)
    }
    if (path === '/databases/db1' && c.method === 'GET') return json({ page: { id: 'db1', title: '库' }, schema, views: [{ id: 'v1', name: '表格', type: 'table' }], seq })
    if (path === '/databases/db1/rows' && c.method === 'GET') {
      const since = u.searchParams.get('since')
      if (since === null) return json({ seq, rows, deleted: [] })
      return json({ seq, rows: rows.filter((r) => (r.seq ?? 0) > Number(since)), deleted: deleted.filter((d) => d.seq > Number(since)).map((d) => d.id) })
    }
    if (path === '/databases/db1/rows' && c.method === 'PATCH') {
      seq++
      for (const op of (c.body as { ops: { row_id: string; set?: Record<string, unknown> }[] }).ops) {
        const r = rows.find((x) => x.id === op.row_id)
        if (r && op.set) Object.assign(r.props, op.set)
        if (r) r.seq = seq
      }
      return json({ seq })
    }
    if (path === '/databases/db1/rows' && c.method === 'POST') {
      seq++
      const ids = (c.body as { rows: { props: Record<string, unknown> }[] }).rows.map((x) => {
        const id = 'r' + nextId++
        rows.push({ id, seq, position: 'z', props: { ...x.props } })
        return id
      })
      return json({ ids, seq }, 201)
    }
    if (path === '/databases/db1/rows' && c.method === 'DELETE') {
      seq++
      for (const id of (c.body as { ids: string[] }).ids) {
        rows = rows.filter((r) => r.id !== id)
        deleted.push({ id, seq })
      }
      return json({ seq })
    }
    if (path === '/databases/db1/schema' && c.method === 'PATCH') return json({ schema, seq })
    return json({ error: { code: 'not_found', message: path } }, 404)
  })
  return {
    ...ff,
    get rows() {
      return rows
    },
    failNext: () => (fail = true),
    /** 别的客户端改了一格 */
    remote: (id: string, set: Record<string, unknown>) => {
      seq++
      const r = rows.find((x) => x.id === id)!
      Object.assign(r.props, set)
      r.seq = seq
    },
    remoteDelete: (id: string) => {
      seq++
      rows = rows.filter((r) => r.id !== id)
      deleted.push({ id, seq })
    },
  }
}

const until = async (f: () => boolean, ms = 5000) => {
  const t0 = Date.now()
  while (!f()) {
    if (Date.now() - t0 > ms) throw new Error('等超时了')
    await new Promise((r) => setTimeout(r, 5))
  }
}

let srv: ReturnType<typeof fakeServer>
beforeEach(() => {
  srv = fakeServer()
})

describe('database', () => {
  it('打开：拿表结构和全部行，WASM 引擎查询', async () => {
    const db = createClient({ fetch: srv.fetch }).database('db1')
    await db.open()
    expect(db.status).toBe('ready')
    expect(db.rows.size).toBe(3)
    expect(db.seq).toBe(3)
    const r = db.engine!.query({ filter: { op: 'and', items: [{ prop: 'st', cond: 'eq', value: 'a' }] }, sorts: [{ prop: 'n', dir: 'desc' }], totals: { n: 'sum' } })
    expect(r.ids).toEqual(['r3', 'r1'])
    expect(r.totals?.n).toBe(14)
    const g = db.engine!.query({ group_by: 'st' })
    expect(g.groups?.map((x) => [x.key, x.count])).toEqual(expect.arrayContaining([['a', 2], ['b', 1]]))
    // 同一个 id 拿到同一个对象
    expect(createClient({ fetch: srv.fetch }).database('db1')).not.toBe(db)
    db.close()
  })

  it('改格子：先改本地（订阅的人马上知道），再发给服务器', async () => {
    const c = createClient({ fetch: srv.fetch })
    const db = c.database('db1')
    expect(c.database('db1')).toBe(db)
    await db.open()
    let changes = 0
    db.subscribe(() => changes++)
    const p = db.setCells([{ row_id: 'r2', key: 'n', value: 100 }])
    expect(db.rows.get('r2')!.props.n).toBe(100) // 还没等服务器
    expect(db.engine!.query({ sorts: [{ prop: 'n', dir: 'desc' }] }).ids[0]).toBe('r2')
    expect(changes).toBeGreaterThan(0)
    await p
    const patch = srv.calls.find((x) => x.method === 'PATCH')!
    expect(patch.body).toEqual({ ops: [{ row_id: 'r2', set: { n: 100 } }] })
    expect(srv.rows.find((r) => r.id === 'r2')!.props.n).toBe(100)
    expect(db.seq).toBe(4)
  })

  it('服务器拒了：改回去，error 里是原因', async () => {
    const db = createClient({ fetch: srv.fetch }).database('db1')
    await db.open()
    srv.failNext()
    await db.setCells([{ row_id: 'r1', key: 'title', value: '改坏的' }])
    expect(db.rows.get('r1')!.props.title).toBe('一')
    expect(db.engine!.computed('r1')).toBeDefined()
    expect(db.error).toContain('模拟的错误')
  })

  it('加行：先给临时编号，服务器给了真编号就换掉，等待期间本地改的也补发', async () => {
    const db = createClient({ fetch: srv.fetch }).database('db1')
    await db.open()
    let real = ''
    const tmp = db.addRow({ title: '新的' }, null, (id) => (real = id))
    expect(tmp).toMatch(/^tmp_/)
    expect(db.rows.has(tmp)).toBe(true)
    db.setCells([{ row_id: tmp, key: 'n', value: 7 }]) // 还没有真编号：只改本地
    await until(() => !!real)
    expect(db.rows.has(tmp)).toBe(false)
    expect(db.rows.get(real)!.props).toMatchObject({ title: '新的', n: 7 })
    await until(() => srv.rows.find((r) => r.id === real)?.props.n === 7)
    expect(db.engine!.query({}).ids).toContain(real)
  })

  it('删行失败也会放回来', async () => {
    const db = createClient({ fetch: srv.fetch }).database('db1')
    await db.open()
    srv.failNext()
    await db.deleteRows(['r1'])
    expect(db.rows.has('r1')).toBe(true)
    await db.deleteRows(['r1'])
    expect(db.rows.has('r1')).toBe(false)
    expect(srv.rows.some((r) => r.id === 'r1')).toBe(false)
  })

  it('增量同步：rows?since= 拿到别人改的和删的；收到 db.changed 事件自己去拉', async () => {
    const handlers = new Map<string, (d: unknown) => void>()
    const events = { on: (n: string, f: (d: unknown) => void) => (handlers.set(n, f), () => handlers.delete(n)) }
    const db = createClient({ fetch: srv.fetch }).database('db1')
    await db.open(events)
    srv.remote('r3', { title: '别人改的' })
    srv.remoteDelete('r2')
    handlers.get('db.changed')!({ db_id: 'db1', seq: 5 })
    await until(() => db.rows.get('r3')?.props.title === '别人改的' && !db.rows.has('r2'))
    const since = srv.calls.filter((x) => x.url.includes('since=')).map((x) => new URL(x.url, 'http://x').searchParams.get('since'))
    expect(since).toContain('3')
    expect(db.engine!.query({}).ids).toEqual(['r1', 'r3'])
    // 别的库的事件不管
    const n = srv.calls.length
    handlers.get('db.changed')!({ db_id: 'other', seq: 99 })
    await new Promise((r) => setTimeout(r, 30))
    expect(srv.calls.length).toBe(n)
  })

  it('表结构改动：先改本地，服务器拒了改回去', async () => {
    const db = createClient({ fetch: srv.fetch }).database('db1')
    await db.open()
    srv.failNext()
    const p = db.renameProp('n', '数量')
    expect(db.prop('n')!.name).toBe('数量')
    await p
    expect(db.prop('n')!.name).toBe('数')
    const op = srv.calls.find((x) => x.method === 'PATCH' && x.url.endsWith('/schema'))!
    expect(op.body).toEqual({ ops: [{ op: 'rename', prop: 'n', name: '数量' }] })
  })

  it('手动排序：挪到最前面，发 { row_id, after: null }', async () => {
    const db = createClient({ fetch: srv.fetch }).database('db1')
    await db.open()
    await db.moveRow('r3', null)
    expect([...db.rows.keys()]).toEqual(['r3', 'r1', 'r2'])
    expect(db.engine!.query({}).ids).toEqual(['r3', 'r1', 'r2'])
    const op = srv.calls.filter((x) => x.method === 'PATCH').at(-1)!
    expect(op.body).toEqual({ ops: [{ row_id: 'r3', after: null }] })
  })

  it('服务器打不开：status = error', async () => {
    const { fetch } = fakeFetch(() => json({ error: { code: 'not_found', message: '没有这个库' } }, 404))
    const db = createClient({ fetch }).database('nope')
    await db.open()
    expect(db.status).toBe('error')
    expect(db.error).toContain('没有这个库')
  })
})
