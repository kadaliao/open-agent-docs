// 冒烟测试：起 Express 宿主、经它用 @open-agent-docs/client 建页面、宿主自己的路由也在、关掉。
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createClient } from '@open-agent-docs/client'
import { createApp } from './server.mjs'

const dir = mkdtempSync(join(tmpdir(), 'oad-express-'))
const { app, oad } = await createApp({ dataDir: dir })
const server = await new Promise((r) => {
  const s = app.listen(0, '127.0.0.1', () => r(s))
})
try {
  const base = `http://127.0.0.1:${server.address().port}`
  assert.match(await (await fetch(base + '/host')).text(), /宿主自己的页面/)
  const client = createClient({ baseUrl: base })
  const me = await client.me()
  assert.equal(me.user.name, '演示用户')
  const page = await client.createPage(me.workspaces[0].id, { title: 'Express 里建的页面' })
  assert.equal(page.title, 'Express 里建的页面')
  // 直接连进程里的服务器、不带宿主给的头：认不出是谁
  assert.equal((await fetch(oad.url + '/api/v1/me')).status, 401)
  console.log('node-inprocess 冒烟测试通过')
} finally {
  server.closeAllConnections()
  await new Promise((r) => server.close(r))
  await oad.close()
  rmSync(dir, { recursive: true, force: true })
}
