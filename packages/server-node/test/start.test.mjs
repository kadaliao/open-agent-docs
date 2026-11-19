// 起服务器、用 @oad/client 建页面、挂到 Node http 上转发、关掉。
import { test } from 'node:test'
import assert from 'node:assert/strict'
import http from 'node:http'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createClient } from '@oad/client'
import { start } from '../index.js'

test('start, create a page with @oad/client, proxy through node http, close', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'oad-node-'))
  const oad = await start({ dataDir: dir, auth: 'none' })
  try {
    assert.match(oad.url, /^http:\/\/127\.0\.0\.1:\d+$/)
    const client = createClient({ baseUrl: oad.url })
    const me = await client.me()
    const ws = me.workspaces[0].id
    const page = await client.createPage(ws, { title: '从 Node 建的页面' })
    assert.equal(page.title, '从 Node 建的页面')
    // 挂到 Node 自己的 http 服务器上
    const host = http.createServer(oad.handler)
    host.on('upgrade', oad.upgrade)
    await new Promise((r) => host.listen(0, '127.0.0.1', r))
    const port = host.address().port
    const via = createClient({ baseUrl: `http://127.0.0.1:${port}` })
    const pages = await via.pages(ws)
    assert.ok(pages.some((p) => p.id === page.id))
    // 实时事件（SSE）经转发也通
    const ctl = new AbortController()
    const sse = await fetch(`http://127.0.0.1:${port}/api/v1/workspaces/${ws}/events`, { signal: ctl.signal })
    assert.equal(sse.headers.get('content-type'), 'text/event-stream')
    ctl.abort()
    host.closeAllConnections()
    await new Promise((r) => host.close(r))
  } finally {
    await oad.close()
    await oad.close()
    rmSync(dir, { recursive: true, force: true })
  }
  // 关了就连不上
  await assert.rejects(fetch(oad.url + '/api/v1/me'))
})

test('bad options reject', async () => {
  await assert.rejects(start({ auth: 'nope' }), /OAD_AUTH/)
})
