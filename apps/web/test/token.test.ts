// 跨域嵌入的登录（src/token.ts）：#token= 只放内存、从地址栏抹掉；宿主的 oad:token 要检查 origin 和来源；
// 401、快过期时向宿主要新的。测试在 Node 里跑，浏览器的几个全局对象用假的。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

type Msg = { type: string; reason?: string }
const parent = { posted: [] as Msg[], postMessage(m: Msg) { this.posted.push(m) } }
let listeners: ((e: unknown) => void)[] = []
let replaced: string[] = []

function b64url(s: string) {
  return Buffer.from(s, 'utf8').toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}
function jwt(claims: Record<string, unknown>) {
  return `${b64url('{"alg":"HS256","typ":"JWT"}')}.${b64url(JSON.stringify(claims))}.c2ln`
}

/** 按给的地址装上假的浏览器环境，重新加载 token.ts */
async function load(href: string) {
  const u = new URL(href)
  parent.posted = []
  listeners = []
  replaced = []
  const loc = { origin: u.origin, pathname: u.pathname, search: u.search, hash: u.hash }
  vi.stubGlobal('location', loc)
  vi.stubGlobal('history', {
    state: null,
    replaceState: (_s: unknown, _t: string, url: string) => {
      replaced.push(url)
      const n = new URL(url, u.origin)
      loc.hash = n.hash
    },
  })
  vi.stubGlobal('window', {
    parent,
    addEventListener: (type: string, f: (e: unknown) => void) => type === 'message' && listeners.push(f),
  })
  vi.stubGlobal('fetch', async () => new Response(JSON.stringify({ mode: 'jwt', allowed_origins: ['https://app.example.com'] })))
  vi.resetModules()
  return import('../src/token')
}

beforeEach(() => vi.useFakeTimers({ now: new Date('2026-11-20T00:00:00Z') }))
afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('#token=', () => {
  it('嵌入模式：读出来放内存，从地址栏抹掉，别的 # 参数留着', async () => {
    const t = jwt({ sub: 'u1', exp: Date.now() / 1000 + 3600 })
    const m = await load(`https://docs.example.com/p/abc?embed=1&theme=dark#token=${t}&x=1`)
    expect(m.getToken()).toBe(t)
    expect(replaced).toEqual(['/p/abc?embed=1&theme=dark#x=1'])
    expect(location.hash).not.toContain('token')
  })

  it('只有 token：# 整个去掉', async () => {
    const m = await load('https://docs.example.com/p/abc?embed=1#token=abc.def.ghi')
    expect(m.getToken()).toBe('abc.def.ghi')
    expect(replaced).toEqual(['/p/abc?embed=1'])
  })

  it('不是嵌入模式：不碰地址，不用 token', async () => {
    const m = await load('https://docs.example.com/p/abc#token=abc.def.ghi')
    expect(m.getToken()).toBeNull()
    expect(replaced).toEqual([])
    expect(listeners).toHaveLength(0)
    // 401 也不发消息
    m.authRequired('unauthorized')
    expect(parent.posted).toEqual([])
  })

  it('embed=0 不算嵌入', async () => {
    const m = await load('https://docs.example.com/p/abc?embed=0#token=a.b.c')
    expect(m.getToken()).toBeNull()
  })
})

describe('JWT 的 exp', () => {
  it('解出 exp；不是 JWT 的返回 null', async () => {
    const m = await load('https://docs.example.com/p/abc?embed=1')
    expect(m.jwtExp(jwt({ sub: 'u', name: '王小明', exp: 1234 }))).toBe(1234)
    expect(m.jwtExp(jwt({ sub: 'u' }))).toBeNull()
    expect(m.jwtExp('oad_agenttoken')).toBeNull()
    expect(m.jwtExp('a.%%%.c')).toBeNull()
  })
})

describe('快过期时向宿主要新的', () => {
  it('离过期 60 秒时发一次 oad:auth-required；换了新的重新算', async () => {
    const t = jwt({ sub: 'u1', exp: Date.now() / 1000 + 120 })
    const m = await load(`https://docs.example.com/p/abc?embed=1#token=${t}`)
    vi.advanceTimersByTime(59_000)
    expect(parent.posted).toEqual([])
    vi.advanceTimersByTime(2_000)
    expect(parent.posted).toEqual([{ type: 'oad:auth-required', reason: 'expiring' }])
    vi.advanceTimersByTime(120_000)
    expect(parent.posted).toHaveLength(1)
    // 宿主回了新的
    m.setToken(jwt({ sub: 'u1', exp: Date.now() / 1000 + 90 }))
    vi.advanceTimersByTime(31_000)
    expect(parent.posted).toHaveLength(2)
  })

  it('拿到时就不到 60 秒：马上要', async () => {
    const t = jwt({ sub: 'u1', exp: Date.now() / 1000 + 30 })
    await load(`https://docs.example.com/p/abc?embed=1#token=${t}`)
    expect(parent.posted).toEqual([{ type: 'oad:auth-required', reason: 'expiring' }])
  })

  it('401：发 unauthorized，同一个 token 只发一次', async () => {
    const m = await load('https://docs.example.com/p/abc?embed=1#token=a.b.c')
    const f = m.watch401(async () => new Response('{}', { status: 401 }))
    await f('/api/v1/me')
    await f('/api/v1/me')
    expect(parent.posted).toEqual([{ type: 'oad:auth-required', reason: 'unauthorized' }])
    m.setToken('d.e.f')
    await f('/api/v1/me')
    expect(parent.posted).toHaveLength(2)
    // 200 不发
    await m.watch401(async () => new Response('{}'))('/api/v1/me')
    expect(parent.posted).toHaveLength(2)
  })
})

describe('宿主发来 oad:token', () => {
  const allowed = async () => ['https://app.example.com']

  it('允许的 origin、来自父窗口：换上，通知订阅的人', async () => {
    const m = await load('https://docs.example.com/p/abc?embed=1')
    const got: string[] = []
    m.onToken((t) => got.push(t))
    expect(await m.handleTokenMessage({ data: { type: 'oad:token', token: 'n.e.w' }, origin: 'https://app.example.com', source: parent as never }, allowed, parent)).toBe(true)
    expect(m.getToken()).toBe('n.e.w')
    expect(got).toEqual(['n.e.w'])
    // 同源的也认
    expect(await m.handleTokenMessage({ data: { type: 'oad:token', token: 's.a.me' }, origin: 'https://docs.example.com', source: parent as never }, allowed, parent)).toBe(true)
  })

  it('别的 origin、不是父窗口、格式不对：不认', async () => {
    const m = await load('https://docs.example.com/p/abc?embed=1#token=o.l.d')
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const bad = [
      { data: { type: 'oad:token', token: 'x' }, origin: 'https://evil.example.com', source: parent },
      { data: { type: 'oad:token', token: 'x' }, origin: 'null', source: parent },
      { data: { type: 'oad:token', token: 'x' }, origin: 'https://app.example.com', source: {} },
      { data: { type: 'oad:token', token: 123 }, origin: 'https://app.example.com', source: parent },
      { data: { type: 'oad:token', token: '' }, origin: 'https://app.example.com', source: parent },
      { data: 'oad:token', origin: 'https://app.example.com', source: parent },
    ]
    for (const e of bad) expect(await m.handleTokenMessage(e as never, allowed, parent)).toBe(false)
    expect(m.getToken()).toBe('o.l.d')
    warn.mockRestore()
  })

  it('允许的列表从 /auth/config 来（真的 message 监听）', async () => {
    const m = await load('https://docs.example.com/p/abc?embed=1')
    expect(listeners).toHaveLength(1)
    listeners[0]({ data: { type: 'oad:token', token: 'f.r.om' }, origin: 'https://app.example.com', source: parent })
    await vi.waitFor(() => expect(m.getToken()).toBe('f.r.om'))
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    listeners[0]({ data: { type: 'oad:token', token: 'e.v.il' }, origin: 'https://evil.example.com', source: parent })
    await Promise.resolve()
    await new Promise((r) => queueMicrotask(() => r(null)))
    expect(m.getToken()).toBe('f.r.om')
  })

  it('origin 比较：大小写、结尾的 / 不算', async () => {
    const m = await load('https://docs.example.com/p/abc?embed=1')
    expect(m.originAllowed('https://APP.example.com', ['https://app.example.com/'])).toBe(true)
    expect(m.originAllowed('https://app.example.com:8443', ['https://app.example.com'])).toBe(false)
    expect(m.originAllowed('', [])).toBe(false)
  })
})
