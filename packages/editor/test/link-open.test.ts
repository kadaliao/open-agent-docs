// 打开正文里的链接：点一下就打开（编辑时也是），⌥ / ⇧ 点、拖选不打开；宿主能接走（本工作区的页面）。
import { Editor } from '@tiptap/core'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { LinkOpen, openHref } from '../src/extensions/link-open'
import { schemaExtensions } from '../src/schema'

const editors: Editor[] = []
afterEach(() => {
  editors.splice(0).forEach((e) => e.destroy())
  vi.restoreAllMocks()
})

function make(editable: boolean, host: (href: string) => boolean) {
  const el = document.createElement('div')
  document.body.append(el)
  const editor = new Editor({
    element: el,
    editable,
    extensions: [...schemaExtensions(), LinkOpen.configure({ open: host })],
    content: '<p>看 <a href="/p/abc123">这一页</a> 和 <a href="https://example.com/x">外面</a></p>',
  })
  editors.push(editor)
  return editor
}

const click = (editor: Editor, text: string, o: MouseEventInit = {}) => {
  const a = [...editor.view.dom.querySelectorAll('a')].find((x) => x.textContent === text)!
  const ev = new MouseEvent('click', { bubbles: true, cancelable: true, button: 0, ...o })
  a.dispatchEvent(ev)
  return ev
}

describe('打开链接', () => {
  it('编辑时点一下就打开：本工作区的页面交给宿主', () => {
    const host = vi.fn(() => true)
    const open = vi.spyOn(window, 'open').mockImplementation(() => null)
    const e = make(true, host)
    const ev = click(e, '这一页')
    expect(host).toHaveBeenCalledWith('/p/abc123')
    expect(ev.defaultPrevented).toBe(true) // 浏览器自己别再跟过去
    expect(open).not.toHaveBeenCalled()
  })

  it('⌥ / ⇧ 点、选着一段字时不打开（要改链接里的字）', () => {
    const host = vi.fn(() => true)
    const e = make(true, host)
    click(e, '这一页', { altKey: true })
    click(e, '这一页', { shiftKey: true })
    e.commands.setTextSelection({ from: 3, to: 6 })
    click(e, '这一页')
    expect(host).not.toHaveBeenCalled()
  })

  it('宿主不接：在新标签页打开', () => {
    const open = vi.spyOn(window, 'open').mockImplementation(() => null)
    const e = make(true, () => false)
    click(e, '外面')
    expect(open).toHaveBeenCalledWith('https://example.com/x', '_blank', 'noopener,noreferrer')
  })

  it('只读时直接点开', () => {
    const host = vi.fn(() => true)
    const e = make(false, host)
    click(e, '这一页')
    expect(host).toHaveBeenCalledWith('/p/abc123')
  })

  it('不开 javascript: 这类地址', () => {
    const open = vi.spyOn(window, 'open').mockImplementation(() => null)
    openHref('javascript:alert(1)', null)
    expect(open).not.toHaveBeenCalled()
  })
})
