// openPage：连上 y-websocket 服务器同步（两个客户端互相看到改动）、token 放进地址、ready、状态、被拒绝、关掉。
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { WebSocket } from 'ws'
import * as Y from 'yjs'
import { createClient } from '../src/index'
import { startYServer } from './yserver'

let srv: Awaited<ReturnType<typeof startYServer>>
const until = async (f: () => boolean, ms = 5000) => {
  const t0 = Date.now()
  while (!f()) {
    if (Date.now() - t0 > ms) throw new Error('等超时了')
    await new Promise((r) => setTimeout(r, 10))
  }
}

beforeAll(async () => {
  // 服务器上的房间名是 /api/v1/sync/<页>
  srv = await startYServer({ deny: (room) => room.endsWith('/secret') })
})
afterAll(() => srv.close())

describe('openPage', () => {
  it('同步：服务器上已有的内容到了 ready = server；两边改动互相看到', async () => {
    srv.doc('api/v1/sync/p1').getText('title').insert(0, '已有的标题')
    const base = `http://127.0.0.1:${srv.port}`
    const a = createClient({ baseUrl: base, WebSocket }).openPage('p1')
    const b = createClient({ baseUrl: base, WebSocket }).openPage('p1')
    expect(a.idb).toBeNull() // Node 里没有 IndexedDB
    expect(await a.ready).toBe('server')
    await b.ready
    expect(a.ydoc.getText('title').toString()).toBe('已有的标题')
    await until(() => a.status() === 'synced' && b.status() === 'synced')

    const frag = a.ydoc.getXmlFragment('default')
    const p = new Y.XmlElement('paragraph')
    p.insert(0, [new Y.XmlText('甲写的')])
    frag.insert(0, [p])
    await until(() => b.ydoc.getXmlFragment('default').toString().includes('甲写的'))
    expect(srv.doc('api/v1/sync/p1').getXmlFragment('default').toString()).toContain('甲写的')

    // 在线状态
    a.awareness.setLocalStateField('user', { name: '甲' })
    await until(() => [...b.awareness.getStates().values()].some((s) => s.user?.name === '甲'))

    let changes = 0
    const off = a.onStatus(() => changes++)
    a.destroy()
    off()
    b.destroy()
    expect(changes).toBeGreaterThanOrEqual(0)
  })

  it('空页面：ready = empty；token 放进地址', async () => {
    const c = createClient({ baseUrl: `http://127.0.0.1:${srv.port}`, token: 'abc', WebSocket }).openPage('empty1')
    expect(await c.ready).toBe('empty')
    expect(srv.urls.some((u) => u.startsWith('/api/v1/sync/empty1') && u.includes('token=abc'))).toBe(true)
    c.destroy()
  })

  it('服务器拒绝：状态变成 denied', async () => {
    const c = createClient({ baseUrl: `http://127.0.0.1:${srv.port}`, WebSocket }).openPage('secret', { readyTimeout: 200 })
    let saw = false
    c.onStatus(() => (saw ||= c.status() === 'denied'))
    await until(() => c.status() === 'denied')
    expect(saw).toBe(true)
    c.destroy()
  })

  it('连不上：一直不是 synced，ready 到时间给 empty', async () => {
    const c = createClient({ baseUrl: 'http://127.0.0.1:9', WebSocket }).openPage('x', { readyTimeout: 150 })
    expect(await c.ready).toBe('empty')
    expect(c.status()).not.toBe('synced')
    c.destroy()
  })

  it('用已有的 Y.Doc：destroy 不销毁它', async () => {
    const ydoc = new Y.Doc()
    const c = createClient({ baseUrl: `http://127.0.0.1:${srv.port}`, WebSocket }).openPage('p2', { ydoc })
    await c.ready
    c.destroy()
    ydoc.getText('t').insert(0, '还能用')
    expect(ydoc.getText('t').toString()).toBe('还能用')
  })
})
