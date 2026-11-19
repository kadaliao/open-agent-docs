// 第 4 阶段（数据库）网页：每种视图浅色 / 深色截图，表格编辑、键盘、复制粘贴、汇总、分组，看板拖动，日历拖动，行面板，公式编辑器，内嵌，乐观写入回滚。
// 截图在 apps/web/e2e-results/phase4/。
// 默认打真服务器；OAD_DB_MOCK=1 时用 db-mock.ts 在浏览器里拦请求（见 db-backend.ts）。
import { expect, test, type Browser, type Page } from '@playwright/test'
import { mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { makeDb, MOCK, type Backend } from './db-backend'
import { taskRows, taskSchema, taskViews } from './db-mock'
import { createPage, workspace } from './helpers'

const OUT = join(dirname(fileURLToPath(import.meta.url)), '../e2e-results/phase4')
mkdirSync(OUT, { recursive: true })
const shot = (page: Page, name: string) => page.screenshot({ path: join(OUT, name + '.png'), animations: 'disabled' })

test.describe.configure({ mode: 'serial' })

let ws = ''
let meId = ''
let be: Backend
let DB = ''
// 视图 id（真服务器可能自己发 id）：按类型找
const V: Record<string, string> = {}

function cover(i: number) {
  const hues = [210, 28, 150, 280, 350, 190, 45, 120]
  const h = hues[i % hues.length]
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${h},70%,62%)"/><stop offset="1" stop-color="hsl(${h + 40},65%,42%)"/></linearGradient></defs><rect width="640" height="360" fill="url(#g)"/><circle cx="${120 + i * 60}" cy="${90 + (i % 3) * 50}" r="${70 + (i % 4) * 18}" fill="rgba(255,255,255,.16)"/><rect x="${300 - i * 20}" y="190" width="260" height="120" rx="18" fill="rgba(255,255,255,.12)"/></svg>`
  return 'data:image/svg+xml;base64,' + Buffer.from(svg).toString('base64')
}

async function newCtx(browser: Browser, scheme: 'light' | 'dark' = 'light') {
  const ctx = await browser.newContext({ colorScheme: scheme, permissions: ['clipboard-read', 'clipboard-write'] })
  await be.install(ctx)
  // 假服务器时：成员、Agent 也是假的（给人员属性用）
  if (MOCK) {
    await ctx.route(/\/api\/v1\/workspaces\/[^/]+\/members$/, (r) =>
      r.fulfill({
        json: {
          members: [
            { role: 'owner', user: { id: meId, name: '我' } },
            { role: 'editor', user: { id: 'u_zhang', name: '张三' } },
            { role: 'editor', user: { id: 'u_li', name: '李四' } },
            { role: 'editor', user: { id: 'u_wang', name: '王五' } },
          ],
        },
      }),
    )
    await ctx.route(/\/api\/v1\/workspaces\/[^/]+\/agents$/, (r) =>
      r.request().method() === 'GET' ? r.fulfill({ json: { agents: [{ id: 'a_writer', name: '写作助手', about: '', scope: {}, created_at: 0 }] } }) : r.fallback(),
    )
  }
  const page = await ctx.newPage()
  page.on('pageerror', (e) => console.log('页面出错：', e.message))
  return { ctx, page }
}

async function openDb(page: Page, view?: string) {
  await page.goto(`/p/${DB}` + (view ? `?view=${V[view] ?? view}` : ''))
  await page.waitForSelector('.db[data-status="ready"]', { timeout: 20_000 })
  await page.waitForTimeout(150)
}

test('准备：库、几行带正文的行页面', async ({ request }) => {
  ws = await workspace(request)
  meId = (await (await request.get('/api/v1/me')).json()).user.id
  const body = '目标：打开 1 万行的库 < 100ms。\n\n- [x] IndexedDB 缓存\n- [ ] 增量同步\n\n> 先量，再改。'
  // 人员：真服务器只认工作区里真有的人和 Agent
  const agent = await (await request.post(`/api/v1/workspaces/${ws}/agents`, { data: { name: '写作助手', about: '', scope: 'write' } })).json()
  const people = MOCK ? ['u_zhang', 'u_li', 'u_wang', 'agent:a_writer'] : [`user:${meId}`, `agent:${agent.agent?.id ?? agent.id}`]
  // 假服务器时，前三行用真的页面（行面板里的正文要连协作文档）；真服务器时行本来就是页面，正文跟着建
  const ids: string[] = []
  if (MOCK)
    for (const [i, title] of ['首页加载提速', '数据库表格视图', '看板拖动排序'].entries()) ids.push((await createPage(request, title, i === 0 ? body : '正文')).id)
  // 封面图：真服务器只认网址或传上去的文件，先传上去
  const images: string[] = []
  for (let i = 0; i < 8; i++) {
    if (MOCK) {
      images.push(cover(i))
      continue
    }
    const svg = Buffer.from(cover(i).split(',')[1], 'base64')
    const up = await request.post(`/api/v1/workspaces/${ws}/files`, { multipart: { file: { name: `cover-${i}.svg`, mimeType: 'image/svg+xml', buffer: svg } } })
    if (!up.ok()) throw new Error('上传失败：' + (await up.text()))
    images.push((await up.json()).url)
  }
  const rows = taskRows(20, people, { ids, images })
  rows[0].body = body
  be = await makeDb(request, ws, '研发任务', taskSchema(), taskViews(), rows, 'dbtasks01')
  DB = be.id
  const info = await (await request.get(`/api/v1/databases/${DB}`)).json().catch(() => null)
  for (const v of (MOCK ? taskViews() : info.views) as { id: string; type: string }[]) V[{ table: 'vt', board: 'vb', calendar: 'vc', gallery: 'vg', list: 'vl' }[v.type]!] = v.id
})

for (const scheme of ['light', 'dark'] as const) {
  test(`截图：每种视图（${scheme}）`, async ({ browser }) => {
    const { ctx, page } = await newCtx(browser, scheme)
    await openDb(page)
    await expect(page.locator('.db-tr').first()).toBeVisible()
    await shot(page, `table-${scheme}`)
    for (const [v, name] of [
      ['vb', 'board'],
      ['vc', 'calendar'],
      ['vg', 'gallery'],
      ['vl', 'list'],
    ] as const) {
      await page.locator('.db-tab', { hasText: { vb: '看板', vc: '日历', vg: '画廊', vl: '列表' }[v] }).click()
      await page.waitForTimeout(350)
      await shot(page, `${name}-${scheme}`)
    }
    // 行面板
    await page.locator('.db-tab', { hasText: '全部任务' }).click()
    await page.locator('.db-tr').first().hover()
    await page.locator('.db-tr').first().locator('.db-open').click()
    await expect(page.locator('.db-peek')).toBeVisible()
    await page.waitForTimeout(800)
    await shot(page, `peek-${scheme}`)
    await page.keyboard.press('Escape')
    await ctx.close()
  })
}

test('表格：改格子、新建选项、日期、键盘导航、复制粘贴、汇总、分组', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  await openDb(page)
  const cell = (r: number, c: number) => page.locator(`[data-cell="${r}:${c}"]`)
  // 双击名称格，改名
  await cell(0, 0).dblclick()
  await expect(page.locator('.db-cell-input')).toBeFocused()
  await page.keyboard.press('Meta+a')
  await page.keyboard.type('首页加载再提速')
  await page.keyboard.press('Enter')
  await expect(cell(0, 0)).toContainText('首页加载再提速')
  await expect.poll(async () => (await be.rows())[0].props.title).toBe('首页加载再提速')
  // 回车之后光标下移一行
  await expect(cell(1, 0)).toHaveClass(/is-active/)
  // 方向键到“优先级”，开选项编辑器，新建一个选项
  await page.keyboard.press('ArrowRight')
  await page.keyboard.press('ArrowRight')
  // 中文输入法要先有能输入的地方：回车打开编辑器再打字（英文字母直接打也能开）
  await page.keyboard.press('Enter')
  await page.keyboard.type('阻塞')
  await expect(page.locator('.db-opt-pop')).toBeVisible()
  await shot(page, 'select-editor')
  await page.keyboard.press('Enter')
  await expect(cell(1, 2)).toContainText('阻塞')
  await expect.poll(async () => (await be.schema()).props.find((p: any) => p.key === 'pri').options.some((o: any) => o.name === '阻塞')).toBe(true)
  // 日期选择器
  await cell(2, 5).dblclick()
  await expect(page.locator('.db-date-pop')).toBeVisible()
  await shot(page, 'date-editor')
  await page.locator('.db-month-day:not(.is-out)', { hasText: /^15$/ }).click()
  await page.keyboard.press('Escape')
  await expect(cell(2, 5)).toContainText('15日')
  // 复制两行三列，贴到下面
  await cell(0, 6).click()
  await cell(1, 8).click({ modifiers: ['Shift'] })
  await expect(page.locator('.db-td.is-range')).toHaveCount(6)
  await page.keyboard.press('Meta+c')
  await expect(page.locator('.db-toast')).toContainText('6 格')
  await cell(5, 6).click()
  await page.keyboard.press('Meta+v')
  const a = await cell(0, 6).innerText()
  await expect(cell(5, 6)).toHaveText(a)
  // 汇总行
  await expect(page.locator('.db-tf.is-set').first()).toBeVisible()
  // 按状态分组
  await page.locator('[data-db-tool="group"]').click()
  await page.locator('.db-menu-item', { hasText: '状态' }).click()
  await expect(page.locator('.db-group-row')).toHaveCount(4)
  await shot(page, 'table-grouped')
  await page.locator('.db-group-row').first().locator('.db-group-toggle').click()
  await page.waitForTimeout(100)
  await shot(page, 'table-grouped-collapsed')
  // 列头菜单
  await page.locator('.db-th', { hasText: '预估' }).click()
  await expect(page.locator('.db-menu')).toBeVisible()
  await shot(page, 'column-menu')
  await page.keyboard.press('Escape')
  // 筛选面板（嵌套条件组）
  await page.locator('[data-db-tool="filter"]').click()
  await page.locator('.db-link-btn', { hasText: '添加筛选条件' }).click()
  await page.locator('.db-link-btn', { hasText: '添加条件组' }).click()
  await page.waitForTimeout(150)
  await shot(page, 'filter')
  await ctx.close()
})

test('中文输入法：选中格子直接打字，组字在格子上完成，打完进编辑器', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  await openDb(page)
  const cell = page.locator('[data-cell="6:0"]')
  await cell.click()
  const cdp = await ctx.newCDPSession(page)
  // 模拟拼音输入：先出组字中的“zhong”，再上屏“中文”
  await cdp.send('Input.imeSetComposition', { text: 'zhong', selectionStart: 5, selectionEnd: 5 })
  await expect(page.locator('.db-ime-sink.is-on')).toBeVisible()
  await shot(page, 'ime-composing')
  await cdp.send('Input.imeSetComposition', { text: 'zhongwen', selectionStart: 8, selectionEnd: 8 })
  await cdp.send('Input.insertText', { text: '中文' })
  const input = page.locator('.db-cell-input')
  await expect(input).toBeFocused()
  await expect(input).toHaveValue('中文')
  await page.keyboard.type('标题')
  await page.keyboard.press('Enter')
  await expect(cell).toContainText('中文标题')
  await ctx.close()
})

test('公式编辑器：补全、实时报错、预览', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  await openDb(page)
  // 列是虚拟滚动的：先滚到右边
  await page.locator('.db-table').evaluate((el) => (el.scrollLeft = el.scrollWidth))
  await page.locator('.db-th', { hasText: '人天' }).click()
  await page.locator('.db-menu-item', { hasText: '编辑公式' }).click()
  const ta = page.locator('.db-formula textarea')
  await expect(ta).toBeVisible()
  await ta.fill('')
  await ta.type('prop("预估（小时）") * ')
  await expect(page.locator('.db-formula-err')).toBeVisible()
  await shot(page, 'formula-error')
  await ta.type('rou')
  await expect(page.locator('.db-formula-item[aria-selected="true"]')).toContainText('round')
  await page.keyboard.press('Tab')
  await ta.type('1.5)')
  await expect(page.locator('.db-formula-ok')).toBeVisible()
  await expect(page.locator('.db-formula-prev').first()).toBeVisible()
  await shot(page, 'formula-ok')
  await page.locator('.db-formula .btn-primary').click()
  await expect.poll(async () => (await be.schema()).props.find((p: any) => p.key === 'f').formula).toContain('round(1.5)')
  await ctx.close()
})

test('看板：卡片拖到别的列改状态，列内拖动排序', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  await openDb(page, 'vb')
  const col = (name: string) => page.locator('.db-col').filter({ has: page.locator('.db-col-head', { hasText: name }) })
  const first = col('未开始').locator('.db-card').first()
  const title = (await first.locator('.db-card-title').innerText()).trim()
  await first.dragTo(col('已完成').locator('.db-col-cards'))
  await expect(col('已完成').locator('.db-card-title', { hasText: title })).toBeVisible()
  await expect.poll(async () => (await be.rows()).find((r) => r.props.title === title)?.props.st).toBe('s4')
  // 列内：第三张拖到第一张上面（都在屏幕里）
  const cards = col('进行中').locator('.db-card')
  const third = (await cards.nth(2).locator('.db-card-title').innerText()).trim()
  const moved = page.waitForResponse((r) => r.url().includes('/rows') && r.request().method() === 'PATCH' && (r.request().postData() ?? '').includes('"after"'))
  await cards.nth(2).dragTo(cards.first(), { targetPosition: { x: 20, y: 4 } })
  expect((await moved).status()).toBe(200)
  await expect(cards.first().locator('.db-card-title')).toHaveText(third)
  // 服务器记住了新顺序（PATCH rows 的 after）
  await expect.poll(async () => (await be.rows()).filter((r) => r.props.st === 's2')[0]?.props.title).toBe(third)
  await ctx.close()
})

test('日历：拖到别的一天改日期；周视图', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  await openDb(page, 'vc')
  const ev = page.locator('.db-cal-day:not(.is-out) .db-cal-ev').first()
  const title = (await ev.innerText()).trim()
  const target = page.locator('.db-cal-day:not(.is-out)').nth(20)
  await ev.dragTo(target)
  await expect(target.locator('.db-cal-ev', { hasText: title })).toBeVisible()
  await page.locator('.db-menu-seg button', { hasText: '周' }).click()
  await page.waitForTimeout(150)
  await shot(page, 'calendar-week')
  await ctx.close()
})

test('乐观写入：服务器拒了就改回去，显示错误', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  await openDb(page)
  const c = page.locator('[data-cell="3:8"]')
  const before = await c.locator('.db-check').getAttribute('aria-checked')
  await be.failNextWrite(page)
  await c.locator('.db-check').click()
  await expect(page.locator('.db-toast.is-error')).toBeVisible()
  await expect(c.locator('.db-check')).toHaveAttribute('aria-checked', before!)
  await ctx.close()
})

test('实时：别人改了，收到 db.changed、rows?since 拉增量；再打开先出缓存', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  await openDb(page)
  // 表格第 5 行是哪一行（前面的测试拖动过顺序）
  const shown = (await page.locator('[data-cell="4:0"] .db-title-text').innerText()).trim()
  const id = (await be.rows()).find((r) => r.props.title === shown)!.id
  await be.remoteSet(id, { title: '别人改的标题' })
  // 真服务器推 db.changed（网页收到后 rows?since=）；假的手动同步一次
  if (!be.live) await page.evaluate((db) => (window as any).__oadDb.get(db).sync(), DB)
  await expect(page.locator('[data-cell="4:0"]')).toContainText('别人改的标题')
  // 再开一次：先出 IndexedDB 的缓存
  await page.waitForTimeout(500)
  await page.reload()
  await page.waitForSelector('.db[data-status]')
  const t = await page.evaluate((db) => (window as any).__oadDb.get(db).timing, DB)
  expect(t.cache).toBeGreaterThan(0)
  await ctx.close()
})

test('内嵌：页面里单独一行库的网址，显示成能用的库', async ({ browser, request }) => {
  const { ctx, page } = await newCtx(browser)
  const host = await createPage(request, '周报', `本周进展如下。\n\n${new URL(`/p/${DB}?view=${V.vb}`, test.info().project.use.baseURL!).href}\n\n以上。`)
  await page.goto(`/p/${host.id}`)
  await expect(page.locator('.oad-embed.is-db .db')).toBeVisible({ timeout: 20_000 })
  await page.waitForTimeout(400)
  await shot(page, 'embed')
  await ctx.close()
})

test('标题列的 key 不叫 title 时（接口、MCP 建的库会随机分配），卡片、日历、列表照样显示行标题', async ({ browser, request }) => {
  test.skip(MOCK, '要真服务器分配 key')
  const r = await request.post(`/api/v1/workspaces/${ws}/databases`, {
    data: {
      title: '随机 key',
      schema: { props: [{ name: '名称', type: 'title' }, { name: '截止', type: 'date' }, { name: '状态', type: 'status' }] },
    },
  })
  expect(r.ok()).toBeTruthy()
  const { page: pg, schema } = await r.json()
  const titleKey = schema.props.find((p: { type: string }) => p.type === 'title').key
  expect(titleKey).not.toBe('title')
  const dateKey = schema.props.find((p: { type: string }) => p.type === 'date').key
  const statusKey = schema.props.find((p: { type: string }) => p.type === 'status').key
  const views = [{ name: '看板', type: 'board', group_by: statusKey }, { name: '日历', type: 'calendar', calendar_by: dateKey }, { name: '列表', type: 'list' }]
  expect((await request.put(`/api/v1/databases/${pg.id}/views`, { data: { views } })).ok()).toBeTruthy()
  const today = new Date().toISOString().slice(0, 10)
  await request.post(`/api/v1/databases/${pg.id}/rows`, { data: { rows: [{ props: { [titleKey]: '随机键的行', [dateKey]: { start: today } } }] } })
  const { ctx, page } = await newCtx(browser)
  await page.goto(`/p/${pg.id}`)
  await page.waitForSelector('.db[data-status="ready"]', { timeout: 20_000 })
  await expect(page.locator('.db-card-title')).toHaveText('随机键的行')
  await page.locator('.db-tab', { hasText: '日历' }).click()
  await expect(page.locator('.db-cal-item, .db-cal-ev').first()).toContainText('随机键的行')
  await page.locator('.db-tab', { hasText: '列表' }).click()
  await expect(page.locator('.db-list-title')).toHaveText('随机键的行')
  await ctx.close()
})
