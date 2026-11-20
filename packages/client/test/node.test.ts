// @open-agent-docs/client/node：webhook 验签、签 JWT（sdks/conformance/vectors.json 的用例）、withUser。
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { createClient, OadConfigError, signJwt, verifyWebhook, WebhookVerificationError, withUser } from '../src/node'
import { fakeFetch, json } from './fake-fetch'

const vectors = JSON.parse(readFileSync(new URL('../../../sdks/conformance/vectors.json', import.meta.url), 'utf8'))
const decode = (s: string) => JSON.parse(Buffer.from(s, 'base64url').toString('utf8'))

describe('verifyWebhook', () => {
  for (const v of vectors.webhook) {
    it(v.name, () => {
      const run = () => verifyWebhook(v.secret, v.headers, v.body, { now: v.now_ms, tolerance: v.tolerance_s })
      if (v.expect === 'ok') expect(run()).toEqual(JSON.parse(v.body))
      else {
        const e = (() => {
          try {
            run()
          } catch (x) {
            return x
          }
        })()
        expect(e).toBeInstanceOf(WebhookVerificationError)
        expect((e as WebhookVerificationError).kind).toBe(v.expect)
      }
    })
  }
  it('Headers 对象、Buffer 正文也行', () => {
    const v = vectors.webhook[0]
    expect(verifyWebhook(v.secret, new Headers(v.headers), Buffer.from(v.body), { now: v.now_ms })).toMatchObject({ id: 42 })
  })
})

describe('signJwt', () => {
  it('vectors.json：头和 claims 对得上，签名能验', async () => {
    const j = vectors.jwt
    const t = signJwt(j.secret, j.input.user_id, {
      name: j.input.name,
      email: j.input.email,
      workspaces: j.input.workspaces,
      ttl: j.input.ttl_s,
      issuer: j.input.issuer,
      audience: j.input.audience,
      now: j.now_s * 1000,
    })
    const [h, c, s] = t.split('.')
    expect(decode(h)).toEqual(j.expect_header)
    expect(decode(c)).toEqual(j.expect_claims)
    const { createHmac } = await import('node:crypto')
    expect(s).toBe(createHmac('sha256', j.secret).update(h + '.' + c).digest('base64url'))
  })
  it('没给的 claims 不写，默认一小时', () => {
    const c = decode(signJwt('s', 'u1', { now: 1_000_000 }).split('.')[1])
    expect(c).toEqual({ sub: 'u1', iat: 1000, exp: 4600 })
  })
})

describe('withUser', () => {
  it('header 模式：X-OAD-Secret、User-Id、User-Name（百分号编码）、User-Email；原来的客户端不变', async () => {
    const { fetch, calls } = fakeFetch(() => json({ user: { id: 'u-42' }, workspaces: [] }))
    const admin = createClient({ baseUrl: 'http://h', fetch, headerSecret: 'sec', headers: { 'x-trace': '1' } })
    const u = admin.withUser('u-42', { name: '王小明', email: 'x@example.com' })
    await u.me()
    await admin.me()
    expect(calls[0].headers).toMatchObject({
      'x-oad-secret': 'sec',
      'x-oad-user-id': 'u-42',
      'x-oad-user-name': encodeURIComponent('王小明'),
      'x-oad-user-email': 'x@example.com',
      'x-trace': '1',
    })
    expect(calls[0].credentials).toBe('omit')
    expect(calls[1].headers['x-oad-user-id']).toBeUndefined()
    // 再换一个人：上一个人的名字不留下
    await u.withUser('bob').me()
    expect(calls[2].headers['x-oad-user-id']).toBe('bob')
    expect(calls[2].headers['x-oad-user-name']).toBeUndefined()
  })

  it('JWT 模式：Bearer 一个签好的 JWT，缓存着用', async () => {
    const { fetch, calls } = fakeFetch(() => json({}))
    const u = withUser({ baseUrl: 'http://h', fetch, jwtSecret: 'k', jwtIssuer: 'iss', jwtTtl: 600 }, 'u9', { name: '小九', workspaces: ['w1'] })
    await u.me()
    await u.me()
    const tok = calls[0].headers.authorization.replace('Bearer ', '')
    expect(calls[1].headers.authorization).toBe('Bearer ' + tok)
    expect(decode(tok.split('.')[1])).toMatchObject({ sub: 'u9', name: '小九', workspaces: ['w1'], iss: 'iss' })
    const c = decode(tok.split('.')[1])
    expect(c.exp - c.iat).toBe(600)
  })

  it('配置不对：两个都给、都没给、header 模式给 workspaces', () => {
    expect(() => withUser({ headerSecret: 'a', jwtSecret: 'b' }, 'u')).toThrow(OadConfigError)
    expect(() => createClient({}).withUser('u')).toThrow(OadConfigError)
    expect(() => withUser({ headerSecret: 'a' }, 'u', { workspaces: ['w'] })).toThrow(OadConfigError)
  })

  it('baseUrl 默认 OAD_BASE_URL，再没有 http://127.0.0.1:8848；token 默认 OAD_TOKEN', async () => {
    const { fetch, calls } = fakeFetch(() => json({}))
    const old = { b: process.env.OAD_BASE_URL, t: process.env.OAD_TOKEN }
    try {
      delete process.env.OAD_BASE_URL
      process.env.OAD_TOKEN = 'oad_env'
      await createClient({ fetch }).me()
      process.env.OAD_BASE_URL = 'http://env:1'
      await createClient({ fetch }).me()
    } finally {
      for (const [k, v] of [['OAD_BASE_URL', old.b], ['OAD_TOKEN', old.t]] as const) if (v === undefined) delete process.env[k]; else process.env[k] = v
    }
    expect(calls[0].url).toBe('http://127.0.0.1:8848/api/v1/me')
    expect(calls[0].headers.authorization).toBe('Bearer oad_env')
    expect(calls[1].url).toBe('http://env:1/api/v1/me')
  })
})
