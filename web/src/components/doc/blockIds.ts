// 段的编号（bid）：只打在“段”上（顶层节点；顶层列表、表格里的每一项、每一行），用来对应服务器的段，作者和评论都挂在上面。
// 每次文档改动后整理一遍：
// - 没有编号的段先接手原来的编号：段落换成标题、套进列表、从列表里拿出来，作者和评论不丢
// - 同一个编号出现两次（复制粘贴），内容和原来一样的那段留着，其余的发新编号
// - 不是段的节点（列表项里的段落、嵌进下一级的项目）把编号清掉
import { Extension } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import { Mapping } from '@tiptap/pm/transform'
import { BLOCK_TYPES, blocksOf, type Loc } from './markdown'

const B62 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789'
export const newId = () => 'u' + Array.from(crypto.getRandomValues(new Uint8Array(10)), b => B62[b % 62]).join('')

export const blockIdsKey = new PluginKey('blockIds')

function descendantId(node: PMNode, free: Set<string>): string | null {
  let found: string | null = null
  node.descendants(n => {
    if (found) return false
    if (n.attrs.bid && free.has(n.attrs.bid)) found = n.attrs.bid
    return !found
  })
  return found
}

export const BlockIds = Extension.create({
  name: 'blockIds',
  addGlobalAttributes() {
    return [
      { types: BLOCK_TYPES, attributes: { bid: { default: null, keepOnSplit: false, parseHTML: () => null, renderHTML: a => (a.bid ? { 'data-bid': a.bid } : {}) } } },
      // 表头那一行：原来的分隔行（| --- | :-: |）原样留着，写回时不改对齐的写法
      { types: ['tableRow'], attributes: { delim: { default: null, keepOnSplit: false, parseHTML: () => null, renderHTML: () => ({}) } } },
    ]
  },
  addProseMirrorPlugins() {
    return [new Plugin({
      key: blockIdsKey,
      appendTransaction(trs, oldState, newState) {
        if (!trs.some(tr => tr.docChanged)) return null
        const mapping = new Mapping()
        trs.forEach(tr => tr.mapping.maps.forEach(m => mapping.appendMap(m)))
        const locs = blocksOf(newState.doc)
        const old = new Map<string, Loc>()
        for (const l of blocksOf(oldState.doc)) if (l.id) old.set(l.id, l)
        const want = new Map<number, string | null>()   // 段的位置 → 应该有的编号
        // 重复的编号：内容和原来一样的留着
        const seen = new Map<string, number[]>()
        locs.forEach((l, k) => { if (l.id) seen.set(l.id, [...(seen.get(l.id) || []), k]) })
        const kept = new Set<string>()
        for (const [id, ks] of seen) {
          const keep = ks.find(k => locs[k].node.textContent === old.get(id)?.node.textContent) ?? ks[0]
          kept.add(id)
          for (const k of ks) if (k !== keep) want.set(k, null)
        }
        // 没编号的段：接手内部带着的编号，或者原来就在这个位置的段的编号
        const free = new Set([...old.keys()].filter(id => !kept.has(id)))
        locs.forEach((l, k) => {
          if (l.id && !want.has(k)) return
          let id = descendantId(l.node, free)
          if (!id) {
            for (const cand of free) {
              const at = mapping.map(old.get(cand)!.pos, 1)
              if (at >= l.pos && at < l.pos + l.node.nodeSize) { id = cand; break }
            }
          }
          if (id) free.delete(id)
          want.set(k, id || newId())
        })
        const tr = newState.tr
        for (const [k, id] of want) {
          const l = locs[k]
          if (l.node.attrs.bid !== id) tr.setNodeAttribute(l.pos, 'bid', id)
        }
        // 不是段的节点上残留的编号清掉（列表项里的段落、嵌进下一级的项目）。整篇扫一遍：几万个节点也只要几毫秒
        const isBlock = new Set(locs.map(l => l.pos))
        newState.doc.descendants((n, pos) => {
          if (n.attrs.bid && !isBlock.has(pos)) tr.setNodeAttribute(pos, 'bid', null)
          return !n.isTextblock
        })
        if (!tr.docChanged) return null
        return tr.setMeta(blockIdsKey, true)
      },
    })]
  },
})
