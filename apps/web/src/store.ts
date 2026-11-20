// 页面树的状态：先用本机存的上一份列表画出来，再拉服务器的，之后跟着 SSE 事件改。
import { useSyncExternalStore } from 'react'
import { api, eventsUrl, type Page } from './api'
import { navigate, pagePath } from './lib'
import { EMBED_AUTH } from './token'

/** 交给订阅的人的事件（评论栏、收件箱、数据库） */
const BUS_EVENTS = ['comment.created', 'comment.updated', 'comment.deleted', 'thread.updated', 'notification', 'db.changed', 'db.schema']

type Listener = () => void

const CACHE_KEY = (ws: string) => `oad:pages:${ws}`

function readCache(ws: string): Page[] {
  try {
    const raw = localStorage.getItem(CACHE_KEY(ws))
    return raw ? (JSON.parse(raw) as Page[]) : []
  } catch {
    return []
  }
}

export class PageStore {
  pages = new Map<string, Page>()
  loaded = false
  private listeners = new Set<Listener>()
  private snapshot: Page[] = []
  private es: { close: () => void } | null = null
  private saveTimer = 0

  constructor(public ws: string) {
    for (const p of readCache(ws)) this.pages.set(p.id, p)
    this.snapshot = [...this.pages.values()]
  }

  subscribe = (l: Listener) => {
    this.listeners.add(l)
    return () => this.listeners.delete(l)
  }
  getSnapshot = () => this.snapshot

  private emit() {
    this.snapshot = [...this.pages.values()]
    this.listeners.forEach((l) => l())
    clearTimeout(this.saveTimer)
    this.saveTimer = window.setTimeout(() => {
      try {
        localStorage.setItem(CACHE_KEY(this.ws), JSON.stringify(this.snapshot))
      } catch {
        /* 存不下就算了 */
      }
    }, 300)
  }

  async load() {
    const list = (await api.pages(this.ws)).filter((p) => p.kind !== 'row')
    this.pages = new Map(list.map((p) => [p.id, p]))
    this.loaded = true
    this.emit()
  }

  upsert(p: Page) {
    if (p.kind === 'row') return
    const old = this.pages.get(p.id)
    // 标题正在本机改着：服务器的事件可能落后一拍，别把输入框里的字盖回去
    const pending = this.localTitles.get(p.id)
    this.pages.set(p.id, pending !== undefined ? { ...old, ...p, title: pending } : { ...old, ...p })
    this.emit()
  }

  remove(ids: string[]) {
    for (const id of ids) this.pages.delete(id)
    this.emit()
  }

  // 子孙也一起去掉（删除时服务器连子页一起进回收站）
  removeTree(id: string) {
    const gone = [id, ...this.descendants(id)]
    this.remove(gone)
    return gone
  }

  descendants(id: string): string[] {
    const out: string[] = []
    const walk = (pid: string) => {
      for (const p of this.pages.values())
        if (p.parent_id === pid) {
          out.push(p.id)
          walk(p.id)
        }
    }
    walk(id)
    return out
  }

  private localTitles = new Map<string, string>()
  private titleTimers = new Map<string, number>()
  // 标题在编辑器里改（写进 Y.Text，服务器保存时同步到 pages.title），侧栏先跟着变
  setLocalTitle(id: string, title: string) {
    const p = this.pages.get(id)
    if (!p || p.title === title) return
    this.localTitles.set(id, title)
    clearTimeout(this.titleTimers.get(id))
    this.titleTimers.set(
      id,
      window.setTimeout(() => this.localTitles.delete(id), 3000),
    )
    this.pages.set(id, { ...p, title })
    this.emit()
  }

  connect() {
    if (this.es) return
    const onPage = (data: unknown) => {
      if (data && typeof data === 'object') this.upsert(data as Page)
    }
    const handle = (type: string, data: unknown) => {
      if (type === 'page.created' || type === 'page.updated' || type === 'page.moved') onPage(data)
      else if (type === 'page.deleted') {
        if (data && typeof data === 'object') this.removeTree((data as Page).id)
      } else if (type === 'resync') {
        // 服务器说落下了事件：重新拉一遍
        this.load().catch(() => {})
        this.bus.get('resync')?.forEach((f) => f(null))
      } else if (BUS_EVENTS.includes(type)) {
        // 评论和通知：交给订阅的人（评论栏、收件箱）
        this.bus.get(type)?.forEach((f) => f(data))
      }
    }
    // 断线重连了：可能落下了事件，重新拉一遍
    let opened = false
    const onOpen = () => {
      if (opened) {
        this.load().catch(() => {})
        this.bus.get('resync')?.forEach((f) => f(null))
      }
      opened = true
    }
    if (EMBED_AUTH) {
      // 跨域嵌入：EventSource 带不了 Authorization，用 fetch 读的那个（每次重连都带最新的 token）
      const h = api.events(this.ws)
      h.on('*', (data, name) => handle(name, data))
      h.onStatus((up) => up && onOpen())
      this.es = { close: () => h.close() }
      return
    }
    const es = new EventSource(eventsUrl(this.ws), { withCredentials: true })
    const parse = (e: Event) => {
      try {
        return JSON.parse((e as MessageEvent).data)
      } catch {
        return null
      }
    }
    for (const type of ['page.created', 'page.updated', 'page.moved', 'page.deleted', 'resync', ...BUS_EVENTS]) es.addEventListener(type, (e) => handle(type, parse(e)))
    es.addEventListener('open', onOpen)
    this.es = es
  }

  private bus = new Map<string, Set<(data: any) => void>>()
  /** 订阅工作区 SSE 里的评论 / 通知事件；'resync' = 可能落下了，自己重新拉 */
  on(type: string, f: (data: any) => void) {
    let set = this.bus.get(type)
    if (!set) this.bus.set(type, (set = new Set()))
    set.add(f)
    return () => {
      set!.delete(f)
    }
  }

  close() {
    this.es?.close()
    this.es = null
  }
}

export function usePages(store: PageStore) {
  return useSyncExternalStore(store.subscribe, store.getSnapshot)
}

export interface TreeNode {
  page: Page
  children: TreeNode[]
}

export function buildTree(pages: Page[]): TreeNode[] {
  const byParent = new Map<string | null, Page[]>()
  const ids = new Set(pages.map((p) => p.id))
  for (const p of pages) {
    // 父页面不在列表里（比如在回收站）就当顶层
    const parent = p.parent_id && ids.has(p.parent_id) ? p.parent_id : null
    const arr = byParent.get(parent)
    if (arr) arr.push(p)
    else byParent.set(parent, [p])
  }
  const make = (pid: string | null): TreeNode[] =>
    (byParent.get(pid) ?? [])
      .sort((a, b) => (a.position < b.position ? -1 : a.position > b.position ? 1 : a.id < b.id ? -1 : 1))
      .map((page) => ({ page, children: make(page.id) }))
  return make(null)
}

/** 新建一页或一个库，放进侧栏，打开它 */
export async function createAndOpen(store: PageStore, kind: 'page' | 'database' = 'page', parent: string | null = null) {
  // 建库返回的是 { page, schema, views, seq }
  const page: Page =
    kind === 'database'
      ? ({ ...(await api.createDatabase(store.ws, { title: '', ...(parent ? { parent_id: parent } : {}) })).page, kind: 'database' } as Page)
      : await api.createPage(store.ws, parent ? { parent_id: parent } : {})
  store.upsert(page)
  navigate(pagePath(page.id))
  return page
}
