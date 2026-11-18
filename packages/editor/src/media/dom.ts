// 画卡片用的小工具：建 DOM、格式化数字和时间、从网址认出是哪个网站。
export type Child = Node | string | null | undefined | false
export function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs?: Record<string, string | number | boolean | null | undefined> | null, ...kids: Child[]): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag)
  if (attrs)
    for (const [k, v] of Object.entries(attrs)) {
      if (v === null || v === undefined || v === false) continue
      if (k === 'class') e.className = String(v)
      else e.setAttribute(k, v === true ? '' : String(v))
    }
  for (const k of kids) if (k !== null && k !== undefined && k !== false) e.append(k)
  return e
}

const SVGNS = 'http://www.w3.org/2000/svg'
/** 图标：viewBox + path（d 可以多段，用 | 隔开） */
export function icon(viewBox: string, d: string, cls = '', fill = 'currentColor') {
  const s = document.createElementNS(SVGNS, 'svg')
  s.setAttribute('viewBox', viewBox)
  s.setAttribute('aria-hidden', 'true')
  if (cls) s.setAttribute('class', cls)
  for (const part of d.split('|')) {
    const p = document.createElementNS(SVGNS, 'path')
    p.setAttribute('d', part)
    p.setAttribute('fill', fill)
    s.append(p)
  }
  return s
}

export function img(src: string | undefined, cls: string, alt = '', extra: Record<string, string> = {}) {
  if (!src) return null
  const i = h('img', { class: cls, src, alt, loading: 'lazy', decoding: 'async', referrerpolicy: 'no-referrer', draggable: 'false', ...extra })
  i.addEventListener('error', () => i.classList.add('is-broken'), { once: true })
  return i
}

/** 服务器转存的图片（/api/v1/media、/api/v1/files）给 srcset */
export function srcset(src: string) {
  if (!/^\/api\/v1\/(media|files)\//.test(src)) return undefined
  const base = src.split('?')[0]
  return [320, 640, 960, 1280, 1920].map((w) => `${base}?w=${w} ${w}w`).join(', ')
}

export function host(url: string) {
  try {
    return new URL(url, location.href).host.replace(/^www\./, '')
  } catch {
    return url
  }
}

// 跟着页面语言（界面是中文就按中文写数字和日期），不看浏览器语言
// 每次现读（换语言不用刷新）
const isZh = () => typeof document === 'undefined' || /^zh/i.test(document.documentElement.lang || 'zh')
export function count(n?: number) {
  if (n === undefined || n === null) return ''
  if (isZh()) {
    if (n >= 1e8) return (n / 1e8).toFixed(n >= 1e9 ? 0 : 1).replace(/\.0$/, '') + '亿'
    if (n >= 1e4) return (n / 1e4).toFixed(n >= 1e5 ? 0 : 1).replace(/\.0$/, '') + '万'
    return n.toLocaleString('zh-CN')
  }
  if (n >= 1e6) return (n / 1e6).toFixed(1).replace(/\.0$/, '') + 'M'
  if (n >= 1e3) return (n / 1e3).toFixed(1).replace(/\.0$/, '') + 'K'
  return String(n)
}

export function duration(sec?: number) {
  if (!sec && sec !== 0) return ''
  const s = Math.round(sec)
  const hh = Math.floor(s / 3600)
  const mm = Math.floor((s % 3600) / 60)
  const ss = String(s % 60).padStart(2, '0')
  return hh ? `${hh}:${String(mm).padStart(2, '0')}:${ss}` : `${mm}:${ss}`
}

export function date(iso?: string, withTime = false) {
  if (!iso) return ''
  const d = new Date(iso)
  if (isNaN(+d)) return ''
  return d.toLocaleString(isZh() ? 'zh-CN' : 'en-US', withTime ? { year: 'numeric', month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit' } : { year: 'numeric', month: 'long', day: 'numeric' })
}

export function bytes(n?: number) {
  if (!n && n !== 0) return ''
  if (n < 1024) return n + ' B'
  if (n < 1024 ** 2) return (n / 1024).toFixed(1) + ' KB'
  if (n < 1024 ** 3) return (n / 1024 ** 2).toFixed(1) + ' MB'
  return (n / 1024 ** 3).toFixed(2) + ' GB'
}

/** 只看网址认网站（没有服务器、解析失败时用）。 */
export function sniff(url: string): { provider: string; id?: string; extra?: Record<string, string> } {
  let u: URL
  try {
    u = new URL(url)
  } catch {
    return { provider: 'generic' }
  }
  const hn = u.hostname.replace(/^(www|m|mobile)\./, '')
  if (hn === 'youtu.be') return { provider: 'youtube', id: u.pathname.slice(1).split('/')[0] }
  if (hn.endsWith('youtube.com') || hn === 'youtube-nocookie.com') {
    const m = /^\/(?:shorts|embed|live|v)\/([\w-]{6,})/.exec(u.pathname)
    const id = m?.[1] ?? u.searchParams.get('v') ?? undefined
    return { provider: 'youtube', id, extra: { short: String(u.pathname.startsWith('/shorts/')) } }
  }
  if (hn === 'x.com' || hn === 'twitter.com') {
    const m = /^\/([^/]+)\/status(?:es)?\/(\d+)/.exec(u.pathname)
    return m ? { provider: 'x', id: m[2], extra: { handle: m[1] } } : { provider: 'generic' }
  }
  if (hn.endsWith('bilibili.com') || hn === 'b23.tv') {
    const m = /\/video\/(BV\w+|av\d+)/i.exec(u.pathname)
    return { provider: 'bilibili', id: m?.[1] }
  }
  if (hn === 'github.com') {
    const p = u.pathname.split('/').filter(Boolean)
    if (p.length >= 2) return { provider: 'github', id: `${p[0]}/${p[1]}`, extra: { kind: p[2] ?? '', number: p[3] ?? '' } }
  }
  if (hn === 'mp.weixin.qq.com') return { provider: 'wechat' }
  if (hn === 'open.spotify.com') return { provider: 'spotify' }
  if (hn === 'vimeo.com' || hn === 'player.vimeo.com') return { provider: 'vimeo', id: /(\d{5,})/.exec(u.pathname)?.[1] }
  if (hn.endsWith('figma.com')) return { provider: 'figma' }
  if (hn.endsWith('loom.com')) return { provider: 'loom', id: /\/(?:share|embed)\/([\w]+)/.exec(u.pathname)?.[1] }
  return { provider: 'generic' }
}

export function isFileUrl(url: string) {
  return /^(https?:\/\/[^/]+)?\/api\/v1\/files\//.test(url)
}

export function fileName(url: string) {
  try {
    return decodeURIComponent(new URL(url, location.href).pathname.split('/').pop() || '')
  } catch {
    return url
  }
}
