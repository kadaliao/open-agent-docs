// 和服务器的 HTTP 接口（docs/phase1-contract.md）。
export const API = '/api/v1'

export interface Page {
  id: string
  workspace_id?: string
  parent_id: string | null
  position: string
  title: string
  icon: string | null
  updated_at: number
}

export interface Me {
  user: { id: string; name: string; email?: string | null; kind?: string }
  workspaces: { id: string; name: string; role: 'viewer' | 'editor' | 'owner' }[]
}

export type AuthMode = 'local' | 'header' | 'none'

export type Scope = 'read' | 'write' | 'admin'

export interface Agent {
  id: string
  name: string
  about?: string | null
  scope: Scope
  webhook?: string | null
  created_by?: string | null
  created_at: number
  last_used_at?: number | null
  revoked_at?: number | null
  // 下面两项约定里没写，服务器给了就显示
  pending?: number
  pending_notifications?: number
  webhook_error?: string | null
}

export interface Person {
  kind: 'user' | 'agent' | string
  id: string
  name: string
}

export interface Comment {
  id: string
  thread_id: string
  author_kind: string
  author_id: string
  author?: Person
  body: string
  created_at: number
  edited_at?: number | null
  deleted_at?: number | null
}

export interface AnchorIn {
  block_id: string | null
  start: string
  end: string
  quote: string
}

export interface Thread {
  id: string
  page_id: string
  block_id?: string | null
  // 约定的表字段是 anchor_start / anchor_end，也接受 anchor 对象
  anchor?: { block_id?: string | null; start?: string | null; end?: string | null; quote?: string | null } | null
  anchor_start?: string | null
  anchor_end?: string | null
  quote?: string | null
  created_by?: string | null
  created_at: number
  resolved_at?: number | null
  resolved_by?: string | null
  comments: Comment[]
}

export interface InboxItem {
  id: number | string
  kind?: string
  type?: string
  page?: { id: string; title?: string } | null
  page_id?: string
  thread_id?: string | null
  comment?: { id?: string; author?: Person; body?: string } | null
  actor?: Person | null
  by?: Person | null
  quote?: string | null
  created_at: number
  read_at?: number | null
  // 服务器把内容放在 data 里（{ id, type, data: { page, thread_id, comment, quote, by }, created_at, read_at }）
  data?: Partial<InboxItem>
}

/** 收件箱一条：data 里的字段摊平到外面 */
export const flatInbox = (n: InboxItem): InboxItem => (n.data ? { ...n.data, ...n, data: undefined } : n)

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message)
  }
}

async function req<T>(method: string, path: string, body?: unknown): Promise<T> {
  const r = await fetch(API + path, {
    method,
    credentials: 'same-origin',
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
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
  if (r.status === 204) return undefined as T
  const text = await r.text()
  return (text ? JSON.parse(text) : undefined) as T
}

export const api = {
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

  inbox: (after?: number | string, limit = 50) =>
    req<{ notifications: InboxItem[] } | InboxItem[]>('GET', `/me/notifications?limit=${limit}${after !== undefined ? `&after=${after}` : ''}`).then((r) =>
      (Array.isArray(r) ? r : r.notifications).map(flatInbox),
    ),
  markRead: (b: { ids: (number | string)[] } | { all: true }) => req<unknown>('POST', '/me/notifications/read', b),
}

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

export const syncUrl = () => `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}${API}/sync`
export const eventsUrl = (ws: string) => `${API}/workspaces/${ws}/events`
