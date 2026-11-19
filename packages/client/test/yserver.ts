// 测试用的最小 y-websocket 服务器（同步 + 在线状态），一个房间一份 Y.Doc。也能让某个房间拒绝（发“没有权限”）。
import * as decoding from 'lib0/decoding'
import * as encoding from 'lib0/encoding'
import { createServer, type IncomingMessage } from 'node:http'
import type { AddressInfo } from 'node:net'
import { WebSocketServer, type WebSocket } from 'ws'
import * as authProtocol from 'y-protocols/auth'
import * as awarenessProtocol from 'y-protocols/awareness'
import * as syncProtocol from 'y-protocols/sync'
import * as Y from 'yjs'

export async function startYServer(o: { deny?: (room: string) => boolean } = {}) {
  const docs = new Map<string, { doc: Y.Doc; aw: awarenessProtocol.Awareness; conns: Set<WebSocket> }>()
  const urls: string[] = []
  const room = (name: string) => {
    let r = docs.get(name)
    if (!r) {
      const doc = new Y.Doc()
      const aw = new awarenessProtocol.Awareness(doc)
      aw.setLocalState(null)
      r = { doc, aw, conns: new Set() }
      const rr = r
      doc.on('update', (u: Uint8Array) => {
        const e = encoding.createEncoder()
        encoding.writeVarUint(e, 0)
        syncProtocol.writeUpdate(e, u)
        const m = encoding.toUint8Array(e)
        rr.conns.forEach((c) => c.send(m))
      })
      aw.on('update', ({ added, updated, removed }: { added: number[]; updated: number[]; removed: number[] }) => {
        const e = encoding.createEncoder()
        encoding.writeVarUint(e, 1)
        encoding.writeVarUint8Array(e, awarenessProtocol.encodeAwarenessUpdate(aw, [...added, ...updated, ...removed]))
        const m = encoding.toUint8Array(e)
        rr.conns.forEach((c) => c.send(m))
      })
      docs.set(name, r)
    }
    return r
  }
  const http = createServer()
  const wss = new WebSocketServer({ server: http })
  wss.on('connection', (ws: WebSocket, req: IncomingMessage) => {
    urls.push(req.url ?? '')
    const name = new URL(req.url ?? '/', 'http://x').pathname.replace(/^\//, '')
    if (o.deny?.(name)) {
      const e = encoding.createEncoder()
      encoding.writeVarUint(e, 2)
      authProtocol.writePermissionDenied(e, 'no')
      ws.send(encoding.toUint8Array(e))
      return
    }
    const r = room(name)
    r.conns.add(ws)
    ws.on('message', (data: Buffer) => {
      const d = decoding.createDecoder(new Uint8Array(data))
      const e = encoding.createEncoder()
      const type = decoding.readVarUint(d)
      if (type === 0) {
        encoding.writeVarUint(e, 0)
        syncProtocol.readSyncMessage(d, e, r.doc, ws)
        if (encoding.length(e) > 1) ws.send(encoding.toUint8Array(e))
      } else if (type === 1) awarenessProtocol.applyAwarenessUpdate(r.aw, decoding.readVarUint8Array(d), ws)
    })
    ws.on('close', () => r.conns.delete(ws))
    const e = encoding.createEncoder()
    encoding.writeVarUint(e, 0)
    syncProtocol.writeSyncStep1(e, r.doc)
    ws.send(encoding.toUint8Array(e))
  })
  await new Promise<void>((res) => http.listen(0, '127.0.0.1', res))
  const port = (http.address() as AddressInfo).port
  return {
    port,
    urls,
    doc: (name: string) => room(name).doc,
    close: () =>
      new Promise<void>((res) => {
        wss.clients.forEach((c) => c.terminate())
        wss.close()
        http.close(() => res())
      }),
  }
}
