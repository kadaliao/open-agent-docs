// 全局状态、接口、地址和实时事件。
// - 地址：/（首页）、/pages/<id>、/trash、/settings；/s/<token>（分享出去的页面）在 main.tsx 里单独处理，不加载这里的东西。
// - 实时：/api/events（SSE），事件和 OSA 资料库的 websocket 一样（open_agent_docs/events.py），转成 window 上的 oad-… 事件给打开着的组件。
import { useSyncExternalStore } from 'react'
import { syncLang, t } from './i18n'
import type { AgentInfo, Bootstrap, Page } from './types'
import { notice } from './components/Dialog'

export type Route = { name: 'home' } | { name: 'page'; id: number } | { name: 'trash' } | { name: 'settings' }

type State = {
  ready: boolean
  authed: boolean
  connected: boolean
  version: string
  agents: AgentInfo[]     // Agent 的名字（段、评论的作者 agent:<id>），撤销了的也在
  colors: string[]        // Agent 的颜色：按编号轮着用
  route: Route
  pages: Page[] | null     // 页面列表（侧栏的树、首页的最近页面）；数据库的行不在里面
}

// Agent 写的段左边那条线的颜色（明暗两种界面都看得清）
const COLORS = ['#5b8def', '#e0794c', '#3fae7c', '#c99a1e', '#d0649a', '#4fa3b8', '#8f7ae6', '#a5803f']

let state: State = { ready: false, authed: false, connected: false, version: '', agents: [], colors: COLORS, route: parse(location.pathname), pages: null }
const subs = new Set<() => void>()
const set = (patch: Partial<State> | ((s: State) => Partial<State>)) => {
  state = { ...state, ...(typeof patch === 'function' ? patch(state) : patch) }
  subs.forEach(f => f())
}
export const getState = () => state

export function useStore<T>(sel: (s: State) => T): T {
  return useSyncExternalStore(cb => (subs.add(cb), () => subs.delete(cb)), () => sel(state))
}

// ---------- REST ----------
/** 接口报错：带着状态码和返回的内容（比如 409 冲突时服务器给的详情）。网络不通时 fetch 自己抛 TypeError，没有 status。 */
export class ApiError extends Error {
  constructor(message: string, readonly status: number, readonly data: any) { super(message) }
}

export async function api<T = any>(path: string, opts: { method?: string; body?: any } = {}): Promise<T> {
  const r = await fetch(path, {
    method: opts.method || 'GET',
    headers: opts.body ? { 'content-type': 'application/json' } : undefined,
    body: opts.body ? JSON.stringify(opts.body) : undefined,
  })
  // 登录过期（改了密码、在别处退出了）：回登录页，登录后还在原来的地址
  if (r.status === 401 && !path.startsWith('/api/shared/') && path !== '/api/login' && state.authed) { disconnect(); set({ authed: false }) }
  const data = await r.json().catch(() => ({}))
  if (!r.ok) throw new ApiError(typeof data.detail === 'string' ? data.detail : t('请求失败（{status}）', { status: r.status }), r.status, data)
  return data
}

export async function bootstrap() {
  const b: Bootstrap = await api('/api/bootstrap')
  if (syncLang(b.lang)) return new Promise<void>(() => {})   // 语言和服务端的不一样：重新加载中
  set({ ready: true, authed: b.authed, version: b.version, agents: b.agents || [] })
  if (b.authed) { connect(); refreshPages() }
}

/** 重新取页面列表：页面列表变了（pages_changed）、断线重连以后。同时只取一次，取的时候又变了就再取一次。 */
let fetching: Promise<void> | null = null, again = false
export function refreshPages(): Promise<void> {
  if (fetching) { again = true; return fetching }
  fetching = api<Page[]>('/api/pages').then(pages => set({ pages })).catch(() => {}).finally(() => {
    fetching = null
    if (again) { again = false; refreshPages() }
  })
  return fetching
}
window.addEventListener('oad-pages', () => { if (state.authed) refreshPages() })

export async function login(password: string) {
  await api('/api/login', { method: 'POST', body: { password } })
  await bootstrap()
}

export async function logout() {
  await api('/api/logout', { method: 'POST' }).catch(() => {})
  disconnect()
  set({ authed: false, agents: [], pages: null })
}

// ---------- 地址 ----------
function parse(path: string): Route {
  const m = path.match(/^\/pages\/(\d+)\/?$/)
  if (m) return { name: 'page', id: Number(m[1]) }
  if (/^\/trash\/?$/.test(path)) return { name: 'trash' }
  if (/^\/settings\/?$/.test(path)) return { name: 'settings' }
  return { name: 'home' }
}
const pathOf = (r: Route) => r.name === 'page' ? `/pages/${r.id}` : r.name === 'home' ? '/' : `/${r.name}`

export function navigate(r: Route) {
  const path = pathOf(r)
  if (location.pathname !== path) history.pushState(null, '', path)
  set({ route: r })
}

/** 打开一个页面；没给就回首页。 */
export function openPage(id: number | null = null) {
  navigate(id ? { name: 'page', id } : { name: 'home' })
}

/** 浏览器前进后退：按地址切回去（不再往历史里加）。 */
export function followUrl() { set({ route: parse(location.pathname) }) }

/** 编辑器的分块加载失败（服务器换了新版、旧的分块已经没了）：提示刷新。 */
let warnedBuild = false
export function markNewBuild() {
  if (warnedBuild) return
  warnedBuild = true
  notice(t('服务器上的网页换了新版，刷新一下才能编辑。'))
}

// ---------- 实时事件（SSE） ----------
let es: EventSource | null = null
let opened = false      // 连上过：之后再连上就是断线重连，要把漏掉的补上
let retry: ReturnType<typeof setTimeout> | undefined

export function connect() {
  if (es) return
  clearTimeout(retry)
  es = new EventSource('/api/events')
  es.onopen = () => {
    set({ connected: true })
    if (opened) resync()
    opened = true
  }
  es.onerror = () => {
    set({ connected: false })
    // 服务器回了错误（比如登录过期）时 EventSource 不会自己重连：问一下是不是还登录着，是的话过一会儿重连
    if (es?.readyState === EventSource.CLOSED) {
      es = null
      api<Bootstrap>('/api/bootstrap').then(b => {
        if (!b.authed) { set({ authed: false }); return }
        retry = setTimeout(connect, 3000)
      }).catch(() => { retry = setTimeout(connect, 3000) })
    }
  }
  es.onmessage = e => { try { handle(JSON.parse(e.data)) } catch { /* 坏的事件不管 */ } }
}

function disconnect() {
  clearTimeout(retry)
  es?.close()
  es = null
  opened = false
  set({ connected: false })
}

async function resync() {
  // 断线重连后：作者名单、页面列表重新取，打开着的文档、数据库补上断线期间漏掉的改动
  try {
    const b: Bootstrap = await api('/api/bootstrap')
    if (syncLang(b.lang)) return
    if (b.agents) set({ agents: b.agents })
  } catch { /* 下次再说 */ }
  window.dispatchEvent(new CustomEvent('oad-pages'))
  window.dispatchEvent(new CustomEvent('oad-page-resync'))
}

function handle(d: any) {
  switch (d.type) {
    case 'agents_changed':
      set({ agents: d.agents || [] })
      window.dispatchEvent(new CustomEvent('oad-agents'))
      break
    case 'pages_changed':
      window.dispatchEvent(new CustomEvent('oad-pages'))
      break
    case 'db_changed':   // 数据库改了（行、属性、表结构、视图）：开着的库、行页面的属性栏重新取
      window.dispatchEvent(new CustomEvent('oad-db', { detail: d }))
      break
    case 'page_comments':   // 某个页面的评论变了：打开着的评论面板去取
      window.dispatchEvent(new CustomEvent('oad-page-comments', { detail: d }))
      break
    case 'page_changed':   // 某个页面的内容改了（Agent 改的，或者你在别的窗口改的）：打开着它的编辑器去取改动
      window.dispatchEvent(new CustomEvent('oad-page', { detail: d }))
      break
  }
}

// 在页面上点了站内的页面链接（/pages/12）
window.addEventListener('oad-open-page', e => openPage(Number((e as CustomEvent).detail)))
