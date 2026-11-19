// 一页的协作文档：Y.Doc + 本机缓存（浏览器里的 IndexedDB）+ WebSocket 同步（y-websocket，地址 /api/v1/sync/{page}）。
// 网页（apps/web）用的就是这一份。
import { IndexeddbPersistence } from 'y-indexeddb'
import { WebsocketProvider } from 'y-websocket'
import * as Y from 'yjs'
import { registerPlugin } from './core.ts'

export type PageStatus = 'offline' | 'connecting' | 'syncing' | 'synced' | 'denied'

export interface OpenPageOptions {
  /** 本机缓存：浏览器里默认开（IndexedDB 名字 oad:page:<id>），Node 里没有 */
  cache?: boolean
  /** 本机没有、服务器也连不上时最多等多久就算“空”（毫秒） */
  readyTimeout?: number
  /** 用已有的 Y.Doc */
  ydoc?: Y.Doc
}

export interface PageConn {
  id: string
  ydoc: Y.Doc
  provider: WebsocketProvider
  awareness: WebsocketProvider['awareness']
  idb: IndexeddbPersistence | null
  /** 'cache'：本机缓存先出来了；'server'：等到了服务器的内容；'empty'：都没有 */
  ready: Promise<'cache' | 'server' | 'empty'>
  status: () => PageStatus
  onStatus: (l: () => void) => () => void
  destroy: () => void
}

export function openPage(pageId: string, o: OpenPageOptions & { url: string; token?: string; WebSocket?: unknown }): PageConn {
  const ydoc = o.ydoc ?? new Y.Doc()
  const hasIdb = typeof indexedDB !== 'undefined'
  const idb = (o.cache ?? hasIdb) && hasIdb ? new IndexeddbPersistence('oad:page:' + pageId, ydoc) : null
  // 不用 BroadcastChannel 在同源标签页之间同步：连上时它要把整份文档编码一遍发出去，改动也都经服务器转发了
  const provider = new WebsocketProvider(o.url, pageId, ydoc, {
    maxBackoffTime: 4000,
    disableBc: true,
    params: o.token ? { token: o.token } : {},
    ...(o.WebSocket ? { WebSocketPolyfill: o.WebSocket as typeof WebSocket } : {}),
  })

  let denied = false
  // 服务器拒绝（没有权限）时 y-websocket 只在控制台打一行，这里接过来
  const defaultAuth = provider.messageHandlers[2]
  provider.messageHandlers[2] = (...args) => {
    denied = true
    emit()
    return defaultAuth(...args)
  }

  const listeners = new Set<() => void>()
  const emit = () => listeners.forEach((l) => l())
  const online = () => typeof navigator === 'undefined' || navigator.onLine !== false
  const status = (): PageStatus => {
    if (denied) return 'denied'
    if (!online() && !provider.wsconnected) return 'offline'
    if (!provider.wsconnected) return provider.wsconnecting ? 'connecting' : 'offline'
    return provider.synced ? 'synced' : 'syncing'
  }
  provider.on('status', emit)
  provider.on('sync', emit)
  const hasWin = typeof window !== 'undefined' && typeof window.addEventListener === 'function'
  if (hasWin) {
    window.addEventListener('online', emit)
    window.addEventListener('offline', emit)
  }

  const frag = ydoc.getXmlFragment('default')
  const ready = new Promise<'cache' | 'server' | 'empty'>((resolve) => {
    let done = false
    const finish = (v: 'cache' | 'server' | 'empty') => {
      if (!done) {
        done = true
        resolve(v)
      }
    }
    idb?.whenSynced.then(() => {
      if (typeof performance !== 'undefined') performance.mark?.('oad:idb')
      if (frag.length > 0) finish('cache')
    })
    // 本机没有：等服务器第一次同步完；连不上就别一直等
    const onSync = (s: boolean) => s && finish(frag.length > 0 || ydoc.getText('title').length > 0 ? 'server' : 'empty')
    provider.on('sync', onSync)
    setTimeout(() => finish(frag.length > 0 ? 'cache' : 'empty'), o.readyTimeout ?? 2500)
  })

  return {
    id: pageId,
    ydoc,
    provider,
    awareness: provider.awareness,
    idb,
    ready,
    status,
    onStatus: (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    destroy: () => {
      if (hasWin) {
        window.removeEventListener('online', emit)
        window.removeEventListener('offline', emit)
      }
      provider.destroy()
      idb?.destroy()
      if (!o.ydoc) ydoc.destroy()
    },
  }
}

registerPlugin('openPage', openPage)
