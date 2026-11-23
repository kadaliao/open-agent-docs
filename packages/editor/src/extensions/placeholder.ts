// 空段落的提示文字。只看光标所在的那一段和“整篇是空的”两种情况，不遍历全文，大文档打字也不多花时间。
import { Extension } from '@tiptap/core'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'

export interface PlaceholderOptions {
  empty: () => string
  line: () => string
}

export const OadPlaceholder = Extension.create<PlaceholderOptions>({
  name: 'oadPlaceholder',
  addOptions() {
    return { empty: () => '', line: () => '' }
  },
  addProseMirrorPlugins() {
    const opts = this.options
    const editor = this.editor
    return [
      new Plugin({
        key: new PluginKey('oadPlaceholder'),
        props: {
          decorations(state) {
            const { doc, selection } = state
            if (!editor.isEditable) return null
            const first = doc.firstChild
            if (doc.childCount === 1 && first && first.type.name === 'paragraph' && first.content.size === 0) {
              return DecorationSet.create(doc, [
                Decoration.node(0, first.nodeSize, { class: 'oad-is-empty oad-is-editor-empty', 'data-placeholder': opts.empty() }),
              ])
            }
            const { $from, empty } = selection
            if (!empty || !editor.isFocused) return null
            const parent = $from.parent
            if (parent.type.name !== 'paragraph' || parent.content.size !== 0) return null
            // 列表、表格里的空段落不显示
            const outer = $from.depth > 1 ? $from.node($from.depth - 1).type.name : 'doc'
            if (outer !== 'doc' && outer !== 'callout' && outer !== 'blockquote' && outer !== 'detailsContent') return null
            const pos = $from.before()
            return DecorationSet.create(doc, [
              Decoration.node(pos, pos + parent.nodeSize, { class: 'oad-is-empty', 'data-placeholder': opts.line() }),
            ])
          },
        },
      }),
    ]
  },
})
