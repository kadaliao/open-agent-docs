// 给光标所在的顶层块加 .oad-live（样式里让它不参与 content-visibility 跳过）。
import { Extension } from '@tiptap/core'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'

export const LiveBlock = Extension.create({
  name: 'oadLiveBlock',
  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey('oadLiveBlock'),
        props: {
          decorations(state) {
            const { $head } = state.selection
            if ($head.depth === 0) {
              const n = $head.nodeAfter
              return n ? DecorationSet.create(state.doc, [Decoration.node($head.pos, $head.pos + n.nodeSize, { class: 'oad-live' })]) : null
            }
            const start = $head.before(1)
            return DecorationSet.create(state.doc, [Decoration.node(start, start + $head.node(1).nodeSize, { class: 'oad-live' })])
          },
        },
      }),
    ]
  },
})

// 闪一下某一块（打开带 #块编号 的链接时）：用 decoration，节点重画了也还在
const flashKey = new PluginKey<{ id: string; until: number } | null>('oadFlash')
export const FlashBlock = Extension.create({
  name: 'oadFlash',
  addProseMirrorPlugins() {
    return [
      new Plugin<{ id: string; until: number } | null>({
        key: flashKey,
        state: {
          init: () => null,
          apply: (tr, v) => (tr.getMeta(flashKey) !== undefined ? tr.getMeta(flashKey) : v),
        },
        props: {
          decorations(state) {
            const f = flashKey.getState(state)
            if (!f) return null
            let deco: Decoration | null = null
            state.doc.descendants((n, pos) => {
              if (deco) return false
              if (n.attrs.id === f.id) deco = Decoration.node(pos, pos + n.nodeSize, { class: 'oad-flash' })
              return !n.isTextblock
            })
            return deco ? DecorationSet.create(state.doc, [deco]) : null
          },
        },
      }),
    ]
  },
})

/** 滚到某一块并闪一下；找不到返回 false */
export function flashBlock(editor: import('@tiptap/core').Editor, id: string) {
  const el = editor.view.dom.querySelector(`[data-id="${id}"]`) as HTMLElement | null
  if (!el) return false
  el.scrollIntoView({ block: 'center' })
  editor.view.dispatch(editor.state.tr.setMeta(flashKey, { id, until: Date.now() + 1600 }))
  setTimeout(() => !editor.isDestroyed && editor.view.dispatch(editor.state.tr.setMeta(flashKey, null)), 1700)
  return true
}
