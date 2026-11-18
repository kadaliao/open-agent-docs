// 代码块：右上角语言选择、自动换行开关、复制；mermaid 语言画成图（Mermaid 按需加载），可以切回源码。
import type { NodeViewRenderer } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { t } from '../strings'
import { blockAttrs, el, setAttr } from './node-views'

const COMMON = ['plaintext', 'bash', 'c', 'cpp', 'csharp', 'css', 'diff', 'go', 'html', 'java', 'javascript', 'json', 'kotlin', 'markdown', 'mermaid', 'php', 'python', 'ruby', 'rust', 'sql', 'swift', 'toml', 'tsx', 'typescript', 'yaml']

let mermaid: Promise<typeof import('mermaid').default> | null = null
let seq = 0
const dark = () => {
  const d = document.documentElement.dataset.theme
  return d ? d === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches
}
async function drawMermaid(code: string) {
  mermaid ??= import('mermaid').then((m) => m.default)
  const m = await mermaid
  m.initialize({ startOnLoad: false, securityLevel: 'strict', theme: dark() ? 'dark' : 'neutral', fontFamily: 'inherit' })
  const { svg } = await m.render('oad-mmd-' + ++seq, code)
  return svg
}

const modes = new Map<string, 'diagram' | 'source'>()
let datalist: HTMLDataListElement | null = null
function langList() {
  if (datalist) return datalist.id
  datalist = el('datalist', '', { id: 'oad-code-langs' }) as HTMLDataListElement
  for (const l of COMMON) datalist.append(el('option', '', { value: l }))
  document.body.append(datalist)
  return datalist.id
}

export const codeBlockView: NodeViewRenderer = (p) => {
  let node: PMNode = p.node
  const dom = el('div', 'oad-code')
  const bar = el('div', 'oad-code-bar', { contenteditable: 'false' })
  const lang = el('input', 'oad-code-lang', { spellcheck: 'false', 'aria-label': t('codeLanguage'), list: langList() })
  lang.placeholder = t('plainText')
  const tools = el('span', 'oad-code-tools')
  const toggle = el('button', 'oad-code-btn oad-code-mode', { type: 'button' })
  const wrap = el('button', 'oad-code-btn', { type: 'button', 'aria-pressed': 'false' })
  wrap.textContent = t('wrapOn')
  const copy = el('button', 'oad-code-btn', { type: 'button' })
  copy.textContent = t('copyCode')
  tools.append(toggle, wrap, copy)
  bar.append(lang, tools)
  const diagram = el('div', 'oad-mermaid', { contenteditable: 'false' })
  const pre = el('pre')
  const code = el('code')
  pre.append(code)
  dom.append(bar, diagram, pre)

  // mermaid：默认看图，点“源码”改代码。按块编号记住，节点视图重建（比如远端改动）后不丢
  let mode: 'diagram' | 'source' = modes.get(node.attrs.id) ?? 'diagram'
  let drawn = ''
  let timer = 0
  const isMermaid = () => (node.attrs.language || '').toLowerCase() === 'mermaid'
  const redraw = () => {
    if (!isMermaid()) return
    const src = node.textContent
    if (src === drawn) return
    clearTimeout(timer)
    timer = window.setTimeout(() => {
      drawn = src
      if (!src.trim()) {
        diagram.textContent = ''
        return
      }
      drawMermaid(src).then(
        (svg) => {
          if (drawn === src) {
            diagram.innerHTML = svg
            diagram.classList.remove('is-error')
          }
        },
        (err) => {
          if (drawn !== src) return
          diagram.textContent = t('diagramError') + String(err?.message ?? err).split('\n')[0]
          diagram.classList.add('is-error')
        },
      )
    }, 250)
  }
  const syncMode = () => {
    const m = isMermaid()
    dom.classList.toggle('is-mermaid', m)
    toggle.hidden = !m
    diagram.hidden = !m || mode === 'source'
    pre.hidden = m && mode === 'diagram' && !!node.textContent.trim()
    toggle.textContent = mode === 'diagram' ? t('showSource') : t('showDiagram')
    if (m) redraw()
  }

  const sync = (n: PMNode) => {
    if (document.activeElement !== lang) lang.value = n.attrs.language ?? ''
    lang.readOnly = !p.editor.isEditable
    dom.classList.toggle('is-wrap', !!n.attrs.wrap)
    wrap.setAttribute('aria-pressed', String(!!n.attrs.wrap))
    dom.dataset.lang = n.attrs.language ?? ''
    blockAttrs(dom, n)
    syncMode()
  }

  lang.addEventListener('change', () => setAttr(p, 'language', lang.value.trim() || null))
  lang.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === 'Escape') {
      e.preventDefault()
      lang.blur()
      p.editor.commands.focus()
    }
  })
  for (const b of [toggle, wrap, copy]) b.addEventListener('mousedown', (e) => e.preventDefault())
  wrap.addEventListener('click', () => setAttr(p, 'wrap', !node.attrs.wrap))
  copy.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(node.textContent)
      copy.textContent = t('copied')
      setTimeout(() => (copy.textContent = t('copyCode')), 1400)
    } catch {
      /* 没有剪贴板权限 */
    }
  })
  toggle.addEventListener('click', () => {
    mode = mode === 'diagram' ? 'source' : 'diagram'
    if (node.attrs.id) modes.set(node.attrs.id, mode)
    syncMode()
  })
  // 在图上双击也进源码
  diagram.addEventListener('dblclick', () => {
    mode = 'source'
    if (node.attrs.id) modes.set(node.attrs.id, mode)
    syncMode()
  })

  sync(node)
  return {
    dom,
    contentDOM: code,
    update(n) {
      if (n.type !== node.type) return false
      node = n
      sync(n)
      return true
    },
    stopEvent: (e) => bar.contains(e.target as Node) || diagram.contains(e.target as Node),
    ignoreMutation: (m) => bar.contains(m.target) || diagram.contains(m.target) || m.target === dom,
    destroy() {
      clearTimeout(timer)
    },
  }
}
