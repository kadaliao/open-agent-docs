// 第 5 阶段网页：⌘K 搜索面板、快捷键、页面历史和对比、库的改动记录、回收站、分享和公开页面、中英文切换、设置、邀请、手机布局、离线（PWA）。
// 截图在 apps/web/e2e-results/phase5/。对着真服务器跑；服务器还没有的接口（邀请、改成员、改个人资料……）在这里补一个假的（真的有了就用真的）。
import { expect, test, type Browser, type BrowserContext, type Page, type Route } from '@playwright/test'
import { mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { makeDb, type Backend } from './db-backend'
import { taskRows, taskSchema, taskViews } from './db-mock'
import { createPage, openPage, workspace } from './helpers'

const OUT = join(dirname(fileURLToPath(import.meta.url)), '../e2e-results/phase5')
mkdirSync(OUT, { recursive: true })
const shot = (page: Page, name: string, full = false) => page.screenshot({ path: join(OUT, name + '.png'), animations: 'disabled', fullPage: full })

test.describe.configure({ mode: 'serial' })

let ws = ''
let meId = ''
let meName = ''
let docId = ''
let otherId = ''
let be: Backend
const missing = new Set<string>()

// 服务器回 404 / 405（还没做这个接口）时用假的
async function fallback(ctx: BrowserContext | Page, url: RegExp, name: string, fake: (route: Route) => unknown, methods?: string[]) {
  await ctx.route(url, async (route) => {
    if (methods && !methods.includes(route.request().method())) return route.fallback()
    const res = await route.fetch().catch(() => null)
    if (res && res.status() !== 404 && res.status() !== 405) return route.fulfill({ response: res })
    missing.add(name)
    return fake(route)
  })
}

const invites: { token: string; url: string; role: string; expires_at: number | null; created_at: number; uses: number }[] = []
async function newCtx(browser: Browser, opts: Parameters<Browser['newContext']>[0] = {}) {
  const ctx = await browser.newContext({ permissions: ['clipboard-read', 'clipboard-write'], ...opts })
  await fallback(ctx, /\/api\/v1\/me$/, 'PATCH /me', async (r) => {
    const body = r.request().postDataJSON() ?? {}
    const me = await (await r.fetch({ method: 'GET' })).json()
    return r.fulfill({ json: { ...me, user: { ...me.user, ...body } } })
  }, ['PATCH'])
  await fallback(ctx, /\/api\/v1\/me\/password$/, 'POST /me/password', (r) => r.fulfill({ json: {} }))
  await fallback(ctx, /\/api\/v1\/workspaces\/[^/]+$/, 'PATCH /workspaces/{ws}', (r) => r.fulfill({ json: { id: ws, ...(r.request().postDataJSON() ?? {}) } }), ['PATCH'])
  await fallback(ctx, /\/api\/v1\/workspaces\/[^/]+\/members\/[^/]+$/, 'PATCH/DELETE members/{user}', (r) => r.fulfill({ json: {} }))
  await fallback(ctx, /\/api\/v1\/workspaces\/[^/]+\/invites(\/[^/]+)?$/, 'invites', (r) => {
    const m = r.request().method()
    if (m === 'POST') {
      const b = r.request().postDataJSON()
      const token = 'tok' + Math.random().toString(36).slice(2, 10)
      const inv = { token, url: `/invite/${token}`, role: b.role, expires_at: b.expires_in ? Date.now() + b.expires_in * 1000 : null, created_at: Date.now(), uses: 0 }
      invites.unshift(inv)
      return r.fulfill({ status: 201, json: inv })
    }
    if (m === 'DELETE') {
      const tok = r.request().url().split('/').pop()
      invites.splice(invites.findIndex((x) => x.token === tok), 1)
      return r.fulfill({ status: 204, body: '' })
    }
    return r.fulfill({ json: { invites } })
  })
  await fallback(ctx, /\/api\/v1\/invites\/[^/]+(\/accept)?$/, 'invite info / accept', async (r) => {
    if (r.request().url().endsWith('/accept')) return r.fulfill({ json: await (await r.fetch({ url: r.request().url().replace(/\/invites\/.*/, '/me'), method: 'GET' })).json() })
    return r.fulfill({ json: { workspace: { id: ws, name: '默认工作区', icon: '✦' }, role: 'editor', invited_by: { name: meName }, expires_at: Date.now() + 7 * 86400_000, signup: true } })
  })
  const page = await ctx.newPage()
  page.on('pageerror', (e) => console.log('页面出错：', e.message))
  return { ctx, page }
}

const mod = 'ControlOrMeta'

test('准备：几页文字、一个库', async ({ request }) => {
  ws = await workspace(request)
  const me = await (await request.get('/api/v1/me')).json()
  meId = me.user.id
  meName = me.user.name
  // 别的测试可能把语言设成了英文
  await request.patch('/api/v1/me', { data: { locale: null } }).catch(() => {})
  const doc = await createPage(
    request,
    '发布计划',
    [
      '## 目标',
      '',
      '本周把 **搜索** 和页面历史上线，同时修好离线打开。',
      '',
      '- 搜索：中文按词切，标题权重高',
      '- 历史：每一版都能对比、恢复',
      '- [ ] 公开分享页面的排版',
      '',
      '> 先量，再改。',
      '',
      '```ts',
      'const hits = await search(ws, "同步协议")',
      '```',
    ].join('\n'),
  )
  docId = doc.id
  otherId = (await createPage(request, '同步协议的设计笔记', '冲突合并用 CRDT；离线编辑在重连后合并，**同步协议** 走 WebSocket。\n\n第二段：搜索结果要高亮命中的词。')).id
  await createPage(request, 'Release checklist', 'Ship the search palette and the history panel.\n\nCheck the public share page typography.')
  const rows = taskRows(12, [`user:${meId}`])
  be = await makeDb(request, ws, '研发任务', taskSchema(), taskViews(), rows)
  // 让搜索索引跟上（服务器防抖 1 秒）
  await new Promise((r) => setTimeout(r, 1500))
})

test('⌘K：边打边搜，结果带高亮片段；最近看过；命令；回车打开', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  await openPage(page, otherId)
  await openPage(page, docId)
  await page.keyboard.press(`${mod}+k`)
  const pal = page.locator('.palette')
  await expect(pal).toBeVisible()
  // 没打字：最近看过 + 命令
  await expect(pal.locator('.palette-label', { hasText: '最近看过' })).toBeVisible()
  await expect(pal.locator('.palette-item', { hasText: '同步协议的设计笔记' })).toBeVisible()
  await shot(page, 'palette-recent')
  await page.keyboard.type('同步协议')
  await expect(pal.locator('.palette-item mark').first()).toBeVisible({ timeout: 5000 })
  await page.waitForTimeout(250)
  await shot(page, 'palette-search')
  const offline = await pal.locator('.palette-note').count()
  test.info().annotations.push({ type: '搜索', description: offline ? '服务器搜索不能用，按标题找' : '服务器全文搜索' })
  await page.keyboard.press('Enter')
  await expect(page).toHaveURL(new RegExp(`/p/(${otherId}|${docId})`))
  // 再开：命令“切换浅色 / 深色”
  await page.keyboard.press(`${mod}+k`)
  await page.keyboard.type('深色')
  await expect(pal.locator('.palette-item[aria-selected="true"]')).toContainText('切换浅色')
  await page.keyboard.press('Escape')
  await expect(pal).toBeHidden()
  // 深色截图
  await page.emulateMedia({ colorScheme: 'dark' })
  await page.keyboard.press(`${mod}+k`)
  await page.keyboard.type('搜索')
  await expect(pal.locator('.palette-item mark').first()).toBeVisible({ timeout: 5000 })
  await page.waitForTimeout(200)
  await shot(page, 'palette-search-dark')
  await ctx.close()
})

test('快捷键：⌘/ 帮助；⌘⌥1 转标题、⌘⇧↓ 挪块、⌘D 复制块；⌘⇧L 换主题', async ({ browser, request }) => {
  const { ctx, page } = await newCtx(browser)
  const p = await createPage(request, '快捷键', '第一段\n\n第二段\n\n第三段')
  await openPage(page, p.id)
  await page.keyboard.press(`${mod}+/`)
  await expect(page.locator('.sc-modal')).toBeVisible()
  await shot(page, 'shortcuts')
  await page.keyboard.press('Escape')
  const blocks = () => page.evaluate(() => (window as any).__oad.editor.getJSON().content.map((b: any) => b.type + ':' + (b.content?.map((x: any) => x.text).join('') ?? '')))
  await page.evaluate(() => {
    const e = (window as any).__oad.editor
    e.commands.setTextSelection(3)
    e.view.focus()
  })
  await page.keyboard.press(`${mod}+Alt+1`)
  expect((await blocks())[0]).toBe('heading:第一段')
  await page.keyboard.press(`${mod}+Shift+ArrowDown`)
  expect((await blocks()).slice(0, 2)).toEqual(['paragraph:第二段', 'heading:第一段'])
  await page.keyboard.press(`${mod}+d`)
  expect((await blocks()).filter((x) => x === 'heading:第一段').length).toBe(2)
  const before = await page.evaluate(() => document.documentElement.dataset.theme ?? '')
  await page.keyboard.press(`${mod}+Shift+l`)
  await expect.poll(() => page.evaluate(() => document.documentElement.dataset.theme ?? '')).not.toBe(before)
  await ctx.close()
})

test('页面历史：存一版、改、看对比（按块）、恢复', async ({ browser, request }) => {
  const { ctx, page } = await newCtx(browser)
  await openPage(page, docId)
  // 存一版（有名字）
  await page.locator('[data-tool="history"]').click()
  await expect(page.locator('.hist')).toBeVisible()
  const unavailable = await page.locator('.hist-note', { hasText: '不支持' }).count()
  test.skip(!!unavailable, '服务器还没有页面历史')
  await page.locator('.hist-name-btn').click()
  await page.locator('.hist-name-form input').fill('上线前')
  await page.locator('.hist-name-form button').click()
  await expect(page.locator('.hist-item', { hasText: '上线前' })).toBeVisible()
  // 改：改一段、加一段、删一段
  await request.put(`/api/v1/pages/${docId}/content`, {
    data: { markdown: '## 目标\n\n本周把 **搜索**、页面历史和回收站上线，同时修好离线打开。\n\n- 搜索：中文按词切，标题权重高\n- [ ] 公开分享页面的排版\n\n新加的一段：发布说明写好以后发给大家。\n\n```ts\nconst hits = await search(ws, "同步协议")\n```' },
  })
  await page.waitForTimeout(600)
  await page.locator('.hist-item', { hasText: '上线前' }).click()
  await expect(page.locator('.hv')).toBeVisible()
  await expect(page.locator('.hd.is-changed').first()).toBeVisible({ timeout: 10_000 })
  await expect(page.locator('.hd.is-added').first()).toBeVisible()
  await expect(page.locator('.hd.is-removed').first()).toBeVisible()
  await page.waitForTimeout(200)
  await shot(page, 'history-diff')
  await page.emulateMedia({ colorScheme: 'dark' })
  await shot(page, 'history-diff-dark')
  await page.emulateMedia({ colorScheme: 'light' })
  // 恢复
  page.once('dialog', (d) => d.accept())
  await page.locator('.hv-bar .btn-primary').click()
  await expect(page.locator('.page-toast')).toContainText('已恢复')
  await expect.poll(() => page.evaluate(() => (window as any).__oad.editor.state.doc.textContent as string)).toContain('历史：每一版都能对比、恢复')
  await expect.poll(() => page.evaluate(() => (window as any).__oad.editor.state.doc.textContent as string)).not.toContain('新加的一段')
  // 恢复记成一版
  await expect(page.locator('.hist-item.is-restore').first()).toBeVisible({ timeout: 10_000 })
  await shot(page, 'history-list')
  await ctx.close()
})

test('库的改动记录：改一格，看记录，撤销', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  await page.goto(`/p/${be.id}`)
  await page.waitForSelector('.db[data-status="ready"]')
  const cell = page.locator('[data-cell="0:0"]')
  await cell.dblclick()
  await page.keyboard.press(`${mod}+a`)
  await page.keyboard.type('改过的标题')
  await page.keyboard.press('Enter')
  await expect(cell).toContainText('改过的标题')
  await page.waitForTimeout(400)
  await page.locator('[data-db-tool="history"]').click()
  const item = page.locator('.dbh-item', { hasText: '改过的标题' }).first()
  const unavailable = await page.locator('.dbh .db-empty-note', { hasText: '不支持' }).count()
  test.skip(!!unavailable, '服务器还没有改动记录')
  await expect(item).toBeVisible()
  await shot(page, 'db-history')
  page.once('dialog', (d) => d.accept())
  await item.locator('.db-link-btn').click()
  await expect(cell).not.toContainText('改过的标题', { timeout: 10_000 })
  await ctx.close()
})

test('回收站：删一页，在回收站里看到，恢复；再删，彻底删除', async ({ browser, request }) => {
  const { ctx, page } = await newCtx(browser)
  const p = await createPage(request, '要删掉的会议记录', '一些内容')
  await page.goto('/')
  const row = page.locator('.tree-row', { hasText: '要删掉的会议记录' })
  await expect(row).toBeVisible()
  await request.delete(`/api/v1/pages/${p.id}`)
  await expect(row).toBeHidden({ timeout: 10_000 })
  await page.locator('.side-link', { hasText: '回收站' }).click()
  const item = page.locator('.trash-item', { hasText: '要删掉的会议记录' })
  await expect(item).toBeVisible()
  await shot(page, 'trash')
  await item.locator('.btn', { hasText: '恢复' }).click()
  await expect(item).toBeHidden()
  await expect(page.locator('.tree-row', { hasText: '要删掉的会议记录' })).toBeVisible({ timeout: 10_000 })
  await request.delete(`/api/v1/pages/${p.id}`)
  await page.reload()
  page.once('dialog', (d) => d.accept())
  await page.locator('.trash-item', { hasText: '要删掉的会议记录' }).locator('.btn-danger').click()
  await expect(page.locator('.trash-item', { hasText: '要删掉的会议记录' })).toBeHidden()
  const left = await (await request.get(`/api/v1/workspaces/${ws}/trash`)).json()
  expect((left.items ?? left).some((x: any) => x.id === p.id)).toBe(false)
  await ctx.close()
})

test('分享：公开、复制链接、带子页面；公开页面不用登录、排版和编辑器一致；停止分享', async ({ browser, request }) => {
  const { ctx, page } = await newCtx(browser)
  await createPage(request, '子页面：附录', '附录内容').then((c) => request.patch(`/api/v1/pages/${c.id}`, { data: { parent_id: docId } }))
  await openPage(page, docId)
  await page.locator('[data-tool="share"]').click()
  const pop = page.locator('.share-pop')
  await expect(pop).toBeVisible()
  await pop.locator('.btn-primary', { hasText: '公开这一页' }).click()
  await expect(pop.locator('.share-link input')).toHaveValue(/\/s\//)
  await pop.locator('.share-opt', { hasText: '包括子页面' }).locator('input').check()
  await page.waitForTimeout(300)
  await shot(page, 'share-dialog')
  const url = await pop.locator('.share-link input').inputValue()
  const s = await (await request.get(`/api/v1/pages/${docId}/share`)).json()
  expect(s.include_children).toBe(true)
  // 公开页面：另开一个没有登录信息的浏览器
  for (const scheme of ['light', 'dark'] as const) {
    const anon = await browser.newContext({ colorScheme: scheme, viewport: { width: 1200, height: 900 } })
    const pub = await anon.newPage()
    const res = await pub.goto(url)
    expect(res?.status()).toBe(200)
    await expect(pub.locator('body')).toContainText('发布计划')
    await expect(pub.locator('body')).toContainText('先量，再改')
    expect(await pub.locator('meta[name="robots"]').getAttribute('content')).toContain('noindex')
    await pub.waitForTimeout(400)
    await shot(pub, `public-${scheme}`, true)
    // 排版：和编辑器同一套字体、正文 16px
    if (scheme === 'light') {
      const f = await pub.evaluate(() => {
        const p = [...document.querySelectorAll('p')].find((x) => x.textContent?.includes('本周'))!
        const cs = getComputedStyle(p)
        return { size: cs.fontSize, family: cs.fontFamily, line: cs.lineHeight }
      })
      test.info().annotations.push({ type: '公开页排版', description: JSON.stringify(f) })
      expect(f.size).toBe('16px')
    }
    await anon.close()
  }
  // 停止分享
  page.once('dialog', (d) => d.accept())
  await pop.locator('.link-btn.is-danger').click()
  await expect(pop.locator('.btn-primary', { hasText: '公开这一页' })).toBeVisible()
  const anon = await browser.newContext()
  const gone = await (await anon.newPage()).goto(url)
  expect([404, 410]).toContain(gone?.status())
  await anon.close()
  await ctx.close()
})

// 界面上的固定文字（不是用户内容）里不能有中文
const CHROME = ['.side-new', '.side-link', '.side-label', '.topbar-right', '.db-toolbar', '.db-tabs .db-tab-add', '.set-tabs', '.set-label', '.palette-label', '.palette-foot', '.sc-modal', '.hist-head', '.hist-name', '.trash .settings-head', '.db-th-add', '.db-tfoot']
async function cjkInChrome(page: Page) {
  return page.evaluate((sels) => {
    const bad: string[] = []
    for (const s of sels)
      document.querySelectorAll(s).forEach((el) => {
        const txt = (el as HTMLElement).innerText ?? ''
        if (/[一-鿿]/.test(txt)) bad.push(s + ': ' + txt.replace(/\s+/g, ' ').slice(0, 80))
      })
    return bad
  }, CHROME)
}

test('英文界面：设置里切换，不刷新就全换；跟着账号记住', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  await page.goto('/settings/profile')
  await expect(page.locator('.set-tabs')).toBeVisible()
  await shot(page, 'settings-profile')
  const loads = await page.evaluate(() => performance.getEntriesByType('navigation').length)
  await page.locator('.seg button', { hasText: 'English' }).click()
  await expect(page.locator('.side-new', { hasText: 'New page' })).toBeVisible()
  await expect(page.locator('.set-tab.is-on')).toHaveText('Profile')
  expect(await page.evaluate(() => performance.getEntriesByType('navigation').length)).toBe(loads)
  expect(await page.evaluate(() => document.documentElement.lang)).toBe('en')
  await shot(page, 'settings-profile-en')
  const bad: string[] = []
  bad.push(...(await cjkInChrome(page)))
  await page.goto('/settings/workspace')
  await expect(page.locator('.set-card').first()).toBeVisible()
  bad.push(...(await cjkInChrome(page)))
  await openPage(page, docId)
  await page.locator('[data-tool="history"]').click()
  await page.waitForTimeout(300)
  bad.push(...(await cjkInChrome(page)))
  await shot(page, 'doc-en')
  await page.keyboard.press('Escape')
  await page.keyboard.press(`${mod}+k`)
  await page.waitForTimeout(200)
  bad.push(...(await cjkInChrome(page)))
  await shot(page, 'palette-en')
  await page.keyboard.press('Escape')
  await page.keyboard.press(`${mod}+/`)
  bad.push(...(await cjkInChrome(page)))
  await shot(page, 'shortcuts-en')
  await page.keyboard.press('Escape')
  await page.goto(`/p/${be.id}`)
  await page.waitForSelector('.db[data-status="ready"]')
  bad.push(...(await cjkInChrome(page)))
  await shot(page, 'db-en')
  await page.goto('/trash')
  await page.waitForTimeout(300)
  bad.push(...(await cjkInChrome(page)))
  expect(bad).toEqual([])
  // 换回中文（也走一遍命令面板）
  await page.keyboard.press(`${mod}+k`)
  await page.keyboard.type('中文')
  await page.keyboard.press('Enter')
  await expect(page.locator('.side-new', { hasText: '新建页面' })).toBeVisible()
  await ctx.close()
})

test('设置：工作区（名字、成员、邀请链接）', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  await page.goto('/settings/workspace')
  await expect(page.locator('.set-member').first()).toBeVisible()
  await page.locator('.set-invite-new .btn-primary').click()
  await expect(page.locator('.set-fresh input')).toHaveValue(/\/invite\//)
  await page.waitForTimeout(200)
  await shot(page, 'settings-workspace', true)
  test.info().annotations.push({ type: '假接口', description: [...missing].join('、') || '无' })
  await ctx.close()
})

test('邀请：没登录打开邀请链接，注册并加入', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser)
  // 模拟没登录：/me 401；注册接口没有就用假的
  let signedUp = false
  await page.route(/\/api\/v1\/me$/, async (r) => (signedUp ? r.fallback() : r.fulfill({ status: 401, json: { error: { code: 'unauthorized', message: 'login' } } })))
  await page.route(/\/api\/v1\/auth\/config$/, (r) => r.fulfill({ json: { mode: 'local' } }))
  await page.route(/\/api\/v1\/auth\/signup$/, async (r) => {
    signedUp = true
    const res = await r.fetch().catch(() => null)
    // 测试服务器是不登录模式，真的注册接口会拒绝（不支持注册）：这时用假的
    if (res && res.ok()) return r.fulfill({ response: res })
    missing.add('POST /auth/signup')
    return r.fulfill({ json: await (await r.fetch({ url: r.request().url().replace('/auth/signup', '/me'), method: 'GET' })).json() })
  })
  await page.goto('/invite/tokdemo123')
  await expect(page.locator('.invite-card h1')).toContainText('加入')
  await page.locator('.invite-form input').nth(0).fill('新同事')
  await page.locator('.invite-form input').nth(1).fill('new@example.com')
  await page.locator('.invite-form input').nth(2).fill('a-long-password')
  await shot(page, 'invite-signup')
  await page.locator('.invite-form .btn-primary').click()
  await expect(page).toHaveURL(/\/(p\/.*)?$/)
  await expect(page.locator('.sidebar')).toBeVisible()
  await ctx.close()
})

test('手机（390×844，触屏）：抽屉侧栏、正文、表格横着滑、看板竖排、⌘K', async ({ browser }) => {
  const { ctx, page } = await newCtx(browser, { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true })
  await openPage(page, docId)
  await page.waitForTimeout(300)
  await shot(page, 'mobile-doc')
  // 抽屉
  await page.locator('.topbar .icon-btn').first().tap()
  await expect(page.locator('.sidebar')).toBeVisible()
  await expect(page.locator('.drawer-mask')).toBeVisible()
  await shot(page, 'mobile-drawer')
  await page.locator('.drawer-mask').tap({ position: { x: 370, y: 400 } })
  await expect(page.locator('.app.is-collapsed')).toBeVisible()
  // 库：表格能横着滑
  await page.goto(`/p/${be.id}`)
  await page.waitForSelector('.db[data-status="ready"]')
  const scroll = await page.locator('.db-table').evaluate((el) => ({ sw: el.scrollWidth, cw: el.clientWidth, body: document.documentElement.scrollWidth }))
  expect(scroll.sw).toBeGreaterThan(scroll.cw)
  expect(scroll.body).toBeLessThanOrEqual(392)
  await shot(page, 'mobile-table')
  await page.locator('.db-table').evaluate((el) => (el.scrollLeft = 300))
  await page.waitForTimeout(150)
  await shot(page, 'mobile-table-scrolled')
  // 看板竖排
  await page.locator('.db-tab', { hasText: '看板' }).tap()
  await page.waitForTimeout(300)
  const cols = await page.locator('.db-col').evaluateAll((els) => els.slice(0, 2).map((e) => e.getBoundingClientRect().top))
  expect(cols[1]).toBeGreaterThan(cols[0])
  await shot(page, 'mobile-board')
  // 行面板
  await page.locator('.db-card').first().tap()
  await expect(page.locator('.db-peek')).toBeVisible()
  await page.waitForTimeout(500)
  await shot(page, 'mobile-row')
  await ctx.close()
})

test('PWA：有 manifest 和 service worker，断网能打开看过的页面', async ({ browser }) => {
  const ctx = await browser.newContext({ serviceWorkers: 'allow' })
  const page = await ctx.newPage()
  const man = await (await page.request.get('/manifest.webmanifest')).json()
  expect(man.icons.some((i: any) => i.sizes === '512x512')).toBe(true)
  await openPage(page, docId)
  await page.evaluate(async () => {
    await navigator.serviceWorker.ready
  })
  // 第一次装上时页面还没被它接管：再开一次
  await page.reload()
  await page.waitForFunction(() => !!navigator.serviceWorker.controller, null, { timeout: 15_000 })
  await openPage(page, docId)
  await page.waitForTimeout(1500)
  await ctx.setOffline(true)
  await page.reload()
  await expect(page.locator('.oad-prose, .doc-snapshot').first()).toContainText('先量，再改', { timeout: 15_000 })
  await expect(page.locator('.status')).toHaveAttribute('data-status', /offline|connecting/)
  await shot(page, 'offline')
  await ctx.setOffline(false)
  await ctx.close()
})
