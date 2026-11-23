// 冒烟测试：起一个 JWT 登录的 Open Agent Docs 和宿主的 server.mjs，用浏览器打开纯 HTML 的宿主页（另一个端口，元素直连服务器：CORS）：
// <oad-editor>、<oad-viewer>、<oad-database> 都发了 oad-ready（token 是元素自己用 getToken 要的），编辑器和只读视图里的图片显示出来（签名地址），
// 在编辑器里写字，oad-change 收到，刷新只读视图看到刚写的字；Bob 打开同一页看到 Alice 写的；最后把编辑器从页面上拿掉，WebSocket 断开。
// 再用第二种写法（PROXY=1，宿主转发 /api）起一次，只读视图和图片照样出来。
//   OAD_BINARY=/path/to/open-agent-docs node smoke.mjs      （不给就找仓库里 target/release、target/debug 下编好的）
import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { chromium } from 'playwright-core'

const here = new URL('.', import.meta.url).pathname
const root = join(here, '../..')
const bin =
  process.env.OAD_BINARY ?? process.env.OAD_BIN ?? [join(root, 'target/release/open-agent-docs'), join(root, 'target/debug/open-agent-docs')].find(existsSync) ?? 'open-agent-docs'
const OAD_PORT = 18894
const PORT = 5184
const PROXY_PORT = 5185
const OAD = `http://127.0.0.1:${OAD_PORT}`
const SECRET = 'demo-jwt-secret-' + Math.random().toString(36).slice(2)
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

// 0. 要用到的包先打好（client 的 node 入口、elements 的 dist）
for (const [pkg, file] of [
  ['@open-agent-docs/client', 'packages/client/dist/node.js'],
  ['@open-agent-docs/elements', 'packages/elements/dist/elements.js'],
])
  if (!existsSync(join(root, file)) || process.env.REBUILD) {
    const r = spawnSync('pnpm', ['--filter', pkg, 'build'], { cwd: root, stdio: 'inherit' })
    if (r.status) throw new Error(pkg + ' 打包失败')
  }

// 1. Open Agent Docs（JWT 登录）
kids.push(
  spawn(bin, ['serve', '--port', String(OAD_PORT)], {
    env: {
      ...process.env,
      OAD_AUTH: 'jwt',
      OAD_JWT_SECRET: SECRET,
      // 宿主页面在另一个端口：允许它跨域（CORS、WebSocket 的 Origin 检查）
      OAD_ALLOWED_ORIGINS: `http://localhost:${PORT}`,
      OAD_DATA_DIR: mkdtempSync(join(tmpdir(), 'oad-embed-elements-')),
      RUST_LOG: 'warn',
    },
    stdio: 'inherit',
  }),
)
await wait(`http://127.0.0.1:${OAD_PORT}/api/v1/auth/config`)

// 2. 宿主
const host = (port, proxy) =>
  kids.push(spawn(process.execPath, ['server.mjs'], { cwd: here, env: { ...process.env, PORT: String(port), OAD_URL: OAD, OAD_JWT_SECRET: SECRET, PROXY: proxy ? '1' : '' }, stdio: 'inherit' }))
host(PORT, false)
await wait(`http://127.0.0.1:${PORT}/`)

// 没有 JWT 直接打服务器：进不去
const anon = await fetch(`http://127.0.0.1:${OAD_PORT}/api/v1/me`)
if (anon.status !== 401) throw new Error('没带 JWT 也进去了：' + anon.status)

const browser = await chromium.launch()
const ctx = await browser.newContext({ baseURL: `http://localhost:${PORT}` })
const page = await ctx.newPage()
// 接口、WebSocket 都直连 Open Agent Docs，不经过宿主
const viaHost = []
const direct = []
page.on('request', (q) => {
  const u = q.url()
  if (u.startsWith(`http://localhost:${PORT}/api/`)) viaHost.push(u)
  if (u.startsWith(OAD + '/api/')) direct.push(u)
})
const errors = []
page.on('pageerror', (e) => errors.push(e.message))
page.on('console', (m) => m.type() === 'error' && console.log('页面控制台：', m.text()))
const logHas = (s) => page.waitForFunction((s) => document.getElementById('log').textContent.includes(s), s, { timeout: 20_000 })
/** 元素里的图片真画出来了（加载成功），地址带签名 */
async function imageShown(p, sel) {
  const img = p.locator(sel).first()
  await img.waitFor({ timeout: 15_000 })
  await p.waitForFunction((el) => el.complete && el.naturalWidth > 0, await img.elementHandle(), { timeout: 15_000 })
  const src = await img.evaluate((el) => el.currentSrc || el.src)
  if (!/[?&]sig=/.test(src)) throw new Error(sel + ' 的地址没有签名：' + src)
  return src
}

// 3. Alice：两个元素都 ready
await page.goto('/login?as=alice')
await logHas('oad-editor ready')
await logHas('oad-viewer ready')
// 图片：编辑器（签名地址 + 服务器地址）、只读视图（format=html 里就是签名地址）
const edSrc = await imageShown(page, 'oad-editor figure.oad-image img')
if (!edSrc.startsWith(OAD + '/api/v1/files/')) throw new Error('编辑器里的图片没拼上服务器地址：' + edSrc)
await imageShown(page, 'oad-viewer .oad-content img')
const editorText = page.locator('oad-editor .oad-prose') // Playwright 的 locator 会穿过 Shadow DOM
await editorText.waitFor()
await page.locator('oad-editor .oad-title').click()
await page.keyboard.press('ControlOrMeta+A')
await page.keyboard.type('Web Component 写的')
await page.keyboard.press('Enter')
await page.keyboard.type('Alice 在宿主页面里写的一段。')
await logHas('oad-editor change content 自己改的')
await page.waitForTimeout(1200) // 等服务器存下
await page.screenshot({ path: join(tmpdir(), 'oad-embed-elements-alice.png'), fullPage: true })

// 4. 只读视图刷新后看到刚写的
await page.getByText('刷新只读视图').click()
await page.locator('oad-viewer').getByText('Alice 在宿主页面里写的一段').waitFor({ timeout: 10_000 })
const viewerTitle = await page.locator('oad-viewer .oad-viewer-title').textContent()
if (viewerTitle !== 'Web Component 写的') throw new Error('只读视图的标题不对：' + viewerTitle)
await page.locator('oad-viewer').screenshot({ path: join(tmpdir(), 'oad-embed-elements-viewer.png') })

// 5. 样式在 Shadow DOM 里：标题是大字、没有输入框的边框（编辑器的样式表进去了），宿主改的强调色也进去了；宿主页面没被插样式
const look = await page.locator('oad-editor').evaluate((el) => {
  const t = getComputedStyle(el.shadowRoot.querySelector('.oad-title'))
  return { size: parseFloat(t.fontSize), border: t.borderTopWidth, accent: getComputedStyle(el.shadowRoot.querySelector('.oad-prose')).getPropertyValue('--oad-accent').trim() }
})
if (look.size < 24 || look.border !== '0px') throw new Error('编辑器的样式没进 Shadow DOM：' + JSON.stringify(look))
if (look.accent !== '#7c3aed') throw new Error('宿主设的 --oad-accent 没进去：' + look.accent)
if (await page.evaluate(() => [...document.styleSheets].some((s) => [...s.cssRules].some((r) => r.cssText.includes('.oad-prose'))))) throw new Error('编辑器的样式漏到宿主页面上了')

// 6. 数据库视图：两行都画出来了
await logHas('oad-database ready')
await page.locator('oad-database').getByText('看评论').first().waitFor({ timeout: 10_000 })
// 弹出菜单挂在 Shadow DOM 里（有样式），点菜单里面不会把它关掉
await page.locator('oad-database [data-db-tool="filter"]').click()
const pop = page.locator('oad-database .oad-db-pop')
await pop.waitFor({ timeout: 5000 })
await pop.click({ position: { x: 10, y: 10 } })
await page.waitForTimeout(200)
if (!(await pop.isVisible())) throw new Error('点了弹出菜单里面，它却关了')
if ((await pop.evaluate((el) => getComputedStyle(el).position)) !== 'fixed') throw new Error('弹出菜单没有样式（不在 Shadow DOM 里？）')
await page.keyboard.press('Escape')

// 7. Bob 打开同一页，看到 Alice 写的（记下协作编辑的 WebSocket，下一步看它关掉）
const sync = []
page.on('websocket', (ws) => ws.url().includes('/api/v1/sync/') && sync.push(ws))
await page.goto('/login?as=bob')
await logHas('oad-editor ready')
await page.locator('oad-editor .oad-prose', { hasText: 'Alice 在宿主页面里写的一段' }).waitFor({ timeout: 15_000 })
if ((await page.locator('#me').textContent()) !== 'Bob 李') throw new Error('不是 Bob')
if (!sync.length || !sync[0].url().includes('token=')) throw new Error('WebSocket 没带 token：' + sync.map((w) => w.url()))

// 8. 把编辑器从页面上拿掉：WebSocket 断开
const closed = sync[0].isClosed() ? Promise.resolve() : sync[0].waitForEvent('close', { timeout: 5000 })
await page.evaluate(() => document.getElementById('editor').remove())
await closed

if (viaHost.length) throw new Error('有请求走了宿主：' + viaHost.slice(0, 3).join(', '))
if (!direct.some((u) => u.includes('/api/v1/files/sign'))) throw new Error('没签图片地址')
if (!sync[0].url().startsWith(`ws://127.0.0.1:${OAD_PORT}/`)) throw new Error('WebSocket 没直连：' + sync[0].url())

// 9. 第二种写法：宿主转发 /api（base-url 不给，同源）
host(PROXY_PORT, true)
await wait(`http://127.0.0.1:${PROXY_PORT}/`)
const p2 = await (await browser.newContext({ baseURL: `http://localhost:${PROXY_PORT}` })).newPage()
p2.on('pageerror', (e) => errors.push(e.message))
await p2.goto('/login?as=alice')
await p2.waitForFunction(() => document.getElementById('log').textContent.includes('oad-viewer ready'), null, { timeout: 20_000 })
const proxied = await imageShown(p2, 'oad-viewer .oad-content img')
if (!proxied.startsWith(`http://localhost:${PROXY_PORT}/api/v1/files/`)) throw new Error('转发时图片应该走宿主：' + proxied)
await imageShown(p2, 'oad-editor figure.oad-image img')

if (errors.length) throw new Error('页面出错：' + errors.join('\n'))
await browser.close()
console.log('embed-elements 冒烟测试通过（截图：' + join(tmpdir(), 'oad-embed-elements-alice.png') + '）')
stop()
process.exit(0)
