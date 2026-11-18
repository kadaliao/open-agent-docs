// 每段左边标出是谁写的：Agent 写的画一条它的颜色，鼠标停上去显示“名字 · 时间”；人写的不画。
// 作者这些放在插件状态里，不放进节点属性：改了不算编辑，也不进撤销记录。
import { Extension } from '@tiptap/core'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'
import { blocksOf } from './markdown'

export type Who = { name: string; color: string } | null
export type GutterMeta = { by: Map<string, { by: string; ts: number }>; conflicts: Set<string>; flash: Set<string>; counts: Map<string, number> }
export const gutterKey = new PluginKey<GutterMeta>('gutter')

export const Gutter = Extension.create<{ who: (by: string) => Who; when: (ts: number) => string }>({
  name: 'gutter',
  addOptions() { return { who: () => null, when: () => '' } },
  addProseMirrorPlugins() {
    const { who, when } = this.options
    return [new Plugin<GutterMeta>({
      key: gutterKey,
      state: {
        init: () => ({ by: new Map(), conflicts: new Set(), flash: new Set(), counts: new Map() }),
        apply: (tr, cur) => ({ ...cur, ...(tr.getMeta(gutterKey) || {}) }),
      },
      props: {
        decorations(state) {
          const meta = gutterKey.getState(state)
          if (!meta) return null
          const decos: Decoration[] = []
          for (const l of blocksOf(state.doc)) {
            if (!l.id) continue
            const m = meta.by.get(l.id)
            const w = m ? who(m.by) : null
            const n = meta.counts.get(l.id) || 0
            const cls = ['blk', w ? 'blk-bot' : '', meta.conflicts.has(l.id) ? 'blk-conflict' : '', meta.flash.has(l.id) ? 'blk-flash' : '',
              n ? 'blk-c' : ''].filter(Boolean).join(' ')
            if (cls === 'blk') continue
            decos.push(Decoration.node(l.pos, l.pos + l.node.nodeSize, {
              class: cls, ...(w ? { style: `--by:${w.color}`, 'data-by': w.name, title: `${w.name} · ${when(m!.ts)}` } : {}),
              ...(n ? { 'data-comments': String(n) } : {}),
            }))
          }
          return DecorationSet.create(state.doc, decos)
        },
      },
    })]
  },
})
