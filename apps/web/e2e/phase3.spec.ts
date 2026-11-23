// 第 3 阶段：贴网址出卡片 + 小菜单、换显示样式、块把手拖动和块菜单、代码高亮和 mermaid、图片改宽度和图注、上传。
// 服务器的 /api/v1/unfurl、/files 有了就用真的；还没有时用 fixtures/unfurl.json 和假的上传接口（测的是网页这边）。
import { expect, test, type BrowserContext, type Page } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createPage, openPage } from './helpers'

const here = dirname(fileURLToPath(import.meta.url))
const fx = JSON.parse(readFileSync(join(here, 'fixtures/unfurl.json'), 'utf8'))
const jpg = readFileSync(join(here, 'fixtures/yt.jpg'))

let realUnfurl = false
let realFiles = false

test.beforeAll(async ({ request }) => {
  const r = await request.get('/api/v1/unfurl?url=' + encodeURIComponent('https://example.com/'))
  realUnfurl = r.status() !== 404 || !(await r.text()).includes('没有这个接口')
  const me = await (await request.get('/api/v1/me')).json()
  const f = await request.post(`/api/v1/workspaces/${me.workspaces[0].id}/files`, { multipart: { file: { name: 'probe.txt', mimeType: 'text/plain', buffer: Buffer.from('probe') } } })
  realFiles = f.ok()
  console.log(`服务器的 unfurl：${realUnfurl ? '有' : '没有，用 fixtures'}；上传：${realFiles ? '有' : '没有，用假的'}`)
})

async function mockServer(ctx: BrowserContext) {
  if (!realUnfurl)
    await ctx.route('**/api/v1/unfurl?**', (r) => {
      const u = new URL(r.request().url()).searchParams.get('url')!
      return fx[u] ? r.fulfill({ json: fx[u] }) : r.fulfill({ status: 404, json: { error: { code: 'not_found', message: '没有这个接口' } } })
    })
  // 卡片里的外站图片不依赖外网
  await ctx.route(/^https:\/\/(i\.ytimg\.com|pbs\.twimg\.com|i\d\.hdslb\.com|avatars\.githubusercontent\.com)\//, (r) => r.fulfill({ body: jpg, contentType: 'image/jpeg' }))
  if (!realFiles) {
    await ctx.route('**/api/v1/workspaces/*/files', (r) =>
      r.fulfill({ json: { id: 'f1', url: '/api/v1/files/f1/cover.jpg', name: 'cover.jpg', mime: 'image/jpeg', size: jpg.length, width: 480, height: 360, blurhash: 'LEHV6nWB2yk8pyo0adR*.7kCMdnj' } }),
    )
    await ctx.route('**/api/v1/files/f1/**', (r) => r.fulfill({ body: jpg, contentType: 'image/jpeg' }))
  }
}

async function setup(browser: import('@playwright/test').Browser, _doc: unknown[]) {
  const ctx = await browser.newContext()
  await mockServer(ctx)
  const page = await ctx.newPage()
  return { ctx, page }
}

const paste = (page: Page, text: string) =>
  page.evaluate((t) => {
    const e = (window as any).__oad.editor
    e.view.focus()
    const dt = new DataTransfer()
    dt.setData('text/plain', t)
    e.view.dom.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }))
  }, text)

const json = (page: Page) => page.evaluate(() => (window as any).__oad.editor.getJSON().content as any[])

test('空行里贴 X 链接：马上出卡片和小菜单，选“书签”换样子；块菜单里还能换回来', async ({ browser, request }) => {
  const p = await createPage(request, '贴链接', '第一段。\n')
  const { ctx, page } = await setup(browser, [])
  await openPage(page, p.id)
  await page.evaluate(() => {
    const e = (window as any).__oad.editor
    e.chain().focus('end').run()
    e.commands.enter()
  })
  const url = 'https://x.com/vercel/status/1683920951807971329'
  await paste(page, url)
  await expect(page.locator('.oad-embed')).toHaveCount(1)
  await expect(page.locator('.oad-paste-menu')).toBeVisible()
  // 卡片：骨架很快换成帖子
  await expect(page.locator('.oad-c-x .oad-c-x-name')).toContainText('Vercel', { timeout: 5_000 })
  await expect(page.locator('.oad-c-x .oad-c-x-linkcard')).toBeVisible()
  // 选“书签”
  await page.keyboard.press('ArrowRight')
  await page.keyboard.press('Enter')
  await expect(page.locator('.oad-paste-menu')).toHaveCount(0)
  await expect(page.locator('.oad-c-bookmark')).toBeVisible()
  expect((await json(page)).find((n) => n.type === 'embed').attrs).toMatchObject({ url, view: 'bookmark' })
  // 块菜单 → 显示成“提及”
  const box = (await page.locator('.oad-embed').boundingBox())!
  await page.mouse.move(box.x + 40, box.y + 20)
  await page.locator('.oad-handle').click()
  await page.locator('.oad-block-menu button', { hasText: '提及' }).click()
  await expect(page.locator('.oad-prose .oad-link-mention')).toBeVisible()
  expect((await json(page)).some((n) => n.content?.some((c: any) => c.type === 'linkMention' && c.attrs.url === url))).toBe(true)
  // 文字中间贴网址 → 普通链接
  await page.evaluate(() => {
    const e = (window as any).__oad.editor
    e.commands.setTextSelection(3)
  })
  await paste(page, 'https://github.com/rust-lang/rust')
  await expect(page.locator('.oad-prose a[href="https://github.com/rust-lang/rust"]:not(.oad-link-mention)')).toBeVisible()
  await ctx.close()
})

test('块把手：拖动排序；块菜单转换类型、复制一份、删除', async ({ browser, request }) => {
  const p = await createPage(request, '把手', '甲段。\n\n乙段。\n\n丙段。\n')
  const { ctx, page } = await setup(browser, [])
  await openPage(page, p.id)
  const texts = async () => (await json(page)).map((n) => n.content?.[0]?.text ?? '')
  const ids0 = (await json(page)).map((n) => n.attrs.id)
  // 把“丙段”拖到最前面
  const third = (await page.locator('.oad-prose > p').nth(2).boundingBox())!
  await page.mouse.move(third.x + 30, third.y + 10)
  const handle = page.locator('.oad-handle')
  await expect(handle).toBeVisible()
  await handle.dragTo(page.locator('.oad-prose > p').nth(0), { targetPosition: { x: 20, y: 2 } })
  await expect.poll(texts).toEqual(['丙段。', '甲段。', '乙段。'])
  // 移动后编号不变
  const ids1 = (await json(page)).map((n) => n.attrs.id)
  expect([...ids1].sort()).toEqual([...ids0].sort())
  // 转换成一级标题
  const b = (await page.locator('.oad-prose > *').nth(0).boundingBox())!
  await page.mouse.move(b.x + 30, b.y + 10)
  await handle.click()
  await page.locator('.oad-block-menu button', { hasText: '一级标题' }).click()
  await expect(page.locator('.oad-prose > h1')).toHaveText('丙段。')
  // 复制一份（新编号）、删除
  await page.mouse.move(b.x + 30, b.y + 10)
  await handle.click()
  await page.locator('.oad-block-menu button', { hasText: '复制一份' }).click()
  await expect(page.locator('.oad-prose > h1')).toHaveCount(2)
  const hs = (await json(page)).filter((n) => n.type === 'heading').map((n) => n.attrs.id)
  expect(new Set(hs).size).toBe(2)
  const b2 = (await page.locator('.oad-prose > h1').nth(1).boundingBox())!
  await page.mouse.move(b2.x + 30, b2.y + 10)
  await handle.click()
  await page.locator('.oad-block-menu button', { hasText: '删除' }).click()
  await expect(page.locator('.oad-prose > h1')).toHaveCount(1)
  // 打开 #块编号 的链接：那一块闪一下
  const id = (await json(page))[2].attrs.id
  await page.goto(`/p/${p.id}#${id}`)
  await page.waitForFunction(() => !!(window as any).__oad?.editor)
  await expect(page.locator(`.oad-prose [data-id="${id}"].oad-flash`)).toBeVisible({ timeout: 5_000 })
  await ctx.close()
})

test('代码块高亮、mermaid 画图；图片改宽度、图注、大图', async ({ browser, request }) => {
  const p = await createPage(request, '代码和图')
  const doc = {
    type: 'doc',
    content: [
      { type: 'codeBlock', attrs: { language: 'rust' }, content: [{ type: 'text', text: 'fn main() {\n    println!("你好");\n}' }] },
      { type: 'codeBlock', attrs: { language: 'mermaid' }, content: [{ type: 'text', text: 'graph LR\n  A --> B' }] },
      { type: 'image', attrs: { src: 'https://pbs.twimg.com/media/demo.jpg', alt: '图' } },
      { type: 'paragraph' },
    ],
  }
  expect((await request.put(`/api/v1/pages/${p.id}/content`, { data: { doc } })).ok()).toBeTruthy()
  const { ctx, page } = await setup(browser, [])
  await openPage(page, p.id)
  await expect(page.locator('.oad-code code span[style*="--shiki-light"]').first()).toBeVisible({ timeout: 10_000 })
  await expect(page.locator('.oad-mermaid svg')).toBeVisible({ timeout: 10_000 })
  // 切到源码再切回图
  await page.locator('.oad-code.oad-is-mermaid .oad-code-mode').click()
  await expect(page.locator('.oad-code.oad-is-mermaid pre')).toBeVisible()
  await page.locator('.oad-code.oad-is-mermaid .oad-code-mode').click()
  // 自动换行开关写进文档
  await page.locator('.oad-code').first().hover()
  await page.locator('.oad-code').first().getByRole('button', { name: '自动换行' }).click()
  expect((await json(page))[0].attrs.wrap).toBe(true)
  // 图片：拖右边改宽度
  const img = page.locator('.oad-image')
  await img.scrollIntoViewIfNeeded()
  await expect(page.locator('.oad-image.oad-is-loaded')).toBeVisible()
  await img.hover()
  const h = (await page.locator('.oad-image-handle.oad-is-right').boundingBox())!
  await page.mouse.move(h.x + 3, h.y + 20)
  await page.mouse.down()
  await page.mouse.move(h.x - 60, h.y + 20, { steps: 6 })
  await page.mouse.up()
  await expect.poll(async () => (await json(page)).find((n) => n.type === 'image').attrs.width).toBeGreaterThan(9)
  // 图注
  await img.click()
  await page.locator('.oad-image-caption').fill('一张测试图')
  await page.locator('.oad-image-caption').press('Enter')
  await expect.poll(async () => (await json(page)).find((n) => n.type === 'image').attrs.caption).toBe('一张测试图')
  // 大图
  await img.hover()
  await page.locator('.oad-image-zoom').click()
  await expect(page.locator('.oad-lightbox')).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(page.locator('.oad-lightbox')).toHaveCount(0)
  await ctx.close()
})

test('贴图片：先显示本地预览和进度，传完换成服务器地址', async ({ browser, request }) => {
  const p = await createPage(request, '上传', '第一段。\n')
  const { ctx, page } = await setup(browser, [])
  await openPage(page, p.id)
  await page.evaluate(async (b64) => {
    const e = (window as any).__oad.editor
    e.chain().focus('end').run()
    e.commands.enter()
    const bin = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))
    const dt = new DataTransfer()
    dt.items.add(new File([bin], 'cover.jpg', { type: 'image/jpeg' }))
    e.view.dom.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }))
  }, jpg.toString('base64'))
  await expect.poll(async () => (await json(page)).find((n) => n.type === 'image')?.attrs.src ?? '', { timeout: 10_000 }).toMatch(/^\/api\/v1\/files\//)
  await expect(page.locator('.oad-upload')).toHaveCount(0)
  // 共享文档里从来没有 blob: 地址
  expect(JSON.stringify(await json(page))).not.toContain('blob:')
  await expect(page.locator('.oad-image.oad-is-loaded')).toBeVisible({ timeout: 10_000 })
  await ctx.close()
})
