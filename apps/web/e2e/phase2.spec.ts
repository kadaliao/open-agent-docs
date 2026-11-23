// 第 2 阶段：在网页上建 Agent（token 只显示一次）；选中文字写评论 @Agent → Agent 经长轮询收到通知 → 经 HTTP 回复 → 网页实时看到；
// 别的客户端在评论的文字前面、中间改字，锚点跟着走；解决 / 重新打开；收件箱。
import { expect, test, type APIRequestContext, type Page } from '@playwright/test'
import { createPage, openPage, workspace } from './helpers'

test.describe.configure({ mode: 'serial' })

let agent: { id: string; name: string; token: string } | null = null

const bearer = (token: string) => ({ Authorization: `Bearer ${token}` })

async function ensureAgent(request: APIRequestContext) {
  if (agent) return agent
  const ws = await workspace(request)
  const r = await request.post(`/api/v1/workspaces/${ws}/agents`, { data: { name: 'helper' + Date.now().toString(36), about: '测试用', scope: 'write' } })
  expect(r.ok(), await r.text()).toBeTruthy()
  const j = await r.json()
  agent = { id: j.agent.id, name: j.agent.name, token: j.token }
  return agent
}

// 选中第 index 个顶层块里的一段文字
async function selectText(page: Page, index: number, text: string) {
  await page.bringToFront()
  await page.evaluate(
    ([i, s]) => {
      const { editor } = (window as any).__oad
      const doc = editor.state.doc
      let pos = 0
      for (let k = 0; k < (i as number); k++) pos += doc.child(k).nodeSize
      const block = doc.child(i as number)
      const off = block.textContent.indexOf(s as string)
      if (off < 0) throw new Error('找不到 ' + s)
      editor.commands.setTextSelection({ from: pos + 1 + off, to: pos + 1 + off + (s as string).length })
      editor.view.focus()
    },
    [index, text] as const,
  )
}

async function comment(page: Page, quote: string, block: number, typed: string, mention?: string) {
  await selectText(page, block, quote)
  const btn = page.locator('.oad-bubble button', { hasText: '评论' })
  await expect(btn).toBeVisible()
  await btn.click()
  const box = page.locator('.oad-thread.oad-is-draft textarea')
  await expect(box).toBeFocused()
  await box.pressSequentially(typed)
  if (mention) {
    await box.pressSequentially(' @' + mention)
    await expect(page.locator('.oad-mention-item', { hasText: mention })).toBeVisible()
    await box.press('Enter')
    await box.pressSequentially('看一下')
  }
  await box.press('Control+Enter')
  await expect(page.locator('.oad-thread.oad-is-draft')).toHaveCount(0)
}

test('在网页上建 Agent，token 只显示一次', async ({ page, request }) => {
  await page.goto('/settings/agents')
  await page.getByRole('button', { name: '新建 Agent' }).click()
  const name = 'bot' + Date.now().toString(36)
  await page.locator('.modal input[name=name]').fill(name)
  await page.locator('.modal input[name=about]').fill('回复评论的测试 Agent')
  await page.locator('.modal .scope-opt', { hasText: '读写' }).click()
  await page.locator('.modal button.btn-primary', { hasText: '创建' }).click()

  const token = (await page.getByTestId('agent-token').textContent())!.trim()
  expect(token).toMatch(/^oad_[0-9A-Za-z]{20,}$/)
  await expect(page.getByTestId('agent-webhook-secret')).not.toBeEmpty()
  await expect(page.getByTestId('agent-claude-cmd')).toContainText(`claude mcp add --transport http oad`)
  await expect(page.getByTestId('agent-claude-cmd')).toContainText(`/mcp --header "Authorization: Bearer ${token}"`)
  await page.getByRole('button', { name: '完成' }).click()

  // 关掉以后页面上再也看不到 token
  await expect(page.locator(`.agent-row[data-agent="${name}"]`)).toBeVisible()
  expect(await page.content()).not.toContain(token)
  await page.reload()
  await expect(page.locator(`.agent-row[data-agent="${name}"]`)).toBeVisible()
  expect(await page.content()).not.toContain(token)

  // token 能用
  const ws = await workspace(request)
  const r = await request.get(`/api/v1/workspaces/${ws}/pages`, { headers: bearer(token) })
  expect(r.status()).toBe(200)
  const list = await (await request.get(`/api/v1/workspaces/${ws}/agents`)).json()
  agent = { id: list.agents.find((a: any) => a.name === name).id, name, token }
})

test('评论 @Agent → Agent 收到通知 → 回复 → 网页实时看到；收件箱', async ({ page, request }) => {
  const a = await ensureAgent(request)
  const doc = await createPage(request, '评论测试', '第一段：这里有一句需要讨论的话。\n\n第二段。\n')
  // 先把游标挪到最新，只看这次的通知
  const before = await (await request.get('/api/v1/agent/notifications?wait=0&limit=100', { headers: bearer(a.token) })).json()
  const after = Math.max(0, ...(before.notifications ?? before).map((n: any) => n.id))

  await openPage(page, doc.id)
  await comment(page, '需要讨论的话', 0, '这句话说得不清楚', a.name)
  const th = page.locator('.oad-thread').first()
  await expect(th.locator('.oad-comment-body .oad-mention.oad-is-agent')).toHaveText('@' + a.name)
  await expect(page.locator('.oad-prose .oad-comment-mark')).toHaveText('需要讨论的话')

  // Agent 长轮询收通知
  const r = await request.get(`/api/v1/agent/notifications?after=${after}&wait=20`, { headers: bearer(a.token) })
  expect(r.ok(), await r.text()).toBeTruthy()
  const got = await r.json()
  const notes = got.notifications ?? got
  const n = notes.find((x: any) => x.page?.id === doc.id)
  expect(n, JSON.stringify(notes)).toBeTruthy()
  expect(n.type).toBe('comment')
  expect(n.quote).toBe('需要讨论的话')
  expect(n.comment.body).toContain(`(agent:${a.id})`)
  expect(typeof n.prompt).toBe('string')
  expect(n.prompt.length).toBeGreaterThan(20)

  // Agent 回复，网页马上看到
  const rep = await request.post(`/api/v1/threads/${n.thread_id}/comments`, { headers: bearer(a.token), data: { body: '已改成更清楚的说法，**请确认**。' } })
  expect(rep.ok(), await rep.text()).toBeTruthy()
  await expect(th.locator('.oad-comment')).toHaveCount(2, { timeout: 5_000 })
  await expect(th.locator('.oad-comment').nth(1).locator('.oad-comment-author')).toHaveText(a.name)
  await expect(th.locator('.oad-comment').nth(1).locator('strong')).toHaveText('请确认')
  await request.post('/api/v1/agent/notifications/ack', { headers: bearer(a.token), data: { ids: [n.id] } })

  // 收件箱：自己参与的那串有新回复
  const bell = page.getByTestId('inbox-count')
  await expect(bell).toBeVisible({ timeout: 5_000 })
  // 先离开这页，从收件箱点回来
  const other = await createPage(request, '别的页')
  await page.goto(`/p/${other.id}`)
  await page.locator('.inbox-btn').click()
  const item = page.locator('.inbox-item', { hasText: '评论测试' }).first()
  await expect(item).toBeVisible()
  await item.click()
  await expect(page).toHaveURL(new RegExp(`/p/${doc.id}\\?thread=${n.thread_id}`))
  await expect(page.locator(`.oad-thread.oad-is-active[data-thread-id="${n.thread_id}"]`)).toBeVisible()
  await expect(page.locator('.oad-prose .oad-comment-mark.oad-is-active')).toHaveText('需要讨论的话')
})

test('别的客户端在评论文字的前面、中间改字，锚点跟着走；解决 / 重新打开', async ({ browser, request }) => {
  const doc = await createPage(request, '锚点', '前面的字。这里是被评论的一段文字，后面还有。\n')
  const ca = await browser.newContext()
  const cb = await browser.newContext()
  const pa = await ca.newPage()
  const pb = await cb.newPage()
  await openPage(pa, doc.id)
  await openPage(pb, doc.id)
  await comment(pa, '被评论的一段文字', 0, '锚点测试')
  await expect(pb.locator('.oad-prose .oad-comment-mark')).toHaveText('被评论的一段文字', { timeout: 5_000 })

  // B 在前面和中间插字
  await pb.bringToFront()
  await pb.evaluate(() => {
    const { editor } = (window as any).__oad
    editor.view.dispatch(editor.state.tr.insertText('【新】', 1))
    const t = editor.state.doc.firstChild.textContent as string
    const at = 1 + t.indexOf('一段文字')
    editor.view.dispatch(editor.state.tr.insertText('（插入）', at))
  })
  await expect(pa.locator('.oad-prose .oad-comment-mark')).toHaveText('被评论的（插入）一段文字', { timeout: 5_000 })
  // 刷新以后从服务器读回来也对
  await pb.reload()
  await pb.waitForFunction(() => !!(window as any).__oad?.editor)
  await expect(pb.locator('.oad-prose .oad-comment-mark')).toHaveText('被评论的（插入）一段文字', { timeout: 10_000 })

  // 解决：正文里的底色没了，评论去了“已解决”；重新打开又回来；B 实时看到
  const th = pa.locator('.oad-thread').first()
  await th.click()
  await th.locator('.oad-thread-resolve').click()
  await expect(pa.locator('.oad-prose .oad-comment-mark')).toHaveCount(0)
  await expect(pb.locator('.oad-prose .oad-comment-mark')).toHaveCount(0, { timeout: 5_000 })
  await expect(pa.locator('.oad-seg button', { hasText: '已解决' })).toHaveAttribute('aria-selected', 'true')
  await pa.locator('.oad-thread').first().locator('.oad-thread-resolve').click()
  await expect(pa.locator('.oad-prose .oad-comment-mark')).toHaveText('被评论的（插入）一段文字')
  await expect(pb.locator('.oad-prose .oad-comment-mark')).toHaveText('被评论的（插入）一段文字', { timeout: 5_000 })
  await ca.close()
  await cb.close()
})

test('回复、编辑、删除自己的评论', async ({ page, request }) => {
  const doc = await createPage(request, '编辑评论', '一段可以评论的文字。\n')
  await openPage(page, doc.id)
  await comment(page, '可以评论', 0, '第一条')
  const th = page.locator('.oad-thread').first()
  await th.locator('textarea').fill('第二条回复')
  await th.locator('textarea').press('Control+Enter')
  await expect(th.locator('.oad-comment')).toHaveCount(2)
  const second = th.locator('.oad-comment').nth(1)
  await second.hover()
  await second.getByRole('button', { name: '编辑' }).click()
  await second.locator('textarea').fill('改过的回复')
  await second.locator('textarea').press('Control+Enter')
  await expect(second.locator('.oad-comment-body')).toHaveText('改过的回复')
  await expect(second.locator('.oad-comment-time')).toContainText('已编辑')
  page.once('dialog', (d) => d.accept())
  await second.hover()
  await second.getByRole('button', { name: '删除' }).click()
  await expect(th.locator('.oad-comment-body', { hasText: '改过的回复' })).toHaveCount(0)
})
