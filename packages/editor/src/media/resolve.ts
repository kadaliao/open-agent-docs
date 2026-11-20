// 显示服务器上的图片、视频、文件之前，把地址换成浏览器能直接打开的：
// - 跨域时（服务器在别的域名）根相对的 /api/v1/files/… 要拼上服务器地址；
// - 用 token 登录时（JWT、Agent token），<img>、<video>、链接带不了 Authorization，要换成签名地址（POST /files/sign，?exp=&sig=）。
// 同域 cookie 登录不用签（浏览器自己带 cookie），宿主不给钩子就是原样用。
// 同一轮要签的地址攒成一批（一个请求最多 500 个），签好的缓存到过期前 60 秒；签名不含 w，缩略图的 w= 自己加。

/** 解析地址的钩子（MediaServices.resolveUrl）：缓存里有的同步给，要问服务器的给 Promise。fresh：不用缓存，重新签（图片加载失败时） */
export type UrlResolver = (url: string, o?: { fresh?: boolean }) => string | Promise<string>

const ASSET = /^(https?:\/\/[^/?#]+)?\/api\/v1\/(files|media)\//i

/** 服务器上的文件、转存的图片（/api/v1/files/…、/api/v1/media/…，根相对的或带域名的） */
export function isServerAsset(url: string) {
  return ASSET.test(url)
}

/** 地址的查询参数改一改：drop 里的去掉，add 里的加在后面（# 之前） */
export function editQuery(url: string, drop: string[], add: Record<string, string> = {}) {
  const hash = url.indexOf('#')
  const frag = hash >= 0 ? url.slice(hash) : ''
  const main = hash >= 0 ? url.slice(0, hash) : url
  const q = main.indexOf('?')
  const path = q >= 0 ? main.slice(0, q) : main
  const parts = (q >= 0 ? main.slice(q + 1) : '').split('&').filter((kv) => kv && !drop.includes(kv.split('=')[0]))
  for (const [k, v] of Object.entries(add)) parts.push(`${k}=${encodeURIComponent(v)}`)
  return path + (parts.length ? '?' + parts.join('&') : '') + frag
}

/** 地址里的某个查询参数 */
export function queryParam(url: string, k: string): string | null {
  const m = new RegExp(`[?&]${k}=([^&#]*)`).exec(url)
  return m ? decodeURIComponent(m[1]) : null
}

export interface SignResult {
  urls: (string | null)[]
  /** 毫秒时间戳 */
  expires_at: number
}

export interface UrlResolverOptions {
  /** 服务器地址（`https://docs.example.com`）；同域给 ''。根相对的地址拼在它后面 */
  baseUrl?: string
  /** 签名：@open-agent-docs/client 的 signFileUrls。不给就只拼地址、不签（同域 cookie 登录、跨子域带 cookie 时） */
  sign?: ((urls: string[]) => Promise<SignResult>) | null
  /** 现在（毫秒），测试用 */
  now?: () => number
}

type Entry = { url: string; until: number }

/**
 * 做一个解析地址的钩子，交给 OadEditor / OadDatabaseView 的 media.resolveUrl。
 * 签过的缓存到过期前 60 秒；签不了的（看不到、不是本站的）照原样用，1 分钟后再试；签名接口出错时也照原样用（不缓存）。
 */
export function createUrlResolver(o: UrlResolverOptions = {}): UrlResolver & { clear: () => void } {
  const base = (o.baseUrl ?? '').replace(/\/+$/, '')
  const now = o.now ?? Date.now
  const sign = o.sign ?? null
  const cache = new Map<string, Entry>()
  let batch: Map<string, { resolve: (u: string) => void }[]> | null = null

  const abs = (u: string) => (base && u.startsWith('/') && !u.startsWith('//') ? base + u : u)

  async function flush(keys: Map<string, { resolve: (u: string) => void }[]>) {
    const list = [...keys.keys()]
    for (let i = 0; i < list.length; i += 500) {
      const part = list.slice(i, i + 500)
      let res: SignResult | null = null
      try {
        res = await sign!(part)
      } catch {
        res = null
      }
      part.forEach((key, j) => {
        const signed = res?.urls?.[j]
        let url: string
        if (res && signed) {
          url = abs(signed)
          cache.set(key, { url, until: res.expires_at - 60_000 })
        } else {
          url = abs(key)
          // 看不到的、不是本站的：先照原样用，过一会儿再问；接口出错：不记
          if (res) cache.set(key, { url, until: now() + 60_000 })
        }
        for (const w of keys.get(key) ?? []) w.resolve(url)
      })
    }
  }

  const resolve = ((url: string, opt: { fresh?: boolean } = {}) => {
    if (!url || !isServerAsset(url)) return url
    // 已经签过的（format=html 给的、宿主自己签的）不再签
    if (queryParam(url, 'sig') !== null || !sign) return abs(url)
    const w = queryParam(url, 'w')
    const key = editQuery(url, ['w'])
    const withW = (u: string) => (w ? editQuery(u, ['w'], { w }) : u)
    const hit = cache.get(key)
    if (hit && !opt.fresh && hit.until > now()) return withW(hit.url)
    cache.delete(key)
    return new Promise<string>((done) => {
      if (!batch) {
        const b = (batch = new Map())
        // 同一轮（一次渲染里的所有图片）攒成一批
        setTimeout(() => {
          if (batch === b) batch = null
          void flush(b)
        }, 0)
      }
      const waiting = batch.get(key)
      const one = { resolve: (u: string) => done(withW(u)) }
      if (waiting) waiting.push(one)
      else batch.set(key, [one])
    })
  }) as UrlResolver & { clear: () => void }
  resolve.clear = () => cache.clear()
  return resolve
}

/** 用钩子解析一个地址；同步给得出的马上回调，不然等 Promise。没有钩子就原样 */
export function withUrl(resolve: UrlResolver | null | undefined, url: string, then: (u: string) => void, o?: { fresh?: boolean }) {
  if (!resolve || !url) return then(url)
  let r: string | Promise<string>
  try {
    r = resolve(url, o)
  } catch {
    return then(url)
  }
  if (typeof r === 'string') then(r)
  else r.then(then, () => then(url))
}

/** 把一个对象（链接卡片的解析结果之类）里所有服务器文件地址都换掉。全在缓存里时同步给 */
export function resolveDeep<T>(value: T, resolve: UrlResolver | null | undefined): T | Promise<T> {
  if (!resolve) return value
  const found: string[] = []
  const collect = (v: unknown) => {
    if (typeof v === 'string') {
      if (isServerAsset(v)) found.push(v)
    } else if (Array.isArray(v)) v.forEach(collect)
    else if (v && typeof v === 'object') Object.values(v).forEach(collect)
  }
  collect(value)
  if (!found.length) return value
  const got = new Map<string, string | Promise<string>>()
  for (const u of found) if (!got.has(u)) got.set(u, resolve(u))
  const build = (m: Map<string, string>): T => {
    const map = (v: unknown): unknown => {
      if (typeof v === 'string') return m.get(v) ?? v
      if (Array.isArray(v)) return v.map(map)
      if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, map(x)]))
      return v
    }
    return map(value) as T
  }
  const entries = [...got.entries()]
  if (entries.every(([, r]) => typeof r === 'string')) return build(new Map(entries as [string, string][]))
  return Promise.all(entries.map(async ([k, r]) => [k, await Promise.resolve(r).catch(() => k)] as [string, string])).then((e) => build(new Map(e)))
}

/** 客户端（@open-agent-docs/client）用 token 登录、或者服务器在别的域名时用的钩子；同域 cookie 登录给 null（不用换）。 */
export function clientUrlResolver(
  client: { baseUrl: string; signFileUrls?: (urls: string[]) => Promise<SignResult> },
  o: { token?: boolean } = {},
): (UrlResolver & { clear: () => void }) | null {
  const base = client.baseUrl ?? ''
  let cross = false
  try {
    cross = !!base && typeof location !== 'undefined' && new URL(base, location.href).origin !== location.origin
  } catch {
    cross = true
  }
  if (!o.token && !cross) return null
  // 跨域但用 cookie（同一个站点的子域名）：只拼地址；用 token：签名
  const sign = o.token && client.signFileUrls ? (urls: string[]) => client.signFileUrls!(urls) : null
  return createUrlResolver({ baseUrl: base, sign })
}
