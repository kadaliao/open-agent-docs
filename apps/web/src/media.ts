// 编辑器用到的外部服务：链接解析（GET /api/v1/unfurl，结果在内存和 IndexedDB 里各存一份，离线、再打开时直接画）、
// 上传（POST /api/v1/workspaces/{ws}/files，带进度）、Markdown 解析（WASM 版 oad-markdown，按需加载）。
import type { MediaServices, Unfurl, Uploaded } from '@oad/editor'
import { API } from './api'

const mem = new Map<string, Unfurl>()
const inflight = new Map<string, Promise<Unfurl | null>>()
const DAY = 86400_000

// ---------- IndexedDB ----------
let dbp: Promise<IDBDatabase | null> | null = null
function db() {
  dbp ??= new Promise((resolve) => {
    try {
      const r = indexedDB.open('oad:unfurl', 1)
      r.onupgradeneeded = () => r.result.createObjectStore('u')
      r.onsuccess = () => resolve(r.result)
      r.onerror = () => resolve(null)
    } catch {
      resolve(null)
    }
  })
  return dbp
}
async function idbGet(url: string): Promise<{ d: Unfurl; at: number } | null> {
  const d = await db()
  if (!d) return null
  return new Promise((resolve) => {
    const r = d.transaction('u').objectStore('u').get(url)
    r.onsuccess = () => resolve(r.result ?? null)
    r.onerror = () => resolve(null)
  })
}
async function idbPut(url: string, v: Unfurl) {
  const d = await db()
  if (!d) return
  try {
    d.transaction('u', 'readwrite').objectStore('u').put({ d: v, at: Date.now() }, url)
  } catch {
    /* 存不下就算了 */
  }
}

async function fetchUnfurl(url: string): Promise<Unfurl | null> {
  const r = await fetch(`${API}/unfurl?url=${encodeURIComponent(url)}`, { credentials: 'same-origin' })
  if (!r.ok) return null
  const d = (await r.json()) as Unfurl
  mem.set(url, d)
  idbPut(url, d)
  return d
}

export function unfurl(url: string): Promise<Unfurl | null> {
  const hit = mem.get(url)
  if (hit) return Promise.resolve(hit)
  let p = inflight.get(url)
  if (p) return p
  p = (async () => {
    const cached = await idbGet(url)
    if (cached) {
      mem.set(url, cached.d)
      // 旧了就后台刷新，先用旧的画
      if (Date.now() - cached.at > DAY || cached.d.stale) fetchUnfurl(url).catch(() => {})
      return cached.d
    }
    try {
      return await fetchUnfurl(url)
    } catch {
      return null
    }
  })().finally(() => inflight.delete(url))
  inflight.set(url, p)
  return p
}

// ---------- 上传 ----------
export function uploader(ws: string) {
  return (file: File, onProgress: (f: number) => void) =>
    new Promise<Uploaded>((resolve, reject) => {
      const xhr = new XMLHttpRequest()
      xhr.open('POST', `${API}/workspaces/${ws}/files`)
      xhr.withCredentials = true
      xhr.upload.onprogress = (e) => e.lengthComputable && onProgress(e.loaded / e.total)
      xhr.onload = () => {
        if (xhr.status >= 200 && xhr.status < 300) {
          try {
            resolve(JSON.parse(xhr.responseText))
          } catch (e) {
            reject(e)
          }
        } else reject(new Error(`HTTP ${xhr.status}`))
      }
      xhr.onerror = () => reject(new Error('network'))
      const fd = new FormData()
      fd.append('file', file, file.name || 'file')
      xhr.send(fd)
    })
}

// ---------- Markdown（WASM） ----------
type MdModule = { default?: unknown; parse?: (md: string) => unknown; available?: boolean }
let md: Promise<MdModule | null> | null = null
export async function parseMarkdown(text: string) {
  md ??= import('@oad/markdown').then(
    async (m: MdModule) => {
      if (m.available === false) return null
      // wasm-bindgen 打的包要先 init
      if (typeof m.default === 'function') await (m.default as () => Promise<unknown>)()
      return m
    },
    () => null,
  )
  const m = await md
  if (!m?.parse) return null
  const out = m.parse(text)
  const doc = typeof out === 'string' ? JSON.parse(out) : out
  return doc && typeof doc === 'object' ? (doc as { type: 'doc'; content?: unknown[] }) : null
}

export function mediaServices(ws: string): Partial<MediaServices> {
  return {
    unfurl,
    peek: (url) => mem.get(url) ?? null,
    upload: uploader(ws),
    remember: (url, d) => {
      mem.set(url, d)
      idbPut(url, d)
    },
    parseMarkdown,
    pageUrl: () => location.origin + location.pathname,
  }
}
