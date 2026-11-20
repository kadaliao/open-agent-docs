// 跨域 iframe + JWT：宿主（http://localhost:<端口>）嵌 Open Agent Docs（http://127.0.0.1:<另一个端口>，OAD_AUTH=jwt），
// 第三方 cookie 用不上，全靠 #token= 和 postMessage。
//   - #token= 读走后地址栏里没有了；接口带 Bearer、WebSocket 带 ?token=，打字存得上；
//   - 一开始给的 token 不对：iframe 发 oad:auth-required（unauthorized），宿主回 oad:token，iframe 用它登录上；
//   - token 快过期（不到 60 秒）：iframe 发 oad:auth-required（expiring），宿主回新的，接着用。
// 这个测试自己起一个 jwt 模式的服务器（不用 playwright.config 里那个不登录的）：OAD_BIN 或 target/debug/open-agent-docs。
import { expect, test, type Frame, type Page } from '@playwright/test'
import { spawn, type ChildProcess } from 'node:child_process'
import { createHmac } from 'node:crypto'
import { existsSync, mkdtempSync } from 'node:fs'
import http from 'node:http'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..')
const bin = process.env.OAD_BIN ?? [join(root, 'target/debug/open-agent-docs'), join(root, 'target/release/open-agent-docs')].find(existsSync)
const OAD_PORT = 18871
const HOST_PORT = 18872
const OAD = `http://127.0.0.1:${OAD_PORT}`
const HOST = `http://localhost:${HOST_PORT}`
const SECRET = 'e2e-embed-jwt-' + Math.random().toString(36).slice(2)

const b64 = (s: string | Buffer) => Buffer.from(s).toString('base64url')
function sign(claims: Record<string, unknown>, secret = SECRET) {
  const head = b64(JSON.stringify({ alg: 'HS256', typ: 'JWT' }))
  const body = b64(JSON.stringify(claims))
  return `${head}.${body}.${b64(createHmac('sha256', secret).update(`${head}.${body}`).digest())}`
}
const now = () => Math.floor(Date.now() / 1000)
const tokenFor = (ttl: number) => sign({ sub: 'host-user-1', name: '宿主用户', exp: now() + ttl })

// 宿主页面：iframe 指过去；收到 oad:auth-required 就（向自己的后端要一个新的，这里直接用页面里的）回 oad:token
const hostHtml = `<!doctype html><meta charset="utf-8"><title>宿主</title>
<iframe id="oad" style="width:900px;height:600px;border:0"></iframe>
<script>
  window.__msgs = []
  window.__next = null
  const frame = document.getElementById('oad')
  addEventListener('message', (e) => {
    if (e.origin !== '${OAD}' || !e.data || typeof e.data.type !== 'string') return
    window.__msgs.push(e.data)
    if (e.data.type === 'oad:auth-required' && window.__next) frame.contentWindow.postMessage({ type: 'oad:token', token: window.__next }, '${OAD}')
  })
  window.__open = (src) => { frame.src = src }
</script>`

let oad: ChildProcess | null = null
let host: http.Server | null = null
let pageId = ''
let token = ''

async function waitUp(url: string) {
  for (let i = 0; i < 300; i++) {
    try {
      if ((await fetch(url)).ok) return
    } catch {
      /* 还没起来 */
    }
    await new Promise((r) => setTimeout(r, 200))
  }
  throw new Error('等不到 ' + url)
}

const api = (path: string, init: RequestInit = {}) =>
  fetch(`${OAD}/api/v1${path}`, { ...init, headers: { authorization: 'Bearer ' + tokenFor(600), 'content-type': 'application/json', ...(init.headers ?? {}) } })

test.describe.configure({ mode: 'serial' })

test.beforeAll(async () => {
  test.skip(!bin, '没有服务器二进制（cargo build -p oad-bin 或者设 OAD_BIN）')
  oad = spawn(bin!, ['serve', '--port', String(OAD_PORT)], {
    env: {
      ...process.env,
      OAD_AUTH: 'jwt',
      OAD_JWT_SECRET: SECRET,
      OAD_ALLOWED_ORIGINS: HOST,
      OAD_DATA_DIR: mkdtempSync(join(tmpdir(), 'oad-e2e-jwt-')),
      OAD_WEB_DIR: join(root, 'apps/web/dist'),
      RUST_LOG: 'warn',
    },
    stdio: ['ignore', 'ignore', 'inherit'],
  })
  host = http.createServer((_, res) => res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }).end(hostHtml)).listen(HOST_PORT)
  await waitUp(`${OAD}/api/v1/auth/config`)
  const conf = await (await fetch(`${OAD}/api/v1/auth/config`)).json()
  expect(conf.mode).toBe('jwt')
  expect(conf.allowed_origins).toContain(HOST)
  const me = await (await api('/me')).json()
  const p = await (await api(`/workspaces/${me.workspaces[0].id}/pages`, { method: 'POST', body: JSON.stringify({ title: '嵌进来的页' }) })).json()
  pageId = p.id
  const put = await api(`/pages/${pageId}/content`, { method: 'PUT', body: JSON.stringify({ markdown: '跨域嵌入的正文\n\n- 第一条' }) })
  expect(put.status).toBe(204)
})

test.afterAll(async () => {
  oad?.kill()
  host?.close()
})

const oadFrame = (p: Page): Frame => {
  const f = p.frames().find((x) => x.url().startsWith(OAD))
  if (!f) throw new Error('没有 iframe')
  return f
}
const msgs = (p: Page) => p.evaluate(() => (window as unknown as { __msgs: { type: string; reason?: string }[] }).__msgs)

test('#token= 登录，地址栏抹掉，打字存得上', async ({ page }) => {
  token = tokenFor(600)
  await page.goto(HOST)
  await page.evaluate((src) => (window as unknown as { __open: (s: string) => void }).__open(src), `${OAD}/p/${pageId}?embed=1#token=${token}`)
  const f = page.frameLocator('#oad')
  await expect(f.locator('.oad-prose').getByText('跨域嵌入的正文')).toBeVisible({ timeout: 20_000 })
  await expect.poll(() => msgs(page).then((m) => m.some((x) => x.type === 'oad:ready'))).toBe(true)
  // token 不在地址里了，也没存进 localStorage
  const frame = oadFrame(page)
  expect(frame.url()).not.toContain('token')
  expect(await frame.evaluate(() => location.hash)).toBe('')
  expect(await frame.evaluate(() => JSON.stringify({ ...localStorage }))).not.toContain(token.slice(-20))
  // 写：经 WebSocket（?token=）存到服务器
  await expect(f.locator('[data-status="synced"]')).toBeAttached({ timeout: 15_000 })
  await f.locator('.oad-prose li').last().click()
  await page.keyboard.press('End')
  await page.keyboard.type('，iframe 里加的')
  await expect.poll(async () => (await (await api(`/pages/${pageId}/content`)).json()).markdown, { timeout: 10_000 }).toContain('iframe 里加的')
  expect((await msgs(page)).filter((m) => m.type === 'oad:auth-required')).toEqual([])
})

test('token 不对：要新的，宿主给了就登录上', async ({ page }) => {
  await page.goto(HOST)
  await page.evaluate((t) => ((window as unknown as { __next: string }).__next = t), tokenFor(600))
  const bad = sign({ sub: 'host-user-1', exp: now() + 600 }, 'wrong-secret')
  await page.evaluate((src) => (window as unknown as { __open: (s: string) => void }).__open(src), `${OAD}/p/${pageId}?embed=1#token=${bad}`)
  await expect.poll(() => msgs(page).then((m) => m.filter((x) => x.type === 'oad:auth-required').map((x) => x.reason))).toContain('unauthorized')
  // 不出登录框，等宿主的 token；来了就画出正文
  await expect(page.frameLocator('#oad').locator('.login')).toHaveCount(0)
  await expect(page.frameLocator('#oad').locator('.oad-prose').getByText('跨域嵌入的正文')).toBeVisible({ timeout: 20_000 })
})

test('快过期时向宿主要新的，换上以后接着用', async ({ page }) => {
  await page.goto(HOST)
  const fresh = tokenFor(600)
  await page.evaluate((t) => ((window as unknown as { __next: string }).__next = t), fresh)
  // 还有 63 秒过期：3 秒后（离过期不到 60 秒）要新的
  await page.evaluate((src) => (window as unknown as { __open: (s: string) => void }).__open(src), `${OAD}/p/${pageId}?embed=1#token=${tokenFor(63)}`)
  const f = page.frameLocator('#oad')
  await expect(f.locator('.oad-prose').getByText('跨域嵌入的正文')).toBeVisible({ timeout: 20_000 })
  await expect.poll(() => msgs(page).then((m) => m.filter((x) => x.type === 'oad:auth-required').map((x) => x.reason)), { timeout: 15_000 }).toContain('expiring')
  // 换上了新的：宿主让它换一页，之后的接口请求带的是新的
  const other = await (await api(`/workspaces/${(await (await api('/me')).json()).workspaces[0].id}/pages`, { method: 'POST', body: JSON.stringify({ title: '另一页' }) })).json()
  await api(`/pages/${other.id}/content`, { method: 'PUT', body: JSON.stringify({ markdown: '另一页的正文' }) })
  const auths: string[] = []
  page.on('request', (q) => {
    if (q.url().startsWith(`${OAD}/api/v1/`) && q.headers()['authorization']) auths.push(q.headers()['authorization'])
  })
  await page.evaluate(([id, origin]) => (document.getElementById('oad') as HTMLIFrameElement).contentWindow!.postMessage({ type: 'oad:navigate', pageId: id }, origin), [other.id, OAD])
  await expect(f.locator('.oad-prose').getByText('另一页的正文')).toBeVisible({ timeout: 20_000 })
  expect(auths.length).toBeGreaterThan(0)
  expect(new Set(auths)).toEqual(new Set(['Bearer ' + fresh]))
  // WebSocket 用新的 token 连上，打字存得上
  await expect(f.locator('[data-status="synced"]')).toBeAttached({ timeout: 15_000 })
  await f.locator('.oad-prose p').getByText('另一页的正文').click()
  await page.keyboard.press('End')
  await page.keyboard.type('，换了 token 以后加的')
  await expect.poll(async () => (await (await api(`/pages/${other.id}/content`)).json()).markdown, { timeout: 10_000 }).toContain('换了 token 以后加的')
  expect((await msgs(page)).filter((m) => m.type === 'oad:auth-required' && m.reason === 'unauthorized')).toEqual([])
})

test('跨域 iframe 里图片能显示（签名地址，不靠 cookie）', async ({ page }) => {
  const me = await (await api('/me')).json()
  const ws = me.workspaces[0].id
  // 一张 2×2 的 PNG
  const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGM4kWIERAwQCgAorgV5QuEn7gAAAABJRU5ErkJggg==', 'base64')
  const fd = new FormData()
  fd.append('file', new Blob([png], { type: 'image/png' }), '图.png')
  const up = await fetch(`${OAD}/api/v1/workspaces/${ws}/files`, { method: 'POST', body: fd, headers: { authorization: 'Bearer ' + tokenFor(600) } })
  expect(up.status).toBe(201)
  const file = await up.json()
  // 不登录取不到（iframe 里没有 cookie，<img> 也带不了 Bearer）
  expect((await fetch(OAD + file.url)).status).toBe(401)
  const p = await (await api(`/workspaces/${ws}/pages`, { method: 'POST', body: JSON.stringify({ title: '有图的页' }) })).json()
  await api(`/pages/${p.id}/content`, { method: 'PUT', body: JSON.stringify({ markdown: `有图的正文\n\n![图](${file.url})` }) })

  await page.goto(HOST)
  await page.evaluate((src) => (window as unknown as { __open: (s: string) => void }).__open(src), `${OAD}/p/${p.id}?embed=1#token=${tokenFor(600)}`)
  const f = page.frameLocator('#oad')
  await expect(f.locator('.oad-prose').getByText('有图的正文')).toBeVisible({ timeout: 20_000 })
  const img = f.locator('figure.oad-image img')
  await expect(img).toHaveAttribute('src', /[?&]sig=/, { timeout: 15_000 })
  await expect.poll(() => img.evaluate((i: HTMLImageElement) => i.complete && i.naturalWidth > 0), { timeout: 15_000 }).toBe(true)
  await expect(f.locator('figure.oad-image.is-broken')).toHaveCount(0)
})
