// 请求怎么发、错误怎么变成 ApiError、语言和登录方式（token / cookie）。
import { describe, expect, it } from 'vitest'
import { ApiError, createClient } from '../src/core'
import { fakeFetch, json } from './fake-fetch'

describe('HTTP 方法', () => {
  it('同源：路径前面加 /api/v1，带 cookie（same-origin），JSON 正文', async () => {
    const { fetch, calls } = fakeFetch(() => json({ id: 'p1', title: 'x' }, 201))
    const c = createClient({ fetch })
    const p = await c.createPage('ws1', { title: 'x', parent_id: null })
    expect(p).toEqual({ id: 'p1', title: 'x' })
    expect(calls[0]).toMatchObject({ url: '/api/v1/workspaces/ws1/pages', method: 'POST', credentials: 'same-origin', body: { title: 'x', parent_id: null } })
    expect(calls[0].headers['content-type']).toBe('application/json')
    expect(calls[0].headers.authorization).toBeUndefined()
  })

  it('别的域名：默认 credentials include；给了 token 就带 Authorization: Bearer（函数每次现取）', async () => {
    let tok = 'aaa'
    const { fetch, calls } = fakeFetch(() => json({ user: { id: 'u' }, workspaces: [] }))
    const c = createClient({ baseUrl: 'https://docs.example.com/', fetch, token: () => tok })
    await c.me()
    tok = 'bbb'
    await c.me()
    expect(calls[0].url).toBe('https://docs.example.com/api/v1/me')
    expect(calls[0].credentials).toBe('include')
    expect(calls[0].headers.authorization).toBe('Bearer aaa')
    expect(calls[1].headers.authorization).toBe('Bearer bbb')
    expect(calls[0].headers['content-type']).toBeUndefined()
  })

  it('credentials 可以关掉（Node、Agent）', async () => {
    const { fetch, calls } = fakeFetch(() => json({}))
    await createClient({ baseUrl: 'http://x', fetch, credentials: 'omit', token: 't' }).me()
    expect(calls[0].credentials).toBe('omit')
  })

  it('语言：Accept-Language 跟着 locale（zh → zh-CN，en → en，函数现取）', async () => {
    let loc = 'zh'
    const { fetch, calls } = fakeFetch(() => json({}))
    const c = createClient({ fetch, locale: () => loc })
    await c.me()
    loc = 'en'
    await c.me()
    expect(calls.map((x) => x.headers['accept-language'])).toEqual(['zh-CN', 'en'])
  })

  it('多带的头原样带上', async () => {
    const { fetch, calls } = fakeFetch(() => json({}))
    await createClient({ fetch, headers: { 'x-trace': '1' } }).me()
    expect(calls[0].headers['x-trace']).toBe('1')
  })

  it('出错：服务器的 { error: { code, message } } 变成 ApiError', async () => {
    const { fetch } = fakeFetch(() => json({ error: { code: 'not_found', message: '这一页不存在' } }, 404))
    const err = await createClient({ fetch })
      .page('nope')
      .catch((e) => e)
    expect(err).toBeInstanceOf(ApiError)
    expect(err).toMatchObject({ status: 404, code: 'not_found', message: '这一页不存在' })
  })

  it('出错但不是 JSON：code 是 http_<状态>', async () => {
    const { fetch } = fakeFetch(() => new Response('bad gateway', { status: 502, statusText: 'Bad Gateway' }))
    const err = await createClient({ fetch })
      .me()
      .catch((e) => e)
    expect(err).toMatchObject({ status: 502, code: 'http_502' })
  })

  it('204 和空正文返回 undefined', async () => {
    const { fetch } = fakeFetch(() => new Response(null, { status: 204 }))
    expect(await createClient({ fetch }).deletePage('p1')).toBeUndefined()
  })

  it('getShare：没分享（404）返回 null，别的错照样抛', async () => {
    let status = 404
    const { fetch } = fakeFetch(() => json({ error: { code: 'not_found', message: '' } }, status))
    const c = createClient({ fetch })
    expect(await c.getShare('p1')).toBeNull()
    status = 403
    await expect(c.getShare('p1')).rejects.toBeInstanceOf(ApiError)
  })

  it('查询参数编码：搜索词、unfurl 网址', async () => {
    const { fetch, calls } = fakeFetch((c) => (c.url.includes('/search') ? json({ results: [{ page_id: 'a' }] }) : json({ kind: 'bookmark' })))
    const c = createClient({ fetch })
    expect(await c.search('ws', '同步 协议&x', 5)).toEqual([{ page_id: 'a' }])
    await c.unfurl('https://a.b/c?d=1&e=2')
    expect(calls[0].url).toBe('/api/v1/workspaces/ws/search?q=%E5%90%8C%E6%AD%A5%20%E5%8D%8F%E8%AE%AE%26x&limit=5&kind=all')
    expect(calls[1].url).toBe('/api/v1/unfurl?url=https%3A%2F%2Fa.b%2Fc%3Fd%3D1%26e%3D2')
  })

  it('unfurl 失败返回 null（卡片按网址画最简单的）', async () => {
    const { fetch } = fakeFetch(() => json({ error: { code: 'x', message: '' } }, 500))
    expect(await createClient({ fetch }).unfurl('https://x')).toBeNull()
  })

  it('收件箱：data 里的字段摊平', async () => {
    const { fetch } = fakeFetch(() => json({ notifications: [{ id: 1, type: 'mention', data: { page: { id: 'p' }, quote: 'q' }, created_at: 1 }] }))
    const [n] = await createClient({ fetch }).inbox()
    expect(n).toMatchObject({ id: 1, type: 'mention', page: { id: 'p' }, quote: 'q' })
  })

  it('没登记协作文档 / 数据库时说清楚要怎么做', () => {
    const c = createClient({ fetch: fakeFetch(() => json({})).fetch })
    expect(() => c.openPage('p')).toThrow(/@oad\/client\/page/)
    expect(() => c.database('d')).toThrow(/@oad\/client\/db/)
  })
  it('上传（Node 里没有 XMLHttpRequest）：multipart，token 也带上', async () => {
    const { fetch, calls } = fakeFetch(() => json({ url: '/api/v1/files/f1/a.txt', name: 'a.txt', mime: 'text/plain', size: 3 }, 201))
    const up = await createClient({ baseUrl: 'http://h', token: 't', fetch }).upload('ws', new File(['abc'], 'a.txt', { type: 'text/plain' }))
    expect(up.url).toBe('/api/v1/files/f1/a.txt')
    expect(calls[0]).toMatchObject({ url: 'http://h/api/v1/workspaces/ws/files', method: 'POST' })
    expect(calls[0].headers.authorization).toBe('Bearer t')
    expect(calls[0].body).toBeInstanceOf(FormData)
    expect(((calls[0].body as FormData).get('file') as File).name).toBe('a.txt')
  })
})
