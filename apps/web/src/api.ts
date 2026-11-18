import { getLocale } from './i18n'
// 和服务器的 HTTP 接口（docs/phase1-contract.md）。
export const API = '/api/v1'

export interface Page {
  id: string
  // 第 4 阶段：database = 数据库，row = 库里的一行（侧栏不列）
  kind?: 'page' | 'database' | 'row' | string
  workspace_id?: string
  parent_id: string | null
  position: string
  title: string
  icon: string | null
  updated_at: number
}

export interface Me {
  user: { id: string; name: string; email?: string | null; kind?: string; avatar?: string | null; locale?: string | null }
  workspaces: { id: string; name: string; icon?: string | null; role: 'viewer' | 'editor' | 'admin' | 'owner' }[]
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
    // 服务器给人看的错误信息按界面语言
    headers: { 'accept-language': getLocale() === 'en' ? 'en' : 'zh-CN', ...(body === undefined ? {} : { 'content-type': 'application/json' }) },
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

  members: (ws: string) =>
    req<{ members: { role: string; user: { id: string; name: string; email?: string | null } }[] }>('GET', `/workspaces/${ws}/members`).then((r) => r.members ?? []),
  inbox: (after?: number | string, limit = 50) =>
    req<{ notifications: InboxItem[] } | InboxItem[]>('GET', `/me/notifications?limit=${limit}${after !== undefined ? `&after=${after}` : ''}`).then((r) =>
      (Array.isArray(r) ? r : r.notifications).map(flatInbox),
    ),
  // 第 4 阶段：数据库（docs/phase4-contract.md）
  createDatabase: (ws: string, b: { title: string; parent_id?: string | null; after?: string; schema?: unknown; views?: unknown }) =>
    req<{ page?: Page; id?: string } & Record<string, unknown>>('POST', `/workspaces/${ws}/databases`, b),
  database: (id: string) => req<import('./db/types').DbMeta>('GET', `/databases/${id}`),
  dbRows: (id: string, since?: number) =>
    req<{ seq: number; rows: import('./db/types').Row[]; deleted?: string[] }>('GET', `/databases/${id}/rows${since !== undefined ? `?since=${since}` : ''}`),
  addRows: (id: string, rows: { props: Record<string, unknown>; after?: string | null }[]) =>
    req<{ ids: string[]; seq: number; rows?: import('./db/types').Row[] }>('POST', `/databases/${id}/rows`, { rows }),
  patchRows: (id: string, ops: { row_id: string; set?: Record<string, unknown>; after?: string | null }[]) => req<{ seq: number }>('PATCH', `/databases/${id}/rows`, { ops }),
  deleteRows: (id: string, ids: string[]) => req<{ seq?: number }>('DELETE', `/databases/${id}/rows`, { ids }),
  patchSchema: (id: string, ops: Record<string, unknown>[]) => req<{ schema: import('./db/types').Schema; seq: number }>('PATCH', `/databases/${id}/schema`, { ops }),
  undoSchema: (id: string, seq: number) => req<{ schema: import('./db/types').Schema; seq: number }>('POST', `/databases/${id}/schema/undo`, { seq }),
  putViews: (id: string, views: import('./db/types').View[]) => req<unknown>('PUT', `/databases/${id}/views`, { views }),
  addView: (id: string, view: Partial<import('./db/types').View>) => req<import('./db/types').View>('POST', `/databases/${id}/views`, view),
  patchView: (id: string, view: string, b: Partial<import('./db/types').View>) => req<import('./db/types').View>('PATCH', `/databases/${id}/views/${view}`, b),
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
}

export interface SearchHit {
  page_id: string
  block_id?: string | null
  title: string
  /** 命中的片段，命中的词用 <mark> 包着（别的都转义过） */
  snippet?: string
  kind: 'page' | 'row' | 'database' | string
  score?: number
  updated_at?: number
  icon?: string | null
}

export interface Version {
  id: string
  created_at: number
  kind: 'auto' | 'agent' | 'restore' | 'manual' | string
  label?: string | null
  authors?: Person[]
}

export interface DiffBlock {
  op: 'same' | 'added' | 'removed' | 'changed'
  id?: string | null
  type?: string
  /** 那一版 / 现在的 Markdown */
  before?: string | null
  after?: string | null
  by?: Person[]
}
export interface VersionDiff {
  blocks: DiffBlock[]
}

export interface DbHistoryItem {
  seq: number
  kind: 'cell' | 'schema' | 'row' | string
  row_id?: string | null
  row_title?: string | null
  prop?: string | null
  prop_name?: string | null
  old?: unknown
  new?: unknown
  /** 给人看的说明（表结构改动） */
  summary?: string | null
  by?: Person | string | null
  at: number
  undone?: boolean
}

export interface TrashItem {
  id: string
  title: string
  icon?: string | null
  kind?: 'page' | 'database' | 'row' | string
  deleted_at: number
  deleted_by?: Person | string | null
  parent_title?: string | null
}

export interface Share {
  url: string
  token?: string
  include_children?: boolean
  indexable?: boolean
  created_at?: number
}

export interface Invite {
  token: string
  url: string
  role: string
  expires_at?: number | null
  created_at?: number
  uses?: number
}

export interface InviteInfo {
  workspace: { id: string; name: string; icon?: string | null }
  role: string
  invited_by?: { name: string } | null
  expires_at?: number | null
  signup?: boolean
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
