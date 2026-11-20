// 放进 Shadow DOM（@open-agent-docs/elements）时：大图预览挂到给的 root 里，代码块的语言候选和输入框在同一个树里。
import { Editor } from '@tiptap/core'
import { afterEach, describe, expect, it } from 'vitest'
import * as Y from 'yjs'
import { OadCollaboration } from '../src/extensions/collaboration'
import { portalRoot, PortalRootExt } from '../src/extensions/portal'
import { schemaExtensions } from '../src/schema'
import { openLightbox } from '../src/views/image'

const editors: Editor[] = []
const hosts: HTMLElement[] = []
afterEach(() => {
  editors.splice(0).forEach((e) => e.destroy())
  hosts.splice(0).forEach((h) => h.remove())
  document.body.querySelectorAll('.oad-lightbox').forEach((m) => m.remove())
})

/** 页面上一个宿主元素，shadow root 里放编辑器和一个浮层容器（和 elements 一样） */
function shadowHost() {
  const host = document.createElement('div')
  document.body.append(host)
  hosts.push(host)
  const shadow = host.attachShadow({ mode: 'open' })
  const mount = document.createElement('div')
  const portal = document.createElement('div')
  shadow.append(mount, portal)
  return { shadow, mount, portal }
}

function make(element: HTMLElement, portal?: HTMLElement) {
  const ydoc = new Y.Doc()
  const editor = new Editor({
    element,
    extensions: [
      ...schemaExtensions(),
      OadCollaboration.configure({ fragment: ydoc.getXmlFragment('default') }),
      ...(portal ? [PortalRootExt.configure({ get: () => portal })] : []),
    ],
  })
  editors.push(editor)
  return editor
}

const esc = () => document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms))

describe('大图预览', () => {
  it('默认挂在 document.body 上，Esc 关掉', async () => {
    openLightbox('/a.png', 'a')
    const mask = document.body.querySelector(':scope > .oad-lightbox')
    expect(mask).not.toBeNull()
    esc()
    await wait(200)
    expect(mask!.isConnected).toBe(false)
  })

  it('给了 root 就挂在 root 里（shadow 里的元素），Esc 照样能关', async () => {
    const { portal, shadow } = shadowHost()
    openLightbox('/a.png', 'a', portal)
    expect(document.body.querySelector('.oad-lightbox')).toBeNull()
    const mask = portal.querySelector(':scope > .oad-lightbox')
    expect(mask).not.toBeNull()
    expect(shadow.querySelector('.oad-lightbox img')).not.toBeNull()
    esc()
    await wait(200)
    expect(mask!.isConnected).toBe(false)
  })
})

describe('portalRoot', () => {
  it('扩展选项给了就用它', () => {
    const { mount, portal } = shadowHost()
    expect(portalRoot(make(mount, portal))).toBe(portal)
  })

  it('没给：编辑器在 shadow root 里就用那个 shadow root，否则 document.body', () => {
    const { mount, shadow } = shadowHost()
    expect(portalRoot(make(mount))).toBe(shadow)
    const plain = document.createElement('div')
    document.body.append(plain)
    hosts.push(plain)
    expect(portalRoot(make(plain))).toBe(document.body)
  })
})

describe('代码块的语言候选', () => {
  it('<datalist> 和语言框在同一个 shadow root 里，两个代码块不撞 id', () => {
    const { mount, shadow } = shadowHost()
    const editor = make(mount)
    editor.commands.setContent({
      type: 'doc',
      content: [
        { type: 'codeBlock', attrs: { language: 'rust' }, content: [{ type: 'text', text: 'fn main() {}' }] },
        { type: 'codeBlock', content: [{ type: 'text', text: 'x' }] },
      ],
    })
    const inputs = [...shadow.querySelectorAll<HTMLInputElement>('input.oad-code-lang')]
    expect(inputs.length).toBe(2)
    const ids = inputs.map((i) => i.getAttribute('list')!)
    expect(new Set(ids).size).toBe(2)
    for (const id of ids) {
      const list = shadow.getElementById(id)
      expect(list?.tagName).toBe('DATALIST')
      expect(list!.querySelector('option[value="python"]')).not.toBeNull()
    }
    expect(document.body.querySelector('datalist')).toBeNull()
  })
})
