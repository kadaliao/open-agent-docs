// 错误：服务器回了错误是 ApiError（status、code、message，限流时有 retryAfter）；连不上、超时是 ApiConnectionError / ApiTimeoutError。
// 判断用 isNotFound(e) 这些函数，不用记状态码；它们也认别的副本里的 ApiError（宿主装了两份包时 instanceof 会失灵）。

export class ApiError extends Error {
  /** 服务器让等多久再试（秒，来自 Retry-After；没有就是 undefined） */
  retryAfter?: number
  /** 服务器给的请求 id（X-Request-Id），报问题时带上 */
  requestId?: string
  constructor(
    public status: number,
    public code: string,
    message: string,
    extra: { retryAfter?: number; requestId?: string } = {},
  ) {
    super(message)
    this.name = 'ApiError'
    if (extra.retryAfter !== undefined) this.retryAfter = extra.retryAfter
    if (extra.requestId !== undefined) this.requestId = extra.requestId
  }
}

/** 连不上、连接被断（请求可能没到服务器） */
export class ApiConnectionError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options)
    this.name = 'ApiConnectionError'
  }
}

/** 超过 timeout 还没回完 */
export class ApiTimeoutError extends ApiConnectionError {
  constructor(message: string) {
    super(message)
    this.name = 'ApiTimeoutError'
  }
}

/** 是不是服务器回的错误（含别的副本里的 ApiError） */
export function isApiError(e: unknown): e is ApiError {
  return e instanceof ApiError || (!!e && typeof e === 'object' && (e as Error).name === 'ApiError' && typeof (e as ApiError).status === 'number')
}
const is = (s: (n: number) => boolean) => (e: unknown): e is ApiError => isApiError(e) && s(e.status)
export const isBadRequest = is((s) => s === 400)
export const isUnauthorized = is((s) => s === 401)
export const isForbidden = is((s) => s === 403)
export const isNotFound = is((s) => s === 404)
export const isConflict = is((s) => s === 409)
export const isGone = is((s) => s === 410)
export const isTooLarge = is((s) => s === 413)
export const isRateLimited = is((s) => s === 429)
export const isServerError = is((s) => s >= 500)
export const isConnectionError = (e: unknown): e is ApiConnectionError =>
  e instanceof ApiConnectionError || (!!e && typeof e === 'object' && ['ApiConnectionError', 'ApiTimeoutError'].includes((e as Error).name))
export const isTimeout = (e: unknown): e is ApiTimeoutError => e instanceof ApiTimeoutError || (!!e && typeof e === 'object' && (e as Error).name === 'ApiTimeoutError')

/** Retry-After：秒数或者 HTTP 日期；看不懂就是 undefined */
export function parseRetryAfter(v: string | null | undefined, now = Date.now()): number | undefined {
  if (!v) return undefined
  if (/^\s*\d+(\.\d+)?\s*$/.test(v)) return Number(v)
  const t = Date.parse(v)
  return Number.isNaN(t) ? undefined : Math.max(0, Math.ceil((t - now) / 1000))
}

/** 把一个不成功的响应变成 ApiError（正文是 { error: { code, message } }，不是 JSON 时 code 是 http_<状态>） */
export async function errorFrom(r: Response): Promise<ApiError> {
  let code = 'http_' + r.status
  let message = r.statusText
  try {
    const j = await r.json()
    code = j?.error?.code ?? code
    message = j?.error?.message ?? message
  } catch {
    /* 不是 JSON */
  }
  return new ApiError(r.status, code, message, {
    retryAfter: parseRetryAfter(r.headers.get('retry-after')),
    requestId: r.headers.get('x-request-id') ?? undefined,
  })
}
