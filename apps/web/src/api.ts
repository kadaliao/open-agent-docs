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
}

export const syncUrl = () => `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}${API}/sync`
export const eventsUrl = (ws: string) => `${API}/workspaces/${ws}/events`
