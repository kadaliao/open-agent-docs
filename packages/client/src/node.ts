// @open-agent-docs/client/node：只在服务器端用的工具（要密钥、要 node:crypto）。
//   import { createClient, verifyWebhook, signJwt } from '@open-agent-docs/client/node'
//   const admin = createClient({ baseUrl, headerSecret: process.env.OAD_HEADER_SECRET })   // 或 jwtSecret
//   const u = admin.withUser('u-42', { name: '王小明' })                                  // 替这个用户调
//   const note = verifyWebhook(secret, req.headers, rawBody)                              // Agent 的 webhook 验签
import { createHmac, timingSafeEqual } from 'node:crypto'
import { createClient as createCore, type ClientOptions, type ClientPlugins, type OadClient } from './core.ts'
import type { AgentNotification } from './types.ts'

export * from './core.ts'

// ---------- 签 JWT ----------
/** 工作区：id 的数组，或者 { id, role } 的数组 */
export type JwtWorkspaces = string[] | { id: string; role?: string }[]

export interface SignJwtOptions {
  name?: string
  email?: string
  workspaces?: JwtWorkspaces
  /** 有效期（秒，默认 3600） */
  ttl?: number
  issuer?: string
  audience?: string
  /** 当成“现在”（毫秒，测试用；默认 Date.now()） */
  now?: number
}

const b64url = (b: Buffer | string) => Buffer.from(b).toString('base64url')

/** HS256 的 JWT，claims：sub、name、email、workspaces、iat、exp、iss、aud（没给的不写）。给 OAD_AUTH=jwt 的服务器用 */
export function signJwt(secret: string, userId: string, o: SignJwtOptions = {}): string {
  const iat = Math.floor((o.now ?? Date.now()) / 1000)
  const claims: Record<string, unknown> = { sub: userId }
  if (o.name !== undefined) claims.name = o.name
  if (o.email !== undefined) claims.email = o.email
  if (o.workspaces !== undefined) claims.workspaces = o.workspaces
  claims.iat = iat
  claims.exp = iat + (o.ttl ?? 3600)
  if (o.issuer !== undefined) claims.iss = o.issuer
  if (o.audience !== undefined) claims.aud = o.audience
  const head = b64url(JSON.stringify({ alg: 'HS256', typ: 'JWT' })) + '.' + b64url(JSON.stringify(claims))
  return head + '.' + b64url(createHmac('sha256', secret).update(head).digest())
}

// ---------- webhook 验签 ----------
export type WebhookErrorKind = 'malformed' | 'stale' | 'signature'

/** 验签失败。kind：malformed（缺头、格式不对）、stale（时间戳太旧或太新）、signature（签名对不上） */
export class WebhookVerificationError extends Error {
  constructor(
    public kind: WebhookErrorKind,
    message: string,
  ) {
    super(message)
    this.name = 'WebhookVerificationError'
  }
}

type HeaderBag = Headers | Record<string, string | string[] | undefined | null>

function header(h: HeaderBag, name: string): string | undefined {
  if (typeof (h as Headers).get === 'function') return (h as Headers).get(name) ?? undefined
  for (const [k, v] of Object.entries(h)) if (k.toLowerCase() === name) return Array.isArray(v) ? v[0] : (v ?? undefined)
  return undefined
}

export interface VerifyWebhookOptions {
  /** 时间戳和现在最多差几秒（前后都算，默认 300） */
  tolerance?: number
  /** 当成“现在”（毫秒，测试用） */
  now?: number
}

/**
 * Agent webhook 验签：签名 = sha256= + HMAC-SHA256(secret, "<X-OAD-Timestamp>.<原始正文>")。
 * body 要原始的字节或字符串（别先 JSON.parse 再 stringify）。对了返回解析好的通知，不对抛 WebhookVerificationError。
 */
export function verifyWebhook(secret: string, headers: HeaderBag, body: string | Uint8Array, o: VerifyWebhookOptions = {}): AgentNotification {
  const ts = header(headers, 'x-oad-timestamp')
  const sig = header(headers, 'x-oad-signature')
  if (!ts || !sig) throw new WebhookVerificationError('malformed', '缺 X-OAD-Timestamp 或 X-OAD-Signature')
  if (!/^\d+$/.test(ts)) throw new WebhookVerificationError('malformed', 'X-OAD-Timestamp 不是整数（毫秒）')
  const m = /^sha256=([0-9a-fA-F]{64})$/.exec(sig)
  if (!m) throw new WebhookVerificationError('malformed', 'X-OAD-Signature 应该是 sha256=<64 位十六进制>')
  const raw = typeof body === 'string' ? Buffer.from(body, 'utf8') : Buffer.from(body.buffer, body.byteOffset, body.byteLength)
  const want = createHmac('sha256', secret).update(ts + '.').update(raw).digest()
  if (!timingSafeEqual(want, Buffer.from(m[1], 'hex'))) throw new WebhookVerificationError('signature', '签名对不上（密钥不对，或者正文被改过）')
  const skew = Math.abs((o.now ?? Date.now()) - Number(ts))
  if (skew > (o.tolerance ?? 300) * 1000) throw new WebhookVerificationError('stale', `时间戳和现在差了 ${Math.round(skew / 1000)} 秒`)
  try {
    return JSON.parse(raw.toString('utf8')) as AgentNotification
  } catch {
    throw new WebhookVerificationError('malformed', '正文不是 JSON')
  }
}

// ---------- 替用户调 ----------
/** 配置不对（不是服务器的错误） */
export class OadConfigError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'OadConfigError'
  }
}

export interface ServerClientOptions extends ClientOptions {
  /** OAD_AUTH=header 的服务器：OAD_HEADER_SECRET */
  headerSecret?: string
  /** OAD_AUTH=jwt 的服务器：OAD_JWT_SECRET（HS256） */
  jwtSecret?: string
  jwtIssuer?: string
  jwtAudience?: string
  /** 签出来的 JWT 有效期（秒，默认 3600） */
  jwtTtl?: number
}

export interface UserInfo {
  name?: string
  email?: string
  /** 只有 JWT 模式能用 */
  workspaces?: JwtWorkspaces
}

export type ServerClient = OadClient & {
  /** 一个新的客户端，所有请求都以这个用户的身份发；原来的不变 */
  withUser(userId: string, info?: UserInfo): ServerClient
}

const env = (k: string) => (typeof process !== 'undefined' ? process.env[k] : undefined) || undefined

/**
 * 服务器端的客户端：baseUrl 默认 OAD_BASE_URL（再没有 http://127.0.0.1:8848），token 默认 OAD_TOKEN，不带 cookie。
 * 给了 headerSecret / jwtSecret 就能 withUser。
 */
export function createClient(opts: ServerClientOptions = {}, extra: ClientPlugins = {}): ServerClient {
  const o: ServerClientOptions = {
    ...opts,
    baseUrl: opts.baseUrl ?? env('OAD_BASE_URL') ?? 'http://127.0.0.1:8848',
    token: opts.token ?? env('OAD_TOKEN'),
    credentials: opts.credentials ?? 'omit',
  }
  const c = createCore(o, extra) as ServerClient
  c.withUser = (userId, info) => withUser(o, userId, info, extra)
  return c
}

/** 以某个用户的身份调：header 模式带 X-OAD-*，JWT 模式签一个（过期前 60 秒换新的） */
export function withUser(opts: ServerClientOptions, userId: string, info: UserInfo = {}, extra: ClientPlugins = {}): ServerClient {
  const { headerSecret, jwtSecret } = opts
  if (headerSecret && jwtSecret) throw new OadConfigError('headerSecret 和 jwtSecret 只能给一个')
  if (!headerSecret && !jwtSecret) throw new OadConfigError('withUser 要 headerSecret（OAD_AUTH=header）或 jwtSecret（OAD_AUTH=jwt）')
  if (!userId) throw new OadConfigError('withUser 要用户 id')
  const next: ServerClientOptions = { ...opts }
  if (headerSecret) {
    if (info.workspaces) throw new OadConfigError('header 模式不支持 workspaces（只有 JWT 能带）')
    // 上一个用户的 X-OAD-* 不能留下
    const h: Record<string, string> = Object.fromEntries(Object.entries(opts.headers ?? {}).filter(([k]) => !k.toLowerCase().startsWith('x-oad-')))
    h['x-oad-secret'] = headerSecret
    h['x-oad-user-id'] = userId
    if (info.name !== undefined) h['x-oad-user-name'] = encodeURIComponent(info.name)
    if (info.email !== undefined) h['x-oad-user-email'] = info.email
    next.headers = h
    next.token = undefined
  } else {
    let tok = ''
    let exp = 0
    next.token = () => {
      const now = Date.now()
      if (!tok || now / 1000 >= exp - 60) {
        const ttl = opts.jwtTtl ?? 3600
        tok = signJwt(jwtSecret!, userId, { ...info, ttl, issuer: opts.jwtIssuer, audience: opts.jwtAudience, now })
        exp = Math.floor(now / 1000) + ttl
      }
      return tok
    }
  }
  return createClient(next, extra)
}
