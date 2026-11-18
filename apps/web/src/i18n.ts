// 界面语言：中文 / 英文。默认跟浏览器；设置里选了就记在本机（localStorage）和账号上（PATCH /me { locale }）。
// 换语言不刷新：各处的 t() 读当前语言，订阅了 useLocale() 的组件（App 根上就有一个）重画。
import { useSyncExternalStore } from 'react'

export type Locale = 'zh' | 'en'
export type LocalePref = Locale | 'auto'
const KEY = 'oad:locale'

function browserLocale(): Locale {
  const langs = typeof navigator !== 'undefined' ? (navigator.languages?.length ? navigator.languages : [navigator.language]) : []
  for (const l of langs) {
    const x = (l || '').toLowerCase()
    if (x.startsWith('zh')) return 'zh'
    if (x.startsWith('en')) return 'en'
  }
  return 'en'
}

function readPref(): LocalePref {
  try {
    const v = localStorage.getItem(KEY)
    return v === 'zh' || v === 'en' ? v : 'auto'
  } catch {
    return 'auto'
  }
}

let pref: LocalePref = readPref()
let current: Locale = pref === 'auto' ? browserLocale() : pref
let version = 0
const listeners = new Set<() => void>()

function apply() {
  if (typeof document !== 'undefined') document.documentElement.lang = current === 'zh' ? 'zh-CN' : 'en'
}
apply()

export const getLocale = () => current
export const getLocalePref = () => pref

/** 换语言（'auto' 是跟浏览器）。返回换完的语言 */
export function setLocale(p: LocalePref): Locale {
  pref = p
  try {
    if (p === 'auto') localStorage.removeItem(KEY)
    else localStorage.setItem(KEY, p)
  } catch {
    /* 存不了就只在这次有效 */
  }
  const next = p === 'auto' ? browserLocale() : p
  if (next !== current) {
    current = next
    apply()
    version++
    listeners.forEach((l) => l())
  }
  return current
}

/** 账号上存的语言（登录后拿到）：本机没选过才用它 */
export function adoptAccountLocale(l: string | null | undefined) {
  if (pref === 'auto' && (l === 'zh' || l === 'en')) setLocale(l)
}

export function onLocale(l: () => void) {
  listeners.add(l)
  return () => listeners.delete(l)
}

/** 组件里用：语言换了就重画 */
export function useLocale(): Locale {
  useSyncExternalStore(onLocale, () => version)
  return current
}

/** 每种语言一张表，t 按当前语言取；英文表缺的键退回中文 */
export function makeT<T extends Record<string, string>>(tables: { zh: T; en: Partial<T> }) {
  return (k: keyof T, vars?: Record<string, string | number>): string => {
    let s: string = (current === 'en' ? tables.en[k] : undefined) ?? tables.zh[k] ?? String(k)
    if (vars) for (const [key, v] of Object.entries(vars)) s = s.split(`{${key}}`).join(String(v))
    return s
  }
}

const tag = () => (current === 'zh' ? 'zh-CN' : 'en-US')

/** 日期（年月日），给人看 */
export function fmtDate(ms: number | string | null | undefined, withTime = false): string {
  if (ms === null || ms === undefined || ms === '') return ''
  const d = new Date(ms)
  if (isNaN(d.getTime())) return ''
  return d.toLocaleString(tag(), withTime ? { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' } : { year: 'numeric', month: 'short', day: 'numeric' })
}

/** 多久以前 */
export function fmtAgo(ms: number | null | undefined): string {
  if (!ms) return ''
  const s = Math.round((Date.now() - ms) / 1000)
  const rtf = new Intl.RelativeTimeFormat(tag(), { numeric: 'auto' })
  if (s < 45) return current === 'zh' ? '刚刚' : 'just now'
  if (s < 3600) return rtf.format(-Math.round(s / 60), 'minute')
  if (s < 86400) return rtf.format(-Math.round(s / 3600), 'hour')
  if (s < 86400 * 7) return rtf.format(-Math.round(s / 86400), 'day')
  return fmtDate(ms)
}
