// 代码块语法高亮：Shiki 算出每段的颜色，画成 decoration（不改文档）。没用到代码块就不下载 Shiki。
// 本地改动只重算碰到的代码块；同一段代码的结果缓存起来。
import { Extension, combineTransactionSteps, getChangedRanges } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { Plugin, PluginKey, type EditorState } from '@tiptap/pm/state'
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view'

type Shiki = typeof import('../media/shiki')
let shiki: Shiki | null = null
let shikiLoading: Promise<Shiki> | null = null
const loadShiki = () => (shikiLoading ??= import('../media/shiki').then((m) => (shiki = m)))

const key = new PluginKey<DecorationSet>('oadCodeHighlight')
const cache = new Map<string, { from: number; to: number; style: string }[]>()
const pending = new Set<string>()

function blocks(doc: PMNode, from = 0, to = doc.content.size) {
  const out: { node: PMNode; pos: number }[] = []
  doc.nodesBetween(from, to, (node, pos) => {
    if (node.type.name === 'codeBlock') {
      out.push({ node, pos })
      return false
    }
    return !node.isTextblock
  })
  return out
}

function decorate(state: EditorState, view: EditorView | null, list: { node: PMNode; pos: number }[], set: DecorationSet) {
  let decos = set
  for (const { node, pos } of list) {
    // 先去掉这一块旧的
    decos = decos.remove(decos.find(pos, pos + node.nodeSize))
    const lang0 = node.attrs.language as string | null
    if (!lang0 || lang0 === 'mermaid') continue
    if (!shiki) {
      loadShiki().then(() => view && !view.isDestroyed && view.dispatch(view.state.tr.setMeta(key, 'refresh')))
      continue
    }
    const lang = shiki.normalizeLang(lang0)
    if (!lang) continue
    const code = node.textContent
    if (!code) continue
    const k = lang + '\u0000' + code
    const toks = cache.get(k)
    if (!toks) {
      if (!pending.has(k)) {
        pending.add(k)
        shiki
          .tokenize(code, lang)
          .then((t) => {
            cache.set(k, t)
            if (cache.size > 400) cache.delete(cache.keys().next().value!)
          })
          .catch(() => cache.set(k, []))
          .finally(() => {
            pending.delete(k)
            if (view && !view.isDestroyed) view.dispatch(view.state.tr.setMeta(key, 'refresh'))
          })
      }
      continue
    }
    const start = pos + 1
    decos = decos.add(
      state.doc,
      toks.map((t) => Decoration.inline(start + t.from, start + t.to, { style: t.style })),
    )
  }
  return decos
}

export const CodeHighlight = Extension.create({
  name: 'oadCodeHighlight',
  addProseMirrorPlugins() {
    let view: EditorView | null = null
    return [
      new Plugin<DecorationSet>({
        key,
        state: {
          init: () => DecorationSet.empty,
          apply(tr, set, old, state) {
            if (tr.getMeta(key) === 'refresh') return decorate(state, view, blocks(state.doc), DecorationSet.empty)
            if (!tr.docChanged) return set
            let mapped = set.map(tr.mapping, tr.doc)
            // 只重算这次改动碰到的代码块
            const ranges = getChangedRanges(combineTransactionSteps(old.doc, [tr]))
            const seen = new Set<number>()
            const touched: { node: PMNode; pos: number }[] = []
            for (const { newRange } of ranges) {
              const from = Math.max(0, newRange.from - 1)
              const to = Math.min(state.doc.content.size, newRange.to + 1)
              for (const b of blocks(state.doc, from, to)) if (!seen.has(b.pos)) seen.add(b.pos) && touched.push(b)
            }
            return touched.length ? decorate(state, view, touched, mapped) : mapped
          },
        },
        props: { decorations: (s) => key.getState(s) },
        view(v) {
          view = v
          // 打开时有代码块才开始加载
          if (blocks(v.state.doc).some((b) => b.node.attrs.language && b.node.attrs.language !== 'mermaid'))
            loadShiki().then(() => !v.isDestroyed && v.dispatch(v.state.tr.setMeta(key, 'refresh')))
          return {
            destroy() {
              view = null
            },
          }
        },
      }),
    ]
  },
})
