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
