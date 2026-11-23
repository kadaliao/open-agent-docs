// 评论锚点：评论挂在一段文字上，两头存成 Yjs RelativePosition（base64），文字改了也跟着走。
// 正文里有评论的文字画底色（decoration，不改文档）；点底色告诉宿主打开哪一串。
import { Extension, type Editor } from '@tiptap/core'
import type { EditorState, Transaction } from '@tiptap/pm/state'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view'
import { absolutePositionToRelativePosition, relativePositionToAbsolutePosition, ySyncPluginKey } from 'y-prosemirror'
import * as Y from 'yjs'
import { isChangeOrigin } from './collaboration'

export interface CommentAnchor {
  block_id: string | null
  start: string
  end: string
  quote: string
}

export interface CommentMark {
  id: string
  anchor: { block_id?: string | null; start?: string | null; end?: string | null } | null
  resolved?: boolean
}

export interface CommentsOptions {
  onOpen: (threadIds: string[]) => void
}

interface PluginState {
  threads: CommentMark[]
  active: string | null
  deco: DecorationSet
  // 每串评论现在的位置（找不到的是 null：原文被删了）
  ranges: Map<string, { from: number; to: number } | null>
}

export const commentsKey = new PluginKey<PluginState>('oadComments')

const toB64 = (u8: Uint8Array) => {
  let s = ''
  for (let i = 0; i < u8.length; i++) s += String.fromCharCode(u8[i])
  return btoa(s)
}
const fromB64 = (s: string) => {
  const bin = atob(s)
  const u8 = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i)
  return u8
}

function binding(state: EditorState) {
  const s = ySyncPluginKey.getState(state)
  return s?.binding ? { doc: s.doc as Y.Doc, type: s.type as Y.XmlFragment, mapping: s.binding.mapping } : null
}

/** 当前选区 → 评论锚点。block_id 是选区开头所在的顶层块（Agent 看到的 ⟦块编号⟧ 就是顶层块）。 */
export function selectionToAnchor(editor: Editor): CommentAnchor | null {
  const { state } = editor
  const { from, to, empty } = state.selection
  const b = binding(state)
  if (empty || !b) return null
  const start = absolutePositionToRelativePosition(from, b.type, b.mapping)
  let end = absolutePositionToRelativePosition(to, b.type, b.mapping)
  // 结尾挂在选区最后一个字上（往左靠），在结尾后面接着打字不会算进评论
  const abs = Y.createAbsolutePositionFromRelativePosition(end, b.doc)
  if (abs && abs.type instanceof Y.XmlText && abs.index > 0) end = Y.createRelativePositionFromTypeIndex(abs.type, abs.index, -1)
  const $from = state.doc.resolve(from)
  const top = $from.depth > 0 ? $from.node(1) : null
  return {
    block_id: (top?.attrs.id as string) ?? null,
    start: toB64(Y.encodeRelativePosition(start)),
    end: toB64(Y.encodeRelativePosition(end)),
    quote: state.doc.textBetween(from, to, '\n', ' '),
  }
}

function resolve(state: EditorState, a: CommentMark['anchor']): { from: number; to: number } | null {
  if (!a?.start || !a?.end) return null
  const b = binding(state)
  if (!b) return null
  try {
    const from = relativePositionToAbsolutePosition(b.doc, b.type, Y.decodeRelativePosition(fromB64(a.start)), b.mapping)
    const to = relativePositionToAbsolutePosition(b.doc, b.type, Y.decodeRelativePosition(fromB64(a.end)), b.mapping)
    if (from === null || to === null || to <= from || to > state.doc.content.size) return null
    return { from, to }
  } catch {
    return null
  }
}

function build(state: EditorState, threads: CommentMark[], active: string | null) {
  const ranges = new Map<string, { from: number; to: number } | null>()
  const decos: Decoration[] = []
  for (const t of threads) {
    const r = resolve(state, t.anchor)
    ranges.set(t.id, r)
    if (!r || t.resolved) continue
    decos.push(
      Decoration.inline(r.from, r.to, { class: 'oad-comment-mark' + (t.id === active ? ' oad-is-active' : ''), 'data-thread': t.id }, { thread: t.id }),
    )
  }
  return { ranges, deco: DecorationSet.create(state.doc, decos) }
}

type Meta = { threads?: CommentMark[]; active?: string | null }

export const Comments = Extension.create<CommentsOptions>({
  name: 'oadComments',
  addOptions() {
    return { onOpen: () => {} }
  },
  addProseMirrorPlugins() {
    const opts = this.options
    return [
      new Plugin<PluginState>({
        key: commentsKey,
        state: {
          init: () => ({ threads: [], active: null, deco: DecorationSet.empty, ranges: new Map() }),
          apply(tr: Transaction, prev, _old, state) {
            const meta = tr.getMeta(commentsKey) as Meta | undefined
            if (meta || (tr.docChanged && isChangeOrigin(tr) && prev.threads.length)) {
              const threads = meta?.threads ?? prev.threads
              const active = meta && 'active' in meta ? (meta.active ?? null) : prev.active
              return { threads, active, ...build(state, threads, active) }
            }
            // 本地改动：Yjs 那边还没写进去，先按这次改动挪位置，下次远端改动或刷新时再按锚点重算
            if (tr.docChanged) {
              const ranges = new Map<string, { from: number; to: number } | null>()
              prev.ranges.forEach((r, id) => {
                if (!r) return ranges.set(id, null)
                const from = tr.mapping.map(r.from, 1)
                const to = tr.mapping.map(r.to, -1)
                ranges.set(id, to > from ? { from, to } : null)
              })
              return { ...prev, ranges, deco: prev.deco.map(tr.mapping, tr.doc) }
            }
            return prev
          },
        },
        props: {
          decorations: (state) => commentsKey.getState(state)?.deco,
          handleClick(view: EditorView, pos: number) {
            const st = commentsKey.getState(view.state)
            if (!st?.threads.length) return false
            const hits = st.deco.find(pos, pos).map((d) => (d.spec as { thread: string }).thread)
            if (hits.length) opts.onOpen(hits)
            return false
          },
        },
      }),
    ]
  },
})

/** 宿主给评论列表（和当前打开的那串） */
export function setCommentThreads(editor: Editor, threads: CommentMark[], active?: string | null) {
  const meta: Meta = { threads }
  if (active !== undefined) meta.active = active
  editor.view.dispatch(editor.state.tr.setMeta(commentsKey, meta))
}

export function setActiveThread(editor: Editor, active: string | null) {
  editor.view.dispatch(editor.state.tr.setMeta(commentsKey, { active }))
}

/** 一串评论现在在正文里的位置；找不到（原文被删了）返回 null */
export function threadRange(editor: Editor, id: string) {
  const st = commentsKey.getState(editor.state)
  const r = st?.ranges.get(id)
  if (r !== undefined) return r
  const t = st?.threads.find((x) => x.id === id)
  return t ? resolve(editor.state, t.anchor) : null
}

/** 滚到那串评论挂的文字 */
export function scrollToThread(editor: Editor, id: string) {
  const r = threadRange(editor, id)
  if (!r) return false
  const dom = editor.view.domAtPos(r.from).node as HTMLElement
  const el = dom.nodeType === 1 ? dom : dom.parentElement
  el?.scrollIntoView({ block: 'center', behavior: 'smooth' })
  return true
}
