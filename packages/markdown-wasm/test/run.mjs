// Rust 那边的测试（crates/oad-markdown/tests/wasm.rs）调这个：读一组 Markdown，用 WASM 版解析、再输出，结果写成 JSON 给 Rust 比。
import { readFileSync, writeFileSync } from 'node:fs'
import { initSync, parse, serialize } from '../dist/oad_markdown.js'

initSync({ module: readFileSync(new URL('../dist/oad_markdown_bg.wasm', import.meta.url)) })
const [input, output] = process.argv.slice(2)
const corpus = JSON.parse(readFileSync(input, 'utf8'))
const out = corpus.map((md) => {
  const doc = parse(md)
  return { doc: JSON.parse(doc), md: serialize(doc) }
})
writeFileSync(output, JSON.stringify(out))
