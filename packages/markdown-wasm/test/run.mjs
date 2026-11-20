// Rust 那边的测试（crates/oad-markdown/tests/wasm.rs）调这个：读一组 Markdown，用 WASM 版解析、再输出、转 HTML，结果写成 JSON 给 Rust 比。
import { readFileSync, writeFileSync } from 'node:fs'
import { contentCss, initSync, parse, serialize, toHtml } from '../dist/oad_markdown.js'

initSync({ module: readFileSync(new URL('../dist/oad_markdown_bg.wasm', import.meta.url)) })
const [input, output] = process.argv.slice(2)
const corpus = JSON.parse(readFileSync(input, 'utf8'))
const BASE = 'https://docs.example.com'
const out = corpus.map((md) => {
  const doc = parse(md)
  return {
    doc: JSON.parse(doc),
    md: serialize(doc),
    // Markdown 进、文档对象进，两种都转一遍
    html: toHtml(md),
    htmlDoc: toHtml(JSON.parse(doc), { baseUrl: BASE }),
  }
})
writeFileSync(output, JSON.stringify({ cases: out, css: contentCss() }))
