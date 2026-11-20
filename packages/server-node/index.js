// @open-agent-docs/server：在 Node 进程里起 Open Agent Docs 服务器。服务器跑在它自己的线程和 tokio 运行时上，不占 Node 的事件循环。
import http from 'node:http'
import net from 'node:net'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const native = require(`./oad-server.${process.platform}-${process.arch}.node`)

/** 配置项：和命令行一样的 OAD_* 名字，也可以写成 dataDir、auth、port 这样；值转成字符串。 */
function toOptions(opts = {}) {
  const out = {}
  for (const [k, v] of Object.entries(opts)) {
    if (v === undefined || v === null) continue
    out[k] = Array.isArray(v) ? v.join(',') : typeof v === 'boolean' ? (v ? '1' : '0') : String(v)
  }
  return out
}

/**
 * 起服务器，监听好了 resolve。不给 port 时随便挑一个空端口（不是命令行默认的 8848）。
 * @returns {Promise<{ url: string, port: number, close(): Promise<void>, handler: Function, upgrade: Function }>}
 */
export async function start(opts = {}) {
  const o = toOptions({ port: 0, ...opts })
  const s = await native.start(o)
  const target = new URL(s.url)
  let closed = false
  return {
    url: s.url,
    port: s.port,
    async close() {
      if (closed) return
      closed = true
      await s.close()
    },
    handler: createHandler(target),
    upgrade: createUpgrade(target),
  }
}

/**
 * 挂到 Node http / Express 上的处理函数：把请求原样转给进程里的服务器（流式，SSE 也行）。
 * 原样转发而不是直接调用 Rust：服务器是 axum（异步、要自己的运行时），按请求过 FFI 要拷贝正文、还接不上 SSE / WebSocket 的长连接。
 * 挂在子路径下（app.use('/docs', handler)）时会把子路径去掉再转，但网页里的链接默认在根上：要么挂在根上，要么只用 /api 和 /s。
 */
function createHandler(target) {
  return function oadHandler(req, res) {
    const headers = { ...req.headers, host: req.headers.host ?? target.host }
    const r = http.request(
      { host: target.hostname, port: target.port, method: req.method, path: req.url, headers },
      (up) => {
        res.writeHead(up.statusCode ?? 502, up.headers)
        up.pipe(res)
      },
    )
    r.on('error', (e) => {
      if (!res.headersSent) res.writeHead(502, { 'content-type': 'application/json' })
      res.end(JSON.stringify({ error: { code: 'bad_gateway', message: String(e.message ?? e) } }))
    })
    // 浏览器那头断了（SSE 关掉之类）：转发的这条也断掉
    res.on('close', () => r.destroy())
    req.pipe(r)
  }
}

/** WebSocket（协同编辑 /api/v1/sync）：`server.on('upgrade', oad.upgrade)`。 */
function createUpgrade(target) {
  return function oadUpgrade(req, socket, head) {
    const up = net.connect(Number(target.port), target.hostname, () => {
      let raw = `${req.method} ${req.url} HTTP/${req.httpVersion}\r\n`
      for (let i = 0; i < req.rawHeaders.length; i += 2) raw += `${req.rawHeaders[i]}: ${req.rawHeaders[i + 1]}\r\n`
      up.write(raw + '\r\n')
      if (head?.length) up.write(head)
      up.pipe(socket)
      socket.pipe(up)
    })
    up.on('error', () => socket.destroy())
    socket.on('error', () => up.destroy())
  }
}
