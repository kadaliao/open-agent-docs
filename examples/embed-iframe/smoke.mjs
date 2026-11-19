// 冒烟测试：起 Open Agent Docs（不登录模式，带网页）、建一页，宿主页面用 iframe 嵌进来：
// 看到正文、没有侧栏和顶栏、收到 oad:ready；宿主发 oad:theme / oad:locale 能换主题和语言；在 iframe 里打字存得上。
//   OAD_BIN=/path/to/open-agent-docs node smoke.mjs     （网页用仓库里 apps/web/dist，先 pnpm --filter @oad/web build）
import { spawn } from 'node:child_process'
import { existsSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { chromium } from 'playwright-core'

const here = new URL('.', import.meta.url).pathname
const bin = process.env.OAD_BIN ?? [join(here, '../../target/release/open-agent-docs'), join(here, '../../target/debug/open-agent-docs')].find(existsSync) ?? 'open-agent-docs'
const web = process.env.OAD_WEB_DIR ?? join(here, '../../apps/web/dist')
const OAD_PORT = 18894
const PORT = 5184
const OAD = `http://127.0.0.1:${OAD_PORT}`
const kids = []
const stop = () => kids.forEach((k) => k.kill())
process.on('exit', stop)
const wait = async (url) => {
  for (let i = 0; i < 150; i++) {
    try {
      if ((await fetch(url)).ok) return
    } catch {
      /* 还没起来 */
    }
    await new Promise((r) => setTimeout(r, 200))
  }
  throw new Error('等不到 ' + url)
}

kids.push(spawn(bin, ['serve', '--port', String(OAD_PORT)], { env: { ...process.env, OAD_AUTH: 'none', OAD_WEB_DIR: web, OAD_DATA_DIR: mkdtempSync(join(tmpdir(), 'oad-embed-iframe-')), RUST_LOG: 'warn' }, stdio: 'inherit' }))
kids.push(spawn(process.execPath, ['serve.mjs'], { cwd: here, env: { ...process.env, PORT: String(PORT) }, stdio: 'inherit' }))
await wait(`${OAD}/api/v1/auth/config`)
await wait(`http://localhost:${PORT}/`)

const api = (p, init) => fetch(OAD + '/api/v1' + p, { headers: { 'content-type': 'application/json' }, ...init }).then((r) => r.json())
const me = await api('/me')
const page = await api(`/workspaces/${me.workspaces[0].id}/pages`, { method: 'POST', body: JSON.stringify({ title: '嵌进来的页' }) })
await fetch(`${OAD}/api/v1/pages/${page.id}/content`, { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ markdown: '这是 Open Agent Docs 里的一页，嵌在别人的网站里。\n\n- 第一条\n- 第二条' }) })

const browser = await chromium.launch()
const p = await browser.newPage({ locale: 'zh-CN' })
p.on('pageerror', (e) => console.log('页面出错：', e.message))
await p.goto(`http://localhost:${PORT}/?oad=${encodeURIComponent(OAD)}&page=${page.id}`)
const f = p.frameLocator('#oad')
await f.locator('.oad-prose').getByText('嵌在别人的网站里').waitFor({ timeout: 20_000 })
if (await f.locator('.sidebar').count()) throw new Error('嵌入模式还有侧栏')
if (await f.locator('.topbar').isVisible()) throw new Error('嵌入模式还有顶栏')
await p.waitForFunction(() => (window.__oadMessages || []).some((m) => m.type === 'oad:ready'), null, { timeout: 10_000 })
await p.screenshot({ path: join(tmpdir(), 'oad-embed-iframe.png') })

// 宿主换主题、语言
await p.click('#dark')
const frame = p.frames().find((x) => x.url().startsWith(OAD))
await frame.waitForFunction(() => document.documentElement.dataset.theme === 'dark')
await p.click('#en')
await frame.waitForFunction(() => document.documentElement.lang === 'en')
await p.screenshot({ path: join(tmpdir(), 'oad-embed-iframe-dark.png') })

// 在 iframe 里打字，存到服务器
await f.locator('.oad-prose li').last().click()
await p.keyboard.press('End')
await p.keyboard.press('Enter')
await p.keyboard.type('在 iframe 里加的')
await p.waitForTimeout(1200)
const md = (await api(`/pages/${page.id}/content`)).markdown
if (!md.includes('在 iframe 里加的')) throw new Error('没存上：' + md)

await browser.close()
console.log('embed-iframe 冒烟测试通过（截图：' + join(tmpdir(), 'oad-embed-iframe*.png') + '）')
stop()
process.exit(0)
