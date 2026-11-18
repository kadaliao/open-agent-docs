// 链接卡片和媒体播放器（按需加载）。全部是“门面”：先画封面 / 卡片，点了才加载 iframe、视频。
// 数据来自服务器的链接解析（Unfurl）；没有数据时只靠网址拼一个最简单的。
import './cards.css'
import { bytes, count, date, duration, fileName, h, host, icon, img, isFileUrl, sniff, srcset } from './dom'
import type { UMedia, Unfurl, XPost } from './types'
import { t } from '../strings'

export interface CardOptions {
  url: string
  view: 'embed' | 'bookmark'
}

export interface CardHandle {
  /** 离开视口：暂停在放的视频 */
  pause?: () => void
  destroy?: () => void
}

// ---------- 图标 ----------
const I = {
  x: ['0 0 24 24', 'M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z'],
  verified: ['0 0 22 22', 'M20.396 11c-.018-.646-.215-1.275-.57-1.816-.354-.54-.852-.972-1.438-1.246.223-.607.27-1.264.14-1.897-.131-.634-.437-1.218-.882-1.687-.47-.445-1.053-.75-1.687-.882-.633-.13-1.29-.083-1.897.14-.273-.587-.704-1.086-1.245-1.44S11.647 1.62 11 1.604c-.646.017-1.273.213-1.813.568s-.969.854-1.24 1.44c-.608-.223-1.267-.272-1.902-.14-.635.13-1.22.436-1.69.882-.445.47-.749 1.055-.878 1.688-.13.633-.08 1.29.144 1.896-.587.274-1.087.705-1.443 1.245-.356.54-.555 1.17-.574 1.817.02.647.218 1.276.574 1.817.356.54.856.972 1.443 1.245-.224.606-.274 1.263-.144 1.896.13.634.433 1.218.877 1.688.47.443 1.054.747 1.687.878.633.132 1.29.084 1.897-.136.274.586.705 1.084 1.246 1.439.54.354 1.17.551 1.816.569.647-.016 1.276-.213 1.817-.567s.972-.854 1.245-1.44c.604.239 1.266.296 1.903.164.636-.132 1.22-.447 1.68-.907.46-.46.776-1.044.908-1.681s.075-1.299-.165-1.903c.586-.274 1.084-.705 1.439-1.246.354-.54.551-1.17.569-1.816zM9.662 14.85l-3.429-3.428 1.293-1.302 2.072 2.072 4.4-4.794 1.347 1.246z'],
  heart: ['0 0 24 24', 'M16.697 5.5c-1.222-.06-2.679.51-3.89 2.16l-.805 1.09-.806-1.09C9.984 6.01 8.526 5.44 7.304 5.5c-1.243.07-2.349.78-2.91 1.91-.552 1.12-.633 2.78.479 4.82 1.074 1.97 3.257 4.27 7.129 6.61 3.87-2.34 6.052-4.64 7.126-6.61 1.111-2.04 1.03-3.7.477-4.82-.561-1.13-1.666-1.84-2.908-1.91z'],
  reply: ['0 0 24 24', 'M1.751 10c0-4.42 3.584-8 8.005-8h4.366c4.49 0 7.501 3.58 7.501 8s-3.02 8-7.5 8h-.234l-4.35 3.29c-.646.48-1.539.03-1.539-.77v-2.52c-3.93-.47-6.249-3.69-6.249-8z'],
  github: ['0 0 16 16', 'M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z'],
  star: ['0 0 16 16', 'M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.751.751 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25Z'],
  fork: ['0 0 16 16', 'M5 5.372v.878c0 .414.336.75.75.75h4.5a.75.75 0 0 0 .75-.75v-.878a2.25 2.25 0 1 1 1.5 0v.878a2.25 2.25 0 0 1-2.25 2.25h-1.5v2.128a2.251 2.251 0 1 1-1.5 0V8.5h-1.5A2.25 2.25 0 0 1 3.5 6.25v-.878a2.25 2.25 0 1 1 1.5 0ZM5 3.25a.75.75 0 1 0-1.5 0 .75.75 0 0 0 1.5 0Zm6.75.75a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Zm-3 8.75a.75.75 0 1 0-1.5 0 .75.75 0 0 0 1.5 0Z'],
  repo: ['0 0 16 16', 'M2 2.5A2.5 2.5 0 0 1 4.5 0h8.75a.75.75 0 0 1 .75.75v12.5a.75.75 0 0 1-.75.75h-2.5a.75.75 0 0 1 0-1.5h1.75v-2h-8a1 1 0 0 0-.714 1.7.75.75 0 1 1-1.072 1.05A2.495 2.495 0 0 1 2 11.5Zm10.5-1h-8a1 1 0 0 0-1 1v6.708A2.486 2.486 0 0 1 4.5 9h8ZM5 12.25a.25.25 0 0 1 .25-.25h3.5a.25.25 0 0 1 .25.25v3.25a.25.25 0 0 1-.4.2l-1.45-1.087a.249.249 0 0 0-.3 0L5.4 15.7a.25.25 0 0 1-.4-.2Z'],
  issueOpen: ['0 0 16 16', 'M8 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z|M8 0a8 8 0 1 1 0 16A8 8 0 0 1 8 0ZM1.5 8a6.5 6.5 0 1 0 13 0 6.5 6.5 0 0 0-13 0Z'],
  issueClosed: ['0 0 16 16', 'M11.28 6.78a.75.75 0 0 0-1.06-1.06L7.25 8.69 5.78 7.22a.75.75 0 0 0-1.06 1.06l2 2a.75.75 0 0 0 1.06 0l3.5-3.5Z|M16 8A8 8 0 1 1 0 8a8 8 0 0 1 16 0Zm-1.5 0a6.5 6.5 0 1 0-13 0 6.5 6.5 0 0 0 13 0Z'],
  issueSkip: ['0 0 16 16', 'M8 0a8 8 0 1 1 0 16A8 8 0 0 1 8 0ZM1.5 8a6.5 6.5 0 1 0 13 0 6.5 6.5 0 0 0-13 0Zm9.78-2.22-5.5 5.5a.749.749 0 0 1-1.275-.326.749.749 0 0 1 .215-.734l5.5-5.5a.751.751 0 0 1 1.042.018.751.751 0 0 1 .018 1.042Z'],
  pr: ['0 0 16 16', 'M1.5 3.25a2.25 2.25 0 1 1 3 2.122v5.256a2.251 2.251 0 1 1-1.5 0V5.372A2.25 2.25 0 0 1 1.5 3.25Zm5.677-.177L9.573.677A.25.25 0 0 1 10 .854V2.5h1A2.5 2.5 0 0 1 13.5 5v5.628a2.251 2.251 0 1 1-1.5 0V5a1 1 0 0 0-1-1h-1v1.646a.25.25 0 0 1-.427.177L7.177 3.427a.25.25 0 0 1 0-.354ZM3.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm0 9.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm8.25.75a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0Z'],
  merged: ['0 0 16 16', 'M5.45 5.154A4.25 4.25 0 0 0 9.25 7.5h1.378a2.251 2.251 0 1 1 0 1.5H9.25A5.734 5.734 0 0 1 5 7.123v3.505a2.25 2.25 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.95-.218ZM4.25 13.5a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Zm8.5-4.5a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5ZM5 3.25a.75.75 0 1 0 0 .005V3.25Z'],
  prClosed: ['0 0 16 16', 'M3.25 1A2.25 2.25 0 0 1 4 5.372v5.256a2.251 2.251 0 1 1-1.5 0V5.372A2.251 2.251 0 0 1 3.25 1Zm9.5 5.5a.75.75 0 0 1 .75.75v3.378a2.251 2.251 0 1 1-1.5 0V7.25a.75.75 0 0 1 .75-.75Zm-2.03-5.273a.75.75 0 0 1 1.06 0l.97.97.97-.97a.748.748 0 0 1 1.265.332.75.75 0 0 1-.205.729l-.97.97.97.97a.751.751 0 0 1-.018 1.042.751.751 0 0 1-1.042.018l-.97-.97-.97.97a.749.749 0 0 1-1.275-.326.749.749 0 0 1 .215-.734l.97-.97-.97-.97a.75.75 0 0 1 0-1.06ZM2.5 3.25a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0ZM3.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm9.5 0a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Z'],
  comment: ['0 0 16 16', 'M1 2.75C1 1.784 1.784 1 2.75 1h10.5c.966 0 1.75.784 1.75 1.75v7.5A1.75 1.75 0 0 1 13.25 12H9.06l-2.573 2.573A1.458 1.458 0 0 1 4 13.543V12H2.75A1.75 1.75 0 0 1 1 10.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h4.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z'],
  play: ['0 0 24 24', 'M8 5.14v13.72a1 1 0 0 0 1.52.85l10.9-6.86a1 1 0 0 0 0-1.7L9.52 4.29A1 1 0 0 0 8 5.14z'],
  file: ['0 0 24 24', 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zm-1 1.5L18.5 9H13zM6 20V4h6v6h6v10z'],
  download: ['0 0 24 24', 'M12 3a1 1 0 0 1 1 1v9.59l3.3-3.3a1 1 0 1 1 1.4 1.42l-5 5a1 1 0 0 1-1.4 0l-5-5a1 1 0 1 1 1.4-1.42l3.3 3.3V4a1 1 0 0 1 1-1zM5 19a1 1 0 0 1 1-1h12a1 1 0 1 1 0 2H6a1 1 0 0 1-1-1z'],
  external: ['0 0 24 24', 'M14 3h7v7h-2V6.41l-9.29 9.3-1.42-1.42L17.59 5H14zM5 5h6v2H7v10h10v-4h2v6H5z'],
  link: ['0 0 24 24', 'M10.59 13.41a1 1 0 0 1 0-1.41l3-3a1 1 0 1 1 1.41 1.41l-3 3a1 1 0 0 1-1.41 0zM8.46 18.36a3.5 3.5 0 0 1-4.95-4.95l2.83-2.83 1.41 1.42-2.83 2.82a1.5 1.5 0 1 0 2.12 2.12l2.83-2.82 1.41 1.41zm9.2-5.3-1.42-1.42 2.83-2.82a1.5 1.5 0 1 0-2.12-2.12L14.12 9.5 12.7 8.09l2.83-2.83a3.5 3.5 0 1 1 4.95 4.95z'],
}
const ico = (k: keyof typeof I, cls = '') => icon(I[k][0], I[k][1], cls)

const LANG_COLORS: Record<string, string> = {
  Rust: '#dea584', TypeScript: '#3178c6', JavaScript: '#f1e05a', Python: '#3572A5', Go: '#00ADD8', Java: '#b07219', 'C++': '#f34b7d',
  C: '#555555', 'C#': '#178600', Ruby: '#701516', Swift: '#F05138', Kotlin: '#A97BFF', Shell: '#89e051', HTML: '#e34c26', CSS: '#563d7c',
  Vue: '#41b883', Dart: '#00B4AB', PHP: '#4F5D95', Lua: '#000080', Zig: '#ec915c', Elixir: '#6e4a7e', Haskell: '#5e5086', Scala: '#c22d40',
}

const ext = (href: string, cls: string, ...kids: (Node | string | null)[]) => h('a', { class: cls, href, target: '_blank', rel: 'noopener noreferrer nofollow' }, ...kids)

function favicon(src?: string, cls = 'c-favicon') {
  if (!src) return h('span', { class: cls + ' is-empty' })
  return img(src, cls)
}

// ---------- 骨架 ----------
export function skeleton(url: string) {
  return h(
    'div',
    { class: 'c-card c-skeleton', 'aria-busy': 'true' },
    h('div', { class: 'c-sk-body' }, h('span', { class: 'c-sk-line w60' }), h('span', { class: 'c-sk-line w90' }), h('span', { class: 'c-sk-meta' }, host(url))),
    h('div', { class: 'c-sk-thumb' }),
  )
}

// ---------- 书签 ----------
function bookmark(d: Unfurl | null, url: string) {
  const title = d?.title || host(url)
  const desc = d?.description
  const thumb = d?.thumbnail?.url
  const author = d?.embed?.type === 'wechat' ? d.embed.account : undefined
  return ext(
    d?.canonical_url || url,
    'c-card c-bookmark' + (thumb ? ' has-thumb' : ''),
    h(
      'div',
      { class: 'c-bm-body' },
      h('div', { class: 'c-bm-title' }, title),
      desc ? h('div', { class: 'c-bm-desc' }, desc) : null,
      h(
        'div',
        { class: 'c-bm-meta' },
        favicon(d?.favicon),
        h('span', { class: 'c-bm-site' }, author || d?.site_name || host(url)),
        (author || d?.site_name) && (author || d?.site_name) !== host(url) ? h('span', { class: 'c-bm-url' }, host(url)) : null,
      ),
    ),
    thumb ? h('div', { class: 'c-bm-thumb' }, img(thumb, '', d?.thumbnail?.alt ?? '', srcset(thumb) ? { srcset: srcset(thumb)!, sizes: '240px' } : {})) : null,
  )
}

// ---------- 视频门面（YouTube、B 站、Vimeo…） ----------
function addParams(src: string, params: Record<string, string>) {
  try {
    const u = new URL(src)
    for (const [k, v] of Object.entries(params)) u.searchParams.set(k, v)
    return u.toString()
  } catch {
    return src
  }
}

function videoFacade(o: {
  url: string
  title?: string
  channel?: string
  avatar?: string
  thumb?: string
  badge: Node
  brand: string
  duration?: string
  stats?: string
  iframe: string
  portrait?: boolean
  pausable?: boolean
}): [HTMLElement, CardHandle] {
  const frame = h('div', { class: 'c-video-frame' + (o.portrait ? ' is-portrait' : '') })
  const cover = h(
    'button',
    { class: 'c-video-cover', type: 'button', 'aria-label': `${t('playVideo')}：${o.title ?? ''}` },
    img(o.thumb, 'c-video-thumb'),
    h('span', { class: 'c-video-shade' }),
    h('span', { class: 'c-video-top' }, o.avatar ? img(o.avatar, 'c-video-avatar') : null, h('span', { class: 'c-video-titles' }, h('span', { class: 'c-video-title' }, o.title ?? host(o.url)), o.channel ? h('span', { class: 'c-video-channel' }, o.channel) : null)),
    h('span', { class: 'c-play c-play-' + o.brand }, o.badge),
    o.duration ? h('span', { class: 'c-video-duration' }, o.duration) : null,
  )
  frame.append(cover)
  let iframe: HTMLIFrameElement | null = null
  cover.addEventListener('click', (e) => {
    e.preventDefault()
    iframe = h('iframe', {
      class: 'c-video-iframe',
      src: o.iframe,
      title: o.title ?? '',
      allow: 'accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share; fullscreen',
      allowfullscreen: true,
      referrerpolicy: 'strict-origin-when-cross-origin',
      frameborder: '0',
    })
    cover.replaceWith(iframe)
  })
  const card = h(
    'div',
    { class: 'c-card c-video c-video-' + o.brand },
    frame,
    h('div', { class: 'c-video-foot' }, h('span', { class: 'c-video-brand' }, o.brand === 'youtube' ? 'YouTube' : o.brand === 'bilibili' ? '哔哩哔哩' : o.brand), o.stats ? h('span', { class: 'c-video-stats' }, o.stats) : null, ext(o.url, 'c-open', ico('external'), t('openOriginal'))),
  )
  return [
    card,
    {
      // YouTube 播放器开了 enablejsapi，可以发消息暂停
      pause: () => {
        if (iframe && o.pausable) iframe.contentWindow?.postMessage(JSON.stringify({ event: 'command', func: 'pauseVideo', args: '' }), '*')
      },
    },
  ]
}

const ytBadge = () => icon('0 0 68 48', 'M66.52 7.74c-.78-2.93-2.49-5.41-5.42-6.19C55.79.13 34 0 34 0S12.21.13 6.9 1.55C3.97 2.33 2.27 4.81 1.48 7.74.06 13.05 0 24 0 24s.06 10.95 1.48 16.26c.78 2.93 2.49 5.41 5.42 6.19C12.21 47.87 34 48 34 48s21.79-.13 27.1-1.55c2.93-.78 4.64-3.26 5.42-6.19C67.94 34.95 68 24 68 24s-.06-10.95-1.48-16.26z|M45 24 27 14v20', 'c-yt-badge')

function youtube(d: Unfurl | null, url: string): [HTMLElement, CardHandle] {
  const e = d?.embed?.type === 'youtube' ? d.embed : null
  const id = e?.video_id ?? sniff(url).id ?? ''
  const short = e?.is_short ?? sniff(url).extra?.short === 'true'
  const src = addParams(e?.embed_url ?? `https://www.youtube-nocookie.com/embed/${id}`, { autoplay: '1', enablejsapi: '1', rel: '0', playsinline: '1' })
  return videoFacade({
    url,
    title: d?.title,
    channel: d?.author?.name,
    thumb: d?.thumbnail?.url ?? (id ? `https://i.ytimg.com/vi/${id}/hqdefault.jpg` : undefined),
    badge: ytBadge(),
    brand: 'youtube',
    iframe: src,
    portrait: short,
    pausable: true,
    stats: e?.is_live ? t('live') : undefined,
  })
}

function bilibili(d: Unfurl | null, url: string): [HTMLElement, CardHandle] {
  const e = d?.embed?.type === 'bilibili' ? d.embed : null
  const id = e?.bvid ?? sniff(url).id ?? ''
  const src = addParams(e?.embed_url ?? `https://player.bilibili.com/player.html?bvid=${id}`, { autoplay: '1', high_quality: '1', danmaku: '0' })
  const stats = [e?.view_count !== undefined ? `${count(e.view_count)} ${t('views')}` : '', e?.danmaku_count !== undefined ? `${count(e.danmaku_count)} ${t('danmaku')}` : ''].filter(Boolean).join(' · ')
  return videoFacade({
    url,
    title: d?.title,
    channel: d?.author?.name,
    avatar: d?.author?.avatar,
    thumb: d?.thumbnail?.url,
    badge: ico('play', 'c-bili-badge'),
    brand: 'bilibili',
    duration: e?.duration_seconds ? duration(e.duration_seconds) : undefined,
    stats,
    iframe: src,
  })
}

/** Spotify、Vimeo、Figma、Loom：各自的官方 iframe 地址 */
function officialIframe(provider: string, url: string): { src: string; ratio?: string; height?: number } | null {
  const s = sniff(url)
  try {
    const u = new URL(url)
    if (provider === 'spotify') {
      const m = /^\/(?:intl-\w+\/)?(track|album|playlist|episode|show|artist)\/(\w+)/.exec(u.pathname)
      if (m) return { src: `https://open.spotify.com/embed/${m[1]}/${m[2]}`, height: m[1] === 'track' || m[1] === 'episode' ? 152 : 352 }
    }
    if (provider === 'vimeo' && s.id) return { src: `https://player.vimeo.com/video/${s.id}?autoplay=1`, ratio: '16 / 9' }
    if (provider === 'figma') return { src: `https://www.figma.com/embed?embed_host=oad&url=${encodeURIComponent(url)}`, ratio: '16 / 10' }
    if (provider === 'loom' && s.id) return { src: `https://www.loom.com/embed/${s.id}?autoplay=1`, ratio: '16 / 9' }
  } catch {
    /* 坏网址 */
  }
  return null
}

function iframeFacade(d: Unfurl | null, url: string, frame: { src?: string; srcdoc?: string; ratio?: string; height?: number; sandbox?: string }): HTMLElement {
  const box = h('div', { class: 'c-frame-box', style: frame.height ? `height:${frame.height}px` : `aspect-ratio:${frame.ratio ?? '16 / 9'}` })
  const cover = h(
    'button',
    { class: 'c-frame-cover', type: 'button' },
    d?.thumbnail?.url ? img(d.thumbnail.url, 'c-frame-thumb') : null,
    h('span', { class: 'c-frame-label' }, favicon(d?.favicon), h('b', null, d?.title || host(url)), h('span', null, t('clickToLoad'))),
  )
  cover.addEventListener('click', (e) => {
    e.preventDefault()
    const f = h('iframe', {
      class: 'c-frame',
      src: frame.src,
      srcdoc: frame.srcdoc,
      sandbox: frame.sandbox,
      allow: 'autoplay; encrypted-media; fullscreen; picture-in-picture; clipboard-write',
      allowfullscreen: true,
      loading: 'lazy',
      frameborder: '0',
    })
    cover.replaceWith(f)
  })
  box.append(cover)
  return h('div', { class: 'c-card c-embed-frame' }, box, h('div', { class: 'c-video-foot' }, favicon(d?.favicon), h('span', { class: 'c-video-brand' }, d?.site_name || host(url)), ext(url, 'c-open', ico('external'), t('openOriginal'))))
}

// ---------- X 帖子 ----------
function xText(p: Omit<XPost, 'type'>): DocumentFragment {
  const frag = document.createDocumentFragment()
  const urls = new Map((p.urls ?? []).map((u) => [u.url, u]))
  // 正文末尾指向配图的 t.co 链接去掉
  const text = p.text.replace(/\s*https:\/\/t\.co\/\w+\s*$/g, (m) => (urls.has(m.trim()) ? m : ''))
  const re = /(https?:\/\/t\.co\/\w+)|(@\w{1,15})|(#[\p{L}\p{N}_]+)|(https?:\/\/[^\s<>"']+)/gu
  let last = 0
  for (const m of text.matchAll(re)) {
    if (m.index! > last) frag.append(text.slice(last, m.index))
    if (m[1]) {
      const u = urls.get(m[1])
      if (u) frag.append(ext(u.expanded_url, 'c-x-link', u.display_url))
    } else if (m[2]) frag.append(ext(`https://x.com/${m[2].slice(1)}`, 'c-x-link', m[2]))
    else if (m[3]) frag.append(ext(`https://x.com/hashtag/${encodeURIComponent(m[3].slice(1))}`, 'c-x-link', m[3]))
    else if (m[4]) frag.append(ext(m[4], 'c-x-link', m[4].replace(/^https?:\/\/(www\.)?/, '').replace(/^(.{28}).+$/, '$1…')))
    last = m.index! + m[0].length
  }
  frag.append(text.slice(last))
  return frag
}

function xMedia(list: UMedia[], handles: CardHandle[]): HTMLElement | null {
  if (!list.length) return null
  const n = Math.min(list.length, 4)
  const grid = h('div', { class: `c-x-media n${n}` })
  const one = n === 1 ? list[0] : null
  if (one?.width && one.height) grid.style.aspectRatio = `${Math.max(0.75, Math.min(2, one.width / one.height))}`
  for (const m of list.slice(0, 4)) {
    if (m.kind === 'image') {
      const a = ext(m.url.replace(/(\?|&)name=\w+/, '$1name=orig'), 'c-x-cell', img(m.url, '', m.alt ?? ''))
      grid.append(a)
    } else {
      const cell = h('button', { class: 'c-x-cell c-x-video', type: 'button', 'aria-label': t('playVideo') }, img(m.poster, ''), h('span', { class: 'c-play c-play-x' }, ico('play')), m.kind === 'gif' ? h('span', { class: 'c-x-gif' }, 'GIF') : m.duration_ms ? h('span', { class: 'c-video-duration' }, duration(m.duration_ms / 1000)) : null)
      let v: HTMLVideoElement | null = null
      cell.addEventListener('click', (e) => {
        e.preventDefault()
        v = h('video', { src: m.url, poster: m.poster, controls: true, autoplay: true, playsinline: true, loop: m.kind === 'gif', muted: m.kind === 'gif' }) as HTMLVideoElement
        v.setAttribute('referrerpolicy', 'no-referrer')
        const wrap = h('div', { class: 'c-x-cell' }, v)
        cell.replaceWith(wrap)
      })
      handles.push({ pause: () => v?.pause() })
      grid.append(cell)
    }
  }
  return grid
}

function xHeader(p: Omit<XPost, 'type'>, small = false) {
  const a = p.author
  return h(
    'div',
    { class: 'c-x-head' + (small ? ' is-small' : '') },
    img(a.avatar, 'c-x-avatar'),
    h('span', { class: 'c-x-names' }, h('span', { class: 'c-x-name' }, a.name ?? a.handle ?? '', p.author_verified ? ico('verified', 'c-x-verified') : null), h('span', { class: 'c-x-handle' }, a.handle ? '@' + a.handle : '')),
  )
}

function xPost(d: Unfurl | null, url: string): [HTMLElement, CardHandle] {
  const p = d?.embed?.type === 'x' ? d.embed : null
  if (!p) {
    // 没拿到帖子内容：按网址画个能点的卡片
    const s = sniff(url)
    return [ext(url, 'c-card c-x c-x-empty', h('div', { class: 'c-x-head' }, h('span', { class: 'c-x-avatar is-empty' }), h('span', { class: 'c-x-names' }, h('span', { class: 'c-x-name' }, d?.author?.name ?? (s.extra?.handle ? '@' + s.extra.handle : 'X')), h('span', { class: 'c-x-handle' }, t('xUnavailable')))), ico('x', 'c-x-logo'), d?.description ? h('div', { class: 'c-x-text' }, d.description) : null), {}]
  }
  const handles: CardHandle[] = []
  const postUrl = d?.canonical_url ?? url
  const body = h('div', { class: 'c-x-text', lang: p.lang ?? null }, xText(p))
  let quoted: HTMLElement | null = null
  if (p.quoted) {
    const q = p.quoted
    quoted = ext(`https://x.com/${q.author.handle ?? 'i'}/status/${q.id}`, 'c-x-quote', xHeader(q, true), h('div', { class: 'c-x-text' }, xText(q)), xMedia(q.media ?? [], handles))
  }
  let card: HTMLElement | null = null
  if (p.card?.url && !(p.media?.length)) {
    card = ext(p.card.url, 'c-x-linkcard' + (p.card.image ? '' : ' no-image'), p.card.image ? img(p.card.image, 'c-x-linkcard-img') : null, h('span', { class: 'c-x-linkcard-body' }, h('span', { class: 'c-x-linkcard-domain' }, p.card.domain ?? host(p.card.url)), h('span', { class: 'c-x-linkcard-title' }, p.card.title ?? ''), p.card.description ? h('span', { class: 'c-x-linkcard-desc' }, p.card.description) : null))
  }
  const el = h(
    'div',
    { class: 'c-card c-x' },
    h('div', { class: 'c-x-top' }, xHeader(p), ext(postUrl, 'c-x-logo-link', ico('x', 'c-x-logo'))),
    p.in_reply_to ? h('div', { class: 'c-x-reply-to' }, t('replyingTo'), ' ', ext(`https://x.com/${p.in_reply_to}`, 'c-x-link', '@' + p.in_reply_to)) : null,
    body,
    xMedia(p.media ?? [], handles),
    card,
    quoted,
    h(
      'div',
      { class: 'c-x-foot' },
      ext(postUrl, 'c-x-time', date(p.created_at, true)),
      h('span', { class: 'c-x-stat c-x-like' }, ico('heart'), count(p.like_count)),
      p.reply_count !== undefined ? h('span', { class: 'c-x-stat' }, ico('reply'), count(p.reply_count)) : null,
      ext(postUrl, 'c-x-view', t('viewOnX')),
    ),
  )
  return [el, { pause: () => handles.forEach((x) => x.pause?.()) }]
}

// ---------- GitHub ----------
function githubRepo(d: Unfurl | null, url: string) {
  const e = d?.embed?.type === 'github_repo' ? d.embed : null
  const s = sniff(url)
  const [owner, repo] = e ? [e.owner, e.repo] : (s.id ?? '/').split('/')
  const lang = e?.language
  return ext(
    d?.canonical_url ?? url,
    'c-card c-gh c-gh-repo',
    h(
      'div',
      { class: 'c-gh-main' },
      h('div', { class: 'c-gh-title' }, ico('repo', 'c-gh-icon'), h('span', { class: 'c-gh-owner' }, owner + ' / '), h('b', null, repo), e?.archived ? h('span', { class: 'c-gh-pill is-archived' }, t('archived')) : null),
      d?.description ? h('div', { class: 'c-gh-desc' }, d.description) : null,
      e?.topics?.length ? h('div', { class: 'c-gh-topics' }, ...e.topics.slice(0, 5).map((x) => h('span', { class: 'c-gh-topic' }, x))) : null,
      h(
        'div',
        { class: 'c-gh-meta' },
        lang ? h('span', { class: 'c-gh-lang' }, h('i', { style: `background:${LANG_COLORS[lang] ?? '#8b949e'}` }), lang) : null,
        e?.stars !== undefined ? h('span', null, ico('star'), count(e.stars)) : null,
        e?.forks !== undefined ? h('span', null, ico('fork'), count(e.forks)) : null,
        e?.license ? h('span', null, e.license) : null,
        e?.pushed_at ? h('span', null, t('updatedAt'), ' ', date(e.pushed_at)) : null,
      ),
    ),
    img(d?.author?.avatar, 'c-gh-avatar'),
    ico('github', 'c-gh-mark'),
  )
}

function githubIssue(d: Unfurl | null, url: string) {
  const e = d?.embed?.type === 'github_issue' ? d.embed : null
  const s = sniff(url)
  const isPr = e?.is_pull_request ?? s.extra?.kind === 'pull'
  const state = e?.state ?? 'open'
  const [k, cls, label] = isPr
    ? state === 'merged'
      ? (['merged', 'is-merged', t('merged')] as const)
      : state === 'closed'
        ? (['prClosed', 'is-closed', t('closed')] as const)
        : state === 'draft'
          ? (['pr', 'is-draft', t('draft')] as const)
          : (['pr', 'is-open', t('open')] as const)
    : state === 'closed'
      ? e?.state_reason === 'not_planned'
        ? (['issueSkip', 'is-skip', t('closed')] as const)
        : (['issueClosed', 'is-done', t('closed')] as const)
      : (['issueOpen', 'is-open', t('open')] as const)
  const owner = e?.owner ?? s.id?.split('/')[0]
  const repo = e?.repo ?? s.id?.split('/')[1]
  const num = e?.number ?? s.extra?.number
  const title = (d?.title ?? '').replace(/\s*·\s*#\d+$/, '').replace(/\s*·\s*(Issue|Pull Request) #\d+.*$/, '') || `#${num}`
  return ext(
    d?.canonical_url ?? url,
    'c-card c-gh c-gh-issue',
    h(
      'div',
      { class: 'c-gh-main' },
      h('div', { class: 'c-gh-repo-line' }, ico('github', 'c-gh-mini'), `${owner}/${repo}`, ' ', h('span', { class: 'c-gh-num' }, '#' + num)),
      h('div', { class: 'c-gh-issue-title' }, h('span', { class: 'c-gh-state-ico ' + cls }, ico(k)), h('b', null, title)),
      h(
        'div',
        { class: 'c-gh-meta' },
        h('span', { class: 'c-gh-pill ' + cls }, ico(k), label),
        d?.author?.name ? h('span', { class: 'c-gh-by' }, img(d.author.avatar, 'c-gh-by-avatar'), d.author.name) : null,
        e?.comments ? h('span', null, ico('comment'), String(e.comments)) : null,
        e?.additions !== undefined ? h('span', { class: 'c-gh-diff' }, h('i', { class: 'add' }, '+' + count(e.additions)), ' ', h('i', { class: 'del' }, '−' + count(e.deletions))) : null,
        d?.published_at ? h('span', null, date(d.published_at)) : null,
      ),
      e?.labels?.length ? h('div', { class: 'c-gh-labels' }, ...e.labels.slice(0, 6).map((l) => h('span', { class: 'c-gh-label', style: l.color ? `--l:#${l.color}` : null }, l.name))) : null,
    ),
  )
}

// ---------- 文件（上传的、或者网址本身就是文件） ----------
function fileCard(d: Unfurl | null, url: string): [HTMLElement, CardHandle] {
  const name = d?.name ?? d?.title ?? fileName(url)
  const mime = d?.mime ?? ''
  const extName = (name.split('.').pop() || '').toLowerCase()
  const isVideo = mime.startsWith('video/') || /^(mp4|webm|mov|m4v)$/.test(extName)
  const isAudio = mime.startsWith('audio/') || /^(mp3|m4a|wav|ogg|flac|aac)$/.test(extName)
  const isPdf = mime === 'application/pdf' || extName === 'pdf'
  const meta = [bytes(d?.size), d?.duration_ms ? duration(d.duration_ms / 1000) : ''].filter(Boolean).join(' · ')
  if (isVideo) {
    const v = h('video', { class: 'c-file-video', src: url, controls: true, preload: 'metadata', playsinline: true, poster: d?.thumbnail?.url }) as HTMLVideoElement
    if (d?.width && d.height) v.style.aspectRatio = `${d.width} / ${d.height}`
    return [h('div', { class: 'c-card c-file c-file-media' }, v, h('div', { class: 'c-file-row' }, h('span', { class: 'c-file-name' }, name), h('span', { class: 'c-file-meta' }, meta), ext(url, 'c-open', ico('download'), t('download')))), { pause: () => v.pause() }]
  }
  if (isAudio) {
    const a = h('audio', { class: 'c-file-audio', src: url, controls: true, preload: 'metadata' }) as HTMLAudioElement
    return [h('div', { class: 'c-card c-file c-file-audio-card' }, h('div', { class: 'c-file-row' }, h('span', { class: 'c-file-badge is-audio' }, '♪'), h('span', { class: 'c-file-name' }, name), h('span', { class: 'c-file-meta' }, meta)), a), { pause: () => a.pause() }]
  }
  const row = h(
    'div',
    { class: 'c-file-row' },
    h('span', { class: 'c-file-badge' + (isPdf ? ' is-pdf' : '') }, isPdf ? 'PDF' : (extName.slice(0, 4).toUpperCase() || ico('file'))),
    h('span', { class: 'c-file-text' }, h('span', { class: 'c-file-name' }, name), h('span', { class: 'c-file-meta' }, meta || mime)),
  )
  const card = h('div', { class: 'c-card c-file' }, row)
  if (isPdf) {
    const btn = h('button', { class: 'c-btn', type: 'button' }, t('preview'))
    let open = false
    let frame: HTMLIFrameElement | null = null
    btn.addEventListener('click', () => {
      open = !open
      btn.textContent = open ? t('collapsePreview') : t('preview')
      if (open) {
        frame ??= h('iframe', { class: 'c-pdf', src: url, title: name }) as HTMLIFrameElement
        card.append(frame)
      } else frame?.remove()
    })
    row.append(btn)
  }
  row.append(h('a', { class: 'c-btn', href: url, download: name }, ico('download'), t('download')))
  return [card, {}]
}

function imageCard(d: Unfurl | null, url: string) {
  const src = d?.thumbnail?.url ?? url
  return ext(url, 'c-card c-image', img(src, '', d?.title ?? '', srcset(src) ? { srcset: srcset(src)!, sizes: '(max-width: 760px) 100vw, 720px' } : {}))
}

// ---------- 入口 ----------
export function render(box: HTMLElement, d: Unfurl | null, o: CardOptions): CardHandle {
  let el: HTMLElement
  let handle: CardHandle = {}
  const s = sniff(o.url)
  const provider = d?.provider && d.provider !== 'generic' ? d.provider : s.provider
  if (o.view === 'bookmark') el = bookmark(d, o.url)
  else if (d?.kind === 'file' || (!d && isFileUrl(o.url))) [el, handle] = fileCard(d, o.url)
  else if (d?.kind === 'image') el = imageCard(d, o.url)
  else if (provider === 'youtube') [el, handle] = youtube(d, o.url)
  else if (provider === 'bilibili') [el, handle] = bilibili(d, o.url)
  else if (provider === 'x') [el, handle] = xPost(d, o.url)
  else if (provider === 'github' && (d?.embed?.type === 'github_issue' || ['issues', 'pull'].includes(s.extra?.kind ?? ''))) el = githubIssue(d, o.url)
  else if (provider === 'github') el = githubRepo(d, o.url)
  else if (['spotify', 'vimeo', 'figma', 'loom'].includes(s.provider) && officialIframe(s.provider, o.url)) el = iframeFacade(d, o.url, officialIframe(s.provider, o.url)!)
  else if (d?.embed?.type === 'oembed' && d.embed.html && (d.embed.oembed_type === 'rich' || d.embed.oembed_type === 'video')) {
    // 别家的 oEmbed HTML 不可信：只放进 sandbox iframe，不给 allow-same-origin
    const w = d.embed.width ?? 16
    const hgt = d.embed.height ?? 9
    el = iframeFacade(d, o.url, { srcdoc: `<!doctype html><meta charset="utf-8"><style>html,body{margin:0}</style>${d.embed.html}`, sandbox: 'allow-scripts allow-popups allow-presentation', ratio: `${w} / ${hgt}` })
  } else el = bookmark(d, o.url)
  box.replaceChildren(el)
  return handle
}
