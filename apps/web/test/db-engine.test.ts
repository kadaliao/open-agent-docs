// 网页对 @open-agent-docs/db（WASM 引擎）的用法：查询的结果形状、汇总、规范化、显示、公式检查，1 万行的查询时间。
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { beforeAll, describe, expect, it } from 'vitest'
import { DbEngine, initSync } from '@open-agent-docs/db'
import { safeQuery, tryNormalize } from '@open-agent-docs/client/db'
import type { DbEngine as Eng, Schema } from '@open-agent-docs/client'

const schema: Schema = {
  props: [
    { key: 'title', name: '名称', type: 'title' },
    {
      key: 'st',
      name: '状态',
      type: 'status',
      options: [
        { id: 's1', name: '未开始', color: 'gray', group: 'todo' },
        { id: 's2', name: '进行中', color: 'blue', group: 'doing' },
        { id: 's3', name: '已完成', color: 'green', group: 'done' },
      ],
    },
    { key: 'est', name: '预估', type: 'number' },
    { key: 'due', name: '截止', type: 'date' },
    { key: 'ok', name: '确认', type: 'checkbox' },
    { key: 'f', name: '人天', type: 'formula', formula: 'prop("预估") / 8' },
  ],
}

let eng: Eng
beforeAll(() => {
  initSync({ module: readFileSync(fileURLToPath(new URL('../../../packages/db-wasm/dist/oad_db_bg.wasm', import.meta.url))) })
  eng = DbEngine.createSync(schema, { id: 'db1', now: Date.UTC(2026, 10, 19), tz: 480 }) as unknown as Eng
  eng.loadRows(
    Array.from({ length: 30 }, (_, i) => ({
      id: 'r' + i,
      props: { title: `任务 ${i}`, st: ['s1', 's2', 's3'][i % 3], est: i, ...(i % 2 ? { ok: true } : {}), ...(i % 5 ? {} : { due: { start: `2026-11-${String(10 + (i % 18)).padStart(2, '0')}` } }) },
    })),
  )
})

describe('@open-agent-docs/db 在网页里的用法', () => {
  it('筛选、排序、分组、汇总', () => {
    const r = safeQuery(eng, { filter: { op: 'and', items: [{ prop: 'st', cond: 'eq', value: 's2' }] }, sorts: [{ prop: 'est', dir: 'desc' }], totals: { est: 'sum', ok: 'percent_checked' } })
    expect(r.ids[0]).toBe('r28')
    expect(r.ids.length).toBe(10)
    expect(r.totals?.est).toBe(145)
    const g = safeQuery(eng, { group_by: 'st' })
    expect(g.groups?.map((x) => x.key)).toEqual(expect.arrayContaining(['s1', 's2', 's3']))
    expect(g.groups?.find((x) => x.key === 's1')?.count).toBe(10)
  })

  it('刚加的、还没填值的条件不算；坏条件不抛错', () => {
    expect(safeQuery(eng, { filter: { op: 'and', items: [{ prop: 'est', cond: 'gt' }] } }).ids.length).toBe(30)
    const bad = safeQuery(eng, { filter: { op: 'and', items: [{ prop: '没有这个', cond: 'eq', value: 1 }] } })
    expect(bad.error).toBeTruthy()
    expect(bad.ids.length).toBe(30)
  })

  it('规范化：认得的转成存的样子，认不出给中文说明', () => {
    expect(tryNormalize(eng, 'est', '12.5')).toEqual({ value: 12.5 })
    expect(tryNormalize(eng, 'st', '进行中')).toEqual({ value: 's2' })
    expect(tryNormalize(eng, 'due', '2026-11-20')).toMatchObject({ value: { start: '2026-11-20' } })
    const e = tryNormalize(eng, 'est', '很多')
    expect('error' in e && e.error.length > 0).toBe(true)
  })

  it('公式：算值、检查', () => {
    expect(eng.computed('r16').f).toBe(2)
    expect(eng.checkFormula('prop("预估") * ', 'f')).toMatchObject({ at: expect.any(Number) })
    expect(eng.checkFormula('round(prop("预估") / 8, 1)', 'f')).toEqual({ ok: true })
    expect(eng.display('st', 's3')).toBe('已完成')
  })

  it('1 万行：筛选 + 排序 + 分组 + 汇总', () => {
    const big = DbEngine.createSync(schema, { id: 'db2' }) as unknown as Eng
    big.loadRows(Array.from({ length: 10_000 }, (_, i) => ({ id: 'b' + i, props: { title: `行 ${(i * 7919) % 10_000}`, st: ['s1', 's2', 's3'][i % 3], est: (i * 31) % 97 } })))
    const t0 = performance.now()
    const r = big.query({ filter: { op: 'and', items: [{ prop: 'est', cond: 'gt', value: 10 }] }, sorts: [{ prop: 'title', dir: 'asc' }], group_by: 'st', totals: { est: 'sum' } })
    const ms = performance.now() - t0
    expect(r.groups?.length).toBe(3)
    console.log(`1 万行查询：${ms.toFixed(1)}ms`)
    expect(ms).toBeLessThan(100)
  })
})
