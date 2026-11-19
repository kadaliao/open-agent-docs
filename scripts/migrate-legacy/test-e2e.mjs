// 端到端：用旧版自己的代码造一份旧数据（make-fixture.py），起一个新版服务器，先 --dry-run（不能写任何东西），
// 再真导一遍、逐项核对，再跑一遍（什么都不该多出来）。
//   OAD_BIN=/path/to/open-agent-docs node test-e2e.mjs [旧版数据目录]
// 不给数据目录就现造（要 uv：cd legacy && uv run …）。先 pnpm --filter @oad/client build。
import assert from 'node:assert/strict'
import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createClient } from '@oad/client/core'

const here = new URL('.', import.meta.url).pathname
const root = join(here, '../..')
const bin = process.env.OAD_BIN ?? [join(root, 'target/release/open-agent-docs'), join(root, 'target/debug/open-agent-docs')].find(existsSync) ?? 'open-agent-docs'
const work = mkdtempSync(join(tmpdir(), 'oad-migrate-'))
let fixture = process.argv[2]
if (!fixture) {
  fixture = join(work, 'legacy')
  const r = spawnSync('uv', ['run', 'python', join(here, 'make-fixture.py'), fixture], { cwd: join(root, 'legacy'), stdio: 'inherit' })
  assert.equal(r.status, 0, '造旧数据失败（要 uv）')
}

const PORT = 18896
const URL_ = `http://127.0.0.1:${PORT}`
const srv = spawn(bin, ['serve', '--port', String(PORT)], { env: { ...process.env, OAD_AUTH: 'none', OAD_DATA_DIR: join(work, 'new'), RUST_LOG: 'warn' }, stdio: 'inherit' })
process.on('exit', () => srv.kill())
for (let i = 0; ; i++) {
  try {
    if ((await fetch(URL_ + '/api/v1/auth/config')).ok) break
  } catch {
    /* 还没起来 */
  }
  if (i > 150) throw new Error('新版服务器起不来')
  await new Promise((r) => setTimeout(r, 200))
}

const files = { map: join(work, 'map.json'), report: join(work, 'report.md'), tokens: join(work, 'tokens.txt') }
const run = (...extra) => {
  const r = spawnSync(process.execPath, [join(here, 'migrate.mjs'), '--from', fixture, '--to', URL_, '--map', files.map, '--report', files.report, '--tokens-out', files.tokens, ...extra], { encoding: 'utf8' })
  if (r.status) console.error(r.stdout, r.stderr)
  assert.equal(r.status, 0, '迁移失败')
  return r.stdout + r.stderr
}
const c = createClient({ baseUrl: URL_, credentials: 'omit' })
const me = await c.me()
const ws = me.workspaces[0].id

// 1. 只看不写
const dry = run('--dry-run')
assert.match(dry, /只看不写/)
assert.equal((await c.pages(ws)).length, 0, 'dry-run 不能建页面')
assert.equal((await c.agents(ws)).length, 0, 'dry-run 不能建 Agent')
assert.ok(!existsSync(files.map), 'dry-run 不写对照表')
assert.ok(!existsSync(files.tokens), 'dry-run 不写 token 文件')

// 2. 真导
const out = run()
const pages = await c.pages(ws)
const byTitle = (t) => pages.find((p) => p.title === t)
const home = byTitle('团队首页')
const plan = byTitle('季度计划')
const notes = byTitle('会议记录 11-15')
assert.ok(home && plan && notes && byTitle('第二个子页面') && byTitle('HTML 页面'), '页面都在')
assert.equal(plan.parent_id, home.id)
assert.equal(notes.parent_id, plan.id)
const md = async (id) => (await c.request('GET', `/pages/${id}/content`)).markdown
const homeMd = await md(home.id)
for (const s of ['# 欢迎', '**工作空间**', '[项目说明](https://example.com/spec)', '- [x] 写完方案', '> 先量，再改。', "```python", '| 名字 | 状态 |', '$$', '$a^2+b^2$']) assert.ok(homeMd.includes(s), `首页正文里有 ${s}：\n${homeMd}`)
const img = /!\[[^\]]*\]\(([^)]+)\)/.exec(homeMd)?.[1]
assert.ok(img && !img.includes('/api/media/'), '图片地址换成了新版的：' + img)
const imgRes = await fetch(new URL(img, URL_))
assert.equal(imgRes.status, 200)
assert.equal(imgRes.headers.get('content-type'), 'image/png')
const htmlMd = await md(byTitle('HTML 页面').id)
assert.match(htmlMd, /## 旧的 HTML 页面/)
assert.match(htmlMd, /\*\*粗体\*\*/)
assert.match(htmlMd, /\[链接\]\(https:\/\/example.com\)/)
assert.match(htmlMd, /- 一\n- 二/)

// 数据库
const proj = byTitle('项目')
const tasks = byTitle('任务')
assert.equal(proj.kind, 'database')
assert.equal(tasks.parent_id, home.id)
const tdb = await c.getDatabase(tasks.id)
const pdb = await c.getDatabase(proj.id)
const P = (db, name) => db.schema.props.find((p) => p.name === name)
for (const [n, t] of [['状态', 'status'], ['优先级', 'select'], ['标签', 'multi_select'], ['截止', 'date'], ['负责人', 'person'], ['工时', 'number'], ['完成', 'checkbox'], ['链接', 'url'], ['邮箱', 'email'], ['电话', 'phone'], ['备注', 'text'], ['附件', 'files'], ['项目', 'relation'], ['创建时间', 'created_time'], ['编号', 'unique_id'], ['人天', 'formula'], ['做完', 'button']])
  assert.equal(P(tdb, n)?.type, t, `任务的「${n}」是 ${t}`)
assert.equal(P(tdb, '编号').prefix, 'T-')
assert.deepEqual(P(tdb, '状态').options.map((o) => [o.name, o.group]), [['未开始', 'todo'], ['进行中', 'doing'], ['已完成', 'done']])
assert.equal(P(pdb, '任务')?.type, 'relation', '双向关联的另一半')
assert.ok(P(pdb, '任务').relation?.mirror_of, '另一半是反向的')
assert.equal(P(pdb, '总工时')?.type, 'rollup')
assert.equal(P(pdb, '任务数')?.type, 'rollup')
const trows = (await c.dbRows(tasks.id)).rows
assert.equal(trows.length, 4)
const row = (t) => trows.find((r) => (r.props[P(tdb, '名称').key] ?? r.props.title) === t)
const r1 = row('搜索框')
assert.equal(r1.props[P(tdb, '状态').key], P(tdb, '状态').options[1].id)
assert.deepEqual(r1.props[P(tdb, '截止').key], { start: '2026-11-20' })
assert.equal(r1.props[P(tdb, '工时').key], 16)
const agentsNow = await c.agents(ws)
const writer = agentsNow.find((a) => a.name === 'Writer')
assert.ok(writer && agentsNow.find((a) => a.name === 'Reader') && agentsNow.find((a) => a.name === 'Hub'), 'Agent 都建了')
assert.ok(!agentsNow.find((a) => a.name === 'Old'), '撤销了的不导')
assert.equal(writer.webhook, 'https://example.com/hooks/writer')
assert.deepEqual(r1.props[P(tdb, '负责人').key], [`agent:${writer.id}`])
const file = r1.props[P(tdb, '附件').key][0]
assert.equal(file.name, 'spec.pdf')
assert.equal((await fetch(new URL(file.url, URL_))).status, 200)
const r2 = row('索引')
assert.deepEqual(r2.props[P(tdb, '截止').key], { start: '2026-11-15T09:30', end: '2026-11-18', time: true })
assert.deepEqual(r2.props[P(tdb, '负责人').key], [`user:${me.user.id}`])
assert.equal(r2.props[P(tdb, '完成').key], true)
const prows = (await c.dbRows(proj.id)).rows
const search = prows.find((r) => Object.values(r.props).includes('搜索'))
assert.deepEqual(new Set(r1.props[P(tdb, '项目').key]), new Set([search.id]), '关联换成了新的行编号')
// 汇总算得出来（服务器查询）
const qr = await c.request('POST', `/databases/${proj.id}/query`, {})
const sr = qr.rows.find((r) => r.id === search.id)
assert.equal(sr.computed[P(pdb, '总工时').key], 40, '总工时 = 16 + 24')
// 行的正文
assert.match(await md(r1.id), /行的正文：搜索框/)
// 视图
const v1 = tdb.views.find((v) => v.name === '全部')
assert.deepEqual(v1.sorts, [{ prop: P(tdb, '工时').key, dir: 'desc' }])
assert.equal(v1.filter.items[0].prop, P(tdb, '完成').key)
assert.ok(v1.hidden.includes(P(tdb, '电话').key))
assert.equal(tdb.views.find((v) => v.name === '看板').group_by, P(tdb, '状态').key)

// 评论
const threads = await c.threads(home.id)
assert.equal(threads.length, 2)
const t1 = threads.find((t) => t.comments[0].body.includes('这段要再写清楚一点'))
assert.match(t1.comments[0].body, /^> 工作空间/)
assert.equal(t1.comments.length, 2)
assert.match(t1.comments[1].body, /^\*\*Writer\*\*：好的/)
assert.ok(threads.find((t) => t.comments[0].body.includes('标题太长')).resolved_at, '解决了的还是解决了')
assert.equal((await c.threads(plan.id)).length, 1)

// Agent 的新 token：只在文件里，权限 600，不在输出里
const tok = readFileSync(files.tokens, 'utf8')
assert.equal(statSync(files.tokens).mode & 0o777, 0o600)
const tokens = [...tok.matchAll(/\ttoken\t(\S+)/g)].map((m) => m[1])
assert.equal(tokens.length, 3)
for (const t of tokens) assert.ok(!out.includes(t), 'token 不能打在输出里')
const asAgent = createClient({ baseUrl: URL_, token: tokens[0], credentials: 'omit' })
assert.ok((await asAgent.pages(ws)).length > 0, '新 token 能用')

// 分享、回收站
const share = await c.getShare(home.id)
assert.ok(share?.url, '分享重建了')
assert.equal((await fetch(new URL(share.url, URL_))).status, 200)
const trash = await c.trash(ws)
assert.ok(trash.some((t) => t.title === '删掉的页面'), '回收站里的页面在新版的回收站里')

// 3. 再跑一遍：什么都不多
const before = { pages: (await c.pages(ws)).length, agents: (await c.agents(ws)).length, rows: trows.length, threads: threads.length }
run()
assert.equal((await c.pages(ws)).length, before.pages)
assert.equal((await c.agents(ws)).length, before.agents)
assert.equal((await c.dbRows(tasks.id)).rows.length, before.rows)
assert.equal((await c.threads(home.id)).length, before.threads)
assert.match(readFileSync(files.report, 'utf8'), /\| pages \| 0 \| 5 \|/)
assert.equal([...readFileSync(files.tokens, 'utf8').matchAll(/\ttoken\t/g)].length, 3, '没有重复建 Agent')

console.log('迁移端到端测试通过；报告：' + files.report)
srv.kill()
process.exit(0)
