// 撤销：编辑器重建插件视图之后（Tiptap 的浮动菜单 registerPlugin 就会）⌘Z 照样能撤销、重做。
import { Editor } from '@tiptap/core'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import { afterEach, describe, expect, it } from 'vitest'
import * as Y from 'yjs'
import { OadCollaboration } from '../src/extensions/collaboration'
import { schemaExtensions } from '../src/schema'

const editors: Editor[] = []
afterEach(() => editors.splice(0).forEach((e) => e.destroy()))

function make() {
  const ydoc = new Y.Doc()
  const editor = new Editor({
    element: document.createElement('div'),
    extensions: [...schemaExtensions(), OadCollaboration.configure({ fragment: ydoc.getXmlFragment('default') })],
  })
  editors.push(editor)
  return editor
}

function type(editor: Editor, text: string) {
  const { from, to } = editor.state.selection
  editor.view.dispatch(editor.state.tr.insertText(text, from, to))
}

const text = (editor: Editor) => editor.state.doc.textContent

describe('撤销', () => {
  it('打字能撤销、重做', () => {
    const e = make()
    type(e, '你好')
    expect(text(e)).toBe('你好')
    expect(e.commands.undo()).toBe(true)
    expect(text(e)).toBe('')
    expect(e.commands.redo()).toBe(true)
    expect(text(e)).toBe('你好')
  })

  it('重建插件视图（registerPlugin）之后还能撤销', () => {
    const e = make()
    // 浮动菜单挂上、摘下各一次：每次都重建全部插件视图
    const key = new PluginKey('test-menu')
    e.registerPlugin(new Plugin({ key }))
    e.unregisterPlugin(key)
    type(e, '后来打的')
    expect(text(e)).toBe('后来打的')
    expect(e.commands.undo()).toBe(true)
    expect(text(e)).toBe('')
  })

  it('编辑器关掉时撤销管理器不再听文档的改动', () => {
    const ydoc = new Y.Doc()
    const e = new Editor({
      element: document.createElement('div'),
      extensions: [...schemaExtensions(), OadCollaboration.configure({ fragment: ydoc.getXmlFragment('default') })],
    })
    const before = (ydoc as unknown as { _observers: Map<string, Set<unknown>> })._observers.get('afterTransaction')?.size ?? 0
    expect(before).toBeGreaterThan(0)
    e.destroy()
    const after = (ydoc as unknown as { _observers: Map<string, Set<unknown>> })._observers.get('afterTransaction')?.size ?? 0
    expect(after).toBeLessThan(before)
  })
})
