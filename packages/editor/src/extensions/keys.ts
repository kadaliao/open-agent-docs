// 和 Notion 一样的块快捷键：⌘⌥0–8 转换块、⌘⇧↑↓ 挪块、⌘D 复制块、⌘⇧M 评论、⌘K 加链接（有选中文字时；没选中交给宿主开搜索）。
// 加粗、斜体、⌘E 行内代码、⌘⇧S 删除线、⌘⇧H 高亮、⌘Z 这些 Tiptap 自带。
import { Extension, type Editor } from '@tiptap/core'
import type { Node as PMNode, ResolvedPos } from '@tiptap/pm/model'
import { TextSelection } from '@tiptap/pm/state'

export interface KeysStorage {
  /** 宿主填：⌘⇧M 评论选中的文字 */
  comment: (() => void) | null
  /** 宿主填：⌘K 选中了文字时要网址 */
  link: (() => void) | null
}

const ITEM = new Set(['listItem', 'taskItem'])

/** 光标所在的“块”：列表里的一项，或者顶层的一块 */
function blockAround($pos: ResolvedPos): { pos: number; node: PMNode; depth: number } | null {
  for (let d = $pos.depth; d > 0; d--) {
    const n = $pos.node(d)
    if (ITEM.has(n.type.name) || d === 1) return { pos: $pos.before(d), node: n, depth: d }
  }
  return null
}

function moveBlock(editor: Editor, dir: -1 | 1) {
  const { state } = editor
  const b = blockAround(state.selection.$from)
  if (!b) return false
  const $b = state.doc.resolve(b.pos)
  const parent = $b.parent
  const idx = $b.index()
  const other = idx + dir
  if (other < 0 || other >= parent.childCount) return true
  const sib = parent.child(other)
  const offset = state.selection.from - b.pos
  const tr = state.tr
  if (dir === -1) {
    const start = b.pos - sib.nodeSize
    tr.replaceWith(start, b.pos + b.node.nodeSize, [b.node, sib])
    tr.setSelection(TextSelection.create(tr.doc, start + offset))
  } else {
    const end = b.pos + b.node.nodeSize + sib.nodeSize
    tr.replaceWith(b.pos, end, [sib, b.node])
    tr.setSelection(TextSelection.create(tr.doc, b.pos + sib.nodeSize + offset))
  }
  editor.view.dispatch(tr.scrollIntoView())
  return true
}

function duplicateBlock(editor: Editor) {
  const { state } = editor
  const b = blockAround(state.selection.$from)
  if (!b) return false
  const at = b.pos + b.node.nodeSize
  // 块编号由 block-id 插件发新的
  const copy = b.node.type.create({ ...b.node.attrs, id: null }, b.node.content, b.node.marks)
  const tr = state.tr.insert(at, copy)
  tr.setSelection(TextSelection.create(tr.doc, Math.min(at + 1 + (state.selection.from - b.pos - 1), tr.doc.content.size)))
  editor.view.dispatch(tr.scrollIntoView())
  return true
}

const turn = (editor: Editor, run: () => boolean) => (editor.isEditable ? run() : false)

export const NotionKeys = Extension.create<object, KeysStorage>({
  name: 'oadKeys',
  priority: 120,
  addStorage() {
    return { comment: null, link: null }
  },
  addKeyboardShortcuts() {
    const e = this.editor
    const c = () => e.chain().focus()
    return {
      'Mod-Alt-0': () => turn(e, () => c().setParagraph().run()),
      'Mod-Alt-1': () => turn(e, () => c().toggleHeading({ level: 1 }).run()),
      'Mod-Alt-2': () => turn(e, () => c().toggleHeading({ level: 2 }).run()),
      'Mod-Alt-3': () => turn(e, () => c().toggleHeading({ level: 3 }).run()),
      'Mod-Alt-4': () => turn(e, () => c().toggleTaskList().run()),
      'Mod-Alt-5': () => turn(e, () => c().toggleBulletList().run()),
      'Mod-Alt-6': () => turn(e, () => c().toggleOrderedList().run()),
      'Mod-Alt-7': () => turn(e, () => (c() as unknown as { setDetails: () => { run: () => boolean } }).setDetails().run()),
      'Mod-Alt-8': () => turn(e, () => c().toggleCodeBlock().run()),
      'Mod-Shift-ArrowUp': () => turn(e, () => moveBlock(e, -1)),
      'Mod-Shift-ArrowDown': () => turn(e, () => moveBlock(e, 1)),
      'Mod-d': () => turn(e, () => duplicateBlock(e)),
      'Mod-Shift-m': () => {
        if (e.state.selection.empty || !this.storage.comment) return false
        this.storage.comment()
        return true
      },
      'Mod-k': () => {
        if (e.state.selection.empty || !this.storage.link) return false
        this.storage.link()
        return true
      },
    }
  },
})
