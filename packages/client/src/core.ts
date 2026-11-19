// 薄封装：每个 HTTP 接口一个方法，返回解析好的 JSON。浏览器（cookie 或 token）和 Node 18+（token）都能用。
import type * as T from './types.ts'
import type { AnchorIn, Agent, AuthMode, Comment, DbHistoryItem, DbMeta, InboxItem, Invite, InviteInfo, Me, Page, Person, Row, Schema, Scope, SearchHit, Share, Thread, TrashItem, Version, VersionDiff, View } from './types.ts'
import type { DbStore } from './db/store.ts'
import { openEvents, type EventsHandle } from './events.ts'
import type { OpenPageOptions, PageConn } from './page.ts'

// 协作文档（要 Yjs）和本地数据库（要 WASM 引擎）放在另外两个模块里，用到才加载：
// import '@oad/client'（全部）或者 '@oad/client/page'、'@oad/client/db' 会把它们登记到这里。
export type OpenPageFn = (id: string, o: OpenPageOptions & { url: string; token?: string; WebSocket?: unknown }) => PageConn
export type DbFactory = (client: OadClient, id: string) => DbStore
export interface ClientPlugins {
  openPage?: OpenPageFn
  database?: DbFactory
}
const globalPlugins: ClientPlugins = {}
/** 全局登记（所有客户端都能用）。打包工具可能把只为副作用引入的模块摇掉，所以要显式调一次：
 *    import { openPage } from '@oad/client/page';  registerPlugin('openPage', openPage) */
export function registerPlugin<K extends keyof ClientPlugins>(k: K, f: NonNullable<ClientPlugins[K]>) {
  globalPlugins[k] = f
}

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

export interface ClientOptions {
  /** 服务器地址，比如 https://docs.example.com；空字符串是同源（网页里） */
  baseUrl?: string
  /** Agent / 服务端用的 token（Authorization: Bearer）。不给就靠 cookie（浏览器里登录过） */
  token?: string | (() => string | null | undefined)
  /** 'include' | 'same-origin' | 'omit'：浏览器里带不带 cookie。默认 baseUrl 为空时 same-origin，否则 include */
  credentials?: RequestCredentials
  /** 换掉 fetch（测试、代理） */
  fetch?: typeof fetch
  /** 服务器给人看的错误信息用哪种语言 */
  locale?: string | (() => string)
  /** 每个请求多带的头（比如 header 登录时宿主自己转发） */
  headers?: Record<string, string>
  /** Node 18–21 没有全局 WebSocket：传一个进来（比如 ws 包） */
  WebSocket?: unknown
}

export type Uploaded = { id?: string; url: string; name: string; mime: string; size: number; width?: number; height?: number; blurhash?: string }

const val = <V>(v: V | (() => V)) => (typeof v === 'function' ? (v as () => V)() : v)

export function createClient(opts: ClientOptions = {}, extra: ClientPlugins = {}) {
  const plugins = new Proxy({} as ClientPlugins, { get: (_, k: keyof ClientPlugins) => extra[k] ?? globalPlugins[k] })
  const base = (opts.baseUrl ?? '').replace(/\/$/, '')
  const API = base + '/api/v1'
  const f = opts.fetch ?? ((...a: Parameters<typeof fetch>) => fetch(...a))
  const credentials = opts.credentials ?? (base ? 'include' : 'same-origin')
  const headers = (json: boolean): Record<string, string> => {
    const h: Record<string, string> = { ...(opts.headers ?? {}) }
    const tok = opts.token ? val(opts.token) : null
    if (tok) h.authorization = 'Bearer ' + tok
    const loc = opts.locale ? val(opts.locale) : null
    if (loc) h['accept-language'] = loc === 'en' ? 'en' : loc === 'zh' ? 'zh-CN' : loc
    if (json) h['content-type'] = 'application/json'
    return h
  }

  async function req<R>(method: string, path: string, body?: unknown): Promise<R> {
    const r = await f(API + path, { method, credentials, headers: headers(body !== undefined), body: body === undefined ? undefined : JSON.stringify(body) })
    if (!r.ok) {
      let code = 'http_' + r.status
      let message = r.statusText
      try {
        const j = await r.json()
        code = j?.error?.code ?? code
        message = j?.error?.message ?? message
      } catch {
        /* 不是 JSON */
      }
      throw new ApiError(r.status, code, message)
    }
    if (r.status === 204) return undefined as R
    const text = await r.text()
    return (text ? JSON.parse(text) : undefined) as R
  }

  const dbs = new Map<string, DbStore>()

  const client = {
    baseUrl: base,
    apiUrl: API,
    /** 发一个任意的请求（还没包成方法的接口） */
    request: req,

  authConfig: () => req<{ mode: AuthMode }>('GET', '/auth/config'),
  login: (email: string, password: string) => req<Me>('POST', '/auth/login', { email, password }),
  logout: () => req<void>('POST', '/auth/logout'),
  me: () => req<Me>('GET', '/me'),
  pages: (ws: string) => req<{ pages: Page[] }>('GET', `/workspaces/${ws}/pages`).then((r) => r.pages),
  createPage: (ws: string, b: { parent_id?: string | null; after?: string; title?: string; icon?: string }) =>
    req<Page>('POST', `/workspaces/${ws}/pages`, b),
  page: (id: string) => req<Page>('GET', `/pages/${id}`),
  // 移动：after 不给 = 放最后，after: null = 放最前
  patchPage: (id: string, b: { title?: string; icon?: string | null; parent_id?: string | null; after?: string | null }) =>
    req<Page>('PATCH', `/pages/${id}`, b),
  deletePage: (id: string) => req<unknown>('DELETE', `/pages/${id}`),

  // 第 2 阶段：作者、评论、Agent、通知（docs/phase2-contract.md）
  authors: (id: string) => req<{ clients: Record<string, Person> }>('GET', `/pages/${id}/authors`),
  threads: (page: string, resolved: 'all' | 'true' | 'false' = 'all') =>
    req<{ threads: Thread[] }>('GET', `/pages/${page}/comments?resolved=${resolved}`).then((r) => r.threads),
  createThread: (page: string, b: { anchor?: AnchorIn | null; body: string; notify?: false | string[] }) =>
    req<Thread>('POST', `/pages/${page}/comments`, b),
  reply: (thread: string, b: { body: string; notify?: false | string[] }) => req<Comment>('POST', `/threads/${thread}/comments`, b),
  editComment: (id: string, body: string) => req<Comment>('PATCH', `/comments/${id}`, { body }),
  deleteComment: (id: string) => req<unknown>('DELETE', `/comments/${id}`),
  resolveThread: (id: string, resolved: boolean) => req<Thread>('PATCH', `/threads/${id}`, { resolved }),

  agents: (ws: string) => req<{ agents: Agent[] }>('GET', `/workspaces/${ws}/agents`).then((r) => r.agents),
  createAgent: (ws: string, b: { name: string; about?: string; scope: Scope; webhook?: string | null }) =>
    req<{ agent: Agent; token: string; webhook_secret: string }>('POST', `/workspaces/${ws}/agents`, b),
  patchAgent: (id: string, b: { name?: string; about?: string; scope?: Scope; webhook?: string | null }) => req<Agent>('PATCH', `/agents/${id}`, b),
  revokeAgent: (id: string) => req<unknown>('DELETE', `/agents/${id}`),
  reissueToken: (id: string) => req<{ token: string }>('POST', `/agents/${id}/token`),
  reissueWebhookSecret: (id: string) => req<{ webhook_secret: string }>('POST', `/agents/${id}/webhook-secret`),

  members: (ws: string) =>
    req<{ members: { role: string; user: { id: string; name: string; email?: string | null } }[] }>('GET', `/workspaces/${ws}/members`).then((r) => r.members ?? []),
  inbox: (after?: number | string, limit = 50) =>
    req<{ notifications: InboxItem[] } | InboxItem[]>('GET', `/me/notifications?limit=${limit}${after !== undefined ? `&after=${after}` : ''}`).then((r) =>
      (Array.isArray(r) ? r : r.notifications).map(flatInbox),
    ),
  // 第 4 阶段：数据库（docs/phase4-contract.md）
  createDatabase: (ws: string, b: { title: string; parent_id?: string | null; after?: string; schema?: unknown; views?: unknown }) =>
    req<{ page?: Page; id?: string } & Record<string, unknown>>('POST', `/workspaces/${ws}/databases`, b),
  getDatabase: (id: string) => req<DbMeta>('GET', `/databases/${id}`),
  dbRows: (id: string, since?: number) =>
    req<{ seq: number; rows: Row[]; deleted?: string[] }>('GET', `/databases/${id}/rows${since !== undefined ? `?since=${since}` : ''}`),
  addRows: (id: string, rows: { props: Record<string, unknown>; after?: string | null }[]) =>
    req<{ ids: string[]; seq: number; rows?: Row[] }>('POST', `/databases/${id}/rows`, { rows }),
  patchRows: (id: string, ops: { row_id: string; set?: Record<string, unknown>; after?: string | null }[]) => req<{ seq: number }>('PATCH', `/databases/${id}/rows`, { ops }),
  deleteRows: (id: string, ids: string[]) => req<{ seq?: number }>('DELETE', `/databases/${id}/rows`, { ids }),
  patchSchema: (id: string, ops: Record<string, unknown>[]) => req<{ schema: Schema; seq: number }>('PATCH', `/databases/${id}/schema`, { ops }),
  undoSchema: (id: string, seq: number) => req<{ schema: Schema; seq: number }>('POST', `/databases/${id}/schema/undo`, { seq }),
  putViews: (id: string, views: View[]) => req<unknown>('PUT', `/databases/${id}/views`, { views }),
  addView: (id: string, view: Partial<View>) => req<View>('POST', `/databases/${id}/views`, view),
  patchView: (id: string, view: string, b: Partial<View>) => req<View>('PATCH', `/databases/${id}/views/${view}`, b),
  deleteView: (id: string, view: string) => req<unknown>('DELETE', `/databases/${id}/views/${view}`),
  pressButton: (id: string, row: string, key: string) => req<unknown>('POST', `/databases/${id}/button/${row}/${key}`),
  markRead: (b: { ids: (number | string)[] } | { all: true }) => req<unknown>('POST', '/me/notifications/read', b),

  // 第 5 阶段（docs/phase5-contract.md）
  search: (ws: string, q: string, limit = 20, kind: 'page' | 'row' | 'all' = 'all') =>
    req<{ results: SearchHit[] }>('GET', `/workspaces/${ws}/search?q=${encodeURIComponent(q)}&limit=${limit}&kind=${kind}`).then((r) => r.results ?? []),
  versions: (page: string) => req<{ versions: Version[] } | Version[]>('GET', `/pages/${page}/versions`).then((r) => (Array.isArray(r) ? r : r.versions ?? [])),
  version: (page: string, v: string) => req<{ doc: unknown; markdown: string }>('GET', `/pages/${page}/versions/${v}`),
  versionDiff: (page: string, v: string, to = 'current') => req<VersionDiff>('GET', `/pages/${page}/versions/${v}/diff?to=${to}`),
  restoreVersion: (page: string, v: string) => req<unknown>('POST', `/pages/${page}/versions/${v}/restore`),
  saveVersion: (page: string, label: string) => req<Version>('POST', `/pages/${page}/versions`, { label }),
  dbHistory: (db: string, q: { row?: string; prop?: string } = {}) =>
    req<{ items: DbHistoryItem[] } | DbHistoryItem[]>('GET', `/databases/${db}/history${q.row || q.prop ? '?' + new URLSearchParams(Object.entries(q).filter(([, v]) => v) as [string, string][]) : ''}`).then((r) =>
      Array.isArray(r) ? r : r.items ?? [],
    ),
  dbUndo: (db: string, seq: number) => req<unknown>('POST', `/databases/${db}/history/undo`, { seq }),
  trash: (ws: string) => req<{ items: TrashItem[] } | TrashItem[]>('GET', `/workspaces/${ws}/trash`).then((r) => (Array.isArray(r) ? r : r.items ?? [])),
  restoreTrash: (id: string) => req<{ page?: Page } | Page>('POST', `/trash/${id}/restore`),
  purgeTrash: (id: string) => req<unknown>('DELETE', `/trash/${id}`),
  getShare: (page: string) =>
    req<Share>('GET', `/pages/${page}/share`).catch((e) => {
      if (e instanceof ApiError && e.status === 404) return null
      throw e
    }),
  createShare: (page: string, b: { include_children?: boolean; indexable?: boolean }) => req<Share>('POST', `/pages/${page}/share`, b),
  revokeShare: (page: string) => req<unknown>('DELETE', `/pages/${page}/share`),
  invites: (ws: string) => req<{ invites: Invite[] } | Invite[]>('GET', `/workspaces/${ws}/invites`).then((r) => (Array.isArray(r) ? r : r.invites ?? [])),
  createInvite: (ws: string, b: { role: string; expires_in?: number | null }) => req<Invite>('POST', `/workspaces/${ws}/invites`, b),
  deleteInvite: (ws: string, token: string) => req<unknown>('DELETE', `/workspaces/${ws}/invites/${token}`),
  inviteInfo: (token: string) => req<InviteInfo>('GET', `/invites/${token}`),
  acceptInvite: (token: string) => req<Me>('POST', `/invites/${token}/accept`),
  signup: (b: { email: string; password: string; name: string; invite?: string }) => req<Me>('POST', '/auth/signup', b),
  patchMember: (ws: string, user: string, role: string) => req<unknown>('PATCH', `/workspaces/${ws}/members/${user}`, { role }),
  removeMember: (ws: string, user: string) => req<unknown>('DELETE', `/workspaces/${ws}/members/${user}`),
  patchWorkspace: (ws: string, b: { name?: string; icon?: string | null }) => req<{ id: string; name: string; icon?: string | null }>('PATCH', `/workspaces/${ws}`, b),
  patchMe: (b: { name?: string; avatar?: string | null; locale?: string | null }) => req<Me | Me['user']>('PATCH', '/me', b),
  changePassword: (current: string, next: string) => req<unknown>('POST', '/me/password', { current, new: next }),
    /** 上传文件（浏览器里有进度） */
    upload(ws: string, file: Blob & { name?: string }, onProgress?: (f: number) => void): Promise<Uploaded> {
      const url = `${API}/workspaces/${ws}/files`
      if (typeof XMLHttpRequest !== 'undefined' && onProgress) {
        return new Promise((resolve, reject) => {
          const xhr = new XMLHttpRequest()
          xhr.open('POST', url)
          xhr.withCredentials = credentials !== 'omit'
          for (const [k, v] of Object.entries(headers(false))) xhr.setRequestHeader(k, v)
          xhr.upload.onprogress = (e) => e.lengthComputable && onProgress(e.loaded / e.total)
          xhr.onload = () => {
            if (xhr.status >= 200 && xhr.status < 300) {
              try {
                resolve(JSON.parse(xhr.responseText))
              } catch (e) {
                reject(e)
              }
            } else reject(new ApiError(xhr.status, 'http_' + xhr.status, xhr.statusText))
          }
          xhr.onerror = () => reject(new Error('network'))
          const fd = new FormData()
          fd.append('file', file, file.name || 'file')
          xhr.send(fd)
        })
      }
      const fd = new FormData()
      fd.append('file', file, file.name || 'file')
      return f(url, { method: 'POST', credentials, headers: headers(false), body: fd }).then(async (r) => {
        if (!r.ok) throw new ApiError(r.status, 'http_' + r.status, r.statusText)
        return r.json()
      })
    },
    /** 链接解析（卡片用）；失败返回 null */
    unfurl: (url: string) => req<unknown>('GET', `/unfurl?url=${encodeURIComponent(url)}`).catch(() => null),
    /** 工作区的实时事件（SSE，断了自动重连，带 Last-Event-ID） */
    events: (ws: string): EventsHandle => openEvents(`${API}/workspaces/${ws}/events`, { credentials, headers: () => headers(false), fetch: f }),
    /** 一页的协作文档：Y.Doc + WebSocket 同步 + 浏览器里的 IndexedDB 缓存 */
    openPage: (id: string, o: OpenPageOptions = {}): PageConn => {
      if (!plugins.openPage) throw new Error("@oad/client：openPage 要先 import '@oad/client/page'（或者从 '@oad/client' 整个引入）")
      return plugins.openPage(id, {
        url: syncUrl(),
        token: opts.token ? val(opts.token) ?? undefined : undefined,
        WebSocket: opts.WebSocket,
        ...o,
      })
    },
    /** 一个数据库：本机缓存 + 增量同步 + WASM 引擎算筛选排序分组；同一个 id 拿到的是同一个对象 */
    database(id: string): DbStore {
      let s = dbs.get(id)
      if (!s) {
        if (!plugins.database) throw new Error("@oad/client：database 要先 import '@oad/client/db'（或者从 '@oad/client' 整个引入）")
        dbs.set(id, (s = plugins.database(client, id)))
      }
      return s
    },
  }

  function syncUrl() {
    if (base) return base.replace(/^http/, 'ws') + '/api/v1/sync'
    const loc = typeof location !== 'undefined' ? location : null
    return loc ? `${loc.protocol === 'https:' ? 'wss' : 'ws'}://${loc.host}/api/v1/sync` : 'ws://127.0.0.1:8848/api/v1/sync'
  }

  return client
}

export type OadClient = ReturnType<typeof createClient>
export * from './types.ts'
export { openEvents, type EventsHandle } from './events.ts'
export type { OpenPageOptions, PageConn, PageStatus } from './page.ts'
export type { DbStore, EventSource_ as EventsLike, Status as DbStatus } from './db/store.ts'
import type { Cond, Filter } from './types.ts'
/** 筛选条件是不是一组（嵌套的 and / or） */
export const isGroup = (x: Cond | Filter): x is Filter => (x as Filter).items !== undefined
export type { T as Types }

/** 收件箱一条：data 里的字段摊平到外面 */
export const flatInbox = (n: InboxItem): InboxItem => (n.data ? { ...n.data, ...n, data: undefined } : n)


/** 评论串的锚点，两种字段写法都认 */
export function threadAnchor(t: Thread) {
  const start = t.anchor?.start ?? t.anchor_start ?? null
  const end = t.anchor?.end ?? t.anchor_end ?? null
  return {
    block_id: t.anchor?.block_id ?? t.block_id ?? null,
    start,
    end,
    quote: t.anchor?.quote ?? t.quote ?? '',
  }
}


