// 薄封装：每个 HTTP 接口一个方法，返回解析好的 JSON。浏览器（cookie 或 token）和 Node 18+（token）都能用。
// 路径、请求体、响应的类型都从 docs/openapi.json 生成的 openapi.ts 来（call() 按 paths 检查），文档和这里对不上时编译就报错。
import type { paths } from './openapi.ts'
import type * as T from './types.ts'
import type {
  AgentNotification,
  AuthConfig,
  Cond,
  DbHistory,
  DbHistoryItem,
  DbMeta,
  Filter,
  InboxItem,
  InboxNotification,
  InviteInfo,
  Schema,
  Thread,
  TrashItem,
  TrashList,
  Version,
  View,
} from './types.ts'
import type { DbStore } from './db/store.ts'
import { ApiConnectionError, ApiError, ApiTimeoutError, errorFrom, isApiError, parseRetryAfter } from './errors.ts'
import { openEvents, type EventsHandle } from './events.ts'
import { VERSION } from './version.ts'
import type { OpenPageOptions, PageConn } from './page.ts'

// 协作文档（要 Yjs）和本地数据库（要 WASM 引擎）放在另外两个模块里，用到才加载：
// import '@open-agent-docs/client'（全部）或者 '@open-agent-docs/client/page'、'@open-agent-docs/client/db' 会把它们登记到这里。
export type OpenPageFn = (id: string, o: OpenPageOptions & { url: string; token?: string; WebSocket?: unknown }) => PageConn
export type DbFactory = (client: OadClient, id: string) => DbStore
export interface ClientPlugins {
  openPage?: OpenPageFn
  database?: DbFactory
}
const globalPlugins: ClientPlugins = {}
/** 全局登记（所有客户端都能用）。打包工具可能把只为副作用引入的模块摇掉，所以要显式调一次：
 *    import { openPage } from '@open-agent-docs/client/page';  registerPlugin('openPage', openPage) */
export function registerPlugin<K extends keyof ClientPlugins>(k: K, f: NonNullable<ClientPlugins[K]>) {
  globalPlugins[k] = f
}

export interface ClientOptions {
  /** 服务器地址，比如 https://docs.example.com；空字符串是同源（网页里） */
  baseUrl?: string
  /** Agent / 服务端用的 token（Authorization: Bearer），也可以是 JWT。不给就靠 cookie（浏览器里登录过） */
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
  /** 失败了最多重试几次（默认 2）：429；502 / 503 / 504（POST、PATCH 除外）；连不上（GET、PUT、DELETE，或者确定没发出去） */
  maxRetries?: number
  /** 连不上（请求没拿到响应）时要不要重试，默认要。网页里设 false：断网了马上报错，不白等一秒多；429、503 这些照样重试 */
  retryConnectionErrors?: boolean
  /** 单个请求的超时（毫秒，默认 60 秒；0 是不限）。事件流只限连接，不限总时长 */
  timeout?: number
}

/** 正文：format 是 'html' 时返回 { html, version }（服务器 0.3 起有） */
export interface HtmlContent {
  html: string
  version: string
  total?: number | null
  /** html 里图片、文件地址的签名什么时候过期（毫秒时间戳）；没有签名地址时是 null。缓存这段 HTML 别超过它 */
  assets_expire_at?: number | null
}

/** 嵌入网页的地址：<base>/p/<id>?embed=1[&theme=][&lang=][#token=]。token 放在 # 后面，不进服务器日志和 Referer */
export function embedUrl(baseUrl: string, pageId: string, o: { theme?: 'light' | 'dark' | string; lang?: string; token?: string } = {}): string {
  let u = baseUrl.replace(/\/+$/, '') + '/p/' + encodeURIComponent(pageId) + '?embed=1'
  if (o.theme) u += '&theme=' + encodeURIComponent(o.theme)
  if (o.lang) u += '&lang=' + encodeURIComponent(o.lang)
  if (o.token) u += '#token=' + o.token
  return u
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
/** 第 n 次重试前等多久（毫秒）：Retry-After 照它（最多 60 秒）；没有就 0.5 秒 × 2^n，最多 8 秒，加 0–25% 的随机 */
export function retryDelay(n: number, retryAfter?: number): number {
  if (retryAfter !== undefined) return Math.min(60, retryAfter) * 1000
  return Math.min(8000, 500 * 2 ** n) * (1 + Math.random() * 0.25)
}
// 这些错误码说明连接根本没建起来（请求没发出去），POST 也能放心重发
const NOT_SENT = new Set(['ECONNREFUSED', 'ENOTFOUND', 'EAI_AGAIN', 'UND_ERR_CONNECT_TIMEOUT', 'UND_ERR_CONNECT'])
const notSent = (e: unknown) => {
  const c = (e as { cause?: { code?: string } })?.cause
  return !!c && NOT_SENT.has(c.code ?? '')
}
// 不直接写 process：浏览器项目（没有 @types/node）也要能编译这个文件
const IS_NODE = (() => {
  const g = globalThis as { process?: { versions?: { node?: string } }; window?: unknown }
  return !!g.process?.versions?.node && typeof g.window === 'undefined'
})()


// ---------- 从 paths 推出每个操作的类型 ----------
type Method = 'get' | 'post' | 'put' | 'patch' | 'delete'
/** 这个路径有哪些方法 */
export type MethodOf<P extends keyof paths> = { [K in Method]: [NonNullable<paths[P][K]>] extends [never] ? never : K }[Method]
type Op<P extends keyof paths, M extends Method> = NonNullable<paths[P][M]>
type Json<X> = X extends { content: { 'application/json': infer J } } ? J : undefined
/** 成功时的响应体（200 / 201；204 是 undefined） */
export type ResponseOf<P extends keyof paths, M extends MethodOf<P>> =
  Op<P, M> extends { responses: infer R }
    ? R extends { 200: infer A }
      ? Json<A>
      : R extends { 201: infer B }
        ? Json<B>
        : undefined
    : never
/** 请求体（JSON） */
export type BodyOf<P extends keyof paths, M extends MethodOf<P>> = Op<P, M> extends { requestBody?: infer B } ? ([NonNullable<B>] extends [never] ? never : Json<NonNullable<B>>) : never
/** 查询参数 */
export type QueryOf<P extends keyof paths, M extends MethodOf<P>> = Op<P, M> extends { parameters: { query?: infer Q } } ? NonNullable<Q> : never
type PathParams<P extends string> = P extends `${string}{${infer K}}${infer Rest}` ? Record<K | keyof PathParams<Rest>, string | number> : Record<never, never>

/** 一页的结果：带 next_cursor 的那几个列表（版本、库的历史、回收站、Agent 通知、收件箱） */
export interface PageOf<I> {
  items: I[]
  next_cursor?: string | null
}
/** 按 next_cursor 一页一页往下取，逐条给出来。break 出循环就不再取了。 */
export async function* paginate<I>(fetchPage: (cursor: string | undefined) => Promise<PageOf<I>>): AsyncGenerator<I, void, undefined> {
  let cursor: string | undefined
  for (;;) {
    const p = await fetchPage(cursor)
    for (const x of p.items) yield x
    // 没有下一页、这页是空的、或者游标没往前走（服务器的毛病）：停
    if (!p.next_cursor || !p.items.length || p.next_cursor === cursor) return
    cursor = p.next_cursor
  }
}

/** 文档里只写了 object 的（表结构、视图）：按手写的类型看 */
const loose = <X>(p: Promise<unknown>) => p as Promise<X>

const val = <V>(v: V | (() => V)) => (typeof v === 'function' ? (v as () => V)() : v)

export function createClient(opts: ClientOptions = {}, extra: ClientPlugins = {}) {
  const plugins = new Proxy({} as ClientPlugins, { get: (_, k: keyof ClientPlugins) => extra[k] ?? globalPlugins[k] })
  const base = (opts.baseUrl ?? '').replace(/\/$/, '')
  const API = base + '/api/v1'
  const f = opts.fetch ?? ((...a: Parameters<typeof fetch>) => fetch(...a))
  const credentials = opts.credentials ?? (base ? 'include' : 'same-origin')
  const maxRetries = opts.maxRetries ?? 2
  const timeout = opts.timeout ?? 60_000
  const headers = (json: boolean): Record<string, string> => {
    // 浏览器不让改 User-Agent，只在 Node 里带
    const h: Record<string, string> = IS_NODE ? { 'user-agent': 'open-agent-docs-js/' + VERSION } : {}
    Object.assign(h, opts.headers ?? {})
    const tok = opts.token ? val(opts.token) : null
    if (tok) h.authorization = 'Bearer ' + tok
    const loc = opts.locale ? val(opts.locale) : null
    if (loc) h['accept-language'] = loc === 'en' ? 'en' : loc === 'zh' ? 'zh-CN' : loc
    if (json) h['content-type'] = 'application/json'
    return h
  }

  /** 发请求，带超时和重试；read 在超时的范围里读响应（读完才算这个请求结束） */
  async function send<R>(url: string, init: RequestInit, read: (r: Response) => Promise<R>): Promise<R> {
    const method = (init.method ?? 'GET').toUpperCase()
    const idempotent = method === 'GET' || method === 'HEAD' || method === 'PUT' || method === 'DELETE'
    for (let n = 0; ; n++) {
      const ctrl = new AbortController()
      let timedOut = false
      const t = timeout > 0 ? setTimeout(() => ((timedOut = true), ctrl.abort()), timeout) : null
      try {
        let r: Response
        try {
          r = await f(url, { ...init, credentials, signal: ctrl.signal })
        } catch (e) {
          if (timedOut) throw new ApiTimeoutError(`${method} ${url}：${timeout} 毫秒没有响应`)
          if (n < maxRetries && opts.retryConnectionErrors !== false && (idempotent || notSent(e))) {
            await sleep(retryDelay(n))
            continue
          }
          throw new ApiConnectionError(`${method} ${url}：连不上（${(e as Error)?.message ?? e}）`, { cause: e })
        }
        if (!r.ok) {
          const retryable = r.status === 429 || ((r.status === 502 || r.status === 503 || r.status === 504) && method !== 'POST' && method !== 'PATCH')
          if (retryable && n < maxRetries) {
            const wait = retryDelay(n, parseRetryAfter(r.headers.get('retry-after')))
            await r.body?.cancel().catch(() => {})
            if (t) clearTimeout(t)
            await sleep(wait)
            continue
          }
          throw await errorFrom(r)
        }
        try {
          return await read(r)
        } catch (e) {
          if (timedOut) throw new ApiTimeoutError(`${method} ${url}：${timeout} 毫秒没有读完`)
          throw e
        }
      } finally {
        if (t) clearTimeout(t)
      }
    }
  }

  const readJson = async <R>(r: Response): Promise<R> => {
    if (r.status === 204) return undefined as R
    const text = await r.text()
    return (text ? JSON.parse(text) : undefined) as R
  }

  /** 发一个任意的请求（路径不在文档里、或者想自己拼时用） */
  function req<R>(method: string, path: string, body?: unknown): Promise<R> {
    return send(API + path, { method, headers: headers(body !== undefined), body: body === undefined ? undefined : JSON.stringify(body) }, readJson<R>)
  }

  /** 拿二进制（文件、导出的 CSV） */
  const blob = (path: string, q?: Record<string, string | number | undefined>) => {
    const qs = q ? Object.entries(q).filter(([, v]) => v !== undefined) : []
    return send(API + path + (qs.length ? '?' + new URLSearchParams(qs.map(([k, v]) => [k, String(v)])) : ''), { headers: headers(false) }, (r) => r.blob())
  }
  const enc = encodeURIComponent
  let defaultWs: Promise<string> | null = null

  /** 按文档调一个接口：路径模板、方法、参数、请求体、响应都有类型 */
  function call<P extends keyof paths & string, M extends MethodOf<P>>(
    method: M,
    path: P,
    o: { path?: PathParams<P>; query?: QueryOf<P, M> | Record<string, string | number | boolean | undefined>; body?: BodyOf<P, M> } = {},
  ): Promise<ResponseOf<P, M>> {
    let url: string = path.replace(/\{(\w+)\}/g, (_, k) => encodeURIComponent(String((o.path as Record<string, unknown>)?.[k] ?? '')))
    if (o.query) {
      const q = Object.entries(o.query as Record<string, unknown>).filter(([, v]) => v !== undefined && v !== null)
      if (q.length) url += '?' + new URLSearchParams(q.map(([k, v]) => [k, String(v)]))
    }
    return req<ResponseOf<P, M>>(method.toUpperCase(), url, o.body)
  }

  const dbs = new Map<string, DbStore>()

  function contentFn(id: string, format: 'html', q?: { limit?: number }): Promise<HtmlContent>
  function contentFn(id: string, format?: 'markdown' | 'json', q?: { limit?: number }): Promise<ResponseOf<'/pages/{id}/content', 'get'>>
  function contentFn(id: string, format: 'markdown' | 'json' | 'html' = 'markdown', q: { limit?: number } = {}) {
    return call('get', '/pages/{id}/content', { path: { id }, query: { format, ...q } })
  }

  const client = {
    baseUrl: base,
    apiUrl: API,
    request: req,
    call,

    // ---------- 登录、自己 ----------
    authConfig: () => call('get', '/auth/config') as Promise<AuthConfig>,
    login: (email: string, password: string) => call('post', '/auth/login', { body: { email, password } }),
    logout: () => call('post', '/auth/logout'),
    signup: (b: BodyOf<'/auth/signup', 'post'>) => call('post', '/auth/signup', { body: b }),
    me: () => call('get', '/me'),
    /** /me 里第一个工作区的 id（取一次就记住） */
    defaultWorkspace: (): Promise<string> =>
      (defaultWs ??= call('get', '/me').then(
        (m) => {
          const id = m.workspaces[0]?.id
          if (!id) throw new Error('这个用户还没有工作区')
          return id
        },
        (e) => {
          defaultWs = null
          throw e
        },
      )),
    patchMe: (b: BodyOf<'/me', 'patch'>) => call('patch', '/me', { body: b }),
    changePassword: (current: string, next: string) => call('post', '/me/password', { body: { current, new: next } }),

    // ---------- 页面 ----------
    pages: (ws: string) => call('get', '/workspaces/{ws}/pages', { path: { ws } }).then((r) => r.pages),
    createPage: (ws: string, b: BodyOf<'/workspaces/{ws}/pages', 'post'>) => call('post', '/workspaces/{ws}/pages', { path: { ws }, body: b }),
    page: (id: string) => call('get', '/pages/{id}', { path: { id } }),
    // 移动：after 不给 = 放最后，after: null = 放最前
    patchPage: (id: string, b: BodyOf<'/pages/{id}', 'patch'>) => call('patch', '/pages/{id}', { path: { id }, body: b }),
    deletePage: (id: string) => call('delete', '/pages/{id}', { path: { id } }),
    /** 正文：Markdown（默认）、文档 JSON 或只读 HTML（{ html, version }），带 version（改的时候当 base）。limit：只要前 N 个顶层块 */
    content: contentFn,
    putContent: (id: string, b: BodyOf<'/pages/{id}/content', 'put'>) => call('put', '/pages/{id}/content', { path: { id }, body: b }),
    block: (id: string, block: string) => call('get', '/pages/{id}/blocks/{block_id}', { path: { id, block_id: block } }),
    putBlock: (id: string, block: string, b: BodyOf<'/pages/{id}/blocks/{block_id}', 'put'>) =>
      call('put', '/pages/{id}/blocks/{block_id}', { path: { id, block_id: block }, body: b }),
    insertBlocks: (id: string, b: BodyOf<'/pages/{id}/blocks', 'post'>) => call('post', '/pages/{id}/blocks', { path: { id }, body: b }),
    deleteBlock: (id: string, block: string) => call('delete', '/pages/{id}/blocks/{block_id}', { path: { id, block_id: block } }),
    authors: (id: string) => call('get', '/pages/{id}/authors', { path: { id } }),

    // ---------- 评论 ----------
    threads: (page: string, resolved: 'all' | 'true' | 'false' = 'all') =>
      call('get', '/pages/{id}/comments', { path: { id: page }, query: { resolved } }).then((r) => r.threads),
    createThread: (page: string, b: BodyOf<'/pages/{id}/comments', 'post'>) => call('post', '/pages/{id}/comments', { path: { id: page }, body: b }),
    reply: (thread: string, b: BodyOf<'/threads/{id}/comments', 'post'>) => call('post', '/threads/{id}/comments', { path: { id: thread }, body: b }),
    editComment: (id: string, body: string) => call('patch', '/comments/{id}', { path: { id }, body: { body } }),
    deleteComment: (id: string) => call('delete', '/comments/{id}', { path: { id } }),
    resolveThread: (id: string, resolved: boolean) => call('patch', '/threads/{id}', { path: { id }, body: { resolved } }),

    // ---------- Agent ----------
    agents: (ws: string) => call('get', '/workspaces/{ws}/agents', { path: { ws } }).then((r) => r.agents),
    createAgent: (ws: string, b: BodyOf<'/workspaces/{ws}/agents', 'post'>) => call('post', '/workspaces/{ws}/agents', { path: { ws }, body: b }),
    patchAgent: (id: string, b: BodyOf<'/agents/{id}', 'patch'>) => call('patch', '/agents/{id}', { path: { id }, body: b }),
    revokeAgent: (id: string) => call('delete', '/agents/{id}', { path: { id } }),
    reissueToken: (id: string) => call('post', '/agents/{id}/token', { path: { id } }),
    reissueWebhookSecret: (id: string) => call('post', '/agents/{id}/webhook-secret', { path: { id } }),
    /** Agent 自己取通知（要 Agent 的 token）。wait：没有就等几秒（长轮询，最多 60） */
    agentNotifications: (q: QueryOf<'/agent/notifications', 'get'> = {}) => call('get', '/agent/notifications', { query: q }),
    ackAgentNotifications: (b: BodyOf<'/agent/notifications/ack', 'post'>) => call('post', '/agent/notifications/ack', { body: b }),
    /** 从 after 往后逐条取完（不等新的） */
    eachAgentNotification: (q: { after?: number; limit?: number } = {}): AsyncGenerator<AgentNotification> =>
      paginate((cursor) => call('get', '/agent/notifications', { query: { ...q, cursor } }).then((r) => ({ items: r.notifications, next_cursor: r.next_cursor }))),
    /** Agent 通知的实时流（SSE，事件名 notification，data 是 AgentNotification）；断了自动重连，从上次收到的接着来 */
    agentEvents: (q: { after?: number } = {}): EventsHandle =>
      openEvents(`${API}/agent/notifications/stream${q.after !== undefined ? '?after=' + q.after : ''}`, {
        credentials,
        headers: () => headers(false),
        fetch: f,
        timeout,
      }),

    // ---------- 成员、邀请、工作区 ----------
    members: (ws: string) => call('get', '/workspaces/{ws}/members', { path: { ws } }).then((r) => r.members),
    patchMember: (ws: string, user: string, role: T.Role) => call('patch', '/workspaces/{ws}/members/{user}', { path: { ws, user }, body: { role } }),
    removeMember: (ws: string, user: string) => call('delete', '/workspaces/{ws}/members/{user}', { path: { ws, user } }),
    patchWorkspace: (ws: string, b: BodyOf<'/workspaces/{ws}', 'patch'>) => call('patch', '/workspaces/{ws}', { path: { ws }, body: b }),
    invites: (ws: string) => call('get', '/workspaces/{ws}/invites', { path: { ws } }).then((r) => r.invites),
    createInvite: (ws: string, b: BodyOf<'/workspaces/{ws}/invites', 'post'>) => call('post', '/workspaces/{ws}/invites', { path: { ws }, body: b }),
    deleteInvite: (ws: string, token: string) => call('delete', '/workspaces/{ws}/invites/{token}', { path: { ws, token } }),
    inviteInfo: (token: string) => call('get', '/invites/{token}', { path: { token } }) as Promise<InviteInfo>,
    acceptInvite: (token: string) => call('post', '/invites/{token}/accept', { path: { token } }),

    // ---------- 收件箱 ----------
    /** 一页（新的在前）；items 是拍平以后的 */
    inbox: (q: QueryOf<'/me/notifications', 'get'> = {}) =>
      call('get', '/me/notifications', { query: { limit: 50, ...q } }).then((r) => ({ ...r, items: r.notifications.map(flatInbox) })),
    eachInbox: (q: { limit?: number } = {}): AsyncGenerator<InboxItem> =>
      paginate((cursor) => call('get', '/me/notifications', { query: { ...q, cursor } }).then((r) => ({ items: r.notifications.map(flatInbox), next_cursor: r.next_cursor }))),
    markRead: (b: BodyOf<'/me/notifications/read', 'post'>) => call('post', '/me/notifications/read', { body: b }),

    // ---------- 数据库 ----------
    createDatabase: (ws: string, b: BodyOf<'/workspaces/{ws}/databases', 'post'>) =>
      loose<DbMeta>(call('post', '/workspaces/{ws}/databases', { path: { ws }, body: b })),
    getDatabase: (id: string) => loose<DbMeta>(call('get', '/databases/{id}', { path: { id } })),
    /** 从 CSV 建一个库（第一行是表头） */
    importCsv: (ws: string, csv: string | Blob, q: { title?: string; parent_id?: string } = {}) => {
      const qs = Object.entries(q).filter(([, v]) => v !== undefined && v !== null) as [string, string][]
      return send(
        `${API}/workspaces/${enc(ws)}/databases/import${qs.length ? '?' + new URLSearchParams(qs) : ''}`,
        { method: 'POST', headers: { ...headers(false), 'content-type': 'text/csv' }, body: csv },
        (r) => readJson<DbMeta>(r),
      )
    },
    /** 导出成 CSV（文字） */
    exportCsv: (id: string) => send(`${API}/databases/${enc(id)}/export.csv`, { headers: headers(false) }, (r) => r.text()),
    dbRows: (id: string, since?: number) => call('get', '/databases/{id}/rows', { path: { id }, query: { since } }),
    addRows: (id: string, rows: BodyOf<'/databases/{id}/rows', 'post'>['rows']) => call('post', '/databases/{id}/rows', { path: { id }, body: { rows } }),
    patchRows: (id: string, ops: BodyOf<'/databases/{id}/rows', 'patch'>['ops']) => call('patch', '/databases/{id}/rows', { path: { id }, body: { ops } }),
    deleteRows: (id: string, ids: string[]) => call('delete', '/databases/{id}/rows', { path: { id }, body: { ids } }),
    deleteRow: (id: string, row: string) => call('delete', '/databases/{id}/rows/{row}', { path: { id, row } }),
    /** 服务器算的查询（筛选、排序、分组、汇总，带 computed） */
    queryDatabase: (id: string, q: BodyOf<'/databases/{id}/query', 'post'>) => call('post', '/databases/{id}/query', { path: { id }, body: q }),
    patchSchema: (id: string, ops: Record<string, unknown>[]) =>
      loose<{ schema: Schema; seq: number; changes: Record<string, unknown>[]; warnings?: string[] }>(
        call('patch', '/databases/{id}/schema', { path: { id }, body: { ops } as BodyOf<'/databases/{id}/schema', 'patch'> }),
      ),
    undoSchema: (id: string, seq: number) =>
      loose<{ schema: Schema; seq: number; views: View[] }>(call('post', '/databases/{id}/schema/undo', { path: { id }, body: { seq } })),
    putViews: (id: string, views: View[]) =>
      loose<{ views: View[] }>(call('put', '/databases/{id}/views', { path: { id }, body: { views } as unknown as BodyOf<'/databases/{id}/views', 'put'> })),
    addView: (id: string, view: Partial<View>) =>
      loose<View>(call('post', '/databases/{id}/views', { path: { id }, body: view as BodyOf<'/databases/{id}/views', 'post'> })),
    patchView: (id: string, view: string, b: Partial<View>) =>
      loose<View>(call('patch', '/databases/{id}/views/{view}', { path: { id, view }, body: b as BodyOf<'/databases/{id}/views/{view}', 'patch'> })),
    deleteView: (id: string, view: string) => call('delete', '/databases/{id}/views/{view}', { path: { id, view } }),
    pressButton: (id: string, row: string, key: string) => call('post', '/databases/{id}/button/{row}/{key}', { path: { id, row, key } }),
    /** 库的历史，一页（新的在前） */
    dbHistory: (db: string, q: QueryOf<'/databases/{id}/history', 'get'> = {}) =>
      call('get', '/databases/{id}/history', { path: { id: db }, query: q }) as Promise<DbHistory>,
    eachDbHistory: (db: string, q: { row?: string; prop?: string; limit?: number } = {}): AsyncGenerator<DbHistoryItem> =>
      paginate((cursor) => (call('get', '/databases/{id}/history', { path: { id: db }, query: { ...q, cursor } }) as Promise<DbHistory>)),
    dbUndo: (db: string, seq: number) => call('post', '/databases/{id}/history/undo', { path: { id: db }, body: { seq } }),

    // ---------- 搜索、历史版本 ----------
    search: (ws: string, q: string, limit = 20, kind: 'page' | 'row' | 'all' = 'all') =>
      call('get', '/workspaces/{ws}/search', { path: { ws }, query: { q, limit, kind } }).then((r) => r.results),
    /** 版本，一页（新的在前） */
    versions: (page: string, q: QueryOf<'/pages/{id}/versions', 'get'> = {}) => call('get', '/pages/{id}/versions', { path: { id: page }, query: q }),
    eachVersion: (page: string, q: { limit?: number } = {}): AsyncGenerator<Version> =>
      paginate((cursor) => call('get', '/pages/{id}/versions', { path: { id: page }, query: { ...q, cursor } }).then((r) => ({ items: r.versions, next_cursor: r.next_cursor }))),
    version: (page: string, v: number | string) => call('get', '/pages/{id}/versions/{v}', { path: { id: page, v } }),
    versionDiff: (page: string, v: number | string, to: number | 'current' = 'current') =>
      call('get', '/pages/{id}/versions/{v}/diff', { path: { id: page, v }, query: { to } }),
    restoreVersion: (page: string, v: number | string) => call('post', '/pages/{id}/versions/{v}/restore', { path: { id: page, v } }),
    saveVersion: (page: string, label: string) => call('post', '/pages/{id}/versions', { path: { id: page }, body: { label } }),

    // ---------- 回收站 ----------
    /** 一页（新删的在前） */
    trash: (ws: string, q: QueryOf<'/workspaces/{ws}/trash', 'get'> = {}) => call('get', '/workspaces/{ws}/trash', { path: { ws }, query: q }) as Promise<TrashList>,
    eachTrash: (ws: string, q: { limit?: number } = {}): AsyncGenerator<TrashItem> =>
      paginate((cursor) => call('get', '/workspaces/{ws}/trash', { path: { ws }, query: { ...q, cursor } }) as Promise<TrashList>),
    restoreTrash: (id: string) => call('post', '/trash/{id}/restore', { path: { id } }),
    purgeTrash: (id: string) => call('delete', '/trash/{id}', { path: { id } }),

    // ---------- 分享 ----------
    getShare: (page: string) =>
      call('get', '/pages/{id}/share', { path: { id: page } }).catch((e) => {
        if (isApiError(e) && e.status === 404) return null
        throw e
      }),
    createShare: (page: string, b: BodyOf<'/pages/{id}/share', 'post'>) => call('post', '/pages/{id}/share', { path: { id: page }, body: b }),
    revokeShare: (page: string) => call('delete', '/pages/{id}/share', { path: { id: page } }),

    // ---------- 文件、链接卡片 ----------
    /** 上传文件（浏览器里有进度）。file 可以是 File / Blob，也可以是字节或字符串（这时用 name 给文件名） */
    upload(ws: string, input: Blob | ArrayBuffer | Uint8Array | string, o?: ((f: number) => void) | { name?: string; onProgress?: (f: number) => void }): Promise<T.Uploaded> {
      const opt = typeof o === 'function' ? { onProgress: o } : (o ?? {})
      const onProgress = opt.onProgress
      const file: Blob & { name?: string } = input instanceof Blob ? input : new Blob([input as BlobPart])
      const name = opt.name || file.name || 'file'
      const url = `${API}/workspaces/${encodeURIComponent(ws)}/files`
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
            } else {
              let code = 'http_' + xhr.status
              let message = xhr.statusText
              try {
                const j = JSON.parse(xhr.responseText)
                code = j?.error?.code ?? code
                message = j?.error?.message ?? message
              } catch {
                /* 不是 JSON */
              }
              reject(new ApiError(xhr.status, code, message))
            }
          }
          xhr.onerror = () => reject(new ApiConnectionError('上传失败：连不上'))
          const fd = new FormData()
          fd.append('file', file, name)
          xhr.send(fd)
        })
      }
      const fd = new FormData()
      fd.append('file', file, name)
      return send(url, { method: 'POST', headers: headers(false), body: fd }, (r) => r.json())
    },
    /** 下载文件（Blob）；w：图片要多宽的缩略图 */
    download: (id: string, q: { w?: number } = {}) => blob(`/files/${enc(id)}`, q),
    /** 同上，地址里带文件名（下载时浏览器用这个名字） */
    downloadNamed: (id: string, name: string, q: { w?: number } = {}) => blob(`/files/${enc(id)}/${enc(name)}`, q),
    /** 按内容哈希取媒体（图片、附件） */
    media: (hash: string, q: { w?: number } = {}) => blob(`/media/${enc(hash)}`, q),
    /** 给文件、转存的图片签名：拿到不用登录就能打开的地址（<img>、链接带不了 token 时用）。urls 是 /api/v1/files/…、/api/v1/media/… 或文件 id，
     *  返回的 urls 一一对应（看不到的、不是本站文件的是 null），expires_at 是过期时间（毫秒）。签名不含 w，可以自己加 &w= */
    signFileUrls: (urls: string[]) => call('post', '/files/sign', { body: { urls } }),
    /** 链接解析（卡片用）；失败返回 null */
    unfurl: (url: string) => call('get', '/unfurl', { query: { url } }).catch(() => null),

    /** 工作区的实时事件（SSE，断了自动重连，带 Last-Event-ID） */
    events: (ws: string): EventsHandle => openEvents(`${API}/workspaces/${enc(ws)}/events`, { credentials, headers: () => headers(false), fetch: f, timeout }),
    /** 一页的协作文档：Y.Doc + WebSocket 同步 + 浏览器里的 IndexedDB 缓存 */
    openPage: (id: string, o: OpenPageOptions = {}): PageConn => {
      if (!plugins.openPage) throw new Error("@open-agent-docs/client：openPage 要先 import '@open-agent-docs/client/page'（或者从 '@open-agent-docs/client' 整个引入）")
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
        if (!plugins.database) throw new Error("@open-agent-docs/client：database 要先 import '@open-agent-docs/client/db'（或者从 '@open-agent-docs/client' 整个引入）")
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
export * from './errors.ts'
export { VERSION } from './version.ts'
export { openEvents, type EventsHandle } from './events.ts'
export type { OpenPageOptions, PageConn, PageStatus } from './page.ts'
export type { DbStore, EventSource_ as EventsLike, Status as DbStatus } from './db/store.ts'
export type { T as Types }

/** 筛选条件是不是一组（嵌套的 and / or） */
export const isGroup = (x: Cond | Filter): x is Filter => (x as Filter).items !== undefined

/** 收件箱一条：data 里的字段摊平到外面 */
export const flatInbox = (n: InboxNotification): InboxItem => ({ ...(n.data as Partial<InboxItem>), ...n, data: undefined })

/** 评论串的锚点（没挂在文字上时各项为空） */
export function threadAnchor(t: Thread) {
  return {
    block_id: t.anchor?.block_id ?? null,
    start: t.anchor?.start ?? null,
    end: t.anchor?.end ?? null,
    quote: t.anchor?.quote ?? '',
  }
}
