// 块编号插件：本地改动里新出现的块、没有编号的块分配编号；拆段、粘贴造成重复时，后出现的那个换新编号。
// 远端（Yjs 同步过来）的改动不动，免得两边互相改编号。
import { Extension, combineTransactionSteps, getChangedRanges } from '@tiptap/core'
import { isChangeOrigin } from '../extensions/collaboration'
import type { Node as PMNode } from '@tiptap/pm/model'
import { Plugin, PluginKey, type Transaction } from '@tiptap/pm/state'
import { ReplaceAroundStep, ReplaceStep } from '@tiptap/pm/transform'
import { BLOCK_TYPES, BLOCK_TYPE_SET, isBlockId, newBlockId } from './ids'

export const blockIdKey = new PluginKey('blockId')

// 这一步有没有放进新的块（拆段、粘贴、包一层），有才需要查重复
function addsBlocks(tr: Transaction): boolean {
  for (const step of tr.steps) {
    if (step instanceof ReplaceStep || step instanceof ReplaceAroundStep) {
      let found = false
      step.slice.content.forEach((n) => {
        if (n.isBlock) found = true
      })
      if (found) return true
    }
  }
  return false
}

export interface BlockIdOptions {
  generate: () => string
}

export const BlockId = Extension.create<BlockIdOptions>({
  name: 'blockId',

  addOptions() {
    return { generate: newBlockId }
  },

  addGlobalAttributes() {
    return [
      {
        types: [...BLOCK_TYPES],
        attributes: {
          id: {
            default: null,
            keepOnSplit: false,
            parseHTML: (el) => el.getAttribute('data-id'),
            renderHTML: (attrs) => (attrs.id ? { 'data-id': attrs.id } : {}),
          },
        },
      },
    ]
  },

  addProseMirrorPlugins() {
    const generate = this.options.generate
    const editor = this.editor
    return [
      new Plugin({
        key: blockIdKey,
        appendTransaction(transactions, oldState, newState) {
          const local = transactions.filter((tr) => tr.docChanged && !isChangeOrigin(tr) && !tr.getMeta(blockIdKey))
          if (!local.length) return null
          // 输入法组字时改节点属性会打断组字，留到下一次改动再补
          if (editor?.view?.composing) return null

          const doc = newState.doc
          const ranges = getChangedRanges(combineTransactionSteps(oldState.doc, [...transactions]))
          const touched = new Map<number, PMNode>()
          for (const { newRange } of ranges) {
            const from = Math.max(0, Math.min(newRange.from, doc.content.size))
            const to = Math.max(from, Math.min(newRange.to, doc.content.size))
            doc.nodesBetween(from, to, (node, pos) => {
              if (BLOCK_TYPE_SET.has(node.type.name)) touched.set(pos, node)
              return !node.isTextblock
            })
          }
          if (!touched.size) return null

          let counts: Map<string, number> | null = null
          if (local.some(addsBlocks)) {
            counts = new Map()
            doc.descendants((node) => {
              const id = node.attrs.id
              if (isBlockId(id)) counts!.set(id, (counts!.get(id) ?? 0) + 1)
              return !node.isTextblock
            })
          }

          const entries = [...touched.entries()].sort((a, b) => a[0] - b[0])
          // 重复的编号：文档里还有没被这次改动碰到的同号节点（原件），碰到的全换；都碰到了就留第一个
          const touchedWith = new Map<string, number>()
          for (const [, n] of entries) {
            const id = n.attrs.id
            if (isBlockId(id)) touchedWith.set(id, (touchedWith.get(id) ?? 0) + 1)
          }
          const kept = new Set<string>()
          const used = counts ? new Set(counts.keys()) : null
          const fresh = () => {
            let id = generate()
            while (used?.has(id)) id = generate()
            used?.add(id)
            return id
          }

          const tr = newState.tr
          for (const [pos, node] of entries) {
            const id = node.attrs.id
            let replace = !isBlockId(id)
            if (!replace && counts && (counts.get(id) ?? 0) > 1) {
              const untouched = counts.get(id)! - (touchedWith.get(id) ?? 0)
              if (untouched > 0 || kept.has(id)) replace = true
              else kept.add(id)
            }
            if (replace) tr.setNodeAttribute(pos, 'id', fresh())
          }
          if (!tr.docChanged) return null
          tr.setMeta(blockIdKey, true)
          return tr
        },
      }),
    ]
  },
})
