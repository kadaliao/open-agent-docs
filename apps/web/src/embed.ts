// iframe 嵌入模式（地址带 ?embed=1）：没有侧栏和顶栏，只有正文；跟宿主页面用 postMessage 说话。
//   网址参数：embed=1，theme=light|dark（不给跟系统），lang=zh|en（不给跟浏览器 / 账号）
//   发给宿主：{ type: 'oad:ready', pageId }、{ type: 'oad:navigate', pageId, title }、{ type: 'oad:height', height }
//   宿主发来：{ type: 'oad:navigate', pageId }、{ type: 'oad:theme', theme }、{ type: 'oad:locale', locale }
//   跨域登录（#token=、oad:token、oad:auth-required）在 token.ts
import { setLocale } from './i18n'
import { applyTheme, navigate, pagePath } from './lib'

const q = new URLSearchParams(location.search)
/** 这次打开是不是嵌在 iframe 里（一开始的地址带 embed=1，之后在里面跳转也算） */
export const EMBED = q.has('embed') && q.get('embed') !== '0'

export function post(msg: Record<string, unknown>) {
  if (!EMBED || window.parent === window) return
  window.parent.postMessage(msg, '*')
}

if (EMBED) {
  document.documentElement.classList.add('oad-embed')
  const theme = q.get('theme')
  if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme
  const lang = q.get('lang')
  if (lang === 'zh' || lang === 'en') setLocale(lang, { persist: false })
  window.addEventListener('message', (e) => {
    const d = e.data as { type?: string; pageId?: string; theme?: string; locale?: string } | null
    if (!d || typeof d !== 'object') return
    if (d.type === 'oad:navigate' && d.pageId) navigate(pagePath(d.pageId))
    else if (d.type === 'oad:theme') {
      if (d.theme === 'light' || d.theme === 'dark') document.documentElement.dataset.theme = d.theme
      else applyTheme('system')
    } else if (d.type === 'oad:locale' && (d.locale === 'zh' || d.locale === 'en')) setLocale(d.locale, { persist: false })
  })
  // 内容高度变了告诉宿主（宿主想让 iframe 跟着内容长高时用）
  let last = 0
  const ro = new ResizeObserver(() => {
    const el = document.querySelector('.doc, .db-page') as HTMLElement | null
    const h = el ? Math.ceil(el.getBoundingClientRect().height) : document.documentElement.scrollHeight
    if (Math.abs(h - last) > 2) {
      last = h
      post({ type: 'oad:height', height: h })
    }
  })
  const watch = () => {
    const el = document.querySelector('.doc, .db-page')
    if (el) ro.observe(el)
    else setTimeout(watch, 200)
  }
  watch()
}
