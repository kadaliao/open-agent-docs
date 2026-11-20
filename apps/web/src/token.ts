// iframe 跨域嵌入时的登录（JWT）。只在嵌入模式（地址带 ?embed=1）里生效，别的时候什么都不做，照常用 cookie。
//   - 地址里的 #token=<jwt>：读出来只放内存（不进 localStorage），马上用 history.replaceState 从地址栏抹掉；
//     之后接口带 Authorization: Bearer，WebSocket 带 ?token=（见 api.ts、conn.ts）。
//   - 宿主发来 { type: 'oad:token', token }：换新的。只认 /auth/config 的 allowed_origins（OAD_ALLOWED_ORIGINS）里的 origin，
//     和同源的；而且得是嵌着我们的那个窗口发的。
//   - 发给宿主 { type: 'oad:auth-required', reason }：接口回 401（reason: 'unauthorized'），或者 token 离过期不到 60 秒（'expiring'）。
//     同一个 token 每种原因只发一次，宿主回 oad:token。
// 这个文件不引别的模块（api.ts 引它，要最先跑完）。

const params = new URLSearchParams(location.search)
const EMBED = params.has('embed') && params.get('embed') !== '0'

/** 离过期还剩多少毫秒时向宿主要新的 */
export const REFRESH_BEFORE_MS = 60_000

let token: string | null = null
let expiryTimer: ReturnType<typeof setTimeout> | null = null
/** 这个 token 已经为哪些原因要过新的了 */
let asked = new Set<string>()
const listeners = new Set<(t: string) => void>()

/** 现在的 token（不是嵌入模式、或者宿主没给，是 null：用 cookie） */
export const getToken = () => token

/** 换了 token 时调（WebSocket 换参数、没登录上的重新登录） */
export function onToken(f: (t: string) => void) {
  listeners.add(f)
  return () => {
    listeners.delete(f)
  }
}

/** JWT 的 exp（秒）；不是 JWT（Agent token、会话令牌）或者没有 exp 返回 null */
export function jwtExp(t: string): number | null {
  const parts = t.split('.')
  if (parts.length !== 3) return null
  try {
    const b64 = parts[1].replace(/-/g, '+').replace(/_/g, '/')
    const json = decodeURIComponent(
      Array.from(atob(b64.padEnd(b64.length + ((4 - (b64.length % 4)) % 4), '=')), (c) => '%' + c.charCodeAt(0).toString(16).padStart(2, '0')).join(''),
    )
    const exp = (JSON.parse(json) as { exp?: unknown }).exp
    return typeof exp === 'number' && Number.isFinite(exp) ? exp : null
  } catch {
    return null
  }
}

function post(msg: Record<string, unknown>) {
  if (window.parent === window) return
  // 不带秘密，发给谁都行（宿主自己检查 origin）
  window.parent.postMessage(msg, '*')
}

/** 向宿主要新的 token（同一个 token 每种原因只要一次） */
export function authRequired(reason: 'unauthorized' | 'expiring') {
  if (!EMBED || asked.has(reason)) return
  asked.add(reason)
  post({ type: 'oad:auth-required', reason })
}

/** 用一个新的 token（宿主给的）。now 给测试用 */
export function setToken(t: string, now = Date.now()) {
  if (!t || t === token) return
  token = t
  asked = new Set()
  if (expiryTimer) clearTimeout(expiryTimer)
  expiryTimer = null
  const exp = jwtExp(t)
  if (exp !== null) {
    const wait = exp * 1000 - REFRESH_BEFORE_MS - now
    if (wait <= 0) authRequired('expiring')
    // setTimeout 最多等 24.8 天
    else if (wait < 2 ** 31 - 1) expiryTimer = setTimeout(() => authRequired('expiring'), wait)
  }
  listeners.forEach((f) => f(t))
}

/** 地址 #… 里的 token=：取出来，地址栏里抹掉（别的 # 参数留着）。没有返回 null */
export function takeHashToken(loc: Pick<Location, 'hash' | 'pathname' | 'search'> = location, hist: Pick<History, 'replaceState' | 'state'> = history): string | null {
  if (!loc.hash.includes('token=')) return null
  const h = new URLSearchParams(loc.hash.slice(1))
  const t = h.get('token')
  if (t === null) return null
  h.delete('token')
  const rest = h.toString()
  hist.replaceState(hist.state, '', loc.pathname + loc.search + (rest ? '#' + rest : ''))
  return t || null
}

/** 宿主的 origin 能不能给我们 token：同源的，或者在 allowed 里（OAD_ALLOWED_ORIGINS，已经规范化成小写、没有结尾的 /） */
export function originAllowed(origin: string, allowed: readonly string[], self = location.origin) {
  if (!origin || origin === 'null') return false
  if (origin === self) return true
  const o = origin.toLowerCase().replace(/\/+$/, '')
  return allowed.some((a) => a.toLowerCase().replace(/\/+$/, '') === o)
}

/** 宿主发来的消息：是 oad:token、来自嵌着我们的窗口、origin 允许，就换上。返回换没换 */
export async function handleTokenMessage(
  e: Pick<MessageEvent, 'data' | 'origin' | 'source'>,
  allowed: () => Promise<readonly string[]>,
  parent: unknown = window.parent,
): Promise<boolean> {
  const d = e.data as { type?: unknown; token?: unknown } | null
  if (!d || typeof d !== 'object' || d.type !== 'oad:token' || typeof d.token !== 'string' || !d.token) return false
  if (e.source !== parent) return false
  if (!originAllowed(e.origin, await allowed())) {
    console.warn(`[oad] 不认 ${e.origin} 发来的 token：把它加进 OAD_ALLOWED_ORIGINS`)
    return false
  }
  setToken(d.token)
  return true
}

// 允许的 origin 只问一次服务器（不用登录）
let allowedP: Promise<readonly string[]> | null = null
const allowedOrigins = () =>
  (allowedP ??= fetch('/api/v1/auth/config', { credentials: 'same-origin' })
    .then((r) => (r.ok ? r.json() : {}))
    .then((c: { allowed_origins?: unknown }) => (Array.isArray(c.allowed_origins) ? c.allowed_origins.filter((x): x is string => typeof x === 'string') : []))
    .catch(() => {
      allowedP = null
      return [] as string[]
    }))

/** 给 createClient 的 fetch：回 401 时向宿主要新 token */
export function watch401(f: typeof fetch = (...a) => fetch(...a)): typeof fetch {
  return async (...a) => {
    const r = await f(...a)
    if (r.status === 401) authRequired('unauthorized')
    return r
  }
}

export const EMBED_AUTH = EMBED

if (EMBED) {
  const t = takeHashToken()
  if (t) setToken(t)
  window.addEventListener('message', (e) => {
    handleTokenMessage(e, allowedOrigins).catch(() => {})
  })
}
