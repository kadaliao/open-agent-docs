// WASM 版的性能：1 万行 × 20 列，筛选 + 排序 + 分组。目标 < 30ms。`node test/bench.mjs`（先 sh build.sh）
import { readFileSync } from 'node:fs'
import { DbEngine, initSync } from '../index.js'

initSync({ module: readFileSync(new URL('../dist/oad_db_bg.wasm', import.meta.url)) })
const NOW = 1_794_709_800_000
const opt = (names) => names.map((name, i) => ({ id: `o${i + 1}`, name, color: 'blue' }))
const schema = {
  props: [
    { key: 'title', name: '名称', type: 'title' },
    { key: 'st', name: '状态', type: 'status', options: [{ id: 's1', name: '未开始', group: 'todo' }, { id: 's2', name: '进行中', group: 'doing' }, { id: 's3', name: '已完成', group: 'done' }] },
    { key: 'pri', name: '优先级', type: 'select', options: opt(['高', '中', '低']) },
    { key: 'tags', name: '标签', type: 'multi_select', options: opt(['前端', '后端', '设计', '测试', '运维', '文档']) },
    { key: 'who', name: '负责人', type: 'person' },
    { key: 'due', name: '截止', type: 'date' },
    { key: 'start', name: '开始', type: 'date' },
    { key: 'price', name: '单价', type: 'number', format: 'cny' },
    { key: 'qty', name: '数量', type: 'number' },
    { key: 'score', name: '评分', type: 'number' },
    { key: 'hours', name: '工时', type: 'number' },
    { key: 'done', name: '完成', type: 'checkbox' },
    { key: 'flag', name: '置顶', type: 'checkbox' },
    { key: 'note', name: '备注', type: 'text' },
    { key: 'url', name: '网址', type: 'url' },
    { key: 'ct', name: '创建时间', type: 'created_time' },
    { key: 'total', name: '金额', type: 'formula', formula: 'prop("单价") * prop("数量")' },
    { key: 'left', name: '剩几天', type: 'formula', formula: 'dateBetween(prop("截止"), today(), "days")' },
    { key: 'label', name: '标记', type: 'formula', formula: 'if(prop("完成"), "✓", prop("优先级") + "·" + toText(prop("评分")))' },
    { key: 'area', name: '区域', type: 'select', options: opt(['华北', '华东', '华南', '西南']) },
  ],
}
let seed = 42
const r = (m) => {
  seed = (seed * 1103515245 + 12345) % 2147483648
  return seed % m
}
const pad = (n) => String(n).padStart(2, '0')
const rows = Array.from({ length: 10_000 }, (_, i) => {
  const tags = [1, 2, 3, 4, 5, 6].filter(() => r(4) === 0).map((k) => `o${k}`)
  const props = {
    title: `任务 ${i}`, st: `s${r(3) + 1}`, pri: `o${r(3) + 1}`, who: [`user:${r(2) + 1}`],
    due: { start: `2026-${pad(r(12) + 1)}-${pad(r(28) + 1)}` }, start: { start: `2026-${pad(r(12) + 1)}-${pad(r(28) + 1)}` },
    price: r(10000) / 100, qty: r(50), score: r(100), hours: r(40), note: `备注 ${r(1000)} 关键词${r(20)}`,
    url: `https://example.com/${r(100)}`, area: `o${r(4) + 1}`,
  }
  if (tags.length) props.tags = tags
  if (r(2) === 0) props.done = true
  if (r(10) === 0) props.flag = true
  return { id: `r${i}`, props, created_at: NOW - r(1000) * 3_600_000, created_by: 'user:1' }
})
const json = JSON.stringify(rows)

function time(label, runs, f) {
  const ts = []
  let out
  for (let i = 0; i < runs; i++) {
    const t = performance.now()
    out = f()
    ts.push(performance.now() - t)
  }
  ts.sort((a, b) => a - b)
  console.log(`${label.padEnd(36)} 中位 ${ts[ts.length >> 1].toFixed(2).padStart(7)}ms  最慢 ${ts[ts.length - 1].toFixed(2).padStart(7)}ms`)
  return out
}

console.log(`1 万行 JSON ${(json.length / 1e6).toFixed(1)} MB`)
const eng = DbEngine.createSync(schema, { now: NOW, tz: 480, people: [{ id: 'user:1', name: '张三' }, { id: 'user:2', name: '李四' }] })
time('加载（JSON 字符串）', 5, () => eng.loadRows(json))
time('加载（对象，要先 JSON.stringify）', 5, () => eng.loadRows(rows))
const view = {
  filter: { op: 'and', items: [
    { prop: 'st', cond: 'neq', value: '已完成' },
    { op: 'or', items: [{ prop: 'score', cond: 'gt', value: 30 }, { prop: 'tags', cond: 'contains', value: '前端' }] },
    { prop: 'due', cond: 'is_within', value: 'this_year' },
  ] },
  sorts: [{ prop: 'pri' }, { prop: 'due', dir: 'desc' }],
  group_by: 'st',
}
const res = time('筛选 + 排序 + 分组（含 JSON 来回）', 30, () => eng.query(view))
console.log(`  → ${res.total} 行，${res.groups.length} 组`)
time('…再加子分组 + 汇总行', 30, () => eng.query({ ...view, sub_group_by: 'pri', totals: { price: 'sum', score: 'avg', tags: 'count_values', done: 'percent_checked', due: 'earliest' } }))
const fv = { filter: [{ prop: 'total', cond: 'gt', value: 1000 }, { prop: 'label', cond: 'contains', value: '高' }], sorts: [{ prop: 'left' }] }
const cold = DbEngine.createSync(schema, { now: NOW, tz: 480 })
cold.loadRows(json)
time('按公式筛选排序（第一次，要算公式）', 1, () => cold.query(fv))
time('按公式筛选排序（有缓存）', 30, () => cold.query(fv))
time('搜索', 30, () => eng.query({ search: '关键词7', sorts: [{ prop: 'title' }] }))
let k = 0
time('改一格 + 重新查', 30, () => {
  k++
  eng.apply([{ row_id: `r${k}`, props: { score: k % 100, qty: k % 7 } }])
  return eng.query(fv)
})
time('displayRow（一行）', 1000, () => eng.displayRow('r5000'))
time('display（一格）', 1000, () => eng.display('price', 1234.5))
