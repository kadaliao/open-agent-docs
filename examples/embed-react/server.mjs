// 宿主的 Node 服务（示意）：宿主自己管登录，转发给 Open Agent Docs 时带上“这是谁”。
// Open Agent Docs 用 header 登录跑：OAD_AUTH=header OAD_HEADER_SECRET=<密钥> open-agent-docs serve
//
//   浏览器 ──► 本服务（:5180）── /api/* 加上 X-OAD-User-* 和 X-OAD-Secret ──► Open Agent Docs（:8848）
//              └ 其余路径：宿主自己的页面（dist/）
//
// 环境变量：PORT（默认 5180）、OAD_URL（默认 http://127.0.0.1:8848）、OAD_HEADER_SECRET（必填，和服务器一样）
import { createReadStream, existsSync, statSync } from 'node:fs'
import http from 'node:http'
import net from 'node:net'
import { extname, join } from 'node:path'

const PORT = Number(process.env.PORT ?? 5180)
const OAD = new URL(process.env.OAD_URL ?? 'http://127.0.0.1:8848')
const SECRET = process.env.OAD_HEADER_SECRET
if (!SECRET) throw new Error('要设 OAD_HEADER_SECRET（和 Open Agent Docs 服务器的一样）')
const DIST = new URL('./dist', import.meta.url).pathname
const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.png': 'image/png' }

// 宿主的“登录”：这里为了演示，用 cookie 里的 demo_user（没有就是 alice）。真的服务换成自己的会话。
const USERS = { alice: { id: 'alice', name: 'Alice 王', email: 'alice@example.com' }, bob: { id: 'bob', name: 'Bob 李', email: 'bob@example.com' } }
function userOf(req) {
  const m = /(?:^|;\s*)demo_user=([a-z]+)/.exec(req.headers.cookie ?? '')
  return USERS[m?.[1]] ?? USERS.alice
}

// 转发的请求头：去掉浏览器自己带的 X-OAD-*（不能让人冒充），加上宿主认定的用户
function forwardHeaders(req) {
  const h = {}
  for (const [k, v] of Object.entries(req.headers)) if (!k.startsWith('x-oad-')) h[k] = v
  const u = userOf(req)
  h.host = OAD.host
  h['x-oad-user-id'] = u.id
  h['x-oad-user-name'] = encodeURIComponent(u.name)
  h['x-oad-user-email'] = u.email
  h['x-oad-secret'] = SECRET
  return h
}

const server = http.createServer((req, res) => {
  if (req.url === '/login' || req.url.startsWith('/login?')) {
    const who = new URL(req.url, 'http://x').searchParams.get('as') ?? 'alice'
    res.writeHead(302, { 'set-cookie': `demo_user=${who}; Path=/; SameSite=Lax`, location: '/' })
    return res.end()
  }
  if (req.url.startsWith('/api/')) {
    const up = http.request({ host: OAD.hostname, port: OAD.port, path: req.url, method: req.method, headers: forwardHeaders(req) }, (r) => {
      res.writeHead(r.statusCode ?? 502, r.headers)
      r.pipe(res)
    })
    up.on('error', () => res.writeHead(502).end('Open Agent Docs 连不上'))
    return req.pipe(up)
  }
  let f = join(DIST, decodeURIComponent(req.url.split('?')[0]))
  if (!existsSync(f) || statSync(f).isDirectory()) f = join(DIST, 'index.html')
  if (!existsSync(f)) return res.writeHead(500).end('先 pnpm build')
  res.writeHead(200, { 'content-type': TYPES[extname(f)] ?? 'application/octet-stream' })
  createReadStream(f).pipe(res)
})

// 协作编辑的 WebSocket（/api/v1/sync/<页>）：同样带上用户头转过去
server.on('upgrade', (req, sock, head) => {
  if (!req.url.startsWith('/api/')) return sock.destroy()
  const up = net.connect(Number(OAD.port), OAD.hostname, () => {
    const lines = [`${req.method} ${req.url} HTTP/1.1`, ...Object.entries(forwardHeaders(req)).map(([k, v]) => `${k}: ${v}`), '', '']
    up.write(lines.join('\r\n'))
    if (head.length) up.write(head)
    up.pipe(sock)
    sock.pipe(up)
  })
  up.on('error', () => sock.destroy())
  sock.on('error', () => up.destroy())
})

server.listen(PORT, () => console.log(`宿主应用：http://localhost:${PORT}（转发到 ${OAD.origin}）`))
