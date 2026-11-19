// 数据库性能（1 万行）：打开（没缓存 / 有 IndexedDB 缓存）、表格上下左右滚动的帧率、筛选 / 排序 / 分组从改视图到画到屏幕上的时间。
// 目标：滚动 60fps；筛选、排序、分组 < 50ms；有缓存打开 < 100ms，没缓存 < 500ms。数字写到 apps/web/e2e-results/db-perf.json。
// 默认打真服务器（行从 /databases/{id}/rows 来，压缩后传）；OAD_DB_MOCK=1 时用 db-mock.ts。
import { expect, test, type Page } from '@playwright/test'
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { makeDb, type Backend } from './db-backend'
import { taskRows, taskSchema } from './db-mock'
import { workspace } from './helpers'
import { traced } from './trace'
import { loadavg, tmpdir } from 'node:os'

const ROUNDS = Number(process.env.OAD_PERF_ROUNDS ?? 7)
const N = 10_000
let DB = ''
const results: Record<string, unknown> = {}
let be: Backend

const pct = (xs: number[], p: number) => {
  const s = [...xs].sort((a, b) => a - b)
  return +s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))].toFixed(1)
}
const summary = (xs: number[]) => ({ n: xs.length, p50: pct(xs, 50), p95: pct(xs, 95), max: +Math.max(...xs).toFixed(1) })

test.describe.configure({ mode: 'serial' })

// 从导航开始：first = 第一次看到行（首屏快照或真的表格），rows = 真的表格的行画出来（都是 DOM 里有了、再过一帧）
const WATCH = () => {
  const w = window as any
  const mo = new MutationObserver(() => {
    if (w.__first === undefined && document.querySelector('.db-tr')) {
      w.__first = -1
      requestAnimationFrame(() => (w.__first = performance.now()))
    }
    if (w.__live === undefined && document.querySelector('.db-page:not(.db-snapshot) .db-tr')) {
      w.__live = -1
      requestAnimationFrame(() => {
        w.__live = performance.now()
        mo.disconnect()
      })
    }
  })
  document.addEventListener('DOMContentLoaded', () => mo.observe(document.body, { childList: true, subtree: true }))
}
async function openTime(page: Page) {
  if (!(page as any).__watching) {
    await page.addInitScript(WATCH)
    ;(page as any).__watching = true
  }
  await page.goto(`/p/${DB}`)
  await page.waitForFunction(() => (window as any).__live > 0, null, { timeout: 30_000 })
  return page.evaluate(
    (db) => {
      const w = window as any
      return { first: w.__first as number, rows: w.__live as number, snapshot: !!document.querySelector('.db-snapshot') || w.__first < w.__live - 1, timing: { ...w.__oadDb.get(db).timing } }
    },
    DB,
  )
}

test('准备 1 万行', async ({ request }) => {
  const ws = await workspace(request)
  const views = [{ id: 'v1', name: '全部', type: 'table', filter: { op: 'and', items: [] }, sorts: [], hidden: [], order: [], widths: {}, totals: { est: 'sum', cost: 'avg' } }]
  const me = (await (await request.get('/api/v1/me')).json()).user.id
  const t0 = Date.now()
  be = await makeDb(request, ws, '一万行', taskSchema(), views, taskRows(N, [`user:${me}`]), 'dbperf10k')
  DB = be.id
  results.setupMs = Date.now() - t0
  results.rows = N
})

test('打开：没有缓存', async ({ browser }) => {
  const nav: number[] = []
  const ready: number[] = []
  for (let i = 0; i < ROUNDS; i++) {
    const ctx = await browser.newContext()
    await be.install(ctx)
    const page = await ctx.newPage()
    const r = await openTime(page)
    nav.push(r.rows)
    // 从库的代码开始打开（DbStore.open）到行画出来
    ready.push(r.timing.ready)
    await ctx.close()
  }
  results.openUncached = { fromNavigation: summary(nav), storeReady: summary(ready) }
  console.log('没缓存：', results.openUncached)
})

test('打开：有 IndexedDB 缓存', async ({ browser }) => {
  const ctx = await browser.newContext()
  await be.install(ctx)
  const page = await ctx.newPage()
  await openTime(page)
  // 等缓存写完
  await expect
    .poll(
      () =>
        page.evaluate(
          (db) =>
            new Promise<boolean>((res) => {
              const r = indexedDB.open('oad:db', 1)
              r.onsuccess = () => {
                try {
                  const g = r.result.transaction('d').objectStore('d').getKey(db)
                  g.onsuccess = () => res(g.result === db)
                  g.onerror = () => res(false)
                } catch {
                  res(false)
                }
              }
              r.onerror = () => res(false)
            }),
          DB,
        ),
      { timeout: 20_000 },
    )
    .toBe(true)
  // 让首屏快照存下来（离开页面时存）
  await page.goto('about:blank')
  const nav: number[] = []
  const first: number[] = []
  const cache: number[] = []
  // 在应用里切过来：从点侧栏到行画出来
  const inApp: number[] = []
  for (let i = 0; i < ROUNDS; i++) {
    const r = await openTime(page)
    nav.push(r.rows)
    first.push(r.first)
    cache.push(r.timing.cache)
    if (!(r.timing.cache > 0)) console.log('timing', JSON.stringify(r.timing))
    expect(r.timing.cache).toBeGreaterThan(0)
  }
  // 在应用里：先去别的页面，再点侧栏回来（库的数据还在内存里，等于“切回来”）
  const other = await page.evaluate(async () => {
    const me = await (await fetch('/api/v1/me')).json()
    const r = await fetch(`/api/v1/workspaces/${me.workspaces[0].id}/pages`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ title: '别的页面' }) })
    return (await r.json()).id as string
  })
  for (let i = 0; i < ROUNDS; i++) {
    await page.goto(`/p/${other}`)
    await page.waitForSelector('.tree-row')
    await page.waitForTimeout(200)
    const t = await page.evaluate(
      (db) =>
        new Promise<number>((resolve) => {
          const row = [...document.querySelectorAll<HTMLElement>('.tree-row')].find((x) => x.textContent?.includes('一万行'))!
          const t0 = performance.now()
          row.click()
          const check = () => {
            if (document.querySelector(`[data-db="${db}"] .db-tr`)) requestAnimationFrame(() => resolve(performance.now() - t0))
            else requestAnimationFrame(check)
          }
          check()
        }),
      DB,
    )
    inApp.push(t)
  }
  results.openCached = { firstContent: summary(first), fromNavigation: summary(nav), idbToRows: summary(cache), inAppSwitch: summary(inApp) }
  console.log('有缓存：', results.openCached)
  await ctx.close()
})

test('滚动：上下、左右', async ({ browser }) => {
  const ctx = await browser.newContext()
  await be.install(ctx)
  const page = await ctx.newPage()
  await openTime(page)
  await page.waitForTimeout(500)
  const run = (axis: 'y' | 'x', px: number, ms: number) =>
    page.evaluate(
      ({ axis, px, ms }) =>
        new Promise<{ frames: number; seconds: number; fps: number; dropped: number; worst: number }>((resolve) => {
          const el = document.querySelector('.db-table') as HTMLElement
          let last = performance.now()
          const t0 = last
          let frames = 0
          let dropped = 0
          let worst = 0
          let dir = 1
          const step = (now: number) => {
            const dt = now - last
            last = now
            if (frames > 0) {
              worst = Math.max(worst, dt)
              if (dt > 16.7 * 1.5) dropped++
            }
            frames++
            if (axis === 'y') {
              el.scrollTop += px * dir
              if (el.scrollTop + el.clientHeight >= el.scrollHeight - 2 || el.scrollTop <= 0) dir = -dir
            } else {
              el.scrollLeft += px * dir
              if (el.scrollLeft + el.clientWidth >= el.scrollWidth - 2 || el.scrollLeft <= 0) dir = -dir
            }
            if (now - t0 < ms) requestAnimationFrame(step)
            else {
              const seconds = (now - t0) / 1000
              resolve({ frames, seconds: +seconds.toFixed(2), fps: +(frames / seconds).toFixed(1), dropped, worst: +worst.toFixed(1) })
            }
          }
          requestAnimationFrame(step)
        }),
      { axis, px, ms },
    )
  // 每帧滚 120px（约 3.5 行）、4 秒；再快速甩（每帧 600px）2 秒。每段都录 trace：
  // 掉帧时分清是页面主线程的活太多（CPU 时间长），还是机器太忙、线程被系统挂起（墙上时间长、CPU 时间短）
  const la0 = loadavg()[0]
  const tr = (name: string, axis: 'y' | 'x', px: number, ms: number) => traced(browser, page, join(tmpdir(), `oad-db-scroll-${name}.json`), () => run(axis, px, ms))
  const y = await tr('vertical', 'y', 120, 4000)
  const fling = await tr('fling', 'y', 600, 2000)
  const x = await tr('horizontal', 'x', 40, 3000)
  const one = (r: Awaited<ReturnType<typeof tr>>) => ({ ...r.result, mainThread: r.main })
  results.scroll = { vertical: one(y), fling: one(fling), horizontal: one(x), loadavg: [+la0.toFixed(1), +loadavg()[0].toFixed(1)] }
  console.log('滚动：', JSON.stringify(results.scroll, null, 1))
  await ctx.close()
  // 页面自己的活：每帧主线程 CPU 时间要在一帧（16ms）以内，CPU 时间超过 50ms 的任务一个都不能有。
  // 墙上时间的掉帧（被系统挂起）记在 dropped / preempted 里，不算代码的问题（机器忙时会有）。
  for (const r of [y, fling, x]) {
    expect(r.main.cpuLong, JSON.stringify(r.main.longTasks)).toBe(0)
    expect(r.main.frameCpu.p95).toBeLessThan(16)
  }
})

test('筛选、排序、分组：改视图到画出来', async ({ browser }) => {
  const ctx = await browser.newContext()
  await be.install(ctx)
  const page = await ctx.newPage()
  await openTime(page)
  await page.waitForTimeout(300)
  const measure = (patch: Record<string, unknown>) =>
    page.evaluate(
      ({ db, patch }) =>
        new Promise<{ total: number; query: number }>((resolve) => {
          const s = (window as any).__oadDb.get(db)
          const t0 = performance.now()
          s.updateView(s.views[0].id, patch)
          // 等 React 画完、浏览器出下一帧
          requestAnimationFrame(() => setTimeout(() => resolve({ total: performance.now() - t0, query: s.timing.query }), 0))
        }),
      { db: DB, patch },
    )
  const out: Record<string, { total: ReturnType<typeof summary>; query: ReturnType<typeof summary> }> = {}
  const cases: [string, Record<string, unknown>, Record<string, unknown>][] = [
    ['filter', { filter: { op: 'and', items: [{ prop: 'st', cond: 'eq', value: 's2' }] } }, { filter: { op: 'and', items: [] } }],
    ['filterNested', { filter: { op: 'and', items: [{ prop: 'est', cond: 'gt', value: 10 }, { op: 'or', items: [{ prop: 'tags', cond: 'contains', value: 't1' }, { prop: 'title', cond: 'contains', value: '看板' }] }] } }, { filter: { op: 'and', items: [] } }],
    ['sort', { sorts: [{ prop: 'est', dir: 'desc' }] }, { sorts: [] }],
    ['sortText', { sorts: [{ prop: 'title', dir: 'asc' }] }, { sorts: [] }],
    ['group', { group_by: 'st' }, { group_by: null }],
    ['groupMulti', { group_by: 'tags' }, { group_by: null }],
  ]
  for (const [name, on, off] of cases) {
    const total: number[] = []
    const query: number[] = []
    for (let i = 0; i < ROUNDS; i++) {
      const r = await measure(on)
      total.push(r.total)
      query.push(r.query)
      await measure(off)
    }
    out[name] = { total: summary(total), query: summary(query) }
  }
  // 搜索框：打一个字到画出来
  const search: number[] = []
  await page.locator('.db-tool-icon[aria-label="搜索"]').click()
  for (let i = 0; i < ROUNDS; i++) {
    const t = await page.evaluate(
      () =>
        new Promise<number>((resolve) => {
          const el = document.querySelector('.db-search') as HTMLInputElement
          const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!
          const t0 = performance.now()
          setter.call(el, el.value ? '' : '提速')
          el.dispatchEvent(new Event('input', { bubbles: true }))
          requestAnimationFrame(() => setTimeout(() => resolve(performance.now() - t0), 0))
        }),
    )
    search.push(t)
  }
  out.search = { total: summary(search), query: summary([0]) }
  results.viewChanges = out
  console.log('改视图（ms）：', JSON.stringify(out, null, 1))
  await ctx.close()
})

test.afterAll(() => {
  const dir = join(dirname(fileURLToPath(import.meta.url)), '../e2e-results')
  mkdirSync(dir, { recursive: true })
  writeFileSync(join(dir, 'db-perf.json'), JSON.stringify(results, null, 2))
})
