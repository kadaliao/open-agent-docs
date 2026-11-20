// 宿主自己的网站：发静态页面（index.html：同域 / 不登录的嵌法；jwt.html：跨域 + JWT 的嵌法）。PORT 默认 5181。
// 设了 OAD_JWT_SECRET（和 Open Agent Docs 的一样）时，它也是宿主的“后端”：
//   GET /token  给当前用户签一个短期 JWT（演示里就是一个固定的用户；真的宿主按自己的登录态签）
//   GET /demo-page  没给页面 id 时，用这个用户建一页演示用的（服务器之间调接口，不经过浏览器，没有跨域的事）
import { createHmac } from 'node:crypto'
import { readFileSync } from 'node:fs'
import http from 'node:http'

const PORT = Number(process.env.PORT ?? 5181)
const OAD = (process.env.OAD_URL ?? 'http://127.0.0.1:8848').replace(/\/$/, '')
const SECRET = process.env.OAD_JWT_SECRET
// token 多久过期（秒）：演示里故意短一点，能看到快过期时 iframe 来要新的
const TTL = Number(process.env.TOKEN_TTL ?? 120)
const pages = { '/': 'index.html', '/index.html': 'index.html', '/jwt.html': 'jwt.html' }

const b64 = (s) => Buffer.from(s).toString('base64url')
/** HS256，claims 见 docs/zh/embedding.md#jwt */
function signJwt(claims) {
  const head = b64(JSON.stringify({ alg: 'HS256', typ: 'JWT' }))
  const body = b64(JSON.stringify(claims))
  return `${head}.${body}.${b64(createHmac('sha256', SECRET).update(`${head}.${body}`).digest())}`
}
const now = () => Math.floor(Date.now() / 1000)
const userToken = (ttl = TTL) => signJwt({ sub: 'demo-user', name: '宿主里的用户', iat: now(), exp: now() + ttl })

let demoPage = null
async function ensureDemoPage() {
  if (demoPage) return demoPage
  const h = { authorization: 'Bearer ' + userToken(60), 'content-type': 'application/json' }
  const me = await (await fetch(`${OAD}/api/v1/me`, { headers: h })).json()
  const p = await (await fetch(`${OAD}/api/v1/workspaces/${me.workspaces[0].id}/pages`, { method: 'POST', headers: h, body: JSON.stringify({ title: '跨域嵌入的演示' }) })).json()
  await fetch(`${OAD}/api/v1/pages/${p.id}/content`, { method: 'PUT', headers: h, body: JSON.stringify({ markdown: '这一页嵌在另一个域名的网站里，用宿主签的 JWT 登录。\n\n- 第一条\n- 第二条' }) })
  return (demoPage = p.id)
}

const json = (res, code, v) => res.writeHead(code, { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' }).end(JSON.stringify(v))

http
  .createServer(async (req, res) => {
    const path = new URL(req.url, 'http://x').pathname
    if (pages[path]) return res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }).end(readFileSync(new URL('./' + pages[path], import.meta.url)))
    if (path === '/token' || path === '/demo-page') {
      if (!SECRET) return json(res, 404, { error: '宿主没设 OAD_JWT_SECRET' })
      try {
        return path === '/token' ? json(res, 200, { token: userToken(), ttl: TTL }) : json(res, 200, { id: await ensureDemoPage() })
      } catch (e) {
        return json(res, 502, { error: String(e) })
      }
    }
    res.writeHead(404).end()
  })
  .listen(PORT, () => {
    console.log(`宿主页面：http://localhost:${PORT}/?oad=${OAD}&page=<页面 id>`)
    if (SECRET) console.log(`跨域 + JWT：http://localhost:${PORT}/jwt.html?oad=${OAD}`)
  })
