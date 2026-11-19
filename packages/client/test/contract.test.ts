// 契约：起一个真的服务器（OAD_BIN），经客户端把大部分接口走一遍，每个 JSON 响应按 docs/openapi.json 里那个操作的响应结构检查。
// 文档和服务器对不上就失败（哪一边错了看报错的位置）。没有 OAD_BIN 时跳过。
//   OAD_BIN=target/release/open-agent-docs pnpm --filter @oad/client test
import { spawn, type ChildProcess } from 'node:child_process'
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { createClient } from '../src/index.ts'
import { checker, type Problem } from './schema-check.ts'

const BIN = process.env.OAD_BIN
const spec = JSON.parse(readFileSync(new URL('../../../docs/openapi.json', import.meta.url), 'utf8'))
const check = checker(spec)
const PORT = 18960 + Math.floor(Math.random() * 30)
const BASE = `http://127.0.0.1:${PORT}`

const seen = new Map<string, { errs: Problem[]; extra: Problem[] }>()
// 记下每个响应，按文档检查
const recording: typeof fetch = async (input, init) => {
  const res = await fetch(input, init)
  const url = new URL(String(input))
  const method = (init?.method ?? 'GET').toUpperCase()
  const ct = res.headers.get('content-type') ?? ''
  if (ct.includes('application/json')) {
    const body = await res.clone().json()
    const r = check.response(method, url.pathname.replace(/^\/api\/v1/, ''), res.status, body)
    const cur = seen.get(r.op) ?? { errs: [], extra: [] }
    cur.errs.push(...r.errs)
    for (const e of r.extra) if (!cur.extra.some((x) => x.at === e.at)) cur.extra.push(e)
    seen.set(r.op, cur)
  } else if (res.status !== 204 && res.ok) {
    const r = check.response(method, url.pathname.replace(/^\/api\/v1/, ''), res.status, undefined)
    if (r.errs.length) seen.set(r.op, { errs: r.errs, extra: [] })
    else seen.set(r.op, seen.get(r.op) ?? { errs: [], extra: [] })
  }
  return res
}

let srv: ChildProcess
beforeAll(async () => {
  if (!BIN) return
  srv = spawn(BIN, ['serve', '--port', String(PORT)], {
    env: { ...process.env, OAD_AUTH: 'none', OAD_DATA_DIR: mkdtempSync(join(tmpdir(), 'oad-contract-')), RUST_LOG: 'warn' },
    stdio: 'ignore',
  })
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(BASE + '/api/v1/auth/config')).ok) return
    } catch {
      /* 还没起来 */
    }
    await new Promise((r) => setTimeout(r, 100))
  }
  throw new Error('服务器起不来')
}, 30_000)
afterAll(() => srv?.kill())

describe.skipIf(!BIN)('契约：真服务器的响应和 docs/openapi.json 一致', () => {
  it('把接口走一遍', async () => {
    const c = createClient({ baseUrl: BASE, credentials: 'omit', fetch: recording })
    await c.authConfig()
    const me = await c.me()
    const ws = me.workspaces[0].id
    // 页面、正文、块
    const p = await c.createPage(ws, { title: '契约' })
    const child = await c.createPage(ws, { title: '子页', parent_id: p.id })
    await c.pages(ws)
    await c.page(p.id)
    await c.patchPage(p.id, { title: '契约测试', icon: '📄' })
    await c.putContent(p.id, { markdown: '# 标题\n\n第一段\n\n- 列表\n' })
    const content = await c.content(p.id)
    const doc = await c.content(p.id, 'json')
    const first = (doc as { doc?: { content?: { attrs?: { id?: string } }[] } }).doc?.content?.[0]?.attrs?.id as string
    await c.block(p.id, first)
    await c.putBlock(p.id, first, { markdown: '# 新标题', base: content.version })
    const ins = await c.insertBlocks(p.id, { markdown: '加的一段' })
    await c.deleteBlock(p.id, ins.ids[0])
    await c.authors(p.id)
    // 评论
    const th = await c.createThread(p.id, { anchor: null, body: '看一下' })
    const rc = await c.reply(th.id, { body: '好' })
    await c.editComment(rc.id, '好的')
    await c.resolveThread(th.id, true)
    await c.threads(p.id)
    await c.deleteComment(rc.id)
    // Agent 和它的通知
    const ag = await c.createAgent(ws, { name: 'Bot', scope: 'write' })
    await c.agents(ws)
    await c.patchAgent(ag.agent.id, { about: '测试' })
    const tok = await c.reissueToken(ag.agent.id)
    await c.reissueWebhookSecret(ag.agent.id)
    await c.createThread(p.id, { anchor: null, body: `@[Bot](agent:${ag.agent.id}) 来看看` })
    const bot = createClient({ baseUrl: BASE, credentials: 'omit', token: tok.token, fetch: recording })
    const notes = await bot.agentNotifications({ limit: 10 })
    expect(notes.notifications.length).toBe(1)
    await bot.ackAgentNotifications({ upto: notes.notifications[0].id })
    // 成员、邀请、工作区、自己
    await c.members(ws)
    await c.patchWorkspace(ws, { name: '工作区' })
    await c.patchMe({ name: '我' })
    const inv = await c.createInvite(ws, { role: 'editor' })
    await c.invites(ws)
    await c.inviteInfo(inv.token)
    await c.deleteInvite(ws, inv.token)
    await c.inbox()
    await c.markRead({ all: true })
    // 数据库
    const db = await c.createDatabase(ws, {
      title: '任务',
      schema: { props: [{ name: '名称', type: 'title' }, { name: '状态', type: 'select', options: ['做', '完'] }, { name: '工时', type: 'number' }, { name: '日期', type: 'date' }] },
    })
    const dbId = db.page.id
    const meta = await c.getDatabase(dbId)
    const k = Object.fromEntries((meta.schema.props as { key: string; name: string }[]).map((x) => [x.name, x.key]))
    const added = await c.addRows(dbId, [{ props: { [k['名称']]: '甲', [k['状态']]: '做', [k['工时']]: 3, [k['日期']]: { start: '2026-11-20' } } }, { props: { [k['名称']]: '乙' } }])
    await c.dbRows(dbId)
    await c.dbRows(dbId, 0)
    await c.patchRows(dbId, [{ row_id: added.ids[1], set: { [k['工时']]: 5 } }])
    await c.queryDatabase(dbId, { sorts: [{ prop: k['工时'], dir: 'desc' }], group_by: k['状态'] })
    const sch = await c.patchSchema(dbId, [{ op: 'add', name: '完成', type: 'button', set: { [k['状态']]: '完' } }])
    const ren = await c.patchSchema(dbId, [{ op: 'rename', prop: k['工时'], name: '小时' }])
    const btn = (sch.schema.props as { key: string; type: string }[]).find((x) => x.type === 'button')!.key
    await c.pressButton(dbId, added.ids[0], btn)
    await c.undoSchema(dbId, ren.seq)
    const v = await c.addView(dbId, { name: '看板', type: 'board', group_by: k['状态'] })
    await c.patchView(dbId, v.id, { name: '看板 2' })
    await c.putViews(dbId, (await c.getDatabase(dbId)).views as never)
    await c.deleteView(dbId, v.id)
    const hist = await c.dbHistory(dbId)
    await c.dbHistory(dbId, { row: added.ids[0] })
    await c.dbUndo(dbId, hist.items[0].seq)
    await c.request('GET', `/databases/${dbId}/export.csv`).catch(() => {})
    await c.deleteRows(dbId, [added.ids[1]])
    // 历史版本
    const sv = await c.saveVersion(p.id, '存一下')
    const vs = await c.versions(p.id)
    await c.version(p.id, String(vs.versions[0].id))
    await c.versionDiff(p.id, String(sv.id))
    await c.restoreVersion(p.id, String(sv.id))
    // 搜索、分享、文件
    await c.search(ws, '标题')
    await c.createShare(p.id, { include_children: false })
    await c.getShare(p.id)
    await c.revokeShare(p.id)
    const up = await c.upload(ws, new File([new Uint8Array([137, 80, 78, 71])], 'a.png', { type: 'image/png' }))
    expect(up.url).toBeTruthy()
    // 回收站
    await c.deletePage(child.id)
    const tr = await c.trash(ws)
    await c.restoreTrash(tr.items[0].id)
    await c.deletePage(child.id)
    await c.purgeTrash((await c.trash(ws)).items[0].id)
    await c.revokeAgent(ag.agent.id)
    // 错误也要按文档
    await c.page('nope').catch(() => {})
    await c.request('POST', `/workspaces/${ws}/pages`, { parent_id: 'nope' }).catch(() => {})

    // 已知的、报给服务器那边的文档问题（修好一条就删一条；新的对不上照样失败）
    const known = (op: string, e: Problem) =>
      // 文档节点（DocNode）：attrs 标成必填，实际空的时候不输出；GET content 的 doc 因此也对不上 oneOf
      (/\/(content|blocks\/\{block_id\}|versions\/\{v\}) → 200$/.test(op) && (/\.attrs$/.test(e.at) || e.at === '$.doc')) ||
      // 服务器查询的行（QueryRow = DbRow + computed）：实际不带 db_id、position
      (/\/query → 200$/.test(op) && /\.(db_id|position)$/.test(e.at)) ||
      // 库的历史：old / new 是任意 JSON，文档写的 object | null
      (/\/history → 200$/.test(op) && /\.(old|new)$/.test(e.at))
    const bad: [string, Problem[]][] = []
    const knownHit: [string, Problem[]][] = []
    for (const [op, r] of seen) {
      const k = r.errs.filter((e) => known(op, e))
      const n = r.errs.filter((e) => !known(op, e))
      if (n.length) bad.push([op, n])
      if (k.length) knownHit.push([op, k])
    }
    const extra = [...seen].filter(([, r]) => r.extra.length).map(([op, r]) => [op, r.extra] as [string, Problem[]])
    const fmt = (l: [string, Problem[]][]) => l.map(([op, es]) => `${op}\n${es.map((e) => `    ${e.at} ${e.msg}`).join('\n')}`).join('\n')
    if (process.env.OAD_CONTRACT_REPORT)
      writeFileSync(process.env.OAD_CONTRACT_REPORT, JSON.stringify({ operations: [...seen.keys()].sort(), bad, known: knownHit, extra }, null, 2))
    expect(seen.size).toBeGreaterThan(50)
    expect(fmt(bad)).toBe('')
  }, 60_000)
})
