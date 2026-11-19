#!/usr/bin/env node
// 把旧版（Python，legacy/）的数据导进新版服务器。读旧版的 SQLite（docs.db），经 @oad/client 调新版的接口写进去。
//
//   node scripts/migrate-legacy/migrate.mjs --from ~/.open-agent-docs --to http://127.0.0.1:8848 [--token <token>] \
//        [--tokens-out ./agent-tokens.txt] [--map ./oad-migration-map.json] [--report ./oad-migration-report.md] [--dry-run]
//
// 导什么：页面（树、顺序、正文；HTML 页面转成 Markdown）、图片和文件（重新上传，正文和文件属性里的地址换掉）、数据库（表结构、选项、
// 视图、行和行的正文、关联（双向的也是）、汇总、公式、按钮）、评论（成串，回复、解决了的）、Agent（重新建，新 token 写进 --tokens-out
// 指定的文件，不打在屏幕上）、公开分享（重新生成链接）、回收站里的页面（导进去再删，留在新版的回收站里）。
// 再跑一次只补没导过的（按 --map 文件）。--dry-run 只看会导什么，不写。
// 要 Node 22.5+（用内置的 node:sqlite）。
import { chmodSync, existsSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { DatabaseSync } from 'node:sqlite'
import { parseArgs } from 'node:util'
import { createClient } from '@oad/client/core'
import { html2md } from './html2md.mjs'

const { values: args } = parseArgs({
  options: {
    from: { type: 'string' },
    to: { type: 'string', default: process.env.OAD_URL ?? 'http://127.0.0.1:8848' },
    token: { type: 'string', default: process.env.OAD_TOKEN },
    header: { type: 'string', multiple: true, default: [] },
    workspace: { type: 'string' },
    map: { type: 'string', default: './oad-migration-map.json' },
    report: { type: 'string', default: './oad-migration-report.md' },
    'tokens-out': { type: 'string' },
    'dry-run': { type: 'boolean', default: false },
    'no-agents': { type: 'boolean', default: false },
    'no-comments': { type: 'boolean', default: false },
    'no-shares': { type: 'boolean', default: false },
    'no-trash': { type: 'boolean', default: false },
    help: { type: 'boolean', short: 'h', default: false },
  },
})

if (args.help || !args.from) {
  console.log(`用法：node migrate.mjs --from <旧版数据目录或 docs.db> --to <新版地址> [选项]

  --token <t>          新版的 token（Agent 的 admin token 或登录会话）；也可以设 OAD_TOKEN。不登录模式不用给
  --header k:v         每个请求多带的头（header 登录时：--header x-oad-user-id:me --header x-oad-secret:...），可以给多次
  --workspace <id>     导进哪个工作区（默认第一个）
  --map <file>         记已经导过的（旧编号 → 新编号），再跑一次只补没导的。默认 ./oad-migration-map.json
  --report <file>      报告（Markdown）。默认 ./oad-migration-report.md
  --tokens-out <file>  重建的 Agent 的新 token 写到这里（权限 600）。不给就不建 Agent
  --dry-run            只看会导什么，不写
  --no-agents --no-comments --no-shares --no-trash   这几样不导`)
  process.exit(args.help ? 0 : 1)
}

const DRY = args['dry-run']
const dbPath = statSync(args.from).isDirectory() ? join(args.from, 'docs.db') : args.from
if (!existsSync(dbPath)) throw new Error(`找不到旧版的数据库：${dbPath}`)
const old = new DatabaseSync(dbPath, { readOnly: true })
const q = (sql, ...p) => old.prepare(sql).all(...p)
const has = (table) => q("select name from sqlite_master where type='table' and name=?", table).length > 0
const cols = (table) => new Set(q(`pragma table_info(${table})`).map((r) => r.name))

const headers = Object.fromEntries(args.header.map((h) => [h.slice(0, h.indexOf(':')).trim().toLowerCase(), h.slice(h.indexOf(':') + 1).trim()]))
const client = createClient({ baseUrl: args.to, token: args.token, credentials: 'omit', headers })

// ---------- 记录（幂等） ----------
const mapFile = resolve(args.map)
const map = existsSync(mapFile)
  ? JSON.parse(readFileSync(mapFile, 'utf8'))
  : { version: 1, source: resolve(dbPath), server: args.to, pages: {}, rows: {}, media: {}, agents: {}, comments: {}, shares: {}, steps: {} }
for (const k of ['pages', 'rows', 'media', 'agents', 'comments', 'shares', 'steps']) map[k] ??= {}
const save = () => !DRY && writeFileSync(mapFile, JSON.stringify(map, null, 2))

// ---------- 报告 ----------
const report = { created: {}, skipped: {}, warnings: [], shares: [], notes: new Set() }
const count = (kind, how = 'created') => (report[how][kind] = (report[how][kind] ?? 0) + 1)
const warn = (s) => {
  report.warnings.push(s)
  console.warn('注意：' + s)
}
const log = (s) => console.log(s)

// ---------- 读旧数据 ----------
const pcols = cols('pages')
const sel = ['id', 'title', 'format', 'content', 'share_token', 'pinned', 'created', 'updated'].concat(['parent_id', 'pos', 'kind', 'schema', 'views', 'props', 'agent_id'].filter((c) => pcols.has(c)))
const pages = q(`select ${sel.join(', ')} from pages order by coalesce(pos, 0), id`).map((p) => ({ ...p, kind: p.kind ?? 'page', parent_id: p.parent_id ?? null }))
const byId = new Map(pages.map((p) => [p.id, p]))
const agentsOld = has('agents') ? q('select id, name, scope, about, webhook, revoked from agents order by id') : []
const agentName = new Map(agentsOld.map((a) => [a.id, a.name]))
const comments = has('page_comments') ? q('select id, page_id, block_id, parent_id, by, text, quote, ts, resolved from page_comments order by id') : []
const trash = has('page_trash') && !args['no-trash'] ? q('select id, page_id, title, kind, format, deleted, data from page_trash order by id') : []
const mediaInfo = has('media') ? new Map(q('select id, mime, name, size from media').map((m) => [m.id, m])) : new Map()

// 父页面先建：按树的深度排
const depth = (p, seen = new Set()) => (p.parent_id && byId.has(p.parent_id) && !seen.has(p.id) ? 1 + depth(byId.get(p.parent_id), seen.add(p.id)) : 0)
const tree = pages.filter((p) => p.kind !== 'row').sort((a, b) => depth(a) - depth(b) || (a.pos ?? 0) - (b.pos ?? 0) || a.id - b.id)

log(`旧版：${tree.filter((p) => p.kind === 'page').length} 页、${tree.filter((p) => p.kind === 'db').length} 个数据库、${pages.filter((p) => p.kind === 'row').length} 行、` +
  `${comments.length} 条评论、${agentsOld.filter((a) => !a.revoked).length} 个 Agent、${mediaInfo.size} 个文件、回收站 ${trash.length} 项`)

// ---------- 新版：登录、工作区 ----------
const me = await client.me().catch((e) => {
  throw new Error(`连不上新版或者没登录（${args.to}）：${e.message}`)
})
const ws = args.workspace ?? me.workspaces[0]?.id
if (!ws) throw new Error('新版里没有工作区')
map.workspace ??= ws
log(`新版：${args.to}，工作区 ${ws}，以 ${me.user.name} 的身份导入${DRY ? '（只看不写）' : ''}`)
// 旧版的“人员”值里的 user（旧版只有一个人）：用 Agent token 导时换成建这个 Agent 的人，再不行用工作区的第一个所有者
let owner = { id: me.user.id, name: me.user.name }
if (me.user.kind === 'agent') {
  const r = await client.request('GET', `/workspaces/${ws}/members`)
  const by = r.agents?.find((a) => a.id === me.user.id)?.created_by
  const m = r.members?.find((x) => x.user.id === by) ?? r.members?.find((x) => x.role === 'owner') ?? r.members?.[0]
  if (m) owner = { id: m.user.id, name: m.user.name }
}

// ---------- Agent ----------
const tokensOut = args['tokens-out'] ? resolve(args['tokens-out']) : null
if (!args['no-agents']) {
  const todo = agentsOld.filter((a) => !a.revoked && !map.agents[a.id])
  if (todo.length && !tokensOut && !DRY) warn(`有 ${todo.length} 个 Agent 没导：要给 --tokens-out <文件>（新 token 写在那里）`)
  else {
    const lines = []
    for (const a of todo) {
      if (DRY) {
        count('agents')
        map.agents[a.id] = `(dry-run-agent-${a.id})`
        continue
      }
      const scope = ['read', 'write', 'admin'].includes(a.scope) ? a.scope : 'write'
      const r = await client.createAgent(ws, { name: a.name, about: a.about || undefined, scope, webhook: a.webhook || null }).catch((e) => {
        warn(`Agent「${a.name}」建不了：${e.message}`)
        return null
      })
      if (!r) continue
      map.agents[a.id] = r.agent.id
      save()
      lines.push(`# ${a.name}（旧编号 ${a.id}，权限 ${scope}）`, `${a.name}\ttoken\t${r.token}`)
      if (a.webhook) lines.push(`${a.name}\twebhook_secret\t${r.webhook_secret}`)
      count('agents')
    }
    if (lines.length) {
      const head = existsSync(tokensOut) ? readFileSync(tokensOut, 'utf8') : '# Open Agent Docs 迁移：重新建的 Agent 的 token（只在这里出现一次，旧的 token 在新版不能用）\n'
      writeFileSync(tokensOut, head + lines.join('\n') + '\n', { mode: 0o600 })
      chmodSync(tokensOut, 0o600)
      log(`新 token 写进了 ${tokensOut}（权限 600）`)
    }
  }
  for (const a of agentsOld.filter((x) => x.revoked)) count('agents（已撤销，不导）', 'skipped')
}
const personOf = (v) => {
  if (v === 'user') return `user:${owner.id}`
  const m = /^agent:(\d+)$/.exec(String(v))
  if (m && map.agents[m[1]]) return `agent:${map.agents[m[1]]}`
  if (m) return null
  return null
}

// ---------- 文件 ----------
const MEDIA_RE = /\/api\/media\/([0-9a-f]{24})/g
async function mediaUrl(mid) {
  if (map.media[mid]) return map.media[mid]
  const info = mediaInfo.get(mid)
  if (!info) {
    warn(`文件 ${mid} 在旧版里已经没有了（被清理了），引用保持原样`)
    return null
  }
  if (DRY) {
    count('files')
    return (map.media[mid] = `/api/v1/files/(dry-run)/${info.name}`)
  }
  const row = q('select data from media where id=?', mid)[0]
  const blob = new Blob([row.data], { type: info.mime || 'application/octet-stream' })
  const file = new File([blob], info.name || `${mid}`, { type: info.mime || 'application/octet-stream' })
  const up = await client.upload(ws, file)
  map.media[mid] = up.url
  save()
  count('files')
  return up.url
}
async function rewriteMedia(md) {
  let out = md ?? ''
  for (const mid of new Set([...out.matchAll(MEDIA_RE)].map((m) => m[1]))) {
    const url = await mediaUrl(mid)
    if (url) out = out.split(`/api/media/${mid}`).join(url)
  }
  return out
}
const pageMarkdown = async (p) => rewriteMedia(p.format === 'html' ? html2md(p.content) : p.content ?? '')

// ---------- 数据库：表结构 ----------
const ROLLUP_FN = { show: 'show_original' }
const SKIP_LATER = new Set(['relation', 'rollup', 'formula', 'button'])
function convertProp(p, phase) {
  const out = { key: p.key, name: p.name, type: p.type }
  if (p.options) out.options = p.options.map((o) => ({ id: o.id, name: o.name, color: o.color ?? 'default', ...(o.group ? { group: o.group } : {}) }))
  if (p.format) out.format = p.format
  if (p.type === 'unique_id') out.prefix = p.prefix ?? ''
  if (phase === 'later') {
    if (p.type === 'formula') out.formula = p.formula ?? ''
    if (p.type === 'button') out.button = { label: p.label ?? p.name, set: p.set ?? {} }
  }
  return out
}
const schemaOf = (p) => {
  try {
    const s = JSON.parse(p.schema || '[]')
    return Array.isArray(s) ? s : s.props ?? []
  } catch {
    return []
  }
}
const OP_COND = { eq: 'eq', neq: 'neq', contains: 'contains', not_contains: 'not_contains', gt: 'gt', gte: 'gte', lt: 'lt', lte: 'lte', empty: 'empty', not_empty: 'not_empty' }
function convertViews(p, keyMap) {
  let vs = []
  try {
    vs = JSON.parse(p.views || '[]')
  } catch {
    /* 没有视图 */
  }
  const k = (x) => keyMap.get(x) ?? x
  return vs.map((v) => ({
    id: v.id,
    name: v.name,
    type: v.type === 'board' ? 'board' : 'table',
    sorts: (v.sort ?? []).map((s) => ({ prop: k(s.prop), dir: s.desc || s.dir === 'desc' ? 'desc' : 'asc' })),
    filter: { op: 'and', items: (v.filter ?? []).map((f) => ({ prop: k(f.prop), cond: OP_COND[f.op] ?? 'eq', ...(f.value !== undefined ? { value: f.value } : {}) })) },
    ...(v.group_by ? { group_by: k(v.group_by) } : {}),
    hidden: (v.hidden ?? []).map(k),
    order: [],
    widths: Object.fromEntries(Object.entries(v.widths ?? {}).map(([a, b]) => [k(a), b])),
  }))
}

// ---------- 数据库：值 ----------
function isoDate(s) {
  const m = /^(\d{4}-\d{2}-\d{2})(?:[ T](\d{2}:\d{2}))?$/.exec(String(s ?? '').trim())
  return m ? { d: m[1] + (m[2] ? 'T' + m[2] : ''), time: !!m[2] } : null
}
async function convertValue(p, v, where) {
  if (v === null || v === undefined || v === '' || (Array.isArray(v) && !v.length)) return undefined
  switch (p.type) {
    case 'title':
    case 'text':
    case 'url':
    case 'email':
    case 'phone':
      return String(v)
    case 'number':
      return typeof v === 'number' ? v : Number(v)
    case 'checkbox':
      return !!v
    case 'select':
    case 'status':
      return v
    case 'multi_select':
      return Array.isArray(v) ? v : [v]
    case 'date': {
      const a = isoDate(v.start)
      const b = v.end ? isoDate(v.end) : null
      if (!a) {
        warn(`${where}：日期「${JSON.stringify(v)}」认不出，没导`)
        return undefined
      }
      return { start: a.d, ...(b ? { end: b.d } : {}), ...(a.time || b?.time ? { time: true } : {}) }
    }
    case 'person': {
      const list = (Array.isArray(v) ? v : [v]).map((x) => personOf(x) ?? (warn(`${where}：人员「${x}」在新版里没有（Agent 没导），去掉了`), null)).filter(Boolean)
      return list.length ? list : undefined
    }
    case 'files': {
      const out = []
      for (const f of Array.isArray(v) ? v : [v]) {
        const url = String(f.url ?? f)
        const m = /\/api\/media\/([0-9a-f]{24})/.exec(url)
        const nu = m ? await mediaUrl(m[1]) : url
        if (nu) out.push({ url: nu, name: f.name ?? url.split('/').pop() })
      }
      return out.length ? out : undefined
    }
    default:
      return undefined // 关联第二遍再填；算出来的、自动编号、按钮不存值
  }
}

// ---------- 页面和数据库 ----------
const shareOf = new Map()
async function importPage(p) {
  const parent = p.parent_id ? map.pages[p.parent_id] ?? null : null
  if (p.parent_id && !parent && byId.get(p.parent_id)?.kind !== 'row') warn(`「${p.title}」的上级 #${p.parent_id} 没导进去，放到最上面`)
  if (map.pages[p.id]) {
    count(p.kind === 'db' ? 'databases' : 'pages', 'skipped')
    return
  }
  if (p.kind === 'db') {
    const sch = schemaOf(p)
    if (sch[0]?.type !== 'title') sch.unshift({ key: 'title', name: '名称', type: 'title' })
    const now = sch.filter((x) => !SKIP_LATER.has(x.type)).map((x) => convertProp(x, 'now'))
    if (DRY) {
      count('databases')
      map.pages[p.id] = `(dry-run-db-${p.id})`
      return
    }
    const r = await client.createDatabase(ws, { title: p.title || '', ...(parent ? { parent_id: parent } : {}), schema: { props: now } })
    map.pages[p.id] = r.page.id
    save()
    count('databases')
    return
  }
  if (p.format === 'html') report.notes.add('HTML 页面转成了 Markdown（只认常见的标签）')
  const md = await pageMarkdown(p)
  if (DRY) {
    count('pages')
    map.pages[p.id] = `(dry-run-page-${p.id})`
    return
  }
  const np = await client.createPage(ws, { title: p.title || '', ...(parent ? { parent_id: parent } : {}) })
  map.pages[p.id] = np.id
  save()
  if (md.trim()) await client.request('PUT', `/pages/${np.id}/content`, { markdown: md })
  count('pages')
}

for (const p of tree) {
  await importPage(p).catch((e) => warn(`「${p.title}」（#${p.id}）没导进去：${e.message}`))
  if (p.share_token) shareOf.set(p.id, p.share_token)
  if (p.pinned) report.notes.add('置顶在新版里没有，置顶过的页面照常导入')
}

// 数据库的行
const dbs = tree.filter((p) => p.kind === 'db' && map.pages[p.id])
for (const db of dbs) {
  const sch = schemaOf(db)
  const rows = pages.filter((r) => r.kind === 'row' && r.parent_id === db.id)
  const todo = rows.filter((r) => !map.rows[r.id])
  rows.filter((r) => map.rows[r.id]).forEach(() => count('rows', 'skipped'))
  for (let i = 0; i < todo.length; i += 500) {
    const part = todo.slice(i, i + 500)
    const input = []
    for (const r of part) {
      let props = {}
      try {
        props = JSON.parse(r.props || '{}')
      } catch {
        /* 空 */
      }
      const out = {}
      for (const p of sch) {
        if (p.type === 'title') {
          if (r.title) out[p.key] = r.title
          continue
        }
        const v = await convertValue(p, props[p.key], `「${db.title}」的「${r.title || '#' + r.id}」的「${p.name}」`)
        if (v !== undefined) out[p.key] = v
      }
      const body = await rewriteMedia(r.format === 'html' ? html2md(r.content) : r.content ?? '')
      input.push({ props: out, ...(body.trim() ? { body } : {}) })
    }
    if (DRY) {
      part.forEach((r) => {
        count('rows')
        map.rows[r.id] = `(dry-run-row-${r.id})`
      })
      continue
    }
    const res = await client.request('POST', `/databases/${map.pages[db.id]}/rows`, { rows: input }).catch((e) => {
      warn(`「${db.title}」的行没导进去：${e.message}`)
      return null
    })
    if (!res) continue
    part.forEach((r, k) => {
      map.rows[r.id] = res.ids[k]
      count('rows')
    })
    save()
  }
}

// 关联、汇总、公式、按钮（要等所有库和行都有了）
for (const db of dbs) {
  if (map.steps[`schema2:${db.id}`] || DRY) {
    if (DRY) for (const p of schemaOf(db).filter((x) => SKIP_LATER.has(x.type) && !x.mirror_of)) count(`属性（${p.type}）`)
    continue
  }
  const newId = map.pages[db.id]
  const sch = schemaOf(db)
  const ops = []
  for (const p of sch.filter((x) => x.type === 'relation' && !x.mirror_of)) {
    const target = p.db === 'self' || p.db === db.id ? newId : map.pages[p.db]
    if (!target) {
      warn(`「${db.title}」的关联「${p.name}」指向的库没导进去，跳过`)
      continue
    }
    const pair = p.pair ? schemaOf(byId.get(p.db) ?? {}).find((x) => x.key === p.pair || x.mirror_of === p.key) : null
    ops.push({ op: 'add', name: p.name, type: 'relation', db_id: target, ...(pair ? { twoway: true, pair_name: pair.name } : {}) })
  }
  if (ops.length) {
    const r = await client.patchSchema(newId, ops).catch((e) => (warn(`「${db.title}」加关联失败：${e.message}`), null))
    if (r) ops.forEach(() => count('属性（relation）'))
  }
  map.steps[`schema2:${db.id}`] = true
  save()
}
// 汇总、公式、按钮在关联之后（汇总要找关联属性；双向关联的另一半这时也有了）
for (const db of dbs) {
  if (map.steps[`schema3:${db.id}`] || DRY) continue
  const newId = map.pages[db.id]
  const sch = schemaOf(db)
  const cur = (await client.getDatabase(newId)).schema.props
  const nameOfKey = new Map(sch.map((p) => [p.key, p.name]))
  const ops = []
  for (const p of sch) {
    if (cur.some((c) => c.name === p.name)) continue
    if (p.type === 'rollup') {
      const relName = nameOfKey.get(p.relation)
      const relOld = sch.find((x) => x.key === p.relation)
      const otherOld = relOld ? byId.get(relOld.db === 'self' ? db.id : relOld.db) : null
      const targetName = otherOld ? schemaOf(otherOld).find((x) => x.key === p.target)?.name : null
      if (!relName || !targetName) {
        warn(`「${db.title}」的汇总「${p.name}」找不到关联或目标属性，跳过`)
        continue
      }
      ops.push({ op: 'add', name: p.name, type: 'rollup', relation: relName, prop: targetName, fn: ROLLUP_FN[p.fn] ?? p.fn, ...(p.format ? { format: p.format } : {}) })
    } else if (p.type === 'formula') ops.push({ op: 'add', name: p.name, type: 'formula', formula: p.formula ?? '', ...(p.format ? { format: p.format } : {}) })
    else if (p.type === 'button') {
      const set = Object.fromEntries(Object.entries(p.set ?? {}).map(([k, v]) => [nameOfKey.get(k) ?? k, v]))
      ops.push({ op: 'add', name: p.name, type: 'button', label: p.label ?? p.name, set })
      if (p.agent || p.comment) report.notes.add('按钮的“写评论、叫 Agent”设置没导（新版按钮只改属性）')
    }
  }
  for (const op of ops) {
    // 一个一个加：一个写错了（比如公式里有新版没有的函数）不影响别的
    const r = await client.patchSchema(newId, [op]).catch((e) => (warn(`「${db.title}」加「${op.name}」（${op.type}）失败：${e.message}`), null))
    if (r) count(`属性（${op.type}）`)
    else if (op.type === 'formula') {
      // 公式加不上：留一列文字，内容是原来的公式，方便手工改
      await client.patchSchema(newId, [{ op: 'add', name: op.name, type: 'text' }]).catch(() => {})
    }
  }
  map.steps[`schema3:${db.id}`] = true
  save()
}
// 关联的值、视图
for (const db of dbs) {
  if (DRY) continue
  const newId = map.pages[db.id]
  const sch = schemaOf(db)
  const cur = (await client.getDatabase(newId)).schema.props
  const keyMap = new Map(sch.map((p) => [p.key, cur.find((c) => c.name === p.name)?.key ?? p.key]))
  if (!map.steps[`rel:${db.id}`]) {
    const ops = []
    for (const r of pages.filter((x) => x.kind === 'row' && x.parent_id === db.id && map.rows[x.id])) {
      const props = JSON.parse(r.props || '{}')
      const set = {}
      for (const p of sch.filter((x) => x.type === 'relation' && !x.mirror_of)) {
        const ids = (props[p.key] ?? []).map((i) => map.rows[i]).filter(Boolean)
        if (ids.length && keyMap.get(p.key)) set[keyMap.get(p.key)] = ids
      }
      if (Object.keys(set).length) ops.push({ row_id: map.rows[r.id], set })
    }
    if (ops.length) await client.patchRows(newId, ops).catch((e) => warn(`「${db.title}」的关联没填上：${e.message}`))
    map.steps[`rel:${db.id}`] = true
    save()
  }
  if (!map.steps[`views:${db.id}`]) {
    const views = convertViews(db, keyMap)
    if (views.length) await client.putViews(newId, views).catch((e) => warn(`「${db.title}」的视图没导：${e.message}`))
    map.steps[`views:${db.id}`] = true
    save()
  }
  if (sch.some((p) => p.type === 'unique_id')) report.notes.add('自动编号由新版重新发号（按导入的顺序，从 1 开始）')
  if (sch.some((p) => /^(created|updated)_(time|by)$/.test(p.type))) report.notes.add('创建时间、编辑时间、创建人这些按导入的时候算（新版不能写）')
}

// ---------- 评论 ----------
if (!args['no-comments']) {
  const roots = comments.filter((c) => !c.parent_id)
  for (const c of roots) {
    const page = map.pages[c.page_id] ?? map.rows[c.page_id]
    if (!page) {
      count('comments（页面没导）', 'skipped')
      continue
    }
    const thread = comments.filter((x) => x.id === c.id || x.parent_id === c.id)
    const fmt = (x) => {
      const who = x.by === 'user' ? '' : /^agent:(\d+)$/.test(x.by) ? `**${agentName.get(Number(x.by.slice(6))) ?? x.by}**：` : `**${x.by}**：`
      return who + x.text
    }
    if (DRY) {
      thread.forEach(() => count('comments'))
      continue
    }
    let tid = map.comments[c.id]
    if (!tid) {
      const body = (c.quote ? `> ${c.quote.replace(/\n/g, ' ')}\n\n` : '') + fmt(c)
      const th = await client.createThread(page, { anchor: null, body, notify: false }).catch((e) => (warn(`评论 #${c.id} 没导：${e.message}`), null))
      if (!th) continue
      tid = map.comments[c.id] = th.id
      save()
      count('comments')
    } else count('comments', 'skipped')
    for (const r of thread.filter((x) => x.id !== c.id)) {
      if (map.comments[r.id]) {
        count('comments', 'skipped')
        continue
      }
      const rc = await client.reply(tid, { body: fmt(r), notify: false }).catch((e) => (warn(`回复 #${r.id} 没导：${e.message}`), null))
      if (rc) {
        map.comments[r.id] = rc.id
        save()
        count('comments')
      }
    }
    if (c.resolved && !map.steps[`resolved:${c.id}`]) {
      await client.resolveThread(tid, true).catch(() => {})
      map.steps[`resolved:${c.id}`] = true
      save()
    }
  }
  if (comments.some((c) => c.block_id)) report.notes.add('评论都挂在整页上（旧版的段编号新版不认）；原来引用的那段文字放在评论开头')
  if (comments.some((c) => c.by !== 'user')) report.notes.add('Agent 写的评论以导入的人的身份发出，开头写着原来的作者')
}

// ---------- 公开分享 ----------
if (!args['no-shares']) {
  for (const [oid] of shareOf) {
    const page = map.pages[oid]
    if (!page) continue
    if (map.shares[oid]) {
      report.shares.push([byId.get(oid).title, map.shares[oid]])
      continue
    }
    if (DRY) {
      count('shares')
      continue
    }
    const s = await client.createShare(page, { include_children: false }).catch((e) => (warn(`「${byId.get(oid).title}」的分享没建：${e.message}`), null))
    if (s) {
      const url = s.url.startsWith('http') ? s.url : args.to.replace(/\/$/, '') + s.url
      map.shares[oid] = url
      save()
      report.shares.push([byId.get(oid).title, url])
      count('shares')
    }
  }
}

// ---------- 回收站 ----------
for (const t of trash) {
  if (map.steps[`trash:${t.id}`]) {
    count('trash', 'skipped')
    continue
  }
  let dump = []
  try {
    dump = JSON.parse(t.data || '{}').pages ?? []
  } catch {
    /* 坏了 */
  }
  const top = dump.find((x) => x.id === t.page_id) ?? dump[0]
  if (!top || (top.kind ?? 'page') !== 'page') {
    warn(`回收站里的「${t.title}」（${t.kind}）没导：只导回收站里的普通页面`)
    continue
  }
  if (DRY) {
    count('trash')
    continue
  }
  const np = await client.createPage(ws, { title: top.title || t.title || '' })
  const md = await rewriteMedia(top.format === 'html' ? html2md(top.content) : top.content ?? '')
  if (md.trim()) await client.request('PUT', `/pages/${np.id}/content`, { markdown: md })
  await client.deletePage(np.id)
  map.steps[`trash:${t.id}`] = np.id
  save()
  count('trash')
}

// ---------- 报告 ----------
report.notes.add('页面的历史版本没导（新版从导入的这一版开始记）')
report.notes.add('正文以导入的人的身份写入')
report.notes.add(`旧版人员属性里的“我”换成了 ${owner.name}`)
const lines = [
  `# Open Agent Docs 迁移报告`,
  '',
  `- 时间：${new Date().toISOString()}`,
  `- 旧版：${resolve(dbPath)}`,
  `- 新版：${args.to}（工作区 ${ws}）`,
  `- 模式：${DRY ? '只看不写（--dry-run）' : '导入'}`,
  `- 对照表：${mapFile}${tokensOut ? `\n- 新 token：${tokensOut}` : ''}`,
  '',
  '## 数量',
  '',
  '| 种类 | 这次导了 | 以前导过、跳过 |',
  '| --- | --- | --- |',
  ...[...new Set([...Object.keys(report.created), ...Object.keys(report.skipped)])].map((k) => `| ${k} | ${report.created[k] ?? 0} | ${report.skipped[k] ?? 0} |`),
  '',
  ...(report.shares.length ? ['## 公开分享的新链接', '', ...report.shares.map(([t, u]) => `- ${t}：${u}`), ''] : []),
  '## 和旧版不一样的地方',
  '',
  ...[...report.notes].map((n) => `- ${n}`),
  '',
  `## 注意（${report.warnings.length}）`,
  '',
  ...(report.warnings.length ? report.warnings.map((w) => `- ${w}`) : ['- 没有']),
  '',
]
writeFileSync(resolve(args.report), lines.join('\n'))
log(`\n${DRY ? '（只看不写）' : '导完了。'}报告：${resolve(args.report)}`)
log(Object.entries(report.created).map(([k, v]) => `${k} ${v}`).join('、') || '没有要导的')
