// 段（服务器切好的 Markdown，见 open_agent_docs/blocks.py）和编辑器文档之间的转换。
// - 每段单独解析，绝不拼起来再解析：拼起来再切边界会变。连着的同一种列表项合进一个列表节点，表头和后面的行合成一张表格。
// - 编辑器认不全的写法（HTML、引用式链接定义、脚注……）放进“Markdown 原文”块，原样保留，Agent 写的 HTML 也不会进编辑器的 HTML 解析。
// - 段 = 顶层节点；但顶层的列表和表格本身不算段，它们的每一项、每一行才是。
import type { Editor, JSONContent } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'

export type Block = { id: string; md: string; gap: string; by: string; v: number; ts: number }
export type Loc = { node: PMNode; pos: number; parent: PMNode | null; index: number; id: string | null }

export const LIST_TYPES = ['bulletList', 'orderedList', 'taskList']
const GROUP_TYPES = [...LIST_TYPES, 'table']
export const BLOCK_TYPES = ['paragraph', 'heading', 'blockquote', 'codeBlock', 'horizontalRule', 'callout', 'details', 'rawBlock',
  'listItem', 'taskItem', 'tableRow']

// ---------- 和 open_agent_docs/blocks.py 一样的粗分类 ----------
const ITEM = /^( {0,3})([-+*]|\d{1,9}[.)])(?=[ \t]|$)/
const HR = /^ {0,3}(?:(?:\*[ \t]*){3,}|(?:-[ \t]*){3,}|(?:_[ \t]*){3,})$/
const DELIM = /^ {0,3}\|?[ \t]*:?-+:?[ \t]*(?:\|[ \t]*:?-+:?[ \t]*)*\|?[ \t]*$/
const cellCount = (line: string) => {
  let s = line.trim()
  if (s.startsWith('|')) s = s.slice(1)
  if (s.endsWith('|') && !s.endsWith('\\|')) s = s.slice(0, -1)
  return s.split(/(?<!\\)\|/).length
}
export function listKind(md: string): string | null {
  const first = md.split('\n', 1)[0]
  const m = first.match(ITEM)
  if (!m || HR.test(first) || m[1].length) return null
  return '-+*'.includes(m[2]) ? m[2] : 'ord' + m[2].slice(-1)
}
export function isHead(md: string): boolean {
  const lines = md.split('\n')
  return lines.length === 2 && lines[0].includes('|') && lines[1].includes('-') && DELIM.test(lines[1]) && cellCount(lines[0]) === cellCount(lines[1])
}
const isPipe = (md: string) => !md.includes('\n') && md.includes('|')

// ---------- 编辑器认不认得 ----------
// marked 自带的，加上 Tiptap 自己切的（taskList / taskItem / orderedList）和我们的提示框、折叠块
const OK = new Set(['paragraph', 'heading', 'list', 'list_item', 'blockquote', 'code', 'table', 'hr', 'space', 'text', 'strong', 'em',
  'del', 'codespan', 'link', 'image', 'br', 'escape', 'checkbox', 'callout', 'details', 'taskList', 'taskItem', 'orderedList'])
const BR = /^<br\s*\/?>$/i

function tokensOk(tokens: any[] | undefined): boolean {
  for (const tk of tokens || []) {
    if (tk.type === 'html') {
      if (!BR.test(String(tk.raw || '').trim())) return false
      continue
    }
    if (!OK.has(tk.type)) return false
    // 子 token 放在哪个字段各家不一样（tokens、items、nestedTokens……）：数组都看一遍
    for (const v of Object.values(tk)) if (Array.isArray(v) && v.some(x => x && typeof x === 'object' && 'type' in x) && !tokensOk(v)) return false
    if (tk.type === 'table' && ![tk.header, ...(tk.rows || [])].every((cells: any[]) => cells.every(c => tokensOk(c.tokens)))) return false
  }
  return true
}

export function supported(editor: Editor, md: string): boolean {
  try { return tokensOk(editor.markdown!.instance.lexer(md) as any[]) } catch { return false }
}

// 段落里的普通换行（软换行）：解析出来是文字里的 \n，一编辑就会被 ProseMirror 读成硬换行、写回成“  \n”。
// 先换成带 soft 标记的换行节点（nodes.tsx 的 SoftBreak），写回时还是 \n
function soft(nodes: JSONContent[] | undefined, inCode = false): JSONContent[] | undefined {
  if (!nodes) return nodes
  const out: JSONContent[] = []
  for (const n of nodes) {
    if (n.type === 'text' && !inCode && n.text?.includes('\n')) {
      n.text.split('\n').forEach((part, k) => {
        if (k) out.push({ type: 'hardBreak', attrs: { soft: true } })
        if (part) out.push({ ...n, text: part })
      })
    } else out.push(n.content ? { ...n, content: soft(n.content, inCode || n.type === 'codeBlock') } : n)
  }
  return out
}
const parse = (editor: Editor, md: string): JSONContent[] => soft(editor.markdown!.parse(md).content) || []

// ---------- 段 → 文档 ----------
const raw = (group: { id: string; md: string; gap: string }[]): JSONContent => ({
  type: 'rawBlock',
  attrs: { bid: group[0].id, md: group.map((b, k) => (k ? b.gap || '\n\n' : '') + b.md).join(''), rest: group.slice(1).map(b => b.id).join(',') },
})

function single(editor: Editor, b: { id: string; md: string }): JSONContent | null {
  if (!b.md.trim()) return { type: 'paragraph', attrs: { bid: b.id } }
  if (!supported(editor, b.md)) return null
  const nodes = parse(editor, b.md)
  if (nodes.length !== 1 || GROUP_TYPES.includes(nodes[0].type!)) return null
  return { ...nodes[0], attrs: { ...nodes[0].attrs, bid: b.id } }
}

function item(editor: Editor, b: { id: string; md: string }): [string, any, JSONContent] | null {
  if (!supported(editor, b.md)) return null
  const nodes = parse(editor, b.md)
  if (nodes.length !== 1 || !LIST_TYPES.includes(nodes[0].type!) || nodes[0].content?.length !== 1) return null
  const it = nodes[0].content[0]
  return [nodes[0].type!, nodes[0].attrs || {}, { ...it, attrs: { ...it.attrs, bid: b.id } }]
}

// 表格格子里的 <br> 是换行（模型常这样写）：解析出来是字面的文字，换成真的换行
function brs(content: JSONContent[] | undefined): JSONContent[] {
  const out: JSONContent[] = []
  for (const n of content || []) {
    if (n.type !== 'text' || !/<br\s*\/?>/i.test(n.text || '')) { out.push(n.content ? { ...n, content: brs(n.content) } : n); continue }
    ;(n.text || '').split(/<br\s*\/?>/i).forEach((s, k) => {
      if (k) out.push({ type: 'hardBreak' })
      if (s) out.push({ ...n, text: s })
    })
  }
  return out
}

function table(editor: Editor, group: { id: string; md: string }[]): JSONContent | null {
  const md = group.map(b => b.md).join('\n')
  if (!supported(editor, md)) return null
  const nodes = parse(editor, md)
  if (nodes.length !== 1 || nodes[0].type !== 'table' || nodes[0].content?.length !== group.length) return null
  const delim = group[0].md.split('\n')[1]
  return {
    ...nodes[0],
    content: nodes[0].content.map((row, k) => ({
      ...row, attrs: { ...row.attrs, bid: group[k].id, ...(k ? {} : { delim }) },
      content: (row.content || []).map(cell => ({ ...cell, content: brs(cell.content) })),
    })),
  }
}

/** 服务器的段 → 编辑器文档（JSON）。 */
export function buildDoc(editor: Editor, blocks: { id: string; md: string; gap: string }[]): JSONContent {
  const out: JSONContent[] = []
  let i = 0
  while (i < blocks.length) {
    const b = blocks[i]
    if (isHead(b.md)) {
      let j = i + 1
      while (j < blocks.length && blocks[j].gap === '\n' && isPipe(blocks[j].md) && !isHead(blocks[j].md)) j++
      const group = blocks.slice(i, j)
      out.push(table(editor, group) ?? raw(group))
      i = j
      continue
    }
    const kind = listKind(b.md)
    if (kind) {
      let j = i
      let cur: JSONContent | null = null
      for (; j < blocks.length && listKind(blocks[j].md) === kind; j++) {
        const got = item(editor, blocks[j])
        if (!got) { cur = null; out.push(raw([blocks[j]])); continue }
        const [type, attrs, it] = got
        if (cur && cur.type === type) cur.content!.push(it)
        else { cur = { type, attrs, content: [it] }; out.push(cur) }
      }
      i = j
      continue
    }
    out.push(single(editor, b) ?? raw([b]))
    i++
  }
  if (!out.length) out.push({ type: 'paragraph' })
  return { type: 'doc', content: out }
}

// ---------- 文档里的段 ----------
export function blocksOf(doc: PMNode): Loc[] {
  const out: Loc[] = []
  doc.forEach((node, offset) => {
    if (GROUP_TYPES.includes(node.type.name)) {
      node.forEach((child, o, k) => out.push({ node: child, pos: offset + 1 + o, parent: node, index: k, id: child.attrs.bid ?? null }))
    } else out.push({ node, pos: offset, parent: null, index: 0, id: node.attrs.bid ?? null })
  })
  return out
}

/** “Markdown 原文”块整组代表的其余段（表格认不全时整张表放进一个原文块）。 */
export const restOf = (node: PMNode): string[] => (node.type.name === 'rawBlock' && node.attrs.rest ? String(node.attrs.rest).split(',') : [])

// ---------- 段 → Markdown ----------
const tidy = (s: string) => s.replace(/^\n+/, '').replace(/\s+$/, '')

function cellMd(editor: Editor, cell: PMNode): string {
  const md = tidy(editor.markdown!.serialize({ type: 'doc', content: cell.toJSON().content || [] }))
  return md.replace(/(?: {2}|\\)\n/g, '<br>').replace(/\n+/g, '<br>').replace(/(?<!\\)\|/g, '\\|')
}

function rowMd(editor: Editor, row: PMNode, header: boolean): string {
  const cells: string[] = [], aligns: (string | null)[] = []
  row.forEach(c => { cells.push(cellMd(editor, c)); aligns.push(c.attrs.align ?? null) })
  const line = '| ' + cells.join(' | ') + ' |'
  if (!header) return line
  const delim = row.attrs.delim && cellCount(row.attrs.delim) === cells.length ? row.attrs.delim
    : '| ' + aligns.map(a => (a === 'left' ? ':---' : a === 'right' ? '---:' : a === 'center' ? ':---:' : '---')).join(' | ') + ' |'
  return line + '\n' + delim
}

/** 一段的 Markdown（列表项要知道在哪种列表里、排第几；表格行要知道是不是表头）。 */
export function serializeBlock(editor: Editor, loc: Pick<Loc, 'node' | 'parent' | 'index'>): string {
  const { node, parent, index } = loc
  const type = node.type.name
  if (type === 'rawBlock') return node.attrs.md || ''
  if (type === 'tableRow') return rowMd(editor, node, index === 0)
  if ((type === 'listItem' || type === 'taskItem') && parent) {
    const attrs = parent.type.name === 'orderedList' ? { ...parent.attrs, start: (parent.attrs.start ?? 1) + index } : parent.attrs
    return tidy(editor.markdown!.serialize({ type: 'doc', content: [{ type: parent.type.name, attrs, content: [node.toJSON()] }] }))
  }
  return tidy(editor.markdown!.serialize({ type: 'doc', content: [node.toJSON()] }))
}
