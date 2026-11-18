// 性能：1000 段的页面打开（有 / 没有本机缓存）、打字延迟（keydown → DOM 改完、→ 下一帧）、主包大小。
// 数字打印出来，也写到 apps/web/e2e-results/perf.json。目标见 docs/rewrite-plan.md。
import { expect, test, type Page } from '@playwright/test'
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { gzipSync } from 'node:zlib'
import { bigMarkdown, caretAtEndOfBlock, createPage, openPage } from './helpers'

const ROUNDS = Number(process.env.OAD_PERF_ROUNDS ?? 7)
const results: Record<string, unknown> = {}

const pct = (xs: number[], p: number) => {
  const s = [...xs].sort((a, b) => a - b)
  return +s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))].toFixed(2)
}
const summary = (xs: number[]) => ({ n: xs.length, p50: pct(xs, 50), p95: pct(xs, 95), min: +Math.min(...xs).toFixed(2), max: +Math.max(...xs).toFixed(2) })

// 从导航开始：first = 第一次看到正文（本机快照或编辑器，DOM 里有了再过一帧），full = 编辑器装好、最后一块也在 DOM 里再过一帧
async function timeToContent(page: Page, id: string) {
  await page.goto(`/p/${id}`)
  await page.waitForFunction(() => document.querySelector('.oad-prose[contenteditable]')?.textContent?.includes('最后一段：在这里打字测延迟'), null, { timeout: 30_000 })
  return page.evaluate(
    () =>
      new Promise<{ first: number; full: number; kind: string }>((r) =>
        requestAnimationFrame(() => {
          const w = window as any
          const full = performance.now()
          r({ first: w.__oadFirstContent ?? full, full, kind: w.__oadFirstKind ?? 'editor' })
        }),
      ),
  )
}

test.describe.configure({ mode: 'serial' })

let pageId = ''
let blockCount = 0

test('准备 1000 段的页面', async ({ request }) => {
  const md = bigMarkdown(1000)
  const p = await createPage(request, '性能测试 1000 段', md)
  pageId = p.id
  const json = await (await request.get(`/api/v1/pages/${p.id}/content?format=json`)).json()
  let n = 0
  const walk = (x: any) => {
    if (x.type !== 'text' && x.type !== 'doc' && x.type !== 'hardBreak' && x.type !== 'mathInline') n++
    x.content?.forEach(walk)
  }
  walk(json.doc)
  blockCount = n
  results.page = { topLevel: json.doc.content.length, blocks: n, markdownKB: +(md.length / 1024).toFixed(1) }
  console.log('页面：', results.page)
})

test('打开：没有本机缓存', async ({ browser }) => {
  const first: number[] = []
  const full: number[] = []
  for (let i = 0; i < ROUNDS; i++) {
    const ctx = await browser.newContext()
    const page = await ctx.newPage()
    const r = await timeToContent(page, pageId)
    first.push(r.first)
    full.push(r.full)
    await ctx.close()
  }
  results.openUncached = { firstContent: summary(first), editorFull: summary(full) }
  console.log('没缓存（新浏览器上下文，从导航开始）：', results.openUncached)
})

test('打开：有本机缓存（IndexedDB + 浏览器缓存）', async ({ browser }) => {
  const ctx = await browser.newContext()
  const page = await ctx.newPage()
  await openPage(page, pageId)
  // 让 IndexedDB 写完
  await page.waitForTimeout(800)
  const first: number[] = []
  const full: number[] = []
  const kinds = new Set<string>()
  for (let i = 0; i < ROUNDS; i++) {
    const r = await timeToContent(page, pageId)
    first.push(r.first)
    full.push(r.full)
    kinds.add(r.kind)
    await page.waitForTimeout(300)
  }
  // 应用里切页面：从点侧栏到正文画完
  const other = await createPage(page.request, '另一页')
  const inApp: number[] = []
  for (let i = 0; i < ROUNDS; i++) {
    await page.goto(`/p/${other.id}`)
    await page.waitForFunction(() => !!(window as any).__oad?.editor)
    await page.waitForTimeout(150)
    const t = await page.evaluate(
      (id) =>
        new Promise<number>((resolve) => {
          const t0 = performance.now()
          const row = document.querySelector(`[data-page-id="${id}"]`) as HTMLElement
          const check = () => {
            if (document.querySelector('.oad-prose[contenteditable]')?.textContent?.includes('最后一段：在这里打字测延迟'))
              requestAnimationFrame(() => resolve(performance.now() - t0))
            else requestAnimationFrame(check)
          }
          row.click()
          check()
        }),
      pageId,
    )
    inApp.push(t)
  }
  results.openCached = { reloadFirstContent: summary(first), firstContentFrom: [...kinds], reloadEditorFull: summary(full), inAppSwitchEditorFull: summary(inApp) }
  console.log('有缓存：', results.openCached)
  await ctx.close()
})

async function measureTyping(page: Page, n: number) {
  await page.evaluate(() => {
    const w = window as any
    w.__lat = { dom: [] as number[], frame: [] as number[] }
    const root = document.querySelector('.oad-prose')!
    let t0 = 0
    let pending = false
    const mo = new MutationObserver(() => {
      if (!pending) return
      pending = false
      const t1 = performance.now()
      w.__lat.dom.push(t1 - t0)
      requestAnimationFrame(() => w.__lat.frame.push(performance.now() - t0))
    })
    mo.observe(root, { childList: true, subtree: true, characterData: true })
    window.addEventListener(
      'keydown',
      (e) => {
        if (e.key.length === 1) {
          t0 = e.timeStamp || performance.now()
          pending = true
        }
      },
      true,
    )
  })
  const text = 'typing延迟测试abcdefghij打字'
  for (let i = 0; i < n; i++) {
    await page.keyboard.type(text[i % text.length])
    await page.waitForTimeout(25)
  }
  await page.waitForTimeout(200)
  return page.evaluate(() => (window as any).__lat as { dom: number[]; frame: number[] })
}

test('打字延迟（1000 段的页面，最后一段）', async ({ browser }) => {
  const ctx = await browser.newContext()
  const page = await ctx.newPage()
  await openPage(page, pageId)
  const last = await page.evaluate(() => (window as any).__oad.editor.state.doc.childCount - 1)
  await caretAtEndOfBlock(page, last)
  const lat = await measureTyping(page, 120)
  results.typing = { keydownToDom: summary(lat.dom), keydownToNextFrame: summary(lat.frame) }
  console.log('打字：', results.typing)

  // 同时有另一个人在别处打字、Agent 在改别的段
  const ctx2 = await browser.newContext()
  const other = await ctx2.newPage()
  await openPage(other, pageId)
  await caretAtEndOfBlock(other, 3)
  let stop = false
  const otherTyping = (async () => {
    while (!stop) {
      await other.keyboard.type('x')
      await other.waitForTimeout(40)
    }
  })()
  const blocks = await page.evaluate(() => (window as any).__oad.editor.getJSON().content.map((n: any) => n.attrs.id))
  const agent = (async () => {
    let i = 0
    while (!stop) {
      await page.request.put(`/api/v1/pages/${pageId}/blocks/${blocks[10]}`, { data: { markdown: `Agent 第 ${i++} 次改写这一段。` } })
      await page.waitForTimeout(150)
    }
  })()
  await caretAtEndOfBlock(page, last)
  const lat2 = await measureTyping(page, 120)
  stop = true
  await Promise.all([otherTyping, agent])
  results.typingConcurrent = { keydownToDom: summary(lat2.dom), keydownToNextFrame: summary(lat2.frame) }
  console.log('打字（同时有人和 Agent 在写）：', results.typingConcurrent)
  await ctx2.close()
  await ctx.close()
})

test('主包大小（gzip）', async () => {
  const dist = resolve(dirname(fileURLToPath(import.meta.url)), '../dist')
  const html = readFileSync(join(dist, 'index.html'), 'utf8')
  const entry = /<script type="module"[^>]*src="\/?([^"]+)"/.exec(html)?.[1]
  expect(entry).toBeTruthy()
  const files = readdirSync(join(dist, 'assets'))
  const size = (f: string) => {
    const b = readFileSync(join(dist, 'assets', f))
    return { file: f, kb: +(b.length / 1024).toFixed(1), gzipKB: +(gzipSync(b, { level: 9 }).length / 1024).toFixed(1) }
  }
  const entryFile = entry!.replace(/^assets\//, '')
  const js = files.filter((f) => f.endsWith('.js')).map(size)
  const css = files.filter((f) => f.endsWith('.css')).map(size)
  const main = js.find((f) => f.file === entryFile)!
  const pageView = js.find((f) => f.file.startsWith('PageView'))
  results.bundle = {
    entry: main,
    pageViewChunk: pageView,
    openPageTotalGzipKB: +(main.gzipKB + (pageView?.gzipKB ?? 0) + (css.find((c) => c.file.startsWith('index'))?.gzipKB ?? 0)).toFixed(1),
    allJs: js,
    css,
  }
  console.log('包大小：', JSON.stringify(results.bundle, null, 2))
})

test.afterAll(() => {
  if (!Object.keys(results).length) return
  const dir = resolve(dirname(fileURLToPath(import.meta.url)), '../e2e-results')
  mkdirSync(dir, { recursive: true })
  writeFileSync(join(dir, 'perf.json'), JSON.stringify({ at: new Date().toISOString(), blockCount, ...results }, null, 2))
})
