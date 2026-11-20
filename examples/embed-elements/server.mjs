// 宿主的 Node 服务（示意）：宿主自己管登录，给浏览器发 Open Agent Docs 的 JWT；页面上的 <oad-editor> / <oad-viewer> 带着 JWT 调接口。
// Open Agent Docs 用 JWT 登录跑：OAD_AUTH=jwt OAD_JWT_SECRET=<密钥> open-agent-docs serve
//
//   浏览器 ──► 本服务（:5184）── /api/*（原样转发，带着浏览器的 Authorization / ?token=）──► Open Agent Docs（:8848）
//              ├ /token：当前用户的 JWT（signJwt）
//              ├ /host/page：宿主后端替当前用户找（没有就建）一页（withUser）
//              ├ /oad/*：@open-agent-docs/elements 的 dist（真实项目用 CDN）
//              └ /：宿主页面（index.html）
//
// 为什么要转发 /api：浏览器跨域直接调 Open Agent Docs 要服务器开 CORS；放在同一个域名下最省事。
// 环境变量：PORT（默认 5184）、OAD_URL（默认 http://127.0.0.1:8848）、OAD_JWT_SECRET（必填，和服务器一样）
import { createReadStream, existsSync, readFileSync, statSync } from 'node:fs'
import http from 'node:http'
import net from 'node:net'
import { createRequire } from 'node:module'
import { dirname, extname, join, normalize } from 'node:path'
import { createClient, signJwt } from '@open-agent-docs/client/node'

const PORT = Number(process.env.PORT ?? 5184)
const OAD = new URL(process.env.OAD_URL ?? 'http://127.0.0.1:8848')
const SECRET = process.env.OAD_JWT_SECRET
if (!SECRET) throw new Error('要设 OAD_JWT_SECRET（和 Open Agent Docs 服务器的一样）')
const here = new URL('.', import.meta.url).pathname
const ELEMENTS = join(dirname(createRequire(import.meta.url).resolve('@open-agent-docs/elements/package.json')), 'dist')
const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.woff2': 'font/woff2' }

// 宿主的“登录”：演示用 cookie 里的 demo_user（没有就是 alice）。真的服务换成自己的会话。
const USERS = { alice: { id: 'alice', name: 'Alice 王', email: 'alice@example.com' }, bob: { id: 'bob', name: 'Bob 李', email: 'bob@example.com' } }
const userOf = (req) => USERS[/(?:^|;\s*)demo_user=([a-z]+)/.exec(req.headers.cookie ?? '')?.[1]] ?? USERS.alice

// 后端自己调 Open Agent Docs：同一个 JWT 密钥，withUser 替当前用户签
const admin = createClient({ baseUrl: OAD.origin, jwtSecret: SECRET, jwtTtl: 600 })
let shared = null // 演示：大家用同一页、同一个库（第一次有人来时建）
async function demoPage(user) {
  const u = admin.withUser(user.id, { name: user.name, email: user.email })
  const ws = await u.defaultWorkspace()
  shared ??= (async () => {
    const page = await u.createPage(ws, { title: '宿主里的共享页' })
    const db = await u.createDatabase(ws, { title: '任务', schema: { props: [{ name: '状态', type: 'select', options: ['要做', '在做', '做完'] }] } })
    const title = db.schema.props.find((p) => p.type === 'title').key
    await u.addRows(db.page.id, [{ props: { [title]: '写周报' } }, { props: { [title]: '看评论' } }])
    return { pageId: page.id, dbId: db.page.id }
  })()
  return shared
}

const send = (res, status, body, type = 'application/json') => res.writeHead(status, { 'content-type': type }).end(typeof body === 'string' ? body : JSON.stringify(body))

function serveFile(res, root, rel) {
  const f = normalize(join(root, rel))
  if (!f.startsWith(root) || !existsSync(f) || statSync(f).isDirectory()) return send(res, 404, 'not found', 'text/plain')
  res.writeHead(200, { 'content-type': TYPES[extname(f)] ?? 'application/octet-stream', 'cache-control': 'no-cache' })
  createReadStream(f).pipe(res)
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://x')
  try {
    if (url.pathname === '/login') {
      res.writeHead(302, { 'set-cookie': `demo_user=${url.searchParams.get('as') ?? 'alice'}; Path=/; SameSite=Lax`, location: '/' })
      return res.end()
    }
    if (url.pathname === '/token') {
      const u = userOf(req)
      // 有效期故意设短一点（10 分钟），元素会在过期前 60 秒自己来换
      return send(res, 200, { token: signJwt(SECRET, u.id, { name: u.name, email: u.email, ttl: 600 }) })
    }
    if (url.pathname === '/host/page') {
      const u = userOf(req)
      return send(res, 200, { ...(await demoPage(u)), user: u })
    }
    if (url.pathname.startsWith('/api/')) {
      // 原样转发（JWT 在浏览器发来的 Authorization 里）
      const up = http.request({ host: OAD.hostname, port: OAD.port, path: req.url, method: req.method, headers: { ...req.headers, host: OAD.host } }, (r) => {
        res.writeHead(r.statusCode ?? 502, r.headers)
        r.pipe(res)
      })
      up.on('error', () => send(res, 502, 'Open Agent Docs 连不上', 'text/plain'))
      return req.pipe(up)
    }
    if (url.pathname.startsWith('/oad/')) {
      if (!existsSync(join(ELEMENTS, 'elements.js'))) return send(res, 500, '先 pnpm --filter @open-agent-docs/elements build', 'text/plain')
      return serveFile(res, ELEMENTS, decodeURIComponent(url.pathname.slice('/oad/'.length)))
    }
    if (url.pathname === '/' || url.pathname === '/index.html') return send(res, 200, readFileSync(join(here, 'index.html'), 'utf8'), TYPES['.html'])
    send(res, 404, 'not found', 'text/plain')
  } catch (e) {
    send(res, 500, String(e?.message ?? e), 'text/plain')
  }
})

// 协作编辑的 WebSocket（/api/v1/sync/<页>?token=<JWT>）：原样转过去
server.on('upgrade', (req, sock, head) => {
  if (!req.url.startsWith('/api/')) return sock.destroy()
  const up = net.connect(Number(OAD.port), OAD.hostname, () => {
    const headers = { ...req.headers, host: OAD.host }
    up.write([`${req.method} ${req.url} HTTP/1.1`, ...Object.entries(headers).map(([k, v]) => `${k}: ${v}`), '', ''].join('\r\n'))
    if (head.length) up.write(head)
    up.pipe(sock)
    sock.pipe(up)
  })
  up.on('error', () => sock.destroy())
  sock.on('error', () => up.destroy())
})

server.listen(PORT, () => console.log(`宿主应用：http://localhost:${PORT}（转发到 ${OAD.origin}）`))
