// Tiptap 3 StarterKit 的 schema，外加每个块级节点一个 id 属性（块编号）。
import { getSchema, Extension } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'

export const BLOCK_TYPES = ['paragraph', 'heading', 'blockquote', 'bulletList', 'orderedList', 'listItem', 'codeBlock', 'horizontalRule']

export const BlockId = Extension.create({
  name: 'blockId',
  addGlobalAttributes() {
    return [{ types: BLOCK_TYPES, attributes: { id: { default: null, keepOnSplit: false } } }]
  },
})

export const extensions = [StarterKit, BlockId]
export const schema = getSchema(extensions)
