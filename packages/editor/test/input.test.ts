// 行内格式的 Markdown 输入（中文挨着符号也认）、标题的最小改动。
import { Editor } from '@tiptap/core'
import { afterEach, describe, expect, it } from 'vitest'
import * as Y from 'yjs'
import { applyTitle, minimalDiff } from '../src/OadTitle'
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

// 像键盘一样一个字一个字打：先给输入规则机会处理
function type(editor: Editor, text: string) {
  for (const ch of text) {
    const { view } = editor
    const { from, to } = view.state.selection
    const handled = view.someProp('handleTextInput', (f) => f(view, from, to, ch, () => view.state.tr.insertText(ch, from, to)))
    if (!handled) view.dispatch(view.state.tr.insertText(ch, from, to))
  }
}

const marksOf = (editor: Editor) =>
  editor.getJSON().content![0].content!.map((n: any) => [n.text, (n.marks ?? []).map((m: any) => m.type).join('+')])

describe('行内格式输入', () => {
  it.each([
    ['这是，**重点**结束', '重点', 'bold'],
    ['中文*斜体*后面', '斜体', 'italic'],
    ['看~~删掉~~了', '删掉', 'strike'],
    ['调用`sync()`函数', 'sync()', 'code'],
    ['标记==高亮==一下', '高亮', 'highlight'],
    ['english **bold** word', 'bold', 'bold'],
  ])('%s', (input, word, mark) => {
    const editor = make()
    editor.commands.setContent({ type: 'doc', content: [{ type: 'paragraph' }] })
    editor.commands.setTextSelection(1)
    type(editor, input)
    const runs = marksOf(editor)
    expect(runs).toContainEqual([word, mark])
    expect(editor.state.doc.textContent).toBe(input.replace(/\*\*|~~|==|[*`]/g, ''))
  })

  it('单词中间的星号不算（a*b*c）', () => {
    const editor = make()
    editor.commands.setContent({ type: 'doc', content: [{ type: 'paragraph' }] })
    editor.commands.setTextSelection(1)
    type(editor, 'a*b*c')
    expect(editor.state.doc.textContent).toBe('a*b*c')
  })

  it('打 **粗体** 时不会先被当成斜体', () => {
    const editor = make()
    editor.commands.setContent({ type: 'doc', content: [{ type: 'paragraph' }] })
    editor.commands.setTextSelection(1)
    type(editor, '**粗**')
    expect(marksOf(editor)).toEqual([['粗', 'bold']])
  })
})

describe('标题', () => {
  it('minimalDiff 只换中间变了的一截', () => {
    expect(minimalDiff('会议纪要', '周会纪要')).toEqual([0, 2, '周会'])
    expect(minimalDiff('abc', 'abXc')).toEqual([2, 0, 'X'])
    expect(minimalDiff('abc', 'ac')).toEqual([1, 1, ''])
    expect(minimalDiff('', 'new')).toEqual([0, 0, 'new'])
    // 不把表情（代理对）拆开
    const [at, del, ins] = minimalDiff('a😀b', 'a😃b')
    expect('a😀b'.slice(0, at) + ins + 'a😀b'.slice(at + del)).toBe('a😃b')
    expect(at).toBe(1)
  })

  it('两边同时改标题，改动都留下', () => {
    const a = new Y.Doc()
    const b = new Y.Doc()
    applyTitle(a.getText('title'), '周会')
    Y.applyUpdate(b, Y.encodeStateAsUpdate(a))
    applyTitle(a.getText('title'), '周会纪要')
    applyTitle(b.getText('title'), '本周周会')
    Y.applyUpdate(b, Y.encodeStateAsUpdate(a))
    Y.applyUpdate(a, Y.encodeStateAsUpdate(b))
    expect(a.getText('title').toString()).toBe(b.getText('title').toString())
    expect(a.getText('title').toString()).toBe('本周周会纪要')
  })
})
