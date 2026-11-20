// 实时事件：解析 SSE（被切成任意小块也行），断了自动重连，带 Last-Event-ID 接着收；关掉以后不再连。
import { describe, expect, it } from 'vitest'
import { createClient } from '../src/core'
import { openEvents } from '../src/events'

/** 一个可以手动推数据、手动断开的流 */
function stream() {
  let ctrl!: ReadableStreamDefaultController<Uint8Array>
  const body = new ReadableStream<Uint8Array>({ start: (c) => (ctrl = c) })
  const enc = new TextEncoder()
  return {
    body,
    push: (s: string) => ctrl.enqueue(enc.encode(s)),
    end: () => ctrl.close(),
  }
}

const until = async (f: () => boolean, ms = 3000) => {
  const t0 = Date.now()
  while (!f()) {
    if (Date.now() - t0 > ms) throw new Error('等超时了')
    await new Promise((r) => setTimeout(r, 5))
  }
}

describe('events', () => {
  it('解析事件名、多行 data、注释心跳，被切开的块也能拼好', async () => {
    const s = stream()
    const fetch = (async () => new Response(s.body, { status: 200 })) as typeof globalThis.fetch
    const ev = openEvents('/e', { credentials: 'same-origin', headers: () => ({}), fetch })
    const got: [string, unknown][] = []
    ev.on('*', (d, n) => got.push([n, d]))
    const pages: unknown[] = []
    ev.on('page.updated', (d) => pages.push(d))
    s.push(': ping\n\nid: 1\nevent: page.upd')
    s.push('ated\ndata: {"id":"p1",\ndata: "title":"甲"}\n\n')
    s.push('event: notification\r\ndata: 不是 JSON\r\n\r\n')
    await until(() => got.length === 2)
    expect(got).toEqual([
      ['page.updated', { id: 'p1', title: '甲' }],
      ['notification', '不是 JSON'],
    ])
    expect(pages).toEqual([{ id: 'p1', title: '甲' }])
    ev.close()
  })

  it('断了重连：第二次带上最后收到的 Last-Event-ID，之前的事件不重复', async () => {
    const streams = [stream(), stream()]
    const seen: Record<string, string>[] = []
    let n = 0
    const fetch = (async (_u: RequestInfo | URL, init: RequestInit = {}) => {
      const h: Record<string, string> = {}
      new Headers(init.headers).forEach((v, k) => (h[k] = v))
      seen.push(h)
      return new Response(streams[n++].body, { status: 200 })
    }) as typeof globalThis.fetch
    const ev = openEvents('/e', { credentials: 'include', headers: () => ({ authorization: 'Bearer t' }), fetch, minRetry: 10 })
    const status: boolean[] = []
    ev.onStatus((c) => status.push(c))
    const ids: string[] = []
    ev.on('db.changed', (d) => ids.push(d.seq))
    streams[0].push('id: 41\nevent: db.changed\ndata: {"seq":"41"}\n\nid: 42\nevent: db.changed\ndata: {"seq":"42"}\n\n')
    await until(() => ids.length === 2)
    streams[0].end()
    await until(() => seen.length === 2)
    expect(seen[0]['last-event-id']).toBeUndefined()
    expect(seen[1]['last-event-id']).toBe('42')
    expect(seen[1].authorization).toBe('Bearer t')
    expect(seen[1].accept).toBe('text/event-stream')
    streams[1].push('id: 43\nevent: db.changed\ndata: {"seq":"43"}\n\n')
    await until(() => ids.length === 3)
    expect(ids).toEqual(['41', '42', '43'])
    expect(status).toEqual([true, false, true])
    ev.close()
  })

  it('连不上（HTTP 500、网络错）就退避重试；close 以后不再连', async () => {
    let tries = 0
    const fetch = (async () => {
      tries++
      if (tries === 1) throw new Error('network')
      return new Response('x', { status: 500 })
    }) as typeof globalThis.fetch
    const ev = openEvents('/e', { credentials: 'omit', headers: () => ({}), fetch, minRetry: 10 })
    await until(() => tries >= 3)
    ev.close()
    const at = tries
    await new Promise((r) => setTimeout(r, 120))
    expect(tries).toBe(at)
  })

  it('client.events(ws)：地址、token', async () => {
    const urls: string[] = []
    const auth: (string | null)[] = []
    const fetch = (async (u: RequestInfo | URL, init: RequestInit = {}) => {
      urls.push(String(u))
      auth.push(new Headers(init.headers).get('authorization'))
      return new Response(new ReadableStream(), { status: 200 })
    }) as typeof globalThis.fetch
    const ev = createClient({ baseUrl: 'http://h:1', token: 'tok', fetch }).events('w1')
    await until(() => urls.length === 1)
    expect(urls[0]).toBe('http://h:1/api/v1/workspaces/w1/events')
    expect(auth[0]).toBe('Bearer tok')
    ev.close()
  })
  it('401 / 403 / 404 不重连：关掉，onError 拿到 ApiError', async () => {
    let n = 0
    const fetch = (async () => {
      n++
      return new Response(JSON.stringify({ error: { code: 'unauthorized', message: '要登录' } }), { status: 401 })
    }) as typeof globalThis.fetch
    const ev = openEvents('/e', { credentials: 'same-origin', headers: () => ({}), fetch, minRetry: 5 })
    const errs: unknown[] = []
    ev.onError((e) => errs.push(e))
    await until(() => errs.length === 1)
    await new Promise((r) => setTimeout(r, 50))
    expect(n).toBe(1)
    expect(errs[0]).toMatchObject({ status: 401, code: 'unauthorized' })
    ev.close()
  })

  it('agentEvents：连 /agent/notifications/stream，after 放进查询参数', async () => {
    const urls: string[] = []
    const s = stream()
    const fetch = (async (u: string) => {
      urls.push(String(u))
      return new Response(s.body, { status: 200 })
    }) as typeof globalThis.fetch
    const ev = createClient({ fetch, baseUrl: 'http://h' }).agentEvents({ after: 7 })
    const got: unknown[] = []
    ev.on('notification', (d) => got.push(d))
    s.push('id: 8\nevent: notification\ndata: {"id":8}\n\n')
    await until(() => got.length === 1)
    expect(urls[0]).toBe('http://h/api/v1/agent/notifications/stream?after=7')
    expect(got).toEqual([{ id: 8 }])
    ev.close()
  })
})
