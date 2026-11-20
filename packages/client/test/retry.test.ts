// 重试、超时、错误的判断函数、embedUrl（照 docs/sdk-contract.md §5、§7、§10）。
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import {
  ApiConnectionError,
  ApiError,
  ApiTimeoutError,
  createClient,
  embedUrl,
  isApiError,
  isConflict,
  isConnectionError,
  isNotFound,
  isRateLimited,
  isServerError,
  isTimeout,
  isUnauthorized,
  retryDelay,
} from '../src/core'
import { fakeFetch, json } from './fake-fetch'

const vectors = JSON.parse(readFileSync(new URL('../../../sdks/conformance/vectors.json', import.meta.url), 'utf8'))
const err = (status: number, extra: Record<string, string> = {}) =>
  new Response(JSON.stringify({ error: { code: 'x', message: 'm' } }), { status, headers: { 'content-type': 'application/json', ...extra } })

describe('重试', () => {
  it('429 带 Retry-After：照它等，再试成功', async () => {
    let n = 0
    const { fetch, calls } = fakeFetch(() => (n++ === 0 ? err(429, { 'retry-after': '0' }) : json({ ok: 1 })))
    expect(await createClient({ fetch }).me()).toEqual({ ok: 1 })
    expect(calls.length).toBe(2)
  })

  it('最多重试 maxRetries 次，最后抛 RateLimit，带 retryAfter', async () => {
    const { fetch, calls } = fakeFetch(() => err(429, { 'retry-after': '0' }))
    const e = await createClient({ fetch, maxRetries: 3 })
      .me()
      .catch((x) => x)
    expect(calls.length).toBe(4)
    expect(isRateLimited(e)).toBe(true)
    expect(e.retryAfter).toBe(0)
  })

  it('maxRetries: 0 不重试', async () => {
    const { fetch, calls } = fakeFetch(() => err(503, { 'retry-after': '0' }))
    await expect(createClient({ fetch, maxRetries: 0 }).me()).rejects.toBeInstanceOf(ApiError)
    expect(calls.length).toBe(1)
  })

  it('GET 遇到 502 / 503 / 504 重试；POST、PATCH 不重试（可能已经生效了），但 429 照样重试', async () => {
    for (const s of [502, 503, 504]) {
      let n = 0
      const { fetch, calls } = fakeFetch(() => (n++ === 0 ? err(s, { 'retry-after': '0' }) : json({})))
      await createClient({ fetch }).me()
      expect(calls.length).toBe(2)
    }
    const post = fakeFetch(() => err(503, { 'retry-after': '0' }))
    await expect(createClient({ fetch: post.fetch }).createPage('w', { title: 'x' })).rejects.toMatchObject({ status: 503 })
    expect(post.calls.length).toBe(1)
    const patch = fakeFetch(() => err(502, { 'retry-after': '0' }))
    await expect(createClient({ fetch: patch.fetch }).patchPage('p', { title: 'x' })).rejects.toMatchObject({ status: 502 })
    expect(patch.calls.length).toBe(1)
    let n = 0
    const post429 = fakeFetch(() => (n++ === 0 ? err(429, { 'retry-after': '0' }) : json({ id: 'p' }, 201)))
    expect(await createClient({ fetch: post429.fetch }).createPage('w', { title: 'x' })).toEqual({ id: 'p' })
    expect(post429.calls.length).toBe(2)
  })

  it('500、404 这些不重试', async () => {
    for (const s of [400, 401, 404, 409, 500]) {
      const { fetch, calls } = fakeFetch(() => err(s))
      await expect(createClient({ fetch }).me()).rejects.toMatchObject({ status: s })
      expect(calls.length).toBe(1)
    }
  })

  it('连不上：GET 重试，POST 只在确定没发出去（ECONNREFUSED）时重试', async () => {
    const reset = Object.assign(new TypeError('fetch failed'), { cause: { code: 'ECONNRESET' } })
    const refused = Object.assign(new TypeError('fetch failed'), { cause: { code: 'ECONNREFUSED' } })
    let calls = 0
    const flaky = (e: Error) =>
      (async () => {
        if (calls++ === 0) throw e
        return json({ id: 'p' }, 201)
      }) as unknown as typeof fetch
    calls = 0
    expect(await createClient({ fetch: flaky(reset) }).me()).toEqual({ id: 'p' })
    calls = 0
    await expect(createClient({ fetch: flaky(reset) }).createPage('w', { title: 'x' })).rejects.toBeInstanceOf(ApiConnectionError)
    expect(calls).toBe(1)
    calls = 0
    expect(await createClient({ fetch: flaky(refused) }).createPage('w', { title: 'x' })).toEqual({ id: 'p' })
    expect(calls).toBe(2)
  })

  it('retryConnectionErrors: false：连不上马上报错，429、503 照样重试', async () => {
    const refused = Object.assign(new TypeError('Failed to fetch'), { cause: { code: 'ECONNREFUSED' } })
    let calls = 0
    const down = (async () => {
      calls++
      throw refused
    }) as unknown as typeof fetch
    const t0 = Date.now()
    const e = await createClient({ fetch: down, retryConnectionErrors: false })
      .me()
      .catch((x) => x)
    expect(e).toBeInstanceOf(ApiConnectionError)
    expect(isConnectionError(e)).toBe(true)
    expect(calls).toBe(1)
    expect(Date.now() - t0).toBeLessThan(400)
    for (const s of [429, 503]) {
      let n = 0
      const { fetch, calls: c } = fakeFetch(() => (n++ === 0 ? err(s, { 'retry-after': '0' }) : json({ ok: 1 })))
      expect(await createClient({ fetch, retryConnectionErrors: false }).me()).toEqual({ ok: 1 })
      expect(c.length).toBe(2)
    }
  })

  it('等待：Retry-After 最多 60 秒；没有就 0.5 秒 × 2^n，最多 8 秒，加 0–25%', () => {
    expect(retryDelay(0, 3)).toBe(3000)
    expect(retryDelay(0, 600)).toBe(60_000)
    for (let n = 0; n < 8; n++) {
      const d = retryDelay(n)
      const base = Math.min(8000, 500 * 2 ** n)
      expect(d).toBeGreaterThanOrEqual(base)
      expect(d).toBeLessThanOrEqual(base * 1.25)
    }
  })
})

describe('超时', () => {
  it('超过 timeout：ApiTimeoutError（不重试）', async () => {
    let calls = 0
    const slow = ((_: unknown, init: RequestInit) =>
      new Promise((_, reject) => {
        calls++
        init.signal?.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')))
      })) as unknown as typeof fetch
    const e = await createClient({ fetch: slow, timeout: 50 })
      .me()
      .catch((x) => x)
    expect(e).toBeInstanceOf(ApiTimeoutError)
    expect(isTimeout(e)).toBe(true)
    expect(isConnectionError(e)).toBe(true)
    expect(calls).toBe(1)
  })
})

describe('错误', () => {
  it('判断函数：按状态码，也认别的副本里的 ApiError', () => {
    expect(isNotFound(new ApiError(404, 'not_found', ''))).toBe(true)
    expect(isNotFound(new ApiError(403, 'forbidden', ''))).toBe(false)
    expect(isUnauthorized(new ApiError(401, 'x', ''))).toBe(true)
    expect(isConflict(new ApiError(409, 'x', ''))).toBe(true)
    expect(isServerError(new ApiError(502, 'x', ''))).toBe(true)
    const foreign = Object.assign(new Error('m'), { name: 'ApiError', status: 404, code: 'not_found' })
    expect(isApiError(foreign)).toBe(true)
    expect(isNotFound(foreign)).toBe(true)
    expect(isNotFound(new Error('x'))).toBe(false)
    expect(isNotFound(null)).toBe(false)
  })

  it('X-Request-Id 放进 requestId', async () => {
    const { fetch } = fakeFetch(() => err(404, { 'x-request-id': 'req-1' }))
    await expect(createClient({ fetch }).page('nope')).rejects.toMatchObject({ requestId: 'req-1', status: 404 })
  })
})

describe('其它', () => {
  it('embedUrl：vectors.json 的用例逐字一样', () => {
    for (const v of vectors.embed_url) expect(embedUrl(v.base_url, v.page_id, v.options)).toBe(v.expect)
  })

  it("content(id, 'html')：带 format=html", async () => {
    const { fetch, calls } = fakeFetch(() => json({ html: '<div class="oad-content"><h1>x</h1></div>', version: '1' }))
    const r = await createClient({ fetch }).content('p1', 'html')
    expect(r.html).toContain('<h1>')
    expect(calls[0].url).toBe('/api/v1/pages/p1/content?format=html')
  })

  it('defaultWorkspace：/me 的第一个，只取一次', async () => {
    const { fetch, calls } = fakeFetch(() => json({ user: { id: 'u' }, workspaces: [{ id: 'w1' }, { id: 'w2' }] }))
    const c = createClient({ fetch })
    expect(await c.defaultWorkspace()).toBe('w1')
    expect(await c.defaultWorkspace()).toBe('w1')
    expect(calls.length).toBe(1)
  })

  it('下载、导出 CSV、导入 CSV、删一行', async () => {
    const { fetch, calls } = fakeFetch((c) => {
      if (c.url.endsWith('export.csv')) return new Response('a,b\n1,2\n', { headers: { 'content-type': 'text/csv' } })
      if (c.url.includes('/files/')) return new Response(new Uint8Array([1, 2, 3]))
      return json({ id: 'd1' }, c.method === 'POST' ? 201 : 200)
    })
    const c = createClient({ fetch })
    expect(await c.exportCsv('d1')).toBe('a,b\n1,2\n')
    const b = await c.download('f1', { w: 320 })
    expect([...new Uint8Array(await b.arrayBuffer())]).toEqual([1, 2, 3])
    await c.downloadNamed('f1', 'a b.png')
    await c.importCsv('ws', 'a,b\n1,2', { title: '表' })
    await c.deleteRow('d1', 'r1')
    expect(calls.map((x) => `${x.method} ${x.url}`)).toEqual([
      'GET /api/v1/databases/d1/export.csv',
      'GET /api/v1/files/f1?w=320',
      'GET /api/v1/files/f1/a%20b.png',
      'POST /api/v1/workspaces/ws/databases/import?title=%E8%A1%A8',
      'DELETE /api/v1/databases/d1/rows/r1',
    ])
    expect(calls[3].headers['content-type']).toBe('text/csv')
  })

  it('上传字节：用 name 给文件名', async () => {
    const { fetch, calls } = fakeFetch(() => json({ id: 'f', url: '/x', name: 'a.txt', mime: 'text/plain', size: 5 }, 201))
    await createClient({ fetch }).upload('ws', new TextEncoder().encode('hello'), { name: 'a.txt' })
    expect(((calls[0].body as FormData).get('file') as File).name).toBe('a.txt')
  })

  it('Node 里带 User-Agent: open-agent-docs-js/<版本>', async () => {
    const { fetch, calls } = fakeFetch(() => json({}))
    await createClient({ fetch }).me()
    expect(calls[0].headers['user-agent']).toMatch(/^open-agent-docs-js\/\d+\.\d+\.\d+/)
  })
})
