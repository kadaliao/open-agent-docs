// Express 宿主：Open Agent Docs 在同一个进程里跑（@open-agent-docs/server），宿主自己的路由在前面，其余请求转给它。
// 宿主用 header 登录告诉 Open Agent Docs 是谁（这里演示用一个固定的用户；真实服务从自己的会话里取）。
import express from 'express'
import { start } from '@open-agent-docs/server'

const SECRET = process.env.OAD_HEADER_SECRET ?? 'demo-secret-change-me'

export async function createApp({ dataDir = './data-node-inprocess' } = {}) {
  const oad = await start({ dataDir, auth: 'header', headerSecret: SECRET })
  const app = express()
  app.get('/host', (_req, res) => res.send('宿主自己的页面。打开 <a href="/">文档</a>。'))
  // 宿主知道用户是谁：转发前加上 X-OAD-User-* 和 X-OAD-Secret（浏览器自己带来的同名头先去掉）
  app.use((req, _res, next) => {
    for (const h of Object.keys(req.headers)) if (h.startsWith('x-oad-')) delete req.headers[h]
    req.headers['x-oad-secret'] = SECRET
    req.headers['x-oad-user-id'] = 'demo-user'
    req.headers['x-oad-user-name'] = encodeURIComponent('演示用户')
    next()
  })
  app.use(oad.handler)
  return { app, oad }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const { app, oad } = await createApp()
  const port = Number(process.env.PORT ?? 3001)
  const server = app.listen(port, '127.0.0.1', () => console.log(`http://127.0.0.1:${port}/host`))
  server.on('upgrade', (req, socket, head) => {
    req.headers['x-oad-secret'] = SECRET
    req.headers['x-oad-user-id'] = 'demo-user'
    oad.upgrade(req, socket, head)
  })
  process.on('SIGINT', async () => {
    server.close()
    await oad.close()
    process.exit(0)
  })
}
