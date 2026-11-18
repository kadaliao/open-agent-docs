// 生成测试文档：N 个顶层块（段落、标题、列表、代码、引用），中英混合，带加粗、斜体、链接、行内代码。
import { writeFileSync } from 'node:fs'
import { schema } from './schema.mjs'

let seed = 42
const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648)
const pick = (a) => a[Math.floor(rnd() * a.length)]
let nid = 0
const id = () => 'b' + (nid++).toString(36).padStart(6, '0')
const ZH = ['文档', '协作', '智能体', '评论', '数据库', '性能', '链接卡片', '实时同步', '版本历史', '富媒体', '编辑器', '服务器']
const EN = ['agent', 'document', 'block', 'editor', 'latency', 'snapshot', 'merge', 'comment', 'schema', 'render']
const words = (n) => Array.from({ length: n }, () => (rnd() < 0.5 ? pick(ZH) : pick(EN))).join(rnd() < 0.5 ? '' : ' ')

function inline(n) {
  const out = []
  for (let i = 0; i < n; i++) {
    const t = words(3 + Math.floor(rnd() * 8)) + ' '
    const r = rnd()
    if (r < 0.15) out.push({ type: 'text', text: t, marks: [{ type: 'bold' }] })
    else if (r < 0.25) out.push({ type: 'text', text: t, marks: [{ type: 'italic' }] })
    else if (r < 0.32) out.push({ type: 'text', text: t, marks: [{ type: 'link', attrs: { href: 'https://example.com/' + nid } }] })
    else if (r < 0.37) out.push({ type: 'text', text: t.trim(), marks: [{ type: 'code' }] }, { type: 'text', text: ' ' })
    else out.push({ type: 'text', text: t })
  }
  return out
}
const para = () => ({ type: 'paragraph', attrs: { id: id() }, content: inline(2 + Math.floor(rnd() * 4)) })
function block() {
  const r = rnd()
  if (r < 0.6) return para()
  if (r < 0.7) return { type: 'heading', attrs: { id: id(), level: 1 + Math.floor(rnd() * 3) }, content: [{ type: 'text', text: words(4) }] }
  if (r < 0.85) {
    const type = rnd() < 0.5 ? 'bulletList' : 'orderedList'
    return { type, attrs: { id: id() }, content: Array.from({ length: 2 + Math.floor(rnd() * 3) }, () => ({ type: 'listItem', attrs: { id: id() }, content: [para()] })) }
  }
  if (r < 0.93) return { type: 'codeBlock', attrs: { id: id(), language: 'rust' }, content: [{ type: 'text', text: 'fn main() {\n    println!("' + words(3) + '");\n}' }] }
  return { type: 'blockquote', attrs: { id: id() }, content: [para()] }
}

const n = Number(process.argv[2] || 1000)
const out = process.argv[3] || '../out/base.json'
const doc = schema.nodeFromJSON({ type: 'doc', content: Array.from({ length: n }, block) })
doc.check()
writeFileSync(out, JSON.stringify(doc.toJSON()))
console.log(`${out}: ${n} 个顶层块，${doc.content.size} 个位置，${(JSON.stringify(doc.toJSON()).length / 1024).toFixed(0)} KB JSON`)

// 每个 mark 在 CRDT 里的 expand 规则，Rust 端照这个配（和 loro-prosemirror 的 getLoroTextStyle 一致）
const styles = Object.fromEntries(Object.entries(schema.marks).map(([k, m]) => [k, m.spec.inclusive !== false ? 'after' : 'none']))
writeFileSync('../out/styles.json', JSON.stringify(styles))
