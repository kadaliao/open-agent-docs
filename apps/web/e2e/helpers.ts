import { expect, type APIRequestContext, type Page } from '@playwright/test'

export interface PageInfo {
  id: string
  title: string
}

export async function workspace(request: APIRequestContext): Promise<string> {
  const me = await (await request.get('/api/v1/me')).json()
  return me.workspaces[0].id
}

export async function createPage(request: APIRequestContext, title: string, markdown?: string): Promise<PageInfo> {
  const ws = await workspace(request)
  const r = await request.post(`/api/v1/workspaces/${ws}/pages`, { data: { title } })
  expect(r.ok()).toBeTruthy()
  const page = await r.json()
  if (markdown !== undefined) {
    const put = await request.put(`/api/v1/pages/${page.id}/content`, { data: { markdown } })
    expect(put.ok(), await put.text()).toBeTruthy()
  }
  return page
}

// 等编辑器挂上、同步好
export async function openPage(page: Page, id: string) {
  await page.goto(`/p/${id}`)
  await page.waitForFunction(() => !!(window as any).__oad?.editor, null, { timeout: 15_000 })
  await expect(page.locator('[data-status="synced"]')).toBeVisible({ timeout: 15_000 })
}

export const docJSON = (page: Page) => page.evaluate(() => (window as any).__oad.editor.getJSON())
export const docText = (page: Page) => page.evaluate(() => (window as any).__oad.editor.state.doc.textContent as string)

// 把光标放到第 n 个顶层块的末尾
export async function caretAtEndOfBlock(page: Page, index: number) {
  await page.bringToFront()
  await page.evaluate((i) => {
    const { editor } = (window as any).__oad
    const doc = editor.state.doc
    let pos = 0
    for (let k = 0; k <= i; k++) pos += doc.child(k).nodeSize
    // 直接设光标、聚焦（Tiptap 的 focus 命令要等一帧）
    editor.commands.setTextSelection(pos - 1)
    editor.view.focus()
  }, index)
  await page.waitForFunction(() => (window as any).__oad.editor.view.hasFocus(), null, { timeout: 5_000 })
}

// 1000 段左右的中英混合文档：段落、标题、列表、待办、代码、引用、表格（比例大致照 docs/baseline.md）
export function bigMarkdown(blocks = 1000, seed = 42): string {
  let s = seed
  const rnd = () => ((s = (s * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff)
  const zh = '我们 今天 讨论 同步 协议 编辑器 服务器 性能 测试 页面 文档 数据库 搜索 结果 用户 团队 计划 进展 问题 方案 需要 已经 可以 这个 一个 时候 因为 所以 但是 然后'.split(' ')
  const en = 'the sync editor server latency page block agent update merge offline cache render index query table'.split(' ')
  const word = () => (rnd() < 0.8 ? zh[Math.floor(rnd() * zh.length)] : ' ' + en[Math.floor(rnd() * en.length)] + ' ')
  const sentence = (n: number) => {
    let out = ''
    for (let i = 0; i < n; i++) {
      let w = word()
      const r = rnd()
      if (r < 0.04) w = `**${w.trim()}**`
      else if (r < 0.06) w = `\`${w.trim()}\``
      else if (r < 0.07) w = `[${w.trim()}](https://example.com/${i})`
      out += w
    }
    return out.replace(/\s+/g, ' ').trim() + '。'
  }
  const out: string[] = []
  let n = 0
  while (n < blocks) {
    const r = rnd()
    if (r < 0.4) {
      out.push(sentence(18 + Math.floor(rnd() * 30)))
      n++
    } else if (r < 0.48) {
      out.push('#'.repeat(1 + Math.floor(rnd() * 3)) + ' ' + sentence(4).replace('。', ''))
      n++
    } else if (r < 0.66) {
      const k = 3
      out.push(Array.from({ length: k }, () => '- ' + sentence(8)).join('\n'))
      n += k
    } else if (r < 0.74) {
      const k = 3
      out.push(Array.from({ length: k }, (_, i) => `${i + 1}. ` + sentence(8)).join('\n'))
      n += k
    } else if (r < 0.79) {
      out.push(Array.from({ length: 2 }, () => `- [${rnd() < 0.5 ? 'x' : ' '}] ` + sentence(6)).join('\n'))
      n += 2
    } else if (r < 0.89) {
      const rows = 3
      out.push(['| 名称 | 数值 | 说明 |', '| --- | --- | --- |', ...Array.from({ length: rows }, (_, i) => `| 项目${i} | ${Math.floor(rnd() * 1000)} | ${sentence(3)} |`)].join('\n'))
      n += rows + 1
    } else if (r < 0.94) {
      out.push('```ts\n' + `const x${n} = await sync(page, { retry: ${n % 5} })\nconsole.log(x${n})` + '\n```')
      n++
    } else {
      out.push('> ' + sentence(14))
      n++
    }
  }
  return out.join('\n\n') + '\n\n最后一段：在这里打字测延迟。\n'
}
