// Service worker：离线也能打开看过的页面。
// - 打包产物（/assets/，文件名带哈希）先读缓存；
// - 页面（导航）先问网络，断网用缓存的 index.html（文档内容本来就在 IndexedDB 里）；
// - 读接口（/me、页面列表、页面信息、库的行……）先问网络，断网用上次的结果；
// - 文件、缩略图先读缓存。
// 改了缓存的做法就改 VERSION，旧的缓存会被清掉。
const VERSION = 'v1'
const SHELL = 'oad-shell-' + VERSION
const ASSETS = 'oad-assets-' + VERSION
const API = 'oad-api-' + VERSION
const MEDIA = 'oad-media-' + VERSION
const KEEP = [SHELL, ASSETS, API, MEDIA]
const MAX_API = 300
const MAX_MEDIA = 400

self.addEventListener('install', (e) => {
  e.waitUntil(
    (async () => {
      const c = await caches.open(SHELL)
      await c.addAll(['/', '/manifest.webmanifest', '/icon.svg', '/icon-192.png']).catch(() => {})
      // 首页引用的主包、样式也先存上
      try {
        const html = await (await fetch('/', { cache: 'no-store' })).text()
        const urls = [...html.matchAll(/(?:src|href)="(\/assets\/[^"]+)"/g)].map((m) => m[1])
        const a = await caches.open(ASSETS)
        await a.addAll(urls)
      } catch {
        /* 下次再说 */
      }
      await self.skipWaiting()
    })(),
  )
})

self.addEventListener('activate', (e) => {
  e.waitUntil(
    (async () => {
      for (const k of await caches.keys()) if (k.startsWith('oad-') && !KEEP.includes(k)) await caches.delete(k)
      await self.clients.claim()
    })(),
  )
})

async function trim(name, max) {
  const c = await caches.open(name)
  const keys = await c.keys()
  for (let i = 0; i < keys.length - max; i++) await c.delete(keys[i])
}

async function cacheFirst(req, name, max) {
  const c = await caches.open(name)
  const hit = await c.match(req)
  if (hit) return hit
  const res = await fetch(req)
  if (res.ok && res.type === 'basic') {
    c.put(req, res.clone())
    if (max) trim(name, max)
  }
  return res
}

async function networkFirst(req, name, fallbackUrl, max) {
  const c = await caches.open(name)
  try {
    const res = await fetch(req)
    if (res.ok) {
      c.put(fallbackUrl ?? req, res.clone())
      if (max) trim(name, max)
    }
    return res
  } catch (err) {
    const hit = (await c.match(fallbackUrl ?? req)) || (fallbackUrl ? null : await c.match(req, { ignoreSearch: false }))
    if (hit) return hit
    throw err
  }
}

// 只缓存读的接口；实时的（事件流、长轮询）、写的、搜索不缓存
const API_OK = [
  /^\/api\/v1\/me$/,
  /^\/api\/v1\/auth\/config$/,
  /^\/api\/v1\/workspaces\/[^/]+\/(pages|members|agents)$/,
  /^\/api\/v1\/pages\/[^/]+$/,
  /^\/api\/v1\/pages\/[^/]+\/(comments|authors)$/,
  /^\/api\/v1\/databases\/[^/]+$/,
  /^\/api\/v1\/unfurl$/,
]

self.addEventListener('fetch', (e) => {
  const req = e.request
  if (req.method !== 'GET') return
  const url = new URL(req.url)
  if (url.origin !== self.location.origin) return
  const p = url.pathname
  if (p.startsWith('/assets/')) return e.respondWith(cacheFirst(req, ASSETS))
  if (/^\/api\/v1\/(files|media)\//.test(p)) return e.respondWith(cacheFirst(req, MEDIA, MAX_MEDIA))
  if (p.startsWith('/api/')) {
    if (API_OK.some((r) => r.test(p))) e.respondWith(networkFirst(req, API, null, MAX_API))
    return
  }
  // 公开分享页是服务器出的，不归这里
  if (req.mode === 'navigate' && !p.startsWith('/s/')) return e.respondWith(networkFirst(req, SHELL, '/'))
  if (/\.(png|svg|webmanifest|ico)$/.test(p)) return e.respondWith(cacheFirst(req, SHELL))
})
