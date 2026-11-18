// 一页的连接：Y.Doc + 本机缓存（IndexedDB）+ WebSocket 同步。单独打包，比编辑器先下载、先连上。
import { IndexeddbPersistence } from 'y-indexeddb'
import { WebsocketProvider } from 'y-websocket'
import * as Y from 'yjs'
import { syncUrl } from './api'

export type Status = 'offline' | 'connecting' | 'syncing' | 'synced' | 'denied'

export interface Conn {
  ydoc: Y.Doc
  provider: WebsocketProvider
  idb: IndexeddbPersistence
  // 'cache'：本机缓存先出来了；'server'：等到了服务器的内容；'empty'：都没有
  ready: Promise<'cache' | 'server' | 'empty'>
  status: () => Status
  onStatus: (l: () => void) => () => void
}

export function openPage(pageId: string): Conn {
  performance.mark('oad:open')
  const ydoc = new Y.Doc()
  const idb = new IndexeddbPersistence('oad:page:' + pageId, ydoc)
  // 不用 BroadcastChannel 在同源标签页之间同步：连上时它要把整份文档编码一遍发出去，改动也都经服务器转发了
  const provider = new WebsocketProvider(syncUrl(), pageId, ydoc, { maxBackoffTime: 4000, disableBc: true })

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
  const status = (): Status => {
    if (denied) return 'denied'
    if (!navigator.onLine && !provider.wsconnected) return 'offline'
    if (!provider.wsconnected) return provider.wsconnecting ? 'connecting' : 'offline'
    return provider.synced ? 'synced' : 'syncing'
  }
  provider.on('status', emit)
  provider.on('sync', emit)
  window.addEventListener('online', emit)
  window.addEventListener('offline', emit)

  const frag = ydoc.getXmlFragment('default')
  const ready = new Promise<'cache' | 'server' | 'empty'>((resolve) => {
    let done = false
    const finish = (v: 'cache' | 'server' | 'empty') => {
      if (!done) {
        done = true
        resolve(v)
      }
    }
    idb.whenSynced.then(() => {
      performance.mark('oad:idb')
      if (frag.length > 0) finish('cache')
    })
    // 本机没有：等服务器第一次同步完；连不上就别一直等
    const onSync = (s: boolean) => s && performance.mark('oad:ws-sync') && finish(frag.length > 0 || ydoc.getText('title').length > 0 ? 'server' : 'empty')
    provider.on('sync', onSync)
    setTimeout(() => finish(frag.length > 0 ? 'cache' : 'empty'), 2500)
  })

  return {
    ydoc,
    provider,
    idb,
    ready,
    status,
    onStatus: (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
  }
}

// 启动时就打开地址栏里那一页（App 里调用），不等登录信息和界面
let preopened: { id: string; conn: Conn } | null = null
export function preopenPage(pageId: string) {
  if (!preopened) preopened = { id: pageId, conn: openPage(pageId) }
}
export function takeConn(pageId: string): Conn {
  const p = preopened
  preopened = null
  if (p && p.id === pageId) return p.conn
  if (p) closePage(p.conn)
  return openPage(pageId)
}

/** 地址栏那一页其实不是文档（比如数据库）：启动时预先开的连接关掉 */
export function dropPreopened(pageId: string) {
  if (preopened?.id === pageId) {
    closePage(preopened.conn)
    preopened = null
  }
}

export function closePage(c: Conn) {
  c.provider.destroy()
  c.idb.destroy()
  c.ydoc.destroy()
}

