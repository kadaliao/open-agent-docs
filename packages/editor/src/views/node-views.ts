// 几个块的节点视图，直接用 DOM 写（不走 React），1000 段的页面打开时也不额外花时间。
// 改属性一律用 setNodeAttribute，只动那一个属性，id 等其他属性留着。
import type { NodeViewRenderer, NodeViewRendererProps } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { NodeSelection, TextSelection } from '@tiptap/pm/state'
import { t } from '../strings'
import { renderMath } from './katex'

export type Props = NodeViewRendererProps

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, attrs?: Record<string, string>) {
  const e = document.createElement(tag)
  if (cls) e.className = cls
  if (attrs) for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v)
  return e
}

export function setAttr(p: Props, key: string, value: unknown) {
  const pos = typeof p.getPos === 'function' ? p.getPos() : undefined
  if (typeof pos !== 'number') return
  const { view } = p.editor
  const node = view.state.doc.nodeAt(pos)
  if (!node || node.attrs[key] === value) return
  view.dispatch(view.state.tr.setNodeAttribute(pos, key, value))
}

/** 这个元素有没有焦点（在 Shadow DOM 里 document.activeElement 是宿主元素，要问它所在的 root） */
export function isFocused(e: Element) {
  return (e.getRootNode() as Document | ShadowRoot).activeElement === e
}

export function blockAttrs(dom: HTMLElement, node: PMNode) {
  if (node.attrs.id) dom.setAttribute('data-id', node.attrs.id)
  else dom.removeAttribute('data-id')
}

// ---------- 折叠块 ----------
export const detailsView: NodeViewRenderer = (p) => {
  let node = p.node
  const dom = el('div', 'oad-details', { 'data-type': 'details' })
  const toggle = el('button', 'oad-details-toggle', { type: 'button', contenteditable: 'false' })
  const content = el('div', 'oad-details-body')
  dom.append(toggle, content)
  const apply = (open: boolean) => {
    dom.classList.toggle('oad-is-open', open)
    toggle.setAttribute('aria-expanded', String(open))
    toggle.setAttribute('aria-label', open ? t('collapse') : t('expand'))
  }
  blockAttrs(dom, node)
  toggle.addEventListener('mousedown', (e) => e.preventDefault())
  toggle.addEventListener('click', () => {
    const open = !node.attrs.open
    if (p.editor.isEditable) setAttr(p, 'open', open)
    else apply(open)
  })
  // 收起时内容区靠 CSS 隐藏（.oad-details:not(.oad-is-open)）
  apply(!!node.attrs.open)
  return {
    dom,
    contentDOM: content,
    update(n) {
      if (n.type !== node.type) return false
      node = n
      apply(!!n.attrs.open)
      blockAttrs(dom, n)
      return true
    },
    ignoreMutation: (m) => m.type !== 'selection' && (toggle.contains(m.target) || m.target === dom),
    stopEvent: (e) => toggle.contains(e.target as Node),
  }
}

// ---------- 提示框 ----------
const CALLOUT_ICONS: Record<string, string> = {
  note: 'i',
  tip: '✦',
  important: '!',
  warning: '⚠',
  caution: '⛔',
}
const KINDS = ['note', 'tip', 'important', 'warning', 'caution'] as const

export const calloutView: NodeViewRenderer = (p) => {
  let node = p.node
  const dom = el('div', 'oad-callout', { 'data-type': 'callout' })
  const head = el('div', 'oad-callout-head', { contenteditable: 'false' })
  const icon = el('button', 'oad-callout-icon', { type: 'button' })
  const title = el('input', 'oad-callout-title', { spellcheck: 'false' })
  title.placeholder = t('calloutTitle')
  title.readOnly = !p.editor.isEditable
  const menu = el('div', 'oad-callout-menu', { role: 'menu' })
  menu.hidden = true
  for (const k of KINDS) {
    const b = el('button', 'oad-callout-kind', { type: 'button', 'data-kind': k, role: 'menuitem' })
    const i = el('span', 'oad-callout-kind-icon')
    i.textContent = CALLOUT_ICONS[k]
    b.append(i, document.createTextNode(t(`callout.${k}` as 'callout.note')))
    b.addEventListener('mousedown', (e) => e.preventDefault())
    b.addEventListener('click', () => {
      setAttr(p, 'kind', k)
      menu.hidden = true
    })
    menu.append(b)
  }
  head.append(icon, title, menu)
  const content = el('div', 'oad-callout-body')
  dom.append(head, content)

  const sync = (n: PMNode) => {
    const kind = KINDS.includes(n.attrs.kind) ? n.attrs.kind : 'note'
    dom.dataset.kind = kind
    icon.textContent = CALLOUT_ICONS[kind]
    icon.title = t(`callout.${kind}` as 'callout.note')
    if (!isFocused(title)) title.value = n.attrs.title ?? ''
    dom.classList.toggle('oad-has-title', !!n.attrs.title)
    blockAttrs(dom, n)
  }
  sync(node)

  icon.addEventListener('mousedown', (e) => e.preventDefault())
  icon.addEventListener('click', () => {
    if (!p.editor.isEditable) return
    menu.hidden = !menu.hidden
    if (!menu.hidden) {
      const close = (e: MouseEvent) => {
        if (!menu.contains(e.target as Node) && e.target !== icon) {
          menu.hidden = true
          document.removeEventListener('mousedown', close, true)
        }
      }
      document.addEventListener('mousedown', close, true)
    }
  })
  const commitTitle = () => setAttr(p, 'title', title.value.trim() || null)
  title.addEventListener('blur', commitTitle)
  title.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === 'Escape') {
      e.preventDefault()
      commitTitle()
      // 回到正文开头
      const pos = typeof p.getPos === 'function' ? p.getPos() : undefined
      if (typeof pos === 'number') {
        const { view } = p.editor
        view.dispatch(view.state.tr.setSelection(TextSelection.near(view.state.doc.resolve(pos + 1))))
        view.focus()
      }
    }
  })

  return {
    dom,
    contentDOM: content,
    update(n) {
      if (n.type !== node.type) return false
      node = n
      sync(n)
      return true
    },
    stopEvent: (e) => head.contains(e.target as Node),
    ignoreMutation: (m) => head.contains(m.target) || m.target === dom,
  }
}

// ---------- 公式块 ----------
export const mathBlockView: NodeViewRenderer = (p) => {
  let node = p.node
  const dom = el('div', 'oad-math-block', { 'data-type': 'mathBlock', contenteditable: 'false' })
  const out = el('div', 'oad-math-render')
  const editor = el('div', 'oad-math-editor')
  const ta = el('textarea', 'oad-math-source', { spellcheck: 'false', rows: '2' })
  ta.placeholder = t('mathPlaceholder')
  const preview = el('div', 'oad-math-preview')
  editor.append(ta, preview)
  editor.hidden = true
  dom.append(out, editor)
  let editing = false

  const draw = () => renderMath(out, node.attrs.latex || '', true, t('mathEmpty'))
  const fit = () => {
    ta.style.height = 'auto'
    ta.style.height = ta.scrollHeight + 'px'
  }
  const open = () => {
    if (!p.editor.isEditable || editing) return
    editing = true
    dom.classList.add('oad-is-editing')
    editor.hidden = false
    ta.value = node.attrs.latex || ''
    renderMath(preview, ta.value, true, '')
    fit()
    ta.focus()
  }
  const close = (moveOut: 1 | 0) => {
    if (!editing) return
    editing = false
    dom.classList.remove('oad-is-editing')
    editor.hidden = true
    setAttr(p, 'latex', ta.value)
    draw()
    const pos = typeof p.getPos === 'function' ? p.getPos() : undefined
    if (typeof pos === 'number' && moveOut) {
      const { view } = p.editor
      const tr = view.state.tr
      const after = pos + tr.doc.nodeAt(pos)!.nodeSize
      // 后面没有可以打字的地方就补一个空段落，免得光标又选中公式块、一打字把它换掉
      const next = tr.doc.resolve(after).nodeAfter
      if (!next || !next.isTextblock) tr.insert(after, view.state.schema.nodes.paragraph.create())
      view.dispatch(tr.setSelection(TextSelection.create(tr.doc, after + 1)).scrollIntoView())
      view.focus()
    }
  }
  ta.addEventListener('input', () => {
    fit()
    renderMath(preview, ta.value, true, '')
  })
  ta.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' || (e.key === 'Enter' && (e.metaKey || e.ctrlKey))) {
      e.preventDefault()
      close(1)
    }
  })
  ta.addEventListener('blur', () => close(0))
  out.addEventListener('click', open)
  draw()
  blockAttrs(dom, node)

  return {
    dom,
    update(n) {
      if (n.type !== node.type) return false
      const changed = n.attrs.latex !== node.attrs.latex
      node = n
      if (changed && !editing) draw()
      blockAttrs(dom, n)
      return true
    },
    selectNode() {
      dom.classList.add('oad-is-selected')
    },
    deselectNode() {
      dom.classList.remove('oad-is-selected')
    },
    stopEvent: (e) => editor.contains(e.target as Node),
    ignoreMutation: () => true,
    destroy() {
      editing = false
    },
  }
}

// ---------- 行内公式 ----------
export const mathInlineView: NodeViewRenderer = (p) => {
  let node = p.node
  const dom = el('span', 'oad-math-inline', { 'data-type': 'mathInline', contenteditable: 'false' })
  const out = el('span', 'oad-math-render')
  const input = el('input', 'oad-math-inline-source', { spellcheck: 'false' })
  input.hidden = true
  dom.append(out, input)
  let editing = false
  const draw = () => renderMath(out, node.attrs.latex || '', false, '∅')
  const fit = () => (input.style.width = Math.max(4, input.value.length + 1) + 'ch')
  const open = () => {
    if (!p.editor.isEditable || editing) return
    editing = true
    input.value = node.attrs.latex || ''
    input.hidden = false
    out.hidden = true
    fit()
    input.focus()
  }
  const close = (dir: 1 | 0) => {
    if (!editing) return
    editing = false
    input.hidden = true
    out.hidden = false
    const pos = typeof p.getPos === 'function' ? p.getPos() : undefined
    const v = input.value.trim()
    const { view } = p.editor
    if (typeof pos === 'number' && !v) {
      // 清空了就删掉
      view.dispatch(view.state.tr.delete(pos, pos + 1))
      view.focus()
      return
    }
    setAttr(p, 'latex', v)
    draw()
    if (typeof pos === 'number' && dir) {
      view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, pos + 1)))
      view.focus()
    }
  }
  input.addEventListener('input', fit)
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === 'Escape') {
      e.preventDefault()
      close(1)
    }
  })
  input.addEventListener('blur', () => close(0))
  out.addEventListener('click', open)
  draw()
  return {
    dom,
    update(n) {
      if (n.type !== node.type) return false
      const changed = n.attrs.latex !== node.attrs.latex
      node = n
      if (changed && !editing) draw()
      return true
    },
    selectNode() {
      dom.classList.add('oad-is-selected')
    },
    deselectNode() {
      dom.classList.remove('oad-is-selected')
    },
    stopEvent: (e) => input.contains(e.target as Node),
    ignoreMutation: () => true,
  }
}

// 公式块刚插进来时直接进入编辑
export function openMathAt(view: import('@tiptap/pm/view').EditorView, pos: number) {
  const dom = view.nodeDOM(pos) as HTMLElement | null
  const target = dom?.querySelector('.oad-math-render') as HTMLElement | null
  if (target) {
    view.dispatch(view.state.tr.setSelection(NodeSelection.create(view.state.doc, pos)))
    target.click()
  }
}
