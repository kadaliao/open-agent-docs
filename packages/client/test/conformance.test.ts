// 统一测试（docs/sdk-contract.md §14）：S1–S15 里 JS 能做的那些。每个场景自己一个临时数据目录、自己一个服务器。
// 服务器从哪来：设了 OAD_BINARY 就用它起子进程（S1 优先进程内的 @open-agent-docs/server）；没设就用进程内的；都没有就跳过（S10、S11、S14 不要服务器，照常跑）。
//   OAD_BINARY=target/debug/open-agent-docs pnpm --filter @open-agent-docs/client test conformance
import { spawn } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { createClient, embedUrl, isNotFound, isRateLimited, isUnauthorized, signJwt, verifyWebhook, WebhookVerificationError, type OadClient } from '../src/node.ts'

const vectors = JSON.parse(readFileSync(new URL('../../../sdks/conformance/vectors.json', import.meta.url), 'utf8'))
const BINARY = process.env.OAD_BINARY

// ---------- 起服务器 ----------
interface Server {
  url: string
  close(): Promise<void>
  how: 'inprocess' | 'binary'
}
type StartFn = (o: Record<string, string | number | undefined>) => Promise<{ url: string; close(): Promise<void> }>

// 进程内的绑定（packages/server-node）：没编好（没有 .node）就是 null
const inprocess: StartFn | null = await import('../../server-node/index.js' as string).then(
  (m: { start: StartFn }) => m.start,
  () => null,
)
const HAVE_SERVER = !!BINARY || !!inprocess

const freePort = () =>
  new Promise<number>((resolve) => {
    const s = createServer().listen(0, '127.0.0.1', () => {
      const p = (s.address() as AddressInfo).port
      s.close(() => resolve(p))
    })
  })

/** 起一个服务器：opts 是 OAD_* 配置的驼峰名（dataDir、auth、headerSecret……）；prefer 说先试哪一种 */
async function start(opts: Record<string, string>, prefer: 'inprocess' | 'binary' = BINARY ? 'binary' : 'inprocess'): Promise<Server> {
  const dir = mkdtempSync(join(tmpdir(), 'oad-conf-js-'))
  const cleanup = () => rmSync(dir, { recursive: true, force: true })
  if ((prefer === 'inprocess' || !BINARY) && inprocess) {
    const s = await inprocess({ dataDir: dir, ...opts })
    return { url: s.url, how: 'inprocess', close: () => s.close().finally(cleanup) }
  }
  // 子进程：<binary> serve，参数转成 OAD_<大写> 环境变量；等 /api/v1/auth/config 回 200（最多 30 秒）
  const port = await freePort()
  const env: Record<string, string> = { ...(process.env as Record<string, string>), RUST_LOG: 'warn', OAD_DATA_DIR: dir }
  for (const [k, v] of Object.entries(opts)) env['OAD_' + k.replace(/[A-Z]/g, (c) => '_' + c).toUpperCase()] = v
  const kid = spawn(BINARY!, ['serve', '--port', String(port)], { env, stdio: ['ignore', 'ignore', 'pipe'] })
  let stderr = ''
  kid.stderr!.on('data', (d) => (stderr = (stderr + d).slice(-4000)))
  let exited: number | null = null
  kid.on('exit', (c) => (exited = c ?? -1))
  const url = `http://127.0.0.1:${port}`
  const t0 = Date.now()
  for (;;) {
    if (exited !== null) throw new Error(`服务器退出了（${exited}）：${stderr.slice(-1000)}`)
    try {
      if ((await fetch(url + '/api/v1/auth/config')).status === 200) break
    } catch {
      /* 还没起来 */
    }
    if (Date.now() - t0 > 30_000) {
      kid.kill('SIGKILL')
      throw new Error('30 秒没起来：' + stderr.slice(-1000))
    }
    await new Promise((r) => setTimeout(r, 100))
  }
  return {
    url,
    how: 'binary',
    close: async () => {
      if (exited === null) {
        const gone = new Promise((r) => kid.once('exit', r))
        kid.kill('SIGTERM')
        const t = setTimeout(() => kid.kill('SIGKILL'), 10_000)
        await gone
        clearTimeout(t)
      }
      cleanup()
    },
  }
}

/** 起一个 header 登录的服务器，跑完关掉 */
async function withServer<R>(opts: Record<string, string>, f: (s: Server) => Promise<R>): Promise<R> {
  const s = await start(opts)
  try {
    return await f(s)
  } finally {
    await s.close()
  }
}
const SECRET = 'conf-secret-' + Math.random().toString(36).slice(2)
const header = { auth: 'header', headerSecret: SECRET }
const userClient = (s: Server, id = 'u1', name = '王小明') => createClient({ baseUrl: s.url, headerSecret: SECRET }).withUser(id, { name })

const until = async (f: () => boolean, ms: number) => {
  const t0 = Date.now()
  while (!f()) {
    if (Date.now() - t0 > ms) throw new Error('等超时了')
    await new Promise((r) => setTimeout(r, 20))
  }
}

describe.skipIf(!HAVE_SERVER)('统一测试：要服务器的场景', () => {
  it('S1 start：auth.config().mode == header；close() 后端口连不上', async () => {
    const s = await start(header, 'inprocess')
    expect((await createClient({ baseUrl: s.url }).authConfig()).mode).toBe('header')
    await s.close()
    await expect(fetch(s.url + '/api/v1/auth/config')).rejects.toThrow()
  })

  it('S2 withUser：名字对，defaultWorkspace 是 /me 的第一个', () =>
    withServer(header, async (s) => {
      const u = userClient(s)
      const me = await u.me()
      expect(me.user.name).toBe('王小明')
      expect(await u.defaultWorkspace()).toBe(me.workspaces[0].id)
    }))

  it('S3 页面：建、读 Markdown、列表、改标题、按 base 替换正文、删了以后 NotFound', () =>
    withServer(header, async (s) => {
      const u = userClient(s)
      const ws = await u.defaultWorkspace()
      const p = await u.createPage(ws, { title: '周报' })
      const first = await u.content(p.id)
      await u.putContent(p.id, { markdown: '# 标题\n\n正文', base: first.version })
      const c = await u.content(p.id)
      expect(c.markdown).toContain('# 标题')
      expect(c.markdown).toContain('正文')
      expect((await u.pages(ws)).some((x) => x.id === p.id)).toBe(true)
      await u.patchPage(p.id, { title: '周报（改）' })
      expect((await u.page(p.id)).title).toBe('周报（改）')
      await u.putContent(p.id, { markdown: '# 标题\n\n第二版', base: c.version })
      expect((await u.content(p.id)).markdown).toContain('第二版')
      await u.deletePage(p.id)
      const e = await u.page(p.id).catch((x) => x)
      expect(isNotFound(e)).toBe(true)
    }))

  it('S4 错误：不存在的页面是 not_found；假 token 是 401', () =>
    withServer(header, async (s) => {
      const e = await userClient(s)
        .page('nope')
        .catch((x) => x)
      expect(isNotFound(e)).toBe(true)
      expect(e.code).toBe('not_found')
      expect(e.message).not.toBe('')
      const bogus = await createClient({ baseUrl: s.url, token: 'oad_bogus' })
        .me()
        .catch((x) => x)
      expect(isUnauthorized(bogus)).toBe(true)
    }))

  it('S5 Agent：用它的 token，me().user.kind == agent', () =>
    withServer(header, async (s) => {
      const u = userClient(s)
      const a = await u.createAgent(await u.defaultWorkspace(), { name: 'bot', scope: 'write' })
      const me = await createClient({ baseUrl: s.url, token: a.token }).me()
      expect(me.user.kind).toBe('agent')
    }))

  it('S6 翻页：5 个版本，limit=2 逐条取完，确实翻了页', () =>
    withServer(header, async (s) => {
      const u = userClient(s)
      const p = await u.createPage(await u.defaultWorkspace(), { title: '版本' })
      for (let i = 0; i < 5; i++) {
        const c = await u.content(p.id)
        await u.putContent(p.id, { markdown: `第 ${i} 版`, base: c.version })
        await u.saveVersion(p.id, 'v' + i)
      }
      let requests = 0
      const counting = createClient({
        baseUrl: s.url,
        headerSecret: SECRET,
        fetch: (async (i: RequestInfo | URL, init?: RequestInit) => {
          if (String(i).includes('/versions')) requests++
          return fetch(i, init)
        }) as typeof fetch,
      }).withUser('u1', { name: '王小明' })
      const ids: unknown[] = []
      for await (const v of counting.eachVersion(p.id, { limit: 2 })) ids.push(v.id)
      expect(ids.length).toBeGreaterThanOrEqual(5)
      expect(new Set(ids).size).toBe(ids.length)
      expect(requests).toBeGreaterThanOrEqual(3)
    }))

  it('S7 事件：开着工作区事件建页面，10 秒内收到 page.created', () =>
    withServer(header, async (s) => {
      const u = userClient(s)
      const ws = await u.defaultWorkspace()
      const ev = u.events(ws)
      try {
        let connected = false
        ev.onStatus((c) => (connected = c))
        const got: { id: string; title?: string }[] = []
        ev.on('page.created', (d) => got.push(d))
        await until(() => connected, 10_000)
        const p = await u.createPage(ws, { title: '事件' })
        await until(() => got.some((x) => x.id === p.id), 10_000)
        expect(got.find((x) => x.id === p.id)!.title).toBe('事件')
      } finally {
        ev.close()
      }
    }))

  it('S8 Agent 通知：评论 @bot，Agent 的流收到，ack 以后 list 里没有', () =>
    withServer(header, async (s) => {
      const u = userClient(s)
      const ws = await u.defaultWorkspace()
      const a = await u.createAgent(ws, { name: 'bot', scope: 'write' })
      const bot = createClient({ baseUrl: s.url, token: a.token })
      const ev = bot.agentEvents()
      try {
        const got: { id: number; comment?: { body?: string } | null }[] = []
        ev.on('notification', (d) => got.push(d))
        const p = await u.createPage(ws, { title: '要 Agent 看的' })
        await u.createThread(p.id, { body: `@[bot](agent:${a.agent.id}) 看看` })
        await until(() => got.length > 0, 10_000)
        expect(got[0].comment?.body).toContain('看看')
        await bot.ackAgentNotifications({ ids: [got[0].id] })
        const left = await bot.agentNotifications()
        expect(left.notifications.some((n) => n.id === got[0].id)).toBe(false)
      } finally {
        ev.close()
      }
    }))

  it('S9 JWT：jwtSecret 签的能用，别的密钥签的是 401', () => {
    const S = 'jwt-conf-' + Math.random().toString(36).slice(2)
    return withServer({ auth: 'jwt', jwtSecret: S }, async (s) => {
      const u = createClient({ baseUrl: s.url, jwtSecret: S }).withUser('u9', { name: '小九' })
      expect((await u.me()).user.name).toBe('小九')
      const bad = await createClient({ baseUrl: s.url, token: signJwt('wrong-secret', 'u9') })
        .me()
        .catch((x) => x)
      expect(isUnauthorized(bad)).toBe(true)
    })
  })

  it('S12 分享：新页面没有；开了 url 含 /s/；关了又没有', () =>
    withServer(header, async (s) => {
      const u = userClient(s)
      const p = await u.createPage(await u.defaultWorkspace(), { title: '分享' })
      expect(await u.getShare(p.id)).toBeNull()
      const sh = await u.createShare(p.id, {})
      expect(sh.url).toContain('/s/')
      await u.revokeShare(p.id)
      expect(await u.getShare(p.id)).toBeNull()
    }))

  it('S13 HTML：含 <h1，不含 <script', (ctx) =>
    withServer(header, async (s) => {
      const u = userClient(s)
      const p = await u.createPage(await u.defaultWorkspace(), { title: 'HTML' })
      const c = await u.content(p.id)
      await u.putContent(p.id, { markdown: '# 标题\n\n<script>alert(1)</script> 正文', base: c.version })
      const r = (await u.content(p.id, 'html').catch((e) => e)) as { html?: string }
      // 服务器还没有 format=html（W-WEB 做完之前）：要么报 400，要么当成 markdown 回来
      if (typeof r?.html !== 'string') return ctx.skip('服务器还不支持 format=html（W-WEB 加上以后这条会跑）')
      expect(r.html).toContain('<h1')
      expect(r.html).not.toContain('<script')
    }))

  it('S15 文件：上传 hello，下载回来字节一样', () =>
    withServer(header, async (s) => {
      const u = userClient(s)
      const up = await u.upload(await u.defaultWorkspace(), new TextEncoder().encode('hello'), { name: 'a.txt' })
      const b = await u.download(up.id)
      expect(new TextDecoder().decode(await b.arrayBuffer())).toBe('hello')
    }))
})

describe('统一测试：不要服务器的场景', () => {
  it('S10 webhook：vectors.json 全过', () => {
    for (const v of vectors.webhook) {
      let kind = 'ok'
      try {
        verifyWebhook(v.secret, v.headers, v.body, { now: v.now_ms, tolerance: v.tolerance_s })
      } catch (e) {
        kind = (e as WebhookVerificationError).kind
      }
      expect(kind, v.name).toBe(v.expect)
    }
  })

  it('S11 嵌入工具：embed_url、jwt 全过', () => {
    for (const v of vectors.embed_url) expect(embedUrl(v.base_url, v.page_id, v.options)).toBe(v.expect)
    const j = vectors.jwt
    const [h, c] = signJwt(j.secret, j.input.user_id, {
      name: j.input.name,
      email: j.input.email,
      workspaces: j.input.workspaces,
      ttl: j.input.ttl_s,
      issuer: j.input.issuer,
      audience: j.input.audience,
      now: j.now_s * 1000,
    })
      .split('.')
      .slice(0, 2)
      .map((x) => JSON.parse(Buffer.from(x, 'base64url').toString('utf8')))
    expect(h).toEqual(j.expect_header)
    expect(c).toEqual(j.expect_claims)
  })

  it('S14 重试：先 429 + Retry-After: 1 再 200 → 成功且等了 ≥ 1 秒；maxRetries 0 → RateLimit，retryAfter 1', async () => {
    let n = 0
    const srv = createServer((_, res) => {
      if (n++ % 2 === 0) res.writeHead(429, { 'retry-after': '1', 'content-type': 'application/json' }).end('{"error":{"code":"rate_limited","message":"慢点"}}')
      else res.writeHead(200, { 'content-type': 'application/json' }).end('{"mode":"header"}')
    })
    await new Promise<void>((r) => srv.listen(0, '127.0.0.1', r))
    const base = `http://127.0.0.1:${(srv.address() as AddressInfo).port}`
    try {
      const t0 = Date.now()
      const c: OadClient = createClient({ baseUrl: base })
      expect(await c.authConfig()).toEqual({ mode: 'header' })
      expect(Date.now() - t0).toBeGreaterThanOrEqual(1000)
      const e = await createClient({ baseUrl: base, maxRetries: 0 })
        .authConfig()
        .catch((x) => x)
      expect(isRateLimited(e)).toBe(true)
      expect(e.retryAfter).toBe(1)
    } finally {
      srv.close()
    }
  })
})
