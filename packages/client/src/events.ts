// 工作区事件（SSE）：page.*、comment.*、thread.updated、notification、db.changed、db.schema、resync。
// 用 fetch 读流（浏览器和 Node 都行，能带 Authorization 头）；断了按 1s、2s、4s… 最多 30s 重连，带 Last-Event-ID 接着收。
// 401、403、404 不重连（重连也没用）：关掉，交给 onError。
import { errorFrom, type ApiError } from './errors.ts'

export interface EventsHandle {
  /** 订阅一种事件（'*' 是全部）；返回取消订阅 */
  on(name: string, f: (data: any, name: string) => void): () => void
  /** 连上 / 断开 */
  onStatus(f: (connected: boolean) => void): () => void
  /** 服务器拒绝（401 / 403 / 404）：流已经关了，不会再重连 */
  onError(f: (e: ApiError) => void): () => void
  close(): void
}

interface Opts {
  credentials: RequestCredentials
  headers: () => Record<string, string>
  fetch: typeof fetch
  /** 第一次重连等多久（毫秒，之后翻倍，最多 30 秒） */
  minRetry?: number
  /** 连接的超时（毫秒，等到响应头为止；读流没有总超时） */
  timeout?: number
}

export function openEvents(url: string, o: Opts): EventsHandle {
  const subs = new Map<string, Set<(d: any, n: string) => void>>()
  const status = new Set<(c: boolean) => void>()
  const errors = new Set<(e: ApiError) => void>()
  let lastId: string | null = null
  let closed = false
  let ctrl: AbortController | null = null
  const first = o.minRetry ?? 1000
  let backoff = first
  let timer: ReturnType<typeof setTimeout> | null = null

  const emit = (name: string, raw: string) => {
    let data: any = raw
    try {
      data = raw ? JSON.parse(raw) : null
    } catch {
      /* 不是 JSON 就原样给 */
    }
    subs.get(name)?.forEach((f) => f(data, name))
    subs.get('*')?.forEach((f) => f(data, name))
  }

  async function run() {
    if (closed) return
    ctrl = new AbortController()
    try {
      const h: Record<string, string> = { ...o.headers(), accept: 'text/event-stream' }
      if (lastId) h['last-event-id'] = lastId
      const c = ctrl
      const t = o.timeout ? setTimeout(() => c.abort(), o.timeout) : null
      let r: Response
      try {
        r = await o.fetch(url, { headers: h, credentials: o.credentials, signal: ctrl.signal })
      } finally {
        if (t) clearTimeout(t)
      }
      if (r.status === 401 || r.status === 403 || r.status === 404) {
        const e = await errorFrom(r)
        closed = true
        errors.forEach((f) => f(e))
        return
      }
      if (!r.ok || !r.body) throw new Error('HTTP ' + r.status)
      backoff = first
      status.forEach((f) => f(true))
      const reader = r.body.getReader()
      const dec = new TextDecoder()
      let buf = ''
      let ev = { name: 'message', data: [] as string[], id: null as string | null }
      for (;;) {
        const { done, value } = await reader.read()
        if (done) break
        buf += dec.decode(value, { stream: true })
        let nl: number
        while ((nl = buf.search(/\r?\n/)) >= 0) {
          const line = buf.slice(0, nl)
          buf = buf.slice(nl + (buf[nl] === '\r' ? 2 : 1))
          if (line === '') {
            if (ev.id !== null) lastId = ev.id
            if (ev.data.length) emit(ev.name, ev.data.join('\n'))
            ev = { name: 'message', data: [], id: null }
          } else if (line.startsWith(':')) {
            /* 心跳 */
          } else {
            const i = line.indexOf(':')
            const k = i < 0 ? line : line.slice(0, i)
            const v = i < 0 ? '' : line.slice(i + 1).replace(/^ /, '')
            if (k === 'event') ev.name = v
            else if (k === 'data') ev.data.push(v)
            else if (k === 'id') ev.id = v
          }
        }
      }
    } catch {
      /* 断了：重连 */
    }
    status.forEach((f) => f(false))
    if (closed) return
    timer = setTimeout(run, backoff)
    backoff = Math.min(30_000, backoff * 2)
  }
  run()

  return {
    on(name, f) {
      let s = subs.get(name)
      if (!s) subs.set(name, (s = new Set()))
      s.add(f)
      return () => s!.delete(f)
    },
    onStatus(f) {
      status.add(f)
      return () => status.delete(f)
    },
    onError(f) {
      errors.add(f)
      return () => errors.delete(f)
    },
    close() {
      closed = true
      if (timer) clearTimeout(timer)
      ctrl?.abort()
    },
  }
}
