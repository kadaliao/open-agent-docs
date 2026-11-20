// 看看打出来多大：入口（一定会下载的）和三个元素第一次用到时各自要下载的（gzip 后）。先 pnpm build。
import { readFileSync, readdirSync } from 'node:fs'
import { gzipSync } from 'node:zlib'

const dist = new URL('../dist/', import.meta.url)
const gz = (f) => gzipSync(readFileSync(new URL(f, dist))).length
const kb = (n) => (n / 1024).toFixed(1) + ' KB'
// 一个文件静态 import 的（不含 import()）
const deps = (f, seen = new Set()) => {
  if (seen.has(f)) return seen
  seen.add(f)
  if (!f.endsWith('.js')) return seen
  const src = readFileSync(new URL(f, dist), 'utf8')
  const dir = f.includes('/') ? f.slice(0, f.lastIndexOf('/') + 1) : ''
  for (const m of src.matchAll(/(?:^|[;\n}])\s*import\s*(?:[^'"()]*?from\s*)?["']([^"']+)["']/g)) {
    const p = new URL(m[1], new URL(dir, dist)).pathname.slice(dist.pathname.length)
    deps(p, seen)
  }
  return seen
}
const total = (fs) => [...fs].reduce((a, f) => a + gz(f), 0)
const entry = deps('elements.js')
console.log('入口 elements.js（含它静态引的）：', kb(total(entry)))
const chunks = readdirSync(new URL('chunks/', dist))
const assets = readdirSync(new URL('assets/', dist))
// 数据库一打开就要的（按需加载但一定会加载）：视图的代码、WASM 引擎
const extra = { '<oad-database>': [...chunks.filter((c) => /^(database|db-wasm)-/.test(c)).map((c) => 'chunks/' + c), ...assets.filter((a) => a.startsWith('oad_db_bg')).map((a) => 'assets/' + a)] }
for (const [el, name] of [['<oad-editor>', 'editor-app-'], ['<oad-database>', 'database-app-']]) {
  const f = chunks.find((c) => c.startsWith(name) && c.endsWith('.js'))
  if (!f) continue
  const all = deps('chunks/' + f, new Set(entry))
  for (const x of extra[el] ?? []) deps(x, all)
  console.log(`${el} 第一次用到时再下载：`, kb(total(all) - total(entry)))
}
console.log('<oad-viewer>：只要入口（服务器没有 format=html 时才下载 Markdown 的 WASM）')
const every = [...chunks.map((c) => 'chunks/' + c), ...assets.map((a) => 'assets/' + a)].reduce((a, c) => a + gz(c), gz('elements.js'))
console.log('dist 全部（含代码高亮、图表、公式这些按需的）：', kb(every))
