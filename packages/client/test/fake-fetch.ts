// 假的 fetch：记下每个请求，按路由回答。
export interface Call {
  url: string
  method: string
  headers: Record<string, string>
  credentials?: RequestCredentials
  body?: unknown
}

export type Handler = (c: Call) => Response | Promise<Response>

export function fakeFetch(handler: Handler) {
  const calls: Call[] = []
  const f = (async (input: RequestInfo | URL, init: RequestInit = {}) => {
    const h: Record<string, string> = {}
    new Headers(init.headers).forEach((v, k) => (h[k] = v))
    let body: unknown = init.body
    if (typeof body === 'string') {
      try {
        body = JSON.parse(body)
      } catch {
        /* 原样 */
      }
    }
    const c: Call = { url: String(input), method: init.method ?? 'GET', headers: h, credentials: init.credentials, body }
    calls.push(c)
    return handler(c)
  }) as typeof fetch
  return { fetch: f, calls }
}

export const json = (v: unknown, status = 200) => new Response(JSON.stringify(v), { status, headers: { 'content-type': 'application/json' } })
