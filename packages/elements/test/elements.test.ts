// 元素的基本行为（jsdom）：注册、属性变了重连、事件、token、移除时断开。服务器用假的 fetch / WebSocket。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import '../src/index.ts'
import { jwtExp } from '../src/base.ts'
import { addCss, toShadowCss } from '../src/styles.ts'
import { OadDatabaseElement, OadEditorElement, OadViewerElement } from '../src/index.ts'

vi.mock('@open-agent-docs/markdown', () => ({
  default: async () => {},
  toHtml: (md: string) => `<div class="oad-content"><p>转出来的：${md}</p></div>`,
}))

// jsdom 没有 ResizeObserver（标题框量宽度用）
;(globalThis as { ResizeObserver?: unknown }).ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
}

interface Call {
  url: string
  method: string
  auth: string | null
  credentials?: RequestCredentials
}
let calls: Call[] = []
let route: (url: URL, c: Call) => Response | undefined
const json = (v: unknown, status = 200) => new Response(JSON.stringify(v), { status, headers: { 'content-type': 'application/json' } })
const pages: Record<string, { id: string; title: string; workspace_id: string }> = {
  p1: { id: 'p1', title: '第一页', workspace_id: 'w1' },
  p2: { id: 'p2', title: '第二页', workspace_id: 'w1' },
}
const defaultRoute = (u: URL): Response => {
  const m = /^\/api\/v1\/pages\/([^/]+)(\/content)?$/.exec(u.pathname)
  if (u.pathname === '/api/v1/me') return json({ user: { id: 'u1', name: '王小明', kind: 'user' }, workspaces: [{ id: 'w1', name: 'W', role: 'editor' }] })
  if (m && !pages[m[1]]) return json({ error: { code: 'not_found', message: '这一页不存在' } }, 404)
  if (m && m[2]) {
    const id = m[1]
    if (u.searchParams.get('format') === 'html')
      return json({ html: `<div class="oad-content"><h1>${pages[id].title}</h1><img src="/api/v1/files/f1/a.png"><a href="/p/p2">去第二页</a><a href="https://example.com/x">外面</a><script>bad()</script></div>`, version: '1' })
    return json({ markdown: '# ' + pages[id].title, version: '1' })
  }
  if (m) return json(pages[m[1]])
  return json({ error: { code: 'not_found', message: '' } }, 404)
}

// 假的 WebSocket：记下连了哪些、关了哪些
class FakeWS {
  static all: FakeWS[] = []
  readyState = 0
  binaryType = 'arraybuffer'
  onopen: (() => void) | null = null
  onclose: ((e: unknown) => void) | null = null
  onmessage: ((e: unknown) => void) | null = null
  onerror: ((e: unknown) => void) | null = null
  closed = false
  constructor(public url: string) {
    FakeWS.all.push(this)
  }
  send() {}
  close() {
    this.closed = true
    this.readyState = 3
    this.onclose?.({ code: 1000 })
  }
}

beforeEach(() => {
  calls = []
  route = () => undefined
  FakeWS.all = []
  vi.stubGlobal('WebSocket', FakeWS)
  vi.stubGlobal('fetch', async (input: RequestInfo | URL, init: RequestInit = {}) => {
    const u = new URL(String(input), 'http://host.test')
    const c: Call = { url: u.href, method: init.method ?? 'GET', auth: new Headers(init.headers).get('authorization'), credentials: init.credentials }
    calls.push(c)
    return route(u, c) ?? defaultRoute(u)
  })
})
afterEach(() => {
  document.body.replaceChildren()
  vi.unstubAllGlobals()
})

const once = <T = unknown>(el: EventTarget, name: string, ms = 5000) =>
  new Promise<CustomEvent<T>>((resolve, reject) => {
    const t = setTimeout(() => reject(new Error('等不到 ' + name)), ms)
    el.addEventListener(name, (e) => (clearTimeout(t), resolve(e as CustomEvent<T>)), { once: true })
  })
const until = async (f: () => boolean, ms = 5000) => {
  const t0 = Date.now()
  while (!f()) {
    if (Date.now() - t0 > ms) throw new Error('等超时了')
    await new Promise((r) => setTimeout(r, 10))
  }
}
const tick = () => new Promise((r) => setTimeout(r, 0))
const fakeJwt = (exp: number) => `x.${btoa(JSON.stringify({ sub: 'u1', exp })).replace(/=+$/, '')}.y`

describe('注册', () => {
  it('三个元素都注册了，createElement 出来就是对应的类', () => {
    expect(customElements.get('oad-editor')).toBe(OadEditorElement)
    expect(customElements.get('oad-viewer')).toBe(OadViewerElement)
    expect(customElements.get('oad-database')).toBe(OadDatabaseElement)
    expect(document.createElement('oad-viewer')).toBeInstanceOf(OadViewerElement)
  })

  it('有 Shadow DOM，属性和 JS 属性对得上', () => {
    const el = document.createElement('oad-editor')
    expect(el.shadowRoot).toBeTruthy()
    el.pageId = 'p9'
    el.comments = true
    expect(el.getAttribute('page-id')).toBe('p9')
    expect(el.hasAttribute('comments')).toBe(true)
    el.setAttribute('base-url', 'https://docs.example.com/')
    expect(el.baseUrl).toBe('https://docs.example.com')
  })

  it('没给 page-id 时什么都不请求', async () => {
    document.body.append(document.createElement('oad-viewer'))
    await tick()
    expect(calls).toEqual([])
  })
})

describe('<oad-viewer>', () => {
  it('请求 format=html，画出来，带 token、不带 cookie；服务器路径补上 base-url；发 oad-ready', async () => {
    const el = document.createElement('oad-viewer')
    el.setAttribute('base-url', 'http://docs.test')
    el.setAttribute('page-id', 'p1')
    el.token = 'oad_tok'
    const ready = once<{ id: string; title: string }>(el, 'oad-ready')
    document.body.append(el)
    expect((await ready).detail).toEqual({ id: 'p1', title: '第一页' })
    const root = el.shadowRoot!
    expect(root.querySelector('.oad-viewer-title')!.textContent).toBe('第一页')
    expect(root.querySelector('h1')).toBeTruthy()
    expect(root.querySelector('img')!.getAttribute('src')).toBe('http://docs.test/api/v1/files/f1/a.png')
    expect(root.querySelector('link[rel=stylesheet]')!.getAttribute('href')).toBe('http://docs.test/embed/content.css')
    const c = calls.find((x) => x.url.includes('format=html'))!
    expect(c.url).toBe('http://docs.test/api/v1/pages/p1/content?format=html')
    expect(c.auth).toBe('Bearer oad_tok')
    expect(c.credentials).toBe('omit')
  })

  it('换 page-id：重新取，旧的换掉', async () => {
    const el = document.createElement('oad-viewer')
    el.setAttribute('page-id', 'p1')
    const r1 = once(el, 'oad-ready')
    document.body.append(el)
    await r1
    const r2 = once<{ id: string }>(el, 'oad-ready')
    el.pageId = 'p2'
    expect((await r2).detail.id).toBe('p2')
    expect(el.shadowRoot!.querySelector('.oad-viewer-title')!.textContent).toBe('第二页')
    expect(el.shadowRoot!.querySelectorAll('link[rel=stylesheet]').length).toBe(1)
  })

  it('在页面里挪位置（先移除再加上）不重连', async () => {
    const a = document.createElement('div')
    const b = document.createElement('div')
    document.body.append(a, b)
    const el = document.createElement('oad-viewer')
    el.setAttribute('page-id', 'p1')
    const r = once(el, 'oad-ready')
    a.append(el)
    await r
    const n = calls.length
    b.append(el)
    await tick()
    await tick()
    expect(calls.length).toBe(n)
  })

  it('点内部链接：发 oad-navigate（能 preventDefault）；外面的链接开新窗口', async () => {
    const el = document.createElement('oad-viewer')
    el.setAttribute('page-id', 'p1')
    el.setAttribute('base-url', 'http://docs.test')
    const r = once(el, 'oad-ready')
    document.body.append(el)
    await r
    const nav = once<{ href: string; pageId?: string }>(el, 'oad-navigate')
    el.addEventListener('oad-navigate', (e) => e.preventDefault())
    const links = el.shadowRoot!.querySelectorAll('a')
    links[0].click()
    expect((await nav).detail).toEqual({ href: 'http://docs.test/p/p2', pageId: 'p2' })
    links[1].dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, composed: true }))
    expect(links[1].target).toBe('_blank')
  })

  it('服务器还没有 format=html（400）：取 Markdown 自己转', async () => {
    route = (u) => (u.searchParams.get('format') === 'html' ? json({ error: { code: 'bad_request', message: 'format 只能是 markdown / json' } }, 400) : undefined)
    const el = document.createElement('oad-viewer')
    el.setAttribute('page-id', 'p1')
    const r = once(el, 'oad-ready')
    document.body.append(el)
    await r
    expect(el.shadowRoot!.textContent).toContain('转出来的：# 第一页')
  })

  it('页面不存在：oad-error（status 404），界面上写一行', async () => {
    const el = document.createElement('oad-viewer')
    el.setAttribute('page-id', 'nope')
    const err = once<{ status?: number; code?: string }>(el, 'oad-error')
    document.body.append(el)
    expect((await err).detail).toMatchObject({ status: 404, code: 'not_found' })
    const box = el.shadowRoot!.querySelector<HTMLElement>('.oad-el-error')!
    expect(box.hidden).toBe(false)
    expect(box.textContent).toBe('找不到')
    // 换成存在的页：错误收起来
    const r = once(el, 'oad-ready')
    el.pageId = 'p1'
    await r
    expect(box.hidden).toBe(true)
  })
})

describe('token', () => {
  it('没有 token：先调 getToken；收到 401 再调一次，用新的重发', async () => {
    let n = 0
    route = (u, c) => (u.pathname === '/api/v1/pages/p1' && c.auth === 'Bearer t1' ? json({ error: { code: 'unauthorized', message: '' } }, 401) : undefined)
    const el = document.createElement('oad-viewer')
    el.setAttribute('page-id', 'p1')
    el.getToken = async () => 't' + ++n
    const r = once(el, 'oad-ready')
    document.body.append(el)
    await r
    expect(n).toBe(2)
    expect(calls.filter((c) => c.url.endsWith('/pages/p1')).map((c) => c.auth)).toEqual(['Bearer t1', 'Bearer t2'])
    expect(el.token).toBe('t2')
  })

  it('JWT 快过期（不到 60 秒）：请求之前先换', async () => {
    const soon = fakeJwt(Math.floor(Date.now() / 1000) + 30)
    const fresh = fakeJwt(Math.floor(Date.now() / 1000) + 3600)
    const el = document.createElement('oad-viewer')
    el.setAttribute('page-id', 'p1')
    el.token = soon
    el.getToken = () => fresh
    const r = once(el, 'oad-ready')
    document.body.append(el)
    await r
    expect(calls.every((c) => c.auth === 'Bearer ' + fresh)).toBe(true)
    expect(jwtExp(fresh)).toBeGreaterThan(Date.now() / 1000)
    expect(jwtExp('oad_agent_token')).toBeNull()
  })

  it('元素定义之前就设了 el.token / el.getToken（脚本后加载）：升级以后照样认', async () => {
    const el = document.createElement('oad-viewer-late') as HTMLElement & { token?: string; getToken?: () => string }
    el.setAttribute('page-id', 'p1')
    el.token = 'abc'
    el.getToken = () => 'fresh'
    customElements.define('oad-viewer-late', class extends OadViewerElement {})
    customElements.upgrade(el)
    expect(Object.prototype.hasOwnProperty.call(el, 'token')).toBe(false)
    expect(el.token).toBe('abc')
    expect(typeof (el as unknown as OadViewerElement).getToken).toBe('function')
    const r = once(el, 'oad-ready')
    document.body.append(el)
    await r
    expect(calls[0].auth).toBe('Bearer abc')
  })
})

describe('<oad-editor>', () => {
  it('连 WebSocket（带 token），内容到了发 oad-ready；换页重连；移除时断开', async () => {
    const el = document.createElement('oad-editor')
    el.setAttribute('base-url', 'http://docs.test')
    el.setAttribute('page-id', 'p1')
    el.token = 'oad_tok'
    document.body.append(el)
    await until(() => FakeWS.all.length === 1, 10_000)
    const ws1 = FakeWS.all[0]
    expect(ws1.url).toMatch(/^ws:\/\/docs\.test\/api\/v1\/sync\/p1\?token=oad_tok$/)
    // 服务器一直不回：readyTimeout（2.5 秒）后也算到了
    const ready = await once<{ id: string; title: string }>(el, 'oad-ready', 5000)
    expect(ready.detail).toEqual({ id: 'p1', title: '第一页' })
    expect(el.shadowRoot!.querySelector('.oad-prose')).toBeTruthy()
    expect(el.shadowRoot!.querySelector('.oad-title')).toBeTruthy()

    // token 换了：下次重连用新的
    el.token = 'oad_new'
    el.pageId = 'p2'
    await until(() => FakeWS.all.length === 2, 10_000)
    expect(ws1.closed).toBe(true)
    expect(FakeWS.all[1].url).toContain('/sync/p2?token=oad_new')

    el.remove()
    await until(() => FakeWS.all[1].closed)
  }, 20_000)

  it('只有查看权限：只读', async () => {
    route = (u) => (u.pathname === '/api/v1/me' ? json({ user: { id: 'u1', name: '王', kind: 'user' }, workspaces: [{ id: 'w1', name: 'W', role: 'viewer' }] }) : undefined)
    const el = document.createElement('oad-editor')
    el.setAttribute('page-id', 'p1')
    document.body.append(el)
    await until(() => !!el.shadowRoot!.querySelector('.oad-prose'), 10_000)
    expect(el.shadowRoot!.querySelector('.oad-prose')!.getAttribute('contenteditable')).toBe('false')
  }, 20_000)
})

describe('样式', () => {
  it(':root 换成 :host，data-theme 换成 theme 属性', () => {
    const css = ":root { --a: 1 } @media (prefers-color-scheme: dark) { :root:not([data-theme='light']) { --a: 2 } } :root[data-theme='dark'] .x { --a: 3 } .oad-rooted {}"
    expect(toShadowCss(css)).toBe(":host { --a: 1 } @media (prefers-color-scheme: dark) { :host(:not([theme='light'])) { --a: 2 } } :host([theme='dark']) .x { --a: 3 } .oad-rooted {}")
  })

  it('后来登记的样式（按需加载的）也加到已经在页面上的元素里；@font-face 挂到 document 上', () => {
    const el = document.createElement('oad-viewer')
    document.body.append(el)
    const count = () => (el.shadowRoot!.adoptedStyleSheets?.length ?? 0) + el.shadowRoot!.querySelectorAll('style').length
    const before = count()
    addCss('@font-face { font-family: X; src: url(x.woff2) } .later { color: red }')
    expect(count()).toBe(before + 1)
    expect(document.head.querySelector('style[data-oad-fonts]')!.textContent).toContain('font-family: X')
  })
})
