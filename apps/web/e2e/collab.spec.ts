// 两个浏览器同时改同一页：收敛、互相看到光标；断网编辑、重连后合并；Agent 通过 HTTP 改一段，网页马上看到，人打的字不丢。
import { expect, test, type Browser, type Page } from '@playwright/test'
import { caretAtEndOfBlock, createPage, docJSON, docText, openPage } from './helpers'

const START = '# 协作测试\n\n第一段。\n\n第二段。\n\n第三段。\n'

async function two(browser: Browser) {
  const a = await browser.newContext()
  const b = await browser.newContext()
  return { a, b, pa: await a.newPage(), pb: await b.newPage() }
}

async function converged(pa: Page, pb: Page) {
  await expect.poll(async () => JSON.stringify(await docJSON(pa)) === JSON.stringify(await docJSON(pb)), { timeout: 10_000 }).toBe(true)
}

test('两个浏览器同时编辑，结果一致，互相看到光标', async ({ browser, request }) => {
  const page = await createPage(request, '协作', START)
  const { a, b, pa, pb } = await two(browser)
  await openPage(pa, page.id)
  await openPage(pb, page.id)

  // 同时打字：一个在第一段末尾，一个在第二段末尾
  await caretAtEndOfBlock(pa, 1)
  await caretAtEndOfBlock(pb, 2)
  await Promise.all([pa.keyboard.type('甲在打字ABC', { delay: 15 }), pb.keyboard.type('乙也在打字xyz', { delay: 15 })])
  await converged(pa, pb)
  let text = await docText(pa)
  expect(text).toContain('第一段。甲在打字ABC')
  expect(text).toContain('第二段。乙也在打字xyz')

  // 同一处同时打字：两人的字会交错（CRDT 的正常结果），但各自的字顺序不乱、两边一样
  await caretAtEndOfBlock(pa, 3)
  await caretAtEndOfBlock(pb, 3)
  await Promise.all([pa.keyboard.type('abcdefgh', { delay: 12 }), pb.keyboard.type('12345678', { delay: 12 })])
  await converged(pa, pb)
  text = await docText(pa)
  const tail = text.slice(text.indexOf('第三段。') + 4)
  expect(tail.replace(/[^a-h]/g, '')).toBe('abcdefgh')
  expect(tail.replace(/[^1-8]/g, '')).toBe('12345678')

  await caretAtEndOfBlock(pb, 3)
  await pb.keyboard.press('Enter')
  await pb.keyboard.type('乙加的新段落')
  await converged(pa, pb)
  expect(await docText(pa)).toContain('乙加的新段落')

  // 光标：对方的光标带名字标签
  await expect(pa.locator('.oad-caret')).toHaveCount(1)
  await expect(pb.locator('.oad-caret')).toHaveCount(1)
  await expect(pa.locator('.oad-caret-label')).toHaveText(/\S/)

  // 新段落拿到了块编号，两边一样，没有重复
  const ids = await pa.evaluate(() => {
    const out: string[] = []
    ;(window as any).__oad.editor.state.doc.descendants((n: any) => {
      if (n.isBlock) out.push(n.attrs.id)
    })
    return out
  })
  expect(ids.every((i) => /^[0-9a-zA-Z]{8}$/.test(i))).toBe(true)
  expect(new Set(ids).size).toBe(ids.length)

  // 标题也是协作的
  await pa.bringToFront()
  await pa.locator('textarea.oad-title').focus()
  await pa.locator('textarea.oad-title').press('End')
  await pa.keyboard.type('（已改）')
  await expect(pb.locator('textarea.oad-title')).toHaveValue('协作（已改）')
  // 侧栏也跟着变（服务器保存后经 SSE 推过来）
  await expect(pb.locator(`[data-page-id="${page.id}"] .tree-title`)).toHaveText('协作（已改）', { timeout: 10_000 })

  await a.close()
  await b.close()
})

test('断网编辑，重连后合并', async ({ browser, request }) => {
  const page = await createPage(request, '断网', START)
  const { a, b, pa, pb } = await two(browser)
  await openPage(pa, page.id)
  await openPage(pb, page.id)

  await b.setOffline(true)
  // Chromium 断网模拟不一定掐掉已经连着的 WebSocket，再从应用里断一次
  await pb.evaluate(() => (window as any).__oad.provider.disconnect())
  await expect(pb.locator('[data-status="offline"]')).toBeVisible()

  await caretAtEndOfBlock(pb, 2)
  await pb.keyboard.type('乙断网时写的')
  await caretAtEndOfBlock(pa, 3)
  await pa.keyboard.type('甲在线写的')
  await pa.waitForTimeout(300)
  expect(await docText(pa)).not.toContain('乙断网时写的')

  await b.setOffline(false)
  await pb.evaluate(() => (window as any).__oad.provider.connect())
  await expect(pb.locator('[data-status="synced"]')).toBeVisible({ timeout: 15_000 })
  await converged(pa, pb)
  const text = await docText(pa)
  expect(text).toContain('乙断网时写的')
  expect(text).toContain('甲在线写的')

  // 刷新后从本机缓存 + 服务器读出来还是一样
  await pb.reload()
  await pb.waitForFunction(() => !!(window as any).__oad?.editor)
  await expect.poll(() => docText(pb)).toContain('乙断网时写的')

  await a.close()
  await b.close()
})

test('Agent 改一段，网页马上看到，人正在打的字不丢', async ({ browser, request }) => {
  const page = await createPage(request, 'Agent', START)
  const ctx = await browser.newContext()
  const p = await ctx.newPage()
  await openPage(p, page.id)

  const blocks = await p.evaluate(() => (window as any).__oad.editor.getJSON().content.map((n: any) => n.attrs.id))
  await caretAtEndOfBlock(p, 1)
  const typing = p.keyboard.type('人在第一段打字', { delay: 20 })
  // 同时 Agent 改第三段、在后面插一段
  const t0 = Date.now()
  const put = await request.put(`/api/v1/pages/${page.id}/blocks/${blocks[3]}`, { data: { markdown: 'Agent 改写了第三段。' } })
  expect(put.ok(), await put.text()).toBeTruthy()
  const ins = await request.post(`/api/v1/pages/${page.id}/blocks`, { data: { markdown: '> Agent 插入的引用', after: blocks[3] } })
  expect(ins.ok(), await ins.text()).toBeTruthy()
  await expect.poll(() => docText(p), { timeout: 5_000 }).toContain('Agent 改写了第三段。')
  const seenMs = Date.now() - t0
  await typing
  await expect.poll(() => docText(p)).toContain('Agent 插入的引用')
  const text = await docText(p)
  expect(text).toContain('第一段。人在第一段打字')
  // 被改的块沿用编号
  const after = await p.evaluate(() => (window as any).__oad.editor.getJSON().content.map((n: any) => n.attrs.id))
  expect(after).toContain(blocks[3])
  console.log(`Agent 改一段 → 网页看到：${seenMs} ms（含两次 HTTP）`)

  // 服务器读到的和网页一致
  const md = await (await request.get(`/api/v1/pages/${page.id}/content?format=markdown`)).json()
  expect(md.markdown).toContain('人在第一段打字')
  await ctx.close()
})
