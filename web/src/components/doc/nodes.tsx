// 编辑器里 Markdown 存得下、但 Tiptap 没有现成写法的几种块：
// - 提示框：GitHub 的写法 > [!NOTE]（TIP / IMPORTANT / WARNING / CAUTION）
// - 折叠块：<details><summary>…</summary> … </details>（展开收起只是界面状态，不算改动）
// - Markdown 原文：编辑器认不全的段原样放着，点开改原文
// - 代码块：围栏比内容里最长的一串反引号还长，代码里有 ``` 也不会被截断
import { useLayoutEffect, useRef, useState } from 'react'
import { Node, mergeAttributes } from '@tiptap/core'
import CodeBlock from '@tiptap/extension-code-block'
import HardBreak from '@tiptap/extension-hard-break'
import { Details as BaseDetails } from '@tiptap/extension-details'
import { NodeViewContent, NodeViewWrapper, ReactNodeViewRenderer, type ReactNodeViewProps } from '@tiptap/react'
import Markdown from '../Markdown'
import { t } from '../../i18n'

// ---------- 提示框 ----------
export const KINDS = ['NOTE', 'TIP', 'IMPORTANT', 'WARNING', 'CAUTION']
const ICONS: Record<string, string> = { NOTE: 'ℹ︎', TIP: '💡', IMPORTANT: '❗', WARNING: '⚠︎', CAUTION: '⛔' }   // i18n-ignore
const CALLOUT = /^ {0,3}> ?\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\][ \t]*(?:\n|$)((?: {0,3}>.*(?:\n|$))*)/i

function CalloutView({ node, updateAttributes, editor }: ReactNodeViewProps) {
  const kind = String(node.attrs.kind || 'NOTE')
  const next = () => editor.isEditable && updateAttributes({ kind: KINDS[(KINDS.indexOf(kind) + 1) % KINDS.length] })
  return (
    <NodeViewWrapper className={'callout callout-' + kind.toLowerCase()} data-callout={kind}>
      <button type="button" className="callout-kind" contentEditable={false} onClick={next} title={t('换一种提示框')}>{ICONS[kind] || ICONS.NOTE}</button>
      <NodeViewContent className="callout-body" />
    </NodeViewWrapper>
  )
}

export const Callout = Node.create({
  name: 'callout',
  group: 'block',
  content: 'block+',
  defining: true,
  addAttributes() {
    return { kind: { default: 'NOTE', parseHTML: el => (el.getAttribute('data-callout') || 'NOTE').toUpperCase(), renderHTML: a => ({ 'data-callout': a.kind }) } }
  },
  parseHTML() { return [{ tag: 'div[data-callout]' }] },
  renderHTML({ HTMLAttributes, node }) {
    return ['div', mergeAttributes(HTMLAttributes, { class: 'callout callout-' + String(node.attrs.kind).toLowerCase() }), 0]
  },
  addNodeView() { return ReactNodeViewRenderer(CalloutView) },
  markdownTokenizer: {
    name: 'callout',
    level: 'block',
    start: (src: string) => src.match(/^ {0,3}> ?\[!/m)?.index ?? -1,
    tokenize(src, _tokens, lexer) {
      const m = src.match(CALLOUT)
      if (!m) return
      return { type: 'callout', raw: m[0], kind: m[1].toUpperCase(), tokens: lexer.blockTokens(m[2].replace(/^ {0,3}> ?/gm, '')) }
    },
  },
  parseMarkdown: (token, h) => {
    const kids = h.parseChildren(token.tokens || [])
    return h.createNode('callout', { kind: token.kind }, kids.length ? kids : [h.createNode('paragraph')])
  },
  renderMarkdown: (node, h) => {
    const inner = h.renderChildren(node.content || [], '\n\n').replace(/\s+$/, '')
    return `> [!${node.attrs?.kind || 'NOTE'}]` + (inner ? '\n' + inner.split('\n').map(l => (l ? '> ' + l : '>')).join('\n') : '')
  },
})

// ---------- 折叠块 ----------
const OPEN = /^ {0,3}<details(\s[^>]*)?>/i
const FENCE = /^ {0,3}(`{3,}|~{3,})/
const decode = (s: string) => s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&')
const encode = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')

/** 从开头的 <details> 到配对的 </details>（数嵌套，围栏代码里的不算）。找不到返回 -1。 */
function detailsEnd(src: string): number {
  let depth = 0, pos = 0, fence = ''
  for (const line of src.split('\n')) {
    const f = line.match(FENCE)
    if (fence) {
      if (f && f[1][0] === fence[0] && f[1].length >= fence.length && !line.slice(f[0].length).trim()) fence = ''
    } else if (f && pos > 0) fence = f[1]
    else {
      depth += (line.match(/<details(?=[\s>/]|$)/gi) || []).length - (line.match(/<\/details\s*>/gi) || []).length
      if (depth <= 0) return pos + line.length
    }
    pos += line.length + 1
  }
  return -1
}

export const Details = BaseDetails.extend({
  addAttributes() {
    return { mdOpen: { default: false, parseHTML: () => false, renderHTML: () => ({}) } }   // 原文里写了 open：写回时照样带上
  },
  markdownTokenizer: {
    name: 'details',
    level: 'block',
    start: (src: string) => src.match(/^ {0,3}<details/im)?.index ?? -1,
    tokenize(src, _tokens, lexer) {
      const open = src.match(OPEN)
      if (!open) return
      const end = detailsEnd(src)
      if (end < 0) return
      const block = src.slice(0, end)
      const after = block.slice(open[0].length)
      const sum = after.match(/^\s*<summary>([\s\S]*?)<\/summary>/i)
      const body = (sum ? after.slice(sum[0].length) : after).replace(/<\/details\s*>\s*$/i, '')
      return { type: 'details', raw: src.slice(0, end + (src[end] === '\n' ? 1 : 0)), open: /\bopen\b/i.test(open[1] || ''),
               summary: sum ? decode(sum[1].trim()) : '', tokens: lexer.blockTokens(body.trim()) }
    },
  },
  parseMarkdown: (token, h) => {
    const kids = h.parseChildren(token.tokens || [])
    return h.createNode('details', { mdOpen: !!token.open }, [
      h.createNode('detailsSummary', {}, token.summary ? [h.createTextNode(token.summary)] : []),
      h.createNode('detailsContent', {}, kids.length ? kids : [h.createNode('paragraph')]),
    ])
  },
  renderMarkdown: (node, h) => {
    const [sum, body] = node.content || []
    const summary = (sum?.content || []).map((c: any) => c.text || '').join('')
    const inner = body ? h.renderChildren(body.content || [], '\n\n').replace(/\s+$/, '') : ''
    return `<details${node.attrs?.mdOpen ? ' open' : ''}><summary>${encode(summary)}</summary>\n\n${inner}\n\n</details>`
  },
}).configure({ persist: false, HTMLAttributes: { class: 'doc-details' } })

// ---------- Markdown 原文 ----------
// 渲染出来什么都看不到的（引用式链接定义、注释这类）直接显示原文，免得像是一个空框
function RawShow({ md }: { md: string }) {
  const box = useRef<HTMLDivElement>(null)
  const [empty, setEmpty] = useState(false)
  useLayoutEffect(() => { setEmpty(!box.current?.innerText.trim()) }, [md])
  return <div contentEditable={false} ref={box}>{empty ? <pre className="raw-src">{md}</pre> : <Markdown text={md} />}</div>
}

function RawView({ node, updateAttributes, editor, selected }: ReactNodeViewProps) {
  const [draft, setDraft] = useState<string | null>(null)
  const md = String(node.attrs.md || '')
  const save = () => { if (draft !== null && draft !== md) updateAttributes({ md: draft }); setDraft(null) }
  return (
    <NodeViewWrapper className={'raw-block' + (selected ? ' selected' : '')} data-no-swipe="">
      <div className="raw-chip" contentEditable={false}>
        <span>{t('Markdown 原文')}</span>
        {editor.isEditable && draft === null && <button type="button" className="link-btn" onClick={() => setDraft(md)}>{t('编辑')}</button>}
        {draft !== null && <>
          <button type="button" className="link-btn" onClick={save}>{t('完成')}</button>
          <button type="button" className="link-btn" onClick={() => setDraft(null)}>{t('取消')}</button>
        </>}
      </div>
      {draft !== null
        ? <textarea className="raw-edit" autoFocus value={draft} rows={Math.min(24, draft.split('\n').length + 1)} spellCheck={false}
            onChange={e => setDraft(e.target.value)} onKeyDown={e => { if (e.key === 'Escape') setDraft(null); if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') save() }} />
        : <RawShow key={md} md={md} />}
    </NodeViewWrapper>
  )
}

export const RawBlock = Node.create({
  name: 'rawBlock',
  group: 'block',
  atom: true,
  selectable: true,
  draggable: false,
  addAttributes() { return { md: { default: '' }, rest: { default: '' } } },
  renderHTML({ HTMLAttributes }) { return ['div', mergeAttributes(HTMLAttributes, { 'data-raw-block': '' })] },
  renderMarkdown: node => String(node.attrs?.md || ''),
  // 原文框里打字、点按钮不要交给编辑器处理
  addNodeView() { return ReactNodeViewRenderer(RawView, { stopEvent: ({ event }) => !(event.type === 'dragstart' || event.type === 'drop') }) },
})

// ---------- 代码块 ----------
export const SafeCodeBlock = CodeBlock.extend({
  renderMarkdown: node => {
    const text = (node.content || []).map((c: any) => c.text || '').join('')
    const longest = Math.max(2, ...[...text.matchAll(/`+/g)].map(m => m[0].length))
    const fence = '`'.repeat(Math.max(3, longest + 1))
    return `${fence}${node.attrs?.language || ''}\n${text}\n${fence}`
  },
})

// ---------- 换行 ----------
// 段落里的普通换行（Markdown 原文里直接回车）带 soft 标记，写回还是一个 \n；Shift+回车打的才写成“  \n”。
// 标记放在 <br data-soft> 上，ProseMirror 重新读这段的 DOM 时也不会丢
export const SoftBreak = HardBreak.extend({
  addAttributes() {
    return { soft: { default: false, parseHTML: el => el.hasAttribute('data-soft'), renderHTML: a => (a.soft ? { 'data-soft': '' } : {}) } }
  },
  renderMarkdown: node => (node.attrs?.soft ? '\n' : '  \n'),
})
