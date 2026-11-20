// 一页的连接：用 @open-agent-docs/client 的 openPage（Y.Doc + 本机缓存 + WebSocket 同步）。单独打包，比编辑器先下载、先连上。
import { registerPlugin, type PageConn, type PageStatus } from '@open-agent-docs/client/core'
import { openPage as open } from '@open-agent-docs/client/page'
import { client } from './api'
import { onToken } from './token'

// 登记 client.openPage（Yjs、y-websocket、y-indexeddb 在这个包里，不进主包）
registerPlugin('openPage', open)

export type Status = PageStatus
export type Conn = PageConn

// 开着的连接：嵌入时宿主换了 token，下次重连带新的（y-websocket 每次连都按 params 重新拼地址）
const live = new Set<Conn>()
onToken((t) => {
  for (const c of live) c.provider.params.token = t
})

export function openPage(pageId: string): Conn {
  performance.mark('oad:open')
  const c = client.openPage(pageId)
  live.add(c)
  const destroy = c.destroy
  c.destroy = () => {
    live.delete(c)
    destroy()
  }
  return c
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
  c.destroy()
}

