// 冒烟测试：起一个 header 登录的 Open Agent Docs、打包宿主页面、起宿主的代理，用浏览器以 Alice 新建一页、写字，
// 再换成 Bob 打开同一页看到她写的字、接着写，最后经代理读回正文（两个人的字都在，作者是宿主给的名字）。
//   OAD_BIN=/path/to/open-agent-docs node smoke.mjs      （不给就找仓库里 target/release、target/debug 下编好的）
import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { chromium } from 'playwright-core'

const here = new URL('.', import.meta.url).pathname
const bin = process.env.OAD_BIN ?? [join(here, '../../target/release/open-agent-docs'), join(here, '../../target/debug/open-agent-docs')].find(existsSync) ?? 'open-agent-docs'
const OAD_PORT = 18893
const PORT = 5183
const SECRET = 'demo-secret-' + Math.random().toString(36).slice(2)
const kids = []
const stop = () => kids.forEach((k) => k.kill())
process.on('exit', stop)

const wait = async (url, ms = 30_000) => {
  const t0 = Date.now()
  for (;;) {
    try {
      if ((await fetch(url)).status < 500) return
    } catch {
      /* 还没起来 */
    }
    if (Date.now() - t0 > ms) throw new Error('等不到 ' + url)
    await new Promise((r) => setTimeout(r, 200))
  }
}

// 1. Open Agent Docs（header 登录）
kids.push(
  spawn(bin, ['serve', '--port', String(OAD_PORT)], {
    env: { ...process.env, OAD_AUTH: 'header', OAD_HEADER_SECRET: SECRET, OAD_DATA_DIR: mkdtempSync(join(tmpdir(), 'oad-embed-react-')), RUST_LOG: 'warn' },
    stdio: 'inherit',
  }),
)
await wait(`http://127.0.0.1:${OAD_PORT}/api/v1/auth/config`)

// 2. 宿主页面打包、起代理
const build = spawnSync('npx', ['vite', 'build', '--logLevel', 'warn'], { cwd: here, stdio: 'inherit' })
if (build.status) throw new Error('打包失败')
kids.push(spawn(process.execPath, ['server.mjs'], { cwd: here, env: { ...process.env, PORT: String(PORT), OAD_URL: `http://127.0.0.1:${OAD_PORT}`, OAD_HEADER_SECRET: SECRET }, stdio: 'inherit' }))
await wait(`http://127.0.0.1:${PORT}/`)

// 不经代理直接打服务器：没有密钥应该进不去
const direct = await fetch(`http://127.0.0.1:${OAD_PORT}/api/v1/me`, { headers: { 'x-oad-user-id': 'mallory' } })
if (direct.status !== 401) throw new Error('没带密钥也进去了：' + direct.status)

const browser = await chromium.launch()
const ctx = await browser.newContext({ baseURL: `http://localhost:${PORT}` })
const page = await ctx.newPage()
page.on('pageerror', (e) => console.log('页面出错：', e.message))

// 3. Alice 新建一页、写标题和正文
await page.goto('/login?as=alice')
await page.waitForSelector('.host-me[data-user]')
await page.getByText('+ 新建页面').click()
await page.waitForSelector('.oad-prose[contenteditable="true"]')
await page.locator('.oad-title').click()
await page.keyboard.type('宿主里写的页')
await page.keyboard.press('Enter')
await page.keyboard.type('Alice 写的第一段。')
await page.waitForTimeout(1200)
const id = await page.locator('.host-nav button.is-on').getAttribute('data-page')
await page.screenshot({ path: join(tmpdir(), 'oad-embed-react-alice.png') })

// 4. Bob 打开同一页，看到 Alice 的字，接着写
await page.goto('/login?as=bob')
await page.waitForSelector('.host-me[data-user]')
await page.locator(`.host-nav button[data-page="${id}"]`).click()
await page.locator('.oad-prose', { hasText: 'Alice 写的第一段' }).waitFor()
await page.locator('.oad-prose p', { hasText: 'Alice 写的第一段' }).click()
await page.keyboard.press('ControlOrMeta+ArrowDown')
await page.keyboard.press('End')
await page.keyboard.press('Enter')
await page.keyboard.type('Bob 补的一句。')
await page.waitForTimeout(1200)
await page.screenshot({ path: join(tmpdir(), 'oad-embed-react-bob.png') })

// 5. 经代理读回来（代理按 cookie 认人）
const me = await page.evaluate(() => fetch('/api/v1/me').then((r) => r.json()))
if (me.user.name !== 'Bob 李') throw new Error('代理没把 Bob 带过去：' + JSON.stringify(me.user))
const md = await page.evaluate((id) => fetch(`/api/v1/pages/${id}/content`).then((r) => r.text()), id)
const info = await page.evaluate((id) => fetch(`/api/v1/pages/${id}`).then((r) => r.json()), id)
if (info.title !== '宿主里写的页') throw new Error('标题不对：' + info.title)
for (const s of ['Alice 写的第一段', 'Bob 补的一句']) if (!md.includes(s)) throw new Error(`正文里没有“${s}”：${md.slice(0, 300)}`)
const authors = await page.evaluate((id) => fetch(`/api/v1/pages/${id}/authors`).then((r) => r.json()), id)
const names = Object.values(authors.clients ?? {}).map((a) => a.name)
if (!names.includes('Alice 王') || !names.includes('Bob 李')) throw new Error('作者不对：' + JSON.stringify(names))

await browser.close()
console.log('embed-react 冒烟测试通过（截图：' + join(tmpdir(), 'oad-embed-react-*.png') + '）')
stop()
process.exit(0)
