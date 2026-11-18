// 协作：直接用 y-prosemirror（不用 Tiptap 的 @tiptap/y-tiptap 分支）。
// y-tiptap 3.3 收到远端改动后恢复光标时，只要光标所在段落的文字变了就改按绝对位置找回光标，
// 两个人在同一段同一处打字时字会乱序（e2e 里复现过），上游 y-prosemirror 按相对位置恢复，没有这个问题。
import { Extension } from '@tiptap/core'
import { Plugin } from '@tiptap/pm/state'
import type { Transaction } from '@tiptap/pm/state'
import type { DecorationAttrs } from '@tiptap/pm/view'
import { redo, undo, yCursorPlugin, ySyncPlugin, ySyncPluginKey, yUndoPlugin, yUndoPluginKey } from 'y-prosemirror'
import type { Awareness } from 'y-protocols/awareness'
import type * as Y from 'yjs'

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    oadCollaboration: {
      undo: () => ReturnType
      redo: () => ReturnType
    }
    oadCaret: {
      updateUser: (user: Record<string, unknown>) => ReturnType
    }
  }
}

/** 这个事务是不是 Yjs 同步过来的（远端改动、撤销重做） */
export const isChangeOrigin = (tr: Transaction) => !!tr.getMeta(ySyncPluginKey)

export const OadCollaboration = Extension.create<{ fragment: Y.XmlFragment | null }>({
  name: 'collaboration',
  priority: 1000,
  addOptions() {
    return { fragment: null }
  },
  addCommands() {
    return {
      undo:
        () =>
        ({ state, tr, dispatch }) => {
          tr.setMeta('preventDispatch', true)
          const um = yUndoPluginKey.getState(state)?.undoManager
          if (!um || um.undoStack.length === 0) return false
          if (!dispatch) return true
          return undo(state)
        },
      redo:
        () =>
        ({ state, tr, dispatch }) => {
          tr.setMeta('preventDispatch', true)
          const um = yUndoPluginKey.getState(state)?.undoManager
          if (!um || um.redoStack.length === 0) return false
          if (!dispatch) return true
          return redo(state)
        },
    }
  },
  addKeyboardShortcuts() {
    return {
      'Mod-z': () => this.editor.commands.undo(),
      'Mod-y': () => this.editor.commands.redo(),
      'Shift-Mod-z': () => this.editor.commands.redo(),
    }
  },
  addProseMirrorPlugins() {
    const fragment = this.options.fragment
    if (!fragment) throw new Error('OadCollaboration 需要 fragment')
    return [
      ySyncPlugin(fragment),
      yUndoPlugin(),
      // 输入规则（比如 # 变标题）单独成一步撤销
      new Plugin({
        filterTransaction: (tr, state) => {
          if (tr.getMeta('inputRule')) yUndoPluginKey.getState(state)?.undoManager.stopCapturing()
          return true
        },
      }),
    ]
  },
})

export interface CaretUser {
  name: string
  color: string
  kind?: string
  [k: string]: unknown
}

export const OadCaret = Extension.create<{
  awareness: Awareness | null
  user: CaretUser | null
  render: (u: CaretUser) => HTMLElement
  selection: (u: CaretUser) => DecorationAttrs
}>({
  name: 'oadCaret',
  addOptions() {
    return { awareness: null, user: null, render: () => document.createElement('span'), selection: () => ({}) }
  },
  addCommands() {
    return {
      updateUser:
        (user) =>
        () => {
          this.options.awareness?.setLocalStateField('user', user)
          return true
        },
    }
  },
  addProseMirrorPlugins() {
    const { awareness, user, render, selection } = this.options
    if (!awareness) return []
    if (user) awareness.setLocalStateField('user', user)
    const safe = (u: any): CaretUser => ({
      ...u,
      name: String(u?.name ?? '?'),
      color: typeof u?.color === 'string' && /^#[0-9a-f]{3,8}$/i.test(u.color) ? u.color : '#8b8b8b',
    })
    return [
      yCursorPlugin(awareness, {
        cursorBuilder: (u: any) => render(safe(u)),
        selectionBuilder: (u: any) => selection(safe(u)),
      }),
    ]
  },
})
