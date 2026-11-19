// 新版（Rust 二进制）对着 docs/rewrite-plan.md 的目标表量服务端那几项：冷启动、空闲内存、二进制大小、
// 10 万段搜索（和旧版 bench.py 同一份语料、同样的查询：先跑 gen-corpus.py）、X / YouTube 卡片（接口和浏览器里贴链接）。
// 浏览器里打开 1000 段页面、打字、1 万行数据库在 apps/web/e2e 的 perf.spec.ts / db-perf.spec.ts 里量。
//
//   uv run bench/gen-corpus.py /tmp/corpus.json
//   node bench/bench-new.mjs --bin target/release/open-agent-docs --corpus /tmp/corpus.json --out /tmp/oad-new [--rounds 3] [--only coldstart,search,unfurl]
//
// 每项跑 --rounds 轮，结果里每轮的数都留着，再给中位数；每项前后记 load average（机器忙不忙）。不打印 token。
import { spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { loadavg, tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { parseArgs } from 'node:util'
import { execFileSync } from 'node:child_process'

const { values: a } = parseArgs({
  options: {
    bin: { type: 'string' },
    corpus: { type: 'string' },
    out: { type: 'string', default: join(tmpdir(), 'oad-bench-new') },
    rounds: { type: 'string', default: '3' },
    only: { type: 'string', default: 'footprint,coldstart,search,unfurl' },
    cold: { type: 'string', default: '7' },
  },
})
const BIN = resolve(a.bin)
const ROUNDS = Number(a.rounds)
const OUT = resolve(a.out)
mkdirSync(OUT, { recursive: true })
const only = new Set(a.only.split(','))
const results = existsSync(join(OUT, 'results.json')) ? JSON.parse(readFileSync(join(OUT, 'results.json'), 'utf8')) : {}
const save = () => writeFileSync(join(OUT, 'results.json'), JSON.stringify(results, null, 1))
const log = (...x) => console.log(...x)
const la = () => loadavg().map((x) => +x.toFixed(1))

// ---------- 统计 ----------
const pct = (xs, p) => {
  const s = [...xs].sort((x, y) => x - y)
  if (!s.length) return null
  const k = (s.length - 1) * (p / 100)
  const lo = Math.floor(k)
  return +(s[lo] + (s[Math.min(lo + 1, s.length - 1)] - s[lo]) * (k - lo)).toFixed(2)
}
const summ = (xs) => ({ n: xs.length, p50: pct(xs, 50), p95: pct(xs, 95), min: pct(xs, 0), max: pct(xs, 100) })
const median = (xs) => pct(xs, 50)
const ms = (t0) => Number(process.hrtime.bigint() - t0) / 1e6
const now = () => process.hrtime.bigint()

// ---------- 服务器 ----------
let nextPort = 19100
async function start(data, env = {}) {
  const port = nextPort++
  const t0 = now()
  const proc = spawn(BIN, ['serve', '--port', String(port)], {
    // 限流关掉：灌 10 万段、跑几百次查询会超过默认的每分钟 1200 次
    env: { ...process.env, OAD_AUTH: 'none', OAD_DATA_DIR: data, RUST_LOG: 'warn', OAD_RATE_LIMIT: '0', ...env },
    stdio: ['ignore', 'ignore', 'pipe'],
  })
  let err = ''
  proc.stderr.on('data', (d) => (err += d))
  const base = `http://127.0.0.1:${port}`
  for (;;) {
    try {
      const r = await fetch(base + '/api/v1/auth/config')
      if (r.ok) break
    } catch {
      /* 还没起来 */
    }
    if (proc.exitCode !== null) throw new Error('服务器退出了：' + err)
    if (ms(t0) > 30_000) throw new Error('服务器 30 秒没起来')
    await new Promise((r) => setTimeout(r, 1))
  }
  const ready = ms(t0)
  return {
    proc,
    base,
    ready,
    rss: () => Number(execFileSync('ps', ['-o', 'rss=', '-p', String(proc.pid)]).toString().trim()) / 1024,
    stop: () => new Promise((r) => (proc.once('exit', r), proc.kill('SIGTERM'))),
  }
}
const api = (base, token) => async (method, path, body) => {
  const r = await fetch(base + '/api/v1' + path, {
    method,
    headers: { ...(body ? { 'content-type': 'application/json' } : {}), ...(token ? { authorization: 'Bearer ' + token } : {}) },
    body: body ? JSON.stringify(body) : undefined,
  })
  if (!r.ok) throw new Error(`${method} ${path} → ${r.status} ${await r.text()}`)
  return r.status === 204 ? null : r.json()
}
const du = (p) => {
  let n = 0
  for (const f of readdirSync(p, { withFileTypes: true })) n += f.isDirectory() ? du(join(p, f.name)) : statSync(join(p, f.name)).size
  return n
}
const MB = (n) => +(n / 1048576).toFixed(1)

// ---------- MCP（Streamable HTTP） ----------
async function mcpSession(base, token) {
  let sid = null
  let id = 0
  const post = async (msg) => {
    const r = await fetch(base + '/mcp', {
      method: 'POST',
      headers: { 'content-type': 'application/json', accept: 'application/json, text/event-stream', authorization: 'Bearer ' + token, ...(sid ? { 'mcp-session-id': sid } : {}) },
      body: JSON.stringify(msg),
    })
    sid = r.headers.get('mcp-session-id') ?? sid
    const text = await r.text()
    if (!text) return null
    if ((r.headers.get('content-type') ?? '').includes('text/event-stream')) {
      const data = text.split('\n').filter((l) => l.startsWith('data:')).map((l) => l.slice(5).trim()).filter(Boolean)
      return data.length ? JSON.parse(data.at(-1)) : null
    }
    return JSON.parse(text)
  }
  await post({ jsonrpc: '2.0', id: ++id, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'bench', version: '1' } } })
  await post({ jsonrpc: '2.0', method: 'notifications/initialized' })
  return { call: (name, args) => post({ jsonrpc: '2.0', id: ++id, method: 'tools/call', params: { name, arguments: args } }) }
}

// ---------- 大小 ----------
if (only.has('footprint')) {
  const webOut = (() => {
    const root = resolve(BIN, '../build')
    if (!existsSync(root)) return null
    const d = readdirSync(root).filter((x) => x.startsWith('oad-server-')).map((x) => join(root, x, 'out/web')).filter(existsSync)
    return d.length ? MB(Math.max(...d.map(du))) : null
  })()
  let image = null
  try {
    image = MB(Number(execFileSync('docker', ['image', 'inspect', 'open-agent-docs:bench', '--format', '{{.Size}}'], { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim()))
  } catch {
    /* 没有镜像 */
  }
  results.footprint = { binary_mb: MB(statSync(BIN).size), embedded_web_mb: webOut, docker_image_mb: image }
  log('大小：', results.footprint)
  save()
}

// ---------- 冷启动、空闲内存 ----------
if (only.has('coldstart')) {
  const before = la()
  const runs = Number(a.cold)
  const out = { empty: [], existing: [], rss_empty: [], rss_existing: [] }
  // 已有数据：一个工作区、20 页
  const existing = mkdtempSync(join(tmpdir(), 'oad-cold-'))
  {
    const s = await start(existing)
    const call = api(s.base)
    const ws = (await call('GET', '/me')).workspaces[0].id
    for (let i = 0; i < 20; i++) {
      const p = await call('POST', `/workspaces/${ws}/pages`, { title: '页 ' + i })
      await call('PUT', `/pages/${p.id}/content`, { markdown: `# 页 ${i}\n\n一段正文 ${i}\n` })
    }
    await s.stop()
  }
  for (let r = 0; r < ROUNDS; r++)
    for (let i = 0; i < runs; i++) {
      const e = await start(mkdtempSync(join(tmpdir(), 'oad-cold-')))
      out.empty.push(e.ready)
      await new Promise((x) => setTimeout(x, 1000))
      out.rss_empty.push(e.rss())
      await e.stop()
      const x = await start(existing)
      out.existing.push(x.ready)
      await new Promise((y) => setTimeout(y, 1000))
      out.rss_existing.push(x.rss())
      await x.stop()
    }
  results.coldstart = {
    empty_ms: summ(out.empty),
    existing_ms: summ(out.existing),
    rss_empty_mb: summ(out.rss_empty),
    rss_existing_mb: summ(out.rss_existing),
    raw: out,
    loadavg_before: before,
    loadavg_after: la(),
  }
  log('冷启动：', results.coldstart.empty_ms, results.coldstart.existing_ms, '空闲内存：', results.coldstart.rss_existing_mb)
  save()
}

// ---------- 搜索（10 万段） ----------
if (only.has('search')) {
  const corpus = JSON.parse(readFileSync(a.corpus, 'utf8'))
  const qs = corpus.queries
  const rounds = []
  for (let round = 0; round < ROUNDS; round++) {
    const before = la()
    const data = mkdtempSync(join(tmpdir(), 'oad-search-'))
    const s = await start(data)
    const call = api(s.base)
    const ws = (await call('GET', '/me')).workspaces[0].id
    let peak = 0
    const tick = setInterval(() => (peak = Math.max(peak, s.rss())), 500)
    const t0 = now()
    const ids = []
    // 灌数据：8 个并发（旧版是串行一个请求建一页，这里一页两个请求：建页面、写正文）
    let next = 0
    await Promise.all(
      Array.from({ length: 8 }, async () => {
        while (next < corpus.pages.length) {
          const i = next++
          const p = corpus.pages[i]
          const pg = await call('POST', `/workspaces/${ws}/pages`, { title: p.title })
          await call('PUT', `/pages/${pg.id}/content`, { markdown: p.content })
          ids[i] = pg.id
        }
      }),
    )
    const loadS = ms(t0) / 1000
    // 索引是后台增量建的：等到最后一页能搜到
    const last = corpus.pages.at(-1).title.split(' ').at(-1) // "#499"
    const t1 = now()
    for (;;) {
      const r = await call('GET', `/workspaces/${ws}/search?q=${encodeURIComponent(corpus.pages.at(-1).title)}&limit=5`)
      if (r.results.some((x) => x.page_id === ids.at(-1))) break
      await new Promise((x) => setTimeout(x, 50))
    }
    const indexedAfterLoadMs = ms(t1)
    void last
    const ui = { zh: [], en: [] }
    const hits = []
    for (let k = 0; k < 3; k++)
      for (const [i, q] of qs.entries()) {
        const t = now()
        const r = await call('GET', `/workspaces/${ws}/search?q=${encodeURIComponent(q)}&limit=20`)
        ;(i < 25 ? ui.zh : ui.en).push(ms(t))
        hits.push(r.results.length)
      }
    const perQ = {}
    for (const q of qs) {
      const t = []
      for (let k = 0; k < 3; k++) {
        const t0 = now()
        await call('GET', `/workspaces/${ws}/search?q=${encodeURIComponent(q)}&limit=20`)
        t.push(ms(t0))
      }
      perQ[q] = median(t)
    }
    const noHit = []
    for (let k = 0; k < 20; k++) {
      const t = now()
      await call('GET', `/workspaces/${ws}/search?q=zzqxw&limit=20`)
      noHit.push(ms(t))
    }
    // MCP search_pages
    const ag = await call('POST', `/workspaces/${ws}/agents`, { name: 'bench', scope: 'read' })
    const mcp = await mcpSession(s.base, ag.token)
    const mt = []
    for (let k = 0; k < 3; k++)
      for (const q of qs) {
        const t = now()
        const r = await mcp.call('search_pages', { query: q })
        if (r?.error) throw new Error('MCP 出错：' + JSON.stringify(r.error))
        mt.push(ms(t))
      }
    // 改一段以后：下一次搜索多久；改的字多久能搜到
    const afterEdit = []
    const fresh = []
    for (let i = 0; i < 8; i++) {
      const pid = ids[(i * 7) % ids.length]
      const doc = await call('GET', `/pages/${pid}/content?format=json`)
      const block = doc.doc.content[3].attrs.id
      const word = `qzx${round}${i}mark`
      await call('PUT', `/pages/${pid}/blocks/${block}`, { markdown: `改了一段 ${word}`, base: doc.version })
      let t = now()
      await call('GET', `/workspaces/${ws}/search?q=${encodeURIComponent(qs[i])}&limit=20`)
      afterEdit.push(ms(t))
      t = now()
      for (;;) {
        const r = await call('GET', `/workspaces/${ws}/search?q=${word}&limit=5`)
        if (r.results.length) break
        await new Promise((x) => setTimeout(x, 20))
      }
      fresh.push(ms(t))
    }
    clearInterval(tick)
    const rssAfter = s.rss()
    await s.stop()
    // 有 10 万段时的冷启动、空闲内存
    const cold = []
    const idle = []
    for (let k = 0; k < 3; k++) {
      const s2 = await start(data)
      cold.push(s2.ready)
      await new Promise((x) => setTimeout(x, 1500))
      idle.push(s2.rss())
      await s2.stop()
    }
    const all = [...ui.zh, ...ui.en]
    const res = {
      pages: corpus.pages.length,
      load_s: +loadS.toFixed(1),
      indexed_after_load_ms: +indexedAfterLoadMs.toFixed(0),
      ui_search_ms: summ(all),
      ui_search_zh_ms: summ(ui.zh),
      ui_search_en_ms: summ(ui.en),
      ui_hits_avg: +(hits.reduce((x, y) => x + y, 0) / hits.length).toFixed(1),
      slowest: Object.entries(perQ).sort((x, y) => y[1] - x[1]).slice(0, 5).map(([q, t]) => ({ q, ms: t })),
      mcp_search_ms: summ(mt),
      no_hit_ms: summ(noHit),
      search_after_edit_ms: summ(afterEdit),
      edit_searchable_after_ms: summ(fresh),
      rss_after_mb: +rssAfter.toFixed(1),
      rss_peak_mb: +peak.toFixed(1),
      data_dir_mb: MB(du(data)),
      coldstart_with_corpus_ms: summ(cold),
      rss_idle_with_corpus_mb: summ(idle),
      loadavg_before: before,
      loadavg_after: la(),
    }
    rounds.push(res)
    log(`搜索第 ${round + 1} 轮：p50 ${res.ui_search_ms.p50} p95 ${res.ui_search_ms.p95} ms；MCP p95 ${res.mcp_search_ms.p95}；load ${before} → ${res.loadavg_after}`)
    results.search = { rounds }
    save()
  }
}

// ---------- X / YouTube 卡片 ----------
if (only.has('unfurl')) {
  const URLS = {
    x: 'https://x.com/vercel/status/1683920951807971329',
    youtube: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
  }
  const require = createRequire(resolve(import.meta.dirname, '../../apps/web/package.json'))
  const { chromium } = require('@playwright/test')
  const rounds = []
  for (let round = 0; round < ROUNDS; round++) {
    const before = la()
    const s = await start(mkdtempSync(join(tmpdir(), 'oad-unfurl-')))
    const call = api(s.base)
    const ws = (await call('GET', '/me')).workspaces[0].id
    const res = { loadavg_before: before }
    // 接口：没缓存（新的数据目录，第一次）、有缓存（再取 10 次）
    for (const [k, u] of Object.entries(URLS)) {
      let t = now()
      const card = await call('GET', `/unfurl?url=${encodeURIComponent(u)}`)
      const uncached = ms(t)
      const cached = []
      for (let i = 0; i < 10; i++) {
        t = now()
        await call('GET', `/unfurl?url=${encodeURIComponent(u)}`)
        cached.push(ms(t))
      }
      res[k] = { api_uncached_ms: +uncached.toFixed(1), api_cached_ms: summ(cached), kind: card.kind, title: !!card.title }
    }
    // 浏览器：在空行里贴链接，到出骨架、到出完整卡片（服务器有缓存；新的浏览器上下文，浏览器里没缓存）
    const browser = await chromium.launch()
    for (const [k, u] of Object.entries(URLS)) {
      const times = { skeleton: [], full: [] }
      for (let i = 0; i < 3; i++) {
        const ctx = await browser.newContext({ viewport: { width: 1320, height: 900 }, serviceWorkers: 'block', locale: 'zh-CN' })
        const page = await ctx.newPage()
        const p = await call('POST', `/workspaces/${ws}/pages`, { title: '卡片 ' + i })
        await call('PUT', `/pages/${p.id}/content`, { markdown: '第一段。\n' })
        await page.goto(`${s.base}/p/${p.id}`)
        await page.waitForFunction(() => (window).__oad?.editor)
        await page.evaluate(() => {
          const e = window.__oad.editor
          e.chain().focus('end').run()
          e.commands.enter()
        })
        const r = await page.evaluate(
          (url) =>
            new Promise((done) => {
              const e = window.__oad.editor
              const t0 = performance.now()
              let sk = null
              const check = () => {
                const box = document.querySelector('.oad-embed')
                if (box && sk === null && box.querySelector('.c-skeleton, .c-card')) sk = performance.now() - t0
                const full = box?.querySelector('.c-card:not(.c-skeleton)')
                if (full) return done({ skeleton: sk, full: performance.now() - t0 })
                if (performance.now() - t0 > 15000) return done({ skeleton: sk, full: null })
                requestAnimationFrame(check)
              }
              const dt = new DataTransfer()
              dt.setData('text/plain', url)
              e.view.dom.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }))
              check()
            }),
          u,
        )
        times.skeleton.push(r.skeleton)
        times.full.push(r.full)
        await ctx.close()
      }
      res[k].browser_skeleton_ms = summ(times.skeleton.filter((x) => x !== null))
      res[k].browser_full_cached_ms = summ(times.full.filter((x) => x !== null))
    }
    await browser.close()
    await s.stop()
    res.loadavg_after = la()
    rounds.push(res)
    log(`卡片第 ${round + 1} 轮：`, JSON.stringify(res))
    results.unfurl = { urls: URLS, rounds }
    save()
  }
}

results.machine = { at: new Date().toISOString(), loadavg_end: la() }
save()
log('结果：' + join(OUT, 'results.json'))
