// 块左边的 ⋮⋮ 把手：鼠标停在一块上时出现；拖动它移动这一块（能拖进 / 拖出列表、引用、提示框），点一下打开块菜单。
// 列表项按项拖，别的按顶层块拖。
import { Extension } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { NodeSelection, Plugin, PluginKey } from '@tiptap/pm/state'
import type { EditorView } from '@tiptap/pm/view'
import { t } from '../strings'

export interface BlockTarget {
  pos: number
  node: PMNode
  rect: DOMRect
}

export interface BlockHandleStorage {
  // 宿主（React）注册：点了把手，打开块菜单
  open: (b: BlockTarget | null) => void
  // 菜单开着时把手别跟着鼠标跑
  locked: boolean
}

const ITEM = new Set(['listItem', 'taskItem'])

/** 鼠标所在位置对应的“可拖动的块”：最里层的列表项，否则顶层块 */
export function blockAt(view: EditorView, x: number, y: number): { pos: number; node: PMNode } | null {
  const box = view.dom.getBoundingClientRect()
  const hit = view.posAtCoords({ left: Math.max(box.left + 2, Math.min(x, box.right - 2)), top: y })
  if (!hit) return null
  const doc = view.state.doc
  const $p = doc.resolve(hit.inside >= 0 ? hit.inside : hit.pos)
  // posAtCoords 的 inside 是鼠标正落在的那个节点（原子块）
  for (let d = $p.depth; d > 0; d--) if (ITEM.has($p.node(d).type.name)) return { pos: $p.before(d), node: $p.node(d) }
  if (hit.inside >= 0) {
    const n = doc.nodeAt(hit.inside)
    const $i = doc.resolve(hit.inside)
    if (n && $i.depth === 0) return { pos: hit.inside, node: n }
  }
  if ($p.depth >= 1) return { pos: $p.before(1), node: $p.node(1) }
  const after = $p.nodeAfter
  return after ? { pos: $p.pos, node: after } : null
}

export const BlockHandle = Extension.create<object, BlockHandleStorage>({
  name: 'oadBlockHandle',
  addStorage() {
    return { open: () => {}, locked: false }
  },
  addProseMirrorPlugins() {
    const storage = this.storage
    const editor = this.editor
    return [
      new Plugin({
        key: new PluginKey('oadBlockHandle'),
        view(view) {
          const host = view.dom.parentElement as HTMLElement
          const handle = document.createElement('div')
          handle.className = 'oad-handle'
          handle.draggable = true
          handle.setAttribute('role', 'button')
          handle.setAttribute('aria-label', t('dragHandle'))
          handle.title = t('dragHandle')
          handle.innerHTML = '<svg viewBox="0 0 10 16" aria-hidden="true"><circle cx="2.5" cy="3" r="1.4"/><circle cx="7.5" cy="3" r="1.4"/><circle cx="2.5" cy="8" r="1.4"/><circle cx="7.5" cy="8" r="1.4"/><circle cx="2.5" cy="13" r="1.4"/><circle cx="7.5" cy="13" r="1.4"/></svg>'
          handle.hidden = true
          host.append(handle)
          let cur: { pos: number; node: PMNode; dom: HTMLElement } | null = null
          let raf = 0

          const place = (b: { pos: number; node: PMNode } | null) => {
            if (!b || !editor.isEditable) {
              cur = null
              handle.hidden = true
              return
            }
            const dom = view.nodeDOM(b.pos) as HTMLElement | null
            if (!dom || dom.nodeType !== 1) return
            cur = { ...b, dom }
            const hr = host.getBoundingClientRect()
            const r = dom.getBoundingClientRect()
            // 和第一行文字对齐
            const lh = parseFloat(getComputedStyle(dom).lineHeight) || 24
            const top = r.top - hr.top + Math.max(0, Math.min(lh, r.height) / 2 - 12) + (b.node.isTextblock || ITEM.has(b.node.type.name) ? parseFloat(getComputedStyle(dom).paddingTop) || 0 : 0)
            handle.style.top = `${top}px`
            handle.style.left = `${r.left - hr.left - 26}px`
            handle.hidden = false
          }

          const onMove = (e: MouseEvent) => {
            if (storage.locked || view.dragging) return
            cancelAnimationFrame(raf)
            raf = requestAnimationFrame(() => {
              const box = view.dom.getBoundingClientRect()
              if (e.clientY < box.top || e.clientY > box.bottom || e.clientX < box.left - 60 || e.clientX > box.right + 20) return place(null)
              if (e.target === handle) return
              place(blockAt(view, e.clientX, e.clientY))
            })
          }
          const onLeave = (e: MouseEvent) => {
            if (storage.locked) return
            if (e.relatedTarget && host.contains(e.relatedTarget as Node)) return
            place(null)
          }

          handle.addEventListener('dragstart', (e) => {
            if (!cur) return
            const sel = NodeSelection.create(view.state.doc, cur.pos)
            view.dispatch(view.state.tr.setSelection(sel))
            const slice = sel.content()
            view.dragging = { slice, move: true } as typeof view.dragging
            e.dataTransfer?.setData('text/plain', cur.node.textContent)
            e.dataTransfer?.setData('text/html', cur.dom.outerHTML)
            if (e.dataTransfer) {
              e.dataTransfer.effectAllowed = 'copyMove'
              e.dataTransfer.setDragImage(cur.dom, 0, 0)
            }
            cur.dom.classList.add('oad-is-dragging')
            const done = () => {
              cur?.dom.classList.remove('oad-is-dragging')
              handle.hidden = true
              document.removeEventListener('dragend', done, true)
            }
            document.addEventListener('dragend', done, true)
          })
          handle.addEventListener('mousedown', (e) => e.stopPropagation())
          handle.addEventListener('click', () => {
            if (!cur) return
            view.dispatch(view.state.tr.setSelection(NodeSelection.create(view.state.doc, cur.pos)))
            storage.open({ pos: cur.pos, node: cur.node, rect: handle.getBoundingClientRect() })
          })

          host.addEventListener('mousemove', onMove)
          host.addEventListener('mouseleave', onLeave)
          return {
            update(v, prev) {
              if (prev.doc !== v.state.doc && cur && !storage.locked) {
                // 文档变了：位置可能不对了，先藏起来，鼠标一动再出来
                handle.hidden = true
                cur = null
              }
            },
            destroy() {
              cancelAnimationFrame(raf)
              host.removeEventListener('mousemove', onMove)
              host.removeEventListener('mouseleave', onLeave)
              handle.remove()
            },
          }
        },
      }),
    ]
  },
})
