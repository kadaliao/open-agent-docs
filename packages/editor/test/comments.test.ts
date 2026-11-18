// 评论锚点跟着文字走（别人在前面、中间改字）；块作者按现存文字最多的 client 算。
import { Editor } from '@tiptap/core'
import { afterEach, describe, expect, it } from 'vitest'
import * as Y from 'yjs'
import { mainClient } from '../src/extensions/block-author'
import { OadCollaboration } from '../src/extensions/collaboration'
import { Comments, selectionToAnchor, setCommentThreads, threadRange } from '../src/extensions/comments'
import { schemaExtensions } from '../src/schema'

const editors: Editor[] = []
afterEach(() => editors.splice(0).forEach((e) => e.destroy()))

function pair() {
  const a = new Y.Doc()
  const b = new Y.Doc()
  a.on('update', (u: Uint8Array, o: unknown) => o !== 'net' && Y.applyUpdate(b, u, 'net'))
  b.on('update', (u: Uint8Array, o: unknown) => o !== 'net' && Y.applyUpdate(a, u, 'net'))
  const mk = (d: Y.Doc) => {
    const e = new Editor({
      element: document.createElement('div'),
      extensions: [...schemaExtensions(), OadCollaboration.configure({ fragment: d.getXmlFragment('default') }), Comments],
    })
    editors.push(e)
    return e
  }
  return { a, b, ea: mk(a), eb: mk(b) }
}

const text = (e: Editor, r: { from: number; to: number } | null) => (r ? e.state.doc.textBetween(r.from, r.to) : null)

describe('评论锚点', () => {
  it('别人在前面、中间、后面改字，锚点跟着走', () => {
    const { ea, eb } = pair()
    ea.commands.setContent({
      type: 'doc',
      content: [{ type: 'paragraph', attrs: { id: 'P0000001' }, content: [{ type: 'text', text: '前文 要评论的这几个字 后文' }] }],
    })
    // 选中“要评论的这几个字”
    const from = 1 + '前文 '.length
    const to = from + '要评论的这几个字'.length
    ea.commands.setTextSelection({ from, to })
    const anchor = selectionToAnchor(ea)!
    expect(anchor.quote).toBe('要评论的这几个字')
    expect(anchor.block_id).toBe('P0000001')
    setCommentThreads(ea, [{ id: 't1', anchor }])
    setCommentThreads(eb, [{ id: 't1', anchor }])
    expect(text(eb, threadRange(eb, 't1'))).toBe('要评论的这几个字')

    // B 在前面插字、在中间插字、在后面插字
    eb.view.dispatch(eb.state.tr.insertText('新加的', 1))
    eb.view.dispatch(eb.state.tr.insertText('（插）', 1 + '新加的前文 要评论的'.length))
    eb.view.dispatch(eb.state.tr.insertText('！', eb.state.doc.content.size - 1))
    expect(text(ea, threadRange(ea, 't1'))).toBe('要评论的（插）这几个字')
    // A 在结尾后面接着打字，不算进评论
    const r = threadRange(ea, 't1')!
    ea.view.dispatch(ea.state.tr.insertText('外', r.to))
    setCommentThreads(eb, [{ id: 't1', anchor }])
    expect(text(eb, threadRange(eb, 't1'))).toBe('要评论的（插）这几个字')
    // 画了底色
    expect(eb.view.dom.querySelector('.oad-comment')?.textContent).toBe('要评论的（插）这几个字')

    // 整段删掉：找不到了
    eb.view.dispatch(eb.state.tr.delete(r.from, r.to + 1))
    expect(threadRange(ea, 't1')).toBeNull()
  })
})

describe('块作者', () => {
  it('取现存文字最多的 client', () => {
    const a = new Y.Doc()
    a.clientID = 11
    const b = new Y.Doc()
    b.clientID = 22
    const p = new Y.XmlElement('paragraph')
    const t = new Y.XmlText()
    p.insert(0, [t])
    a.getXmlFragment('default').insert(0, [p])
    t.insert(0, 'abc')
    Y.applyUpdate(b, Y.encodeStateAsUpdate(a))
    const tb = (b.getXmlFragment('default').get(0) as Y.XmlElement).get(0) as Y.XmlText
    tb.insert(3, '一二三四五')
    Y.applyUpdate(a, Y.encodeStateAsUpdate(b))
    expect(mainClient(a.getXmlFragment('default').get(0) as Y.XmlElement)).toBe(22)
    tb.delete(3, 4)
    Y.applyUpdate(a, Y.encodeStateAsUpdate(b))
    expect(mainClient(a.getXmlFragment('default').get(0) as Y.XmlElement)).toBe(11)
  })
})
