// 鼠标停在一块上时，左边显示这块是谁写的：数这块里现存文字各属于哪个 Yjs client，取最多的那个，
// 再用宿主给的 client → 人 / Agent 对照表（服务器的 /pages/{id}/authors）换成名字。
import { Extension } from '@tiptap/core'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import type { EditorView } from '@tiptap/pm/view'
import { ySyncPluginKey } from 'y-prosemirror'
import * as Y from 'yjs'

export interface AuthorInfo {
  kind: 'user' | 'agent' | string
  id: string
  name: string
}

export interface BlockAuthorOptions {
  // client id（十进制字符串）→ 作者
  authors: () => Record<string, AuthorInfo>
  // 碰到对照表里没有的 client id（新来的人），宿主可以去重新拉一次
  onUnknown: (clientId: string) => void
  label: (a: AuthorInfo) => string
}

/** 一块里现存文字最多的那个 client；没有文字的块（图片、分割线）算建它的那个 */
export function mainClient(el: Y.XmlElement | Y.XmlText): number | null {
  const count = new Map<number, number>()
  const walk = (t: Y.AbstractType<any>) => {
    let item = t._start
    while (item) {
      if (!item.deleted) {
        const c = item.content
        if (c instanceof Y.ContentType) walk(c.type)
        else if (item.countable) count.set(item.id.client, (count.get(item.id.client) ?? 0) + item.length)
      }
      item = item.right
    }
  }
  walk(el)
  let best: number | null = null
  let n = 0
  count.forEach((v, k) => {
    if (v > n) {
      n = v
      best = k
    }
  })
  return best ?? el._item?.id.client ?? null
}

export const blockAuthorKey = new PluginKey('oadBlockAuthor')

export const BlockAuthor = Extension.create<BlockAuthorOptions>({
  name: 'oadBlockAuthor',
  addOptions() {
    return { authors: () => ({}), onUnknown: () => {}, label: (a) => a.name }
  },
  addProseMirrorPlugins() {
    const opts = this.options
    return [
      new Plugin({
        key: blockAuthorKey,
        view(view: EditorView) {
          const tag = document.createElement('div')
          tag.className = 'oad-block-author'
          tag.hidden = true
          const host = view.dom.parentElement
          host?.appendChild(tag)
          let current = -1
          let timer = 0
          let raf = 0

          const hide = () => {
            current = -1
            clearTimeout(timer)
            tag.hidden = true
          }
          const show = (index: number, dom: HTMLElement, retry = true) => {
            const s = ySyncPluginKey.getState(view.state)
            const frag = s?.type as Y.XmlFragment | undefined
            const el = frag?.get(index) as Y.XmlElement | Y.XmlText | undefined
            if (!el || !host) return
            const client = mainClient(el)
            if (client === null) return
            const a = opts.authors()[String(client)]
            if (!a) {
              // 宿主去拉一次对照表，过一会儿再试
              opts.onUnknown(String(client))
              if (retry) timer = window.setTimeout(() => current === index && show(index, dom, false), 700)
              return
            }
            tag.textContent = opts.label(a)
            tag.dataset.kind = a.kind
            const hr = host.getBoundingClientRect()
            const r = dom.getBoundingClientRect()
            tag.style.top = `${r.top - hr.top}px`
            tag.hidden = false
          }
          const onMove = (e: MouseEvent) => {
            cancelAnimationFrame(raf)
            raf = requestAnimationFrame(() => {
              // 找到鼠标下面的顶层块
              let el = e.target as HTMLElement | null
              while (el && el.parentElement !== view.dom) el = el.parentElement
              if (!el) return hide()
              let index = -1
              try {
                index = view.state.doc.resolve(view.posAtDOM(el, 0)).index(0)
              } catch {
                return hide()
              }
              if (index === current) return
              hide()
              current = index
              timer = window.setTimeout(() => show(index, el!), 350)
            })
          }
          view.dom.addEventListener('mousemove', onMove)
          view.dom.addEventListener('mouseleave', hide)
          view.dom.addEventListener('keydown', hide)
          return {
            update: (_v, prev) => {
              if (prev.doc !== view.state.doc) hide()
            },
            destroy() {
              view.dom.removeEventListener('mousemove', onMove)
              view.dom.removeEventListener('mouseleave', hide)
              view.dom.removeEventListener('keydown', hide)
              clearTimeout(timer)
              cancelAnimationFrame(raf)
              tag.remove()
            },
          }
        },
      }),
    ]
  },
})
