// 检查 sdks/operations.json 和 docs/openapi.json 对得上：每个 operationId 恰好登记一次（operations 或 skip），名字不重复，
// 提示字段合法。服务器加了接口忘了起名字时这里失败。  node sdks/check-operations.mjs
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
const spec = JSON.parse(readFileSync(root + 'docs/openapi.json', 'utf8'))
const table = JSON.parse(readFileSync(root + 'sdks/operations.json', 'utf8'))

const errors = []
const ids = new Map()
for (const [path, item] of Object.entries(spec.paths)) {
  for (const [method, op] of Object.entries(item)) {
    if (!['get', 'post', 'put', 'patch', 'delete'].includes(method)) continue
    if (!op.operationId) errors.push(`${method.toUpperCase()} ${path} 没有 operationId`)
    else if (ids.has(op.operationId)) errors.push(`operationId 重复：${op.operationId}`)
    else ids.set(op.operationId, { path, method, op })
  }
}

const KEYS = new Set(['name', 'unwrap', 'paginate', 'null_on_404', 'sse', 'upload', 'download', 'js'])
const names = new Map()
for (const [id, e] of Object.entries(table.operations)) {
  const o = ids.get(id)
  if (!o) {
    errors.push(`operations.json 里的 ${id} 在 openapi.json 里没有`)
    continue
  }
  if (id in table.skip) errors.push(`${id} 同时在 operations 和 skip 里`)
  for (const k of Object.keys(e)) if (!KEYS.has(k)) errors.push(`${id}：不认识的字段 ${k}`)
  if (!/^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$/.test(e.name ?? '')) errors.push(`${id}：名字 ${e.name} 不是 snake_case 的点路径`)
  if (names.has(e.name)) errors.push(`名字重复：${e.name}（${names.get(e.name)} 和 ${id}）`)
  names.set(e.name, id)
  // 资源和方法不能同名（pages.list 是方法，就不能再有 pages.list.x）
  const resp = o.op.responses ?? {}
  const ok = Object.keys(resp).find((k) => k.startsWith('2'))
  const schema = ok && resp[ok].content?.['application/json']?.schema
  const props = schema ? deref(schema)?.properties ?? {} : {}
  for (const k of ['unwrap', 'paginate']) {
    if (e[k] && !(e[k] in props)) errors.push(`${id}：${k} 的字段 ${e[k]} 不在响应里（有 ${Object.keys(props).join(', ')}）`)
  }
  if (e.paginate && !('next_cursor' in props)) errors.push(`${id}：paginate 但响应里没有 next_cursor`)
  if (e.paginate && !(o.op.parameters ?? []).some((p) => p.name === 'cursor')) errors.push(`${id}：paginate 但没有 cursor 参数`)
  if (e.sse && !resp[ok]?.content?.['text/event-stream']) errors.push(`${id}：sse 但响应不是 text/event-stream`)
  if (e.sse && !['workspace', 'agent'].includes(e.sse)) errors.push(`${id}：sse 只能是 workspace / agent`)
  if (e.upload && !['file', 'csv'].includes(e.upload)) errors.push(`${id}：upload 只能是 file / csv`)
}
for (const n of names.keys()) {
  for (const m of names.keys()) if (m.startsWith(n + '.')) errors.push(`${n} 是方法，又是 ${m} 的前缀`)
}
for (const id of Object.keys(table.skip)) if (!ids.has(id)) errors.push(`skip 里的 ${id} 在 openapi.json 里没有`)
for (const id of ids.keys()) if (!(id in table.operations) && !(id in table.skip)) errors.push(`openapi.json 的 ${id} 没在 sdks/operations.json 里起名字`)

function deref(s) {
  while (s && s.$ref) s = spec.components.schemas[s.$ref.split('/').pop()]
  return s
}

if (errors.length) {
  console.error(errors.map((e) => '✗ ' + e).join('\n'))
  process.exit(1)
}
console.log(`✓ sdks/operations.json：${names.size} 个接口有名字，${Object.keys(table.skip).length} 个跳过，和 openapi.json 一致`)
