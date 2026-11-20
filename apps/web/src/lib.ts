import { fmtAgo } from './i18n'
// 零碎的工具：路由、主题、用户颜色。
import { useSyncExternalStore } from 'react'

// ---------- 路由：/ 和 /p/<页面编号> ----------
const routeListeners = new Set<() => void>()
const notify = () => routeListeners.forEach((l) => l())
window.addEventListener('popstate', notify)

export function navigate(path: string, replace = false) {
  if (location.pathname + location.search === path) return
  if (replace) history.replaceState(null, '', path)
  else history.pushState(null, '', path)
  notify()
}

export function usePath() {
  return useSyncExternalStore(
    (l) => {
      routeListeners.add(l)
      return () => routeListeners.delete(l)
    },
    () => location.pathname + location.search,
  )
}

export const pagePath = (id: string) => `/p/${id}`
export const pageIdOf = (path: string) => /^\/p\/([^/?#]+)/.exec(path)?.[1] ?? null
export const threadOf = (path: string) => /[?&]thread=([^&#]+)/.exec(path)?.[1] ?? null
export const threadPath = (page: string, thread: string) => `/p/${page}?thread=${thread}`

// ---------- 主题 ----------
export type Theme = 'system' | 'light' | 'dark'
const THEME_KEY = 'oad:theme'

export function getTheme(): Theme {
  try {
    const v = localStorage.getItem(THEME_KEY)
    return v === 'light' || v === 'dark' ? v : 'system'
  } catch {
    return 'system'
  }
}

const themeListeners = new Set<() => void>()
export function useTheme(): Theme {
  return useSyncExternalStore(
    (l) => {
      themeListeners.add(l)
      return () => themeListeners.delete(l)
    },
    getTheme,
  )
}
/** 现在看起来是不是深色 */
export const isDark = () => {
  const t = getTheme()
  return t === 'dark' || (t === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
}
/** ⌘⇧L：浅色、深色来回换 */
export const toggleTheme = () => applyTheme(isDark() ? 'light' : 'dark')

export function applyTheme(t: Theme) {
  queueMicrotask(() => themeListeners.forEach((l) => l()))
  const root = document.documentElement
  if (t === 'system') delete root.dataset.theme
  else root.dataset.theme = t
  try {
    if (t === 'system') localStorage.removeItem(THEME_KEY)
    else localStorage.setItem(THEME_KEY, t)
  } catch {
    /* 忽略 */
  }
}

// ---------- 用户颜色：按编号固定选一个 ----------
const COLORS = ['#e5484d', '#f76b15', '#d6a000', '#30a46c', '#12a594', '#0090ff', '#3e63dd', '#8e4ec6', '#d6409f', '#7c6f64']

export function colorFor(id: string): string {
  let h = 2166136261
  for (let i = 0; i < id.length; i++) h = Math.imul(h ^ id.charCodeAt(i), 16777619)
  return COLORS[(h >>> 0) % COLORS.length]
}

export function initials(name: string): string {
  const s = name.trim()
  if (!s) return '?'
  // 中文名取第一个字，英文取首字母
  const first = [...s][0]
  if (/[㐀-鿿]/.test(first)) return first
  const parts = s.split(/\s+/)
  return (parts[0][0] + (parts[1]?.[0] ?? '')).toUpperCase()
}

export function storageGet<T>(key: string): T | null {
  try {
    const v = localStorage.getItem(key)
    return v ? (JSON.parse(v) as T) : null
  } catch {
    return null
  }
}

export function storageSet(key: string, v: unknown) {
  try {
    if (v === null) localStorage.removeItem(key)
    else localStorage.setItem(key, JSON.stringify(v))
  } catch {
    /* 忽略 */
  }
}

// 退出登录（或登录过期）时清掉本机缓存的页面内容：页面列表、首屏快照、IndexedDB 里的正文。主题这类设置留着
export async function clearLocalData() {
  try {
    for (const k of Object.keys(localStorage)) if (k.startsWith('oad:') && k !== 'oad:theme') localStorage.removeItem(k)
  } catch {
    /* 忽略 */
  }
  try {
    const dbs = await indexedDB.databases()
    await Promise.all(
      dbs
        .filter((d) => d.name?.startsWith('oad:page:'))
        .map(
          (d) =>
            new Promise<void>((resolve) => {
              const r = indexedDB.deleteDatabase(d.name!)
              r.onsuccess = r.onerror = r.onblocked = () => resolve()
            }),
        ),
    )
  } catch {
    /* 老浏览器没有 databases() */
  }
}

// 多久以前（跟着界面语言）
export const ago = fmtAgo

export async function copyText(s: string) {
  try {
    await navigator.clipboard.writeText(s)
    return true
  } catch {
    const ta = document.createElement('textarea')
    ta.value = s
    document.body.append(ta)
    ta.select()
    const ok = document.execCommand('copy')
    ta.remove()
    return ok
  }
}

// ---------- 最近看过的页面（⌘K 里列） ----------
const RECENT_KEY = 'oad:recent'
export function recentPages(): string[] {
  return storageGet<string[]>(RECENT_KEY) ?? []
}
export function pushRecent(id: string) {
  const list = recentPages().filter((x) => x !== id)
  list.unshift(id)
  storageSet(RECENT_KEY, list.slice(0, 20))
}

/** 上次打开的页面：打开首页时回到它 */
export const LAST_PAGE_KEY = 'oad:last-page'

/** 页面删了 / 不存在了：从最近看过、上次打开里去掉。不然首页会一直跳回这个打不开的页面 */
export function forgetPages(ids: string[]) {
  const gone = new Set(ids)
  const list = recentPages()
  if (list.some((x) => gone.has(x))) storageSet(RECENT_KEY, list.filter((x) => !gone.has(x)))
  const last = storageGet<string>(LAST_PAGE_KEY)
  if (last && gone.has(last)) storageSet(LAST_PAGE_KEY, null)
}

// ---------- 全局命令：快捷键、⌘K 面板发出来，页面、侧栏接 ----------
export type Command = 'palette' | 'shortcuts' | 'history' | 'share' | 'sidebar' | 'newPage' | 'newDatabase'
export function runCommand(c: Command) {
  window.dispatchEvent(new CustomEvent<Command>('oad:cmd', { detail: c }))
}
export function onCommand(f: (c: Command) => void) {
  const h = (e: Event) => f((e as CustomEvent<Command>).detail)
  window.addEventListener('oad:cmd', h)
  return () => window.removeEventListener('oad:cmd', h)
}
