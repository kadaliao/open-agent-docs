// WASM 版和 Rust 版算出来一样：golden.json 是 crates/oad-db/tests/golden.rs 用 Rust（同样的 JSON 接口）跑出来的，
// 这里用 WASM 版（经 index.js 的 DbEngine）跑同样的步骤逐个比。改了 oad-db 先 `cargo test -p oad-db --test golden` 再 `sh build.sh`。
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { DbEngine, initSync } from '../index.js'

initSync({ module: readFileSync(new URL('../dist/oad_db_bg.wasm', import.meta.url)) })
const cases = JSON.parse(readFileSync(new URL('./golden.json', import.meta.url), 'utf8'))

function step(eng, s) {
  const err = (f) => {
    try {
      return f()
    } catch (e) {
      return { error: e.message }
    }
  }
  switch (s.op) {
    case 'query':
      return err(() => eng.query(s.view))
    case 'computed':
      return eng.computed(s.row)
    case 'displayRow':
      return eng.displayRow(s.row)
    case 'display':
      return eng.display(s.key, s.value)
    case 'normalize':
      return err(() => ({ value: eng.normalize(s.key, s.input) }))
    case 'checkFormula':
      return eng.checkFormula(s.src, s.key ?? undefined)
    case 'apply':
      return eng.apply(s.changes)
    case 'setSchema':
      eng.setSchema(s.schema)
      return null
    case 'schemaErrors':
      return eng.schemaErrors()
    default:
      throw new Error(s.op)
  }
}

for (const c of cases) {
  test(c.name, () => {
    const eng = DbEngine.createSync(c.schema, c.opts)
    for (const r of c.related) eng.loadRelated(r.id, r.schema, r.rows)
    eng.loadRows(c.rows)
    c.steps.forEach((s, i) => {
      assert.deepStrictEqual(step(eng, s), s.want, `第 ${i + 1} 步 ${s.op} ${JSON.stringify(s.view ?? s.key ?? s.row ?? s.src ?? '')}`)
    })
    eng.free()
  })
}

test('create 是异步的、接受 JSON 字符串', async () => {
  const c = cases[0]
  const eng = await DbEngine.create(JSON.stringify(c.schema), c.opts)
  eng.loadRows(JSON.stringify(c.rows))
  assert.equal(eng.query({}).total, c.rows.length)
  assert.throws(() => eng.query({ sorts: [{ prop: '没有' }] }), /排序里没有属性/)
  assert.throws(() => eng.normalize('price', '很多'), /要写数字/)
  eng.free()
})

test('不固定时钟时跟着系统时间', async () => {
  const eng = await DbEngine.create({ props: [{ key: 'title', name: '名称', type: 'title' }, { key: 'f', name: '今天', type: 'formula', formula: 'today()' }] })
  eng.loadRows([{ id: 'a', props: {} }])
  const d = new Date()
  const want = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  assert.deepEqual(eng.computed('a').f, { start: want })
  eng.free()
})
