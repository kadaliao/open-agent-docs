import type { IncomingMessage, ServerResponse } from 'node:http'
import type { Duplex } from 'node:stream'

/** 配置项：OAD_* 名字（OAD_DATA_DIR）或者驼峰（dataDir），见 docs/zh/configuration.md。 */
export interface StartOptions {
  /** 不给时随便挑一个空端口 */
  port?: number
  host?: string
  dataDir?: string
  auth?: 'local' | 'header' | 'jwt' | 'none'
  publicUrl?: string
  allowedHosts?: string[] | string
  allowedOrigins?: string[] | string
  headerSecret?: string
  jwtSecret?: string
  jwtJwksUrl?: string
  databaseUrl?: string
  blobUrl?: string
  rateLimit?: number
  [key: string]: string | number | boolean | string[] | undefined
}

export interface OadServer {
  /** http://127.0.0.1:<端口> */
  url: string
  port: number
  /** 关掉（页面都存好后 resolve），可以调多次 */
  close(): Promise<void>
  /** 挂到 Node http / Express：把请求转给进程里的服务器 */
  handler(req: IncomingMessage, res: ServerResponse): void
  /** WebSocket：server.on('upgrade', oad.upgrade) */
  upgrade(req: IncomingMessage, socket: Duplex, head: Buffer): void
}

export function start(options?: StartOptions): Promise<OadServer>
