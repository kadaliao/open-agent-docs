// @oad/db：数据库计算引擎（crates/oad-db 编的 WASM，和服务器同一份 Rust 代码）。接口见 docs/phase4-contract.md「计算引擎」。
// 进出 WASM 都是 JSON 字符串（一次解析最快）；这里包成对象。行、表结构、改动、视图也可以直接给 JSON 字符串（比如 fetch 来的原文），省一次转换。
import init, { initSync, RawEngine } from './dist/oad_db.js'

export { init, initSync }
export default init

const text = (x) => (typeof x === 'string' ? x : JSON.stringify(x ?? null))
const value = (x) => JSON.stringify(x === undefined ? null : x)

let ready = null
// 已经 initSync 过的话 init 直接返回
const ensure = () => (ready ??= init())

export class DbEngine {
  /** @param {RawEngine} raw */
  constructor(raw) {
    this.raw = raw
  }

  /** 建引擎：先加载 WASM（只加载一次）。opts：{ id?, now?, tz?, people? } */
  static async create(schema, opts) {
    await ensure()
    return DbEngine.createSync(schema, opts)
  }

  /** 已经 init / initSync 过时同步建 */
  static createSync(schema, opts) {
    return new DbEngine(new RawEngine(text(schema), text(opts ?? {})))
  }

  loadRows(rows) {
    this.raw.loadRows(text(rows))
  }

  loadRelated(dbId, schema, rows) {
    this.raw.loadRelated(dbId, text(schema), text(rows))
  }

  apply(changes) {
    return JSON.parse(this.raw.apply(text(changes)))
  }

  setSchema(schema) {
    this.raw.setSchema(text(schema))
  }

  setPeople(people) {
    this.raw.setPeople(text(people))
  }

  setClock(nowMs, tzOffsetMinutes) {
    this.raw.setClock(nowMs, tzOffsetMinutes)
  }

  query(view) {
    return JSON.parse(this.raw.query(text(view ?? {})))
  }

  computed(rowId) {
    return JSON.parse(this.raw.computed(rowId))
  }

  display(key, v) {
    return this.raw.display(key, value(v))
  }

  displayRow(rowId) {
    return JSON.parse(this.raw.displayRow(rowId))
  }

  checkFormula(src, key) {
    return JSON.parse(this.raw.checkFormula(src, key ?? undefined))
  }

  normalize(key, input) {
    return JSON.parse(this.raw.normalize(key, value(input)))
  }

  schemaErrors() {
    return JSON.parse(this.raw.schemaErrors())
  }

  free() {
    this.raw.free()
  }
}
