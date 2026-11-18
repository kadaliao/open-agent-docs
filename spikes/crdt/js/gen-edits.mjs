// Agent 的改动（Rust 端执行）和人的改动（JS 端执行）挑的块。
import { readFileSync, writeFileSync } from 'node:fs'
const base = JSON.parse(readFileSync('../out/base.json'))
const top = base.content
const paras = top.map((b, i) => [b, i]).filter(([b]) => b.type === 'paragraph')
const P = paras[10][0], Q = paras[20][0], R = paras[30][0], S = paras[40][0], D = paras[50][0], B = paras[60][0]
const list = top.find((b) => b.type === 'bulletList' && b.content.length >= 2)
const L = list.content[1].content[0]          // 列表第二项里的段落

const clone = (x) => JSON.parse(JSON.stringify(x))
const p2 = clone(P)
p2.content[0].text = 'AGENT' + p2.content[0].text.slice(4)                 // 开头改几个字
p2.content.push({ type: 'text', text: '（Agent 补充一句）' })                // 结尾加一句
const l2 = clone(L)
l2.content = [{ type: 'text', text: '列表里这一项被 Agent 改写了，' }, { type: 'text', text: '带一点加粗', marks: [{ type: 'bold' }] }]
const edits = [
  { op: 'replace', id: P.attrs.id, nodes: [p2] },
  { op: 'replace', id: Q.attrs.id, nodes: [{ type: 'heading', attrs: { id: Q.attrs.id, level: 2 }, content: [{ type: 'text', text: 'Agent 把这一段改成了标题' }] }] },
  { op: 'insert_after', id: R.attrs.id, nodes: [
    { type: 'paragraph', attrs: { id: 'agentnew1' }, content: [{ type: 'text', text: 'Agent 新加的第一段' }] },
    { type: 'paragraph', attrs: { id: 'agentnew2' }, content: [{ type: 'text', text: 'Agent 新加的第二段，有' }, { type: 'text', text: '链接', marks: [{ type: 'link', attrs: { href: 'https://x.com/', target: '_blank', rel: 'noopener noreferrer nofollow', class: null, title: null } }] }] },
  ] },
  { op: 'replace', id: L.attrs.id, nodes: [l2] },
  { op: 'delete', id: S.attrs.id },
]
writeFileSync('../out/edits.json', JSON.stringify(edits))
writeFileSync('../out/targets.json', JSON.stringify({ P: P.attrs.id, Q: Q.attrs.id, R: R.attrs.id, S: S.attrs.id, D: D.attrs.id, B: B.attrs.id, L: L.attrs.id }))
console.log('edits.json / targets.json 写好了')
