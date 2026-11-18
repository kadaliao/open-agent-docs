// 链接卡片块（embed{url, view}）的节点视图。先画骨架（不用等任何东西），卡片代码和解析结果按需来：
// 离视口近了才画卡片；离开视口时暂停正在放的视频。
import type { NodeViewRenderer } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { media } from '../extensions/media'
import type { Unfurl } from '../media/types'

type Cards = typeof import('../media/cards')
let cards: Cards | null = null
let cardsLoading: Promise<Cards> | null = null
export const loadCards = () => (cardsLoading ??= import('../media/cards').then((m) => (cards = m)))

// 同一个滚动容器里的卡片共用一个 IntersectionObserver。root 要设成滚动容器：
// 用视口当 root 的话，外层滚动容器会把交叉区域裁掉，rootMargin 的“提前”就不起作用了
type Watch = { near: () => void; away: () => void }
const watched = new Map<Element, Watch>()
const observers = new Map<Element | null, IntersectionObserver>()

function scrollParent(el: Element | null): Element | null {
  for (let e = el?.parentElement; e; e = e.parentElement) {
    const o = getComputedStyle(e).overflowY
    if (o === 'auto' || o === 'scroll') return e
  }
  return null
}

function watch(el: Element, w: Watch, host: Element) {
  // 没有 IntersectionObserver（测试环境）：当作一直在视口里
  if (typeof IntersectionObserver === 'undefined') {
    queueMicrotask(w.near)
    return () => {}
  }
  const root = scrollParent(host)
  let io = observers.get(root)
  if (!io) {
    io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const x = watched.get(e.target)
          if (x) (e.isIntersecting ? x.near : x.away)()
        }
      },
      { root, rootMargin: '800px 0px' },
    )
    observers.set(root, io)
  }
  watched.set(el, w)
  io.observe(el)
  return () => {
    watched.delete(el)
    io!.unobserve(el)
  }
}

export const embedView: NodeViewRenderer = (p) => {
  let node: PMNode = p.node
  const dom = document.createElement('div')
  dom.className = 'oad-embed'
  dom.setAttribute('data-type', 'embed')
  dom.contentEditable = 'false'
  const box = document.createElement('div')
  box.className = 'oad-embed-box'
  dom.append(box)

  let data: Unfurl | null = null
  let shown = '' // 现在画的是哪个网址 + 样式
  let handle: { pause?: () => void; destroy?: () => void } = {}
  let near = false
  let alive = true
  // 解析结果到了（或者等太久了）才画卡片，免得先画一个简单的、再跳成富卡片
  let resolved = false
  let slow = 0

  const key = () => `${node.attrs.url}\u0000${node.attrs.view}`
  const sync = () => {
    dom.dataset.view = node.attrs.view
    if (node.attrs.id) dom.dataset.id = node.attrs.id
  }

  const draw = () => {
    if (!alive || !cards || !near || !resolved) return
    const k = key()
    if (k === shown && data) return
    handle.destroy?.()
    handle = cards.render(box, data, { url: node.attrs.url || '', view: node.attrs.view === 'bookmark' ? 'bookmark' : 'embed' })
    shown = k
  }

  const resolve = () => {
    const url = node.attrs.url || ''
    const svc = media(p.editor)
    const cached = svc.peek(url)
    if (cached) {
      data = cached
      resolved = true
      return draw()
    }
    clearTimeout(slow)
    slow = window.setTimeout(() => {
      resolved = true
      draw()
    }, 2500)
    const want = url
    svc.unfurl(url).then(
      (d) => {
        if (!alive || node.attrs.url !== want) return
        clearTimeout(slow)
        resolved = true
        data = d
        shown = ''
        draw()
      },
      () => {},
    )
  }

  const start = () => {
    sync()
    data = null
    shown = ''
    resolved = false
    // 没画出卡片前先放骨架
    if (cards) box.replaceChildren(cards.skeleton(node.attrs.url || ''))
    else {
      box.innerHTML = ''
      const sk = document.createElement('div')
      sk.className = 'oad-embed-pending'
      sk.textContent = node.attrs.url || ''
      box.append(sk)
    }
    loadCards().then(() => {
      if (!alive) return
      if (!shown) box.replaceChildren(cards!.skeleton(node.attrs.url || ''))
      draw()
    })
    resolve()
  }

  start()
  // 节点视图建好时 dom 还没挂进页面，等一帧再找滚动容器
  let unwatch = () => {}
  requestAnimationFrame(() => {
    if (!alive) return
    unwatch = watch(dom, w, p.editor.view.dom)
  })
  const w: Watch = {
    near: () => {
      near = true
      draw()
    },
    away: () => {
      near = false
      handle.pause?.()
    },
  }

  // 编辑时：点卡片（链接）先选中这一块，按住 Cmd / Ctrl 点才打开；按钮（播放、预览）照常
  dom.addEventListener('click', (e) => {
    const a = (e.target as HTMLElement).closest('a')
    if (a && p.editor.isEditable && !(e.metaKey || e.ctrlKey) && !a.hasAttribute('download')) e.preventDefault()
  })

  return {
    dom,
    update(n) {
      if (n.type !== node.type) return false
      const urlChanged = n.attrs.url !== node.attrs.url
      const viewChanged = n.attrs.view !== node.attrs.view
      node = n
      sync()
      if (urlChanged) start()
      else if (viewChanged) {
        shown = ''
        draw()
      }
      return true
    },
    selectNode: () => dom.classList.add('is-selected'),
    deselectNode: () => dom.classList.remove('is-selected'),
    // 卡片里的按钮、播放器、视频控件自己处理事件
    stopEvent: (e) => {
      const t = e.target as HTMLElement
      if (e.type === 'dragstart' || e.type === 'mousedown') return !!t.closest('button, video, audio, iframe, .c-frame-box')
      return !!t.closest('button, video, audio, iframe')
    },
    ignoreMutation: () => true,
    destroy() {
      alive = false
      clearTimeout(slow)
      unwatch()
      handle.destroy?.()
    },
  }
}
