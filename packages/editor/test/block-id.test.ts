import { Editor } from '@tiptap/core'
import { OadCollaboration } from '../src/extensions/collaboration'
import { TextSelection } from '@tiptap/pm/state'
import { afterEach, describe, expect, it } from 'vitest'
import * as Y from 'yjs'
import { isBlockId, schemaExtensions } from '../src/schema'

const editors: Editor[] = []
afterEach(() => editors.splice(0).forEach((e) => e.destroy()))

function make(ydoc = new Y.Doc()) {
  const editor = new Editor({
    element: document.createElement('div'),
    extensions: [...schemaExtensions(), OadCollaboration.configure({ fragment: ydoc.getXmlFragment('default') })],
  })
  editors.push(editor)
  return { editor, ydoc }
}

const ids = (e: Editor) => {
  const out: string[] = []
  e.state.doc.descendants((n) => {
    if (n.isBlock) out.push(n.attrs.id)
  })
  return out
}

const para = (id: string | null, t: string) => ({ type: 'paragraph', attrs: { id }, content: [{ type: 'text', text: t }] })

describe('块编号', () => {
  it('拆段：前一半留原编号，后一半换新编号', () => {
    const { editor } = make()
    editor.commands.setContent({ type: 'doc', content: [para('AAAAAAAA', 'hello world')] })
    editor.commands.setTextSelection(6)
    editor.commands.splitBlock()
    const [a, b] = ids(editor)
    expect(a).toBe('AAAAAAAA')
    expect(isBlockId(b)).toBe(true)
    expect(b).not.toBe(a)
  })

  it('在段首回车：两段编号不同，其中一个是原编号', () => {
    const { editor } = make()
    editor.commands.setContent({ type: 'doc', content: [para('AAAAAAAA', 'hello')] })
    editor.commands.setTextSelection(1)
    editor.commands.splitBlock()
    const got = ids(editor)
    expect(got).toHaveLength(2)
    expect(got).toContain('AAAAAAAA')
    expect(new Set(got).size).toBe(2)
    got.forEach((i) => expect(isBlockId(i)).toBe(true))
  })

  it('粘贴出重复编号：原来那段不动，新的换号；没编号的块补上', () => {
    const { editor } = make()
    editor.commands.setContent({ type: 'doc', content: [para('AAAAAAAA', 'one'), para('BBBBBBBB', 'two')] })
    // 插到最前面：重复的在原件之前，也是新的换号
    editor.commands.insertContentAt(0, [para('BBBBBBBB', 'copy'), para(null, 'new')])
    const got = ids(editor)
    expect(got).toHaveLength(4)
    expect(got[2]).toBe('AAAAAAAA')
    expect(got[3]).toBe('BBBBBBBB')
    expect(new Set(got).size).toBe(4)
    got.forEach((i) => expect(isBlockId(i)).toBe(true))
  })

  it('打字不改编号', () => {
    const { editor } = make()
    editor.commands.setContent({ type: 'doc', content: [para('AAAAAAAA', 'abc')] })
    editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, 4)).insertText('d'))
    expect(ids(editor)).toEqual(['AAAAAAAA'])
    expect(editor.state.doc.textContent).toBe('abcd')
  })

  it('换块类型（段落变标题）沿用编号', () => {
    const { editor } = make()
    editor.commands.setContent({ type: 'doc', content: [para('AAAAAAAA', 'abc')] })
    editor.commands.setTextSelection(2)
    editor.commands.setHeading({ level: 2 })
    expect(editor.state.doc.firstChild!.type.name).toBe('heading')
    expect(ids(editor)).toEqual(['AAAAAAAA'])
  })

  it('远端来的没编号的块不动', () => {
    const remote = new Y.Doc()
    const { editor, ydoc } = make()
    ydoc.on('update', (u: Uint8Array, origin: unknown) => origin !== 'remote' && Y.applyUpdate(remote, u, 'remote'))
    remote.on('update', (u: Uint8Array, origin: unknown) => origin !== 'remote' && Y.applyUpdate(ydoc, u, 'remote'))
    const frag = remote.getXmlFragment('default')
    const p = new Y.XmlElement('paragraph')
    p.insert(0, [new Y.XmlText('from server')])
    frag.insert(0, [p])
    expect(editor.state.doc.textContent).toBe('from server')
    expect(ids(editor)).toEqual([null])
    expect((ydoc.getXmlFragment('default').get(0) as Y.XmlElement).getAttribute('id')).toBeUndefined()
    // 本地在这段里打字，才补编号
    editor.view.dispatch(editor.state.tr.insertText('!', editor.state.doc.content.size - 1))
    const [id] = ids(editor)
    expect(isBlockId(id)).toBe(true)
    expect((remote.getXmlFragment('default').get(0) as Y.XmlElement).getAttribute('id')).toBe(id)
  })
})
