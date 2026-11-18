// 图片块：拖左右边改宽度（width 是占正文宽度的百分比），图注，点放大镜看大图；
// 服务器的图片给 srcset（?w=），知道 blurhash 和宽高时先画模糊占位、不跳版。
import type { NodeViewRenderer } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { media } from '../extensions/media'
import { srcset } from '../media/dom'
import type { Unfurl } from '../media/types'
import { t } from '../strings'
import { blockAttrs, el, setAttr } from './node-views'

let blurhash: Promise<typeof import('blurhash')> | null = null
const placeholders = new Map<string, string>()

async function blurUrl(hash: string, w: number, h: number) {
  const hit = placeholders.get(hash)
  if (hit) return hit
  blurhash ??= import('blurhash')
  const { decode } = await blurhash
  const W = 32
  const H = Math.max(1, Math.round((W * h) / w)) || 32
  const px = decode(hash, W, H)
  const c = document.createElement('canvas')
  c.width = W
  c.height = H
  const ctx = c.getContext('2d')!
  const data = ctx.createImageData(W, H)
  data.data.set(px)
  ctx.putImageData(data, 0, 0)
  const url = c.toDataURL()
  placeholders.set(hash, url)
  return url
}

/** 本页面上传过的图片：地址 → 宽高、blurhash（上传完就知道，不用再问服务器） */
export const knownImages = new Map<string, { width?: number; height?: number; blurhash?: string }>()

export function openLightbox(src: string, alt = '') {
  const mask = el('div', 'oad-lightbox', { role: 'dialog', 'aria-label': alt || t('zoomIn') })
  const big = el('img', '', { src: src.split('?')[0], alt, referrerpolicy: 'no-referrer' })
  mask.append(big)
  const close = () => {
    mask.classList.add('is-closing')
    setTimeout(() => mask.remove(), 140)
    document.removeEventListener('keydown', onKey, true)
  }
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      close()
    }
  }
  mask.addEventListener('click', close)
  document.addEventListener('keydown', onKey, true)
  document.body.append(mask)
}

export const imageView: NodeViewRenderer = (p) => {
  let node: PMNode = p.node
  const dom = el('figure', 'oad-image', { 'data-type': 'image' })
  const frame = el('div', 'oad-image-frame', { contenteditable: 'false' })
  const img = el('img', '', { loading: 'lazy', decoding: 'async', draggable: 'false' })
  const broken = el('div', 'oad-image-broken')
  broken.hidden = true
  const zoom = el('button', 'oad-image-zoom', { type: 'button', 'aria-label': t('zoomIn'), title: t('zoomIn') })
  zoom.innerHTML = '<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M10 3a7 7 0 0 1 5.6 11.2l4.6 4.6-1.4 1.4-4.6-4.6A7 7 0 1 1 10 3zm0 2a5 5 0 1 0 0 10 5 5 0 0 0 0-10zm-1 2h2v2h2v2h-2v2H9v-2H7V9h2z"/></svg>'
  const left = el('span', 'oad-image-handle is-left', { 'aria-hidden': 'true' })
  const right = el('span', 'oad-image-handle is-right', { 'aria-hidden': 'true' })
  frame.append(img, broken, zoom, left, right)
  const caption = el('input', 'oad-image-caption', { spellcheck: 'false', 'aria-label': t('captionPlaceholder') })
  caption.placeholder = t('captionPlaceholder')
  dom.append(frame, caption)
  let alive = true

  const applyWidth = (w: number | null) => {
    dom.style.width = w ? `${w}%` : ''
    dom.classList.toggle('has-width', !!w)
  }

  const placeholder = (info: { width?: number; height?: number; blurhash?: string } | null) => {
    if (!info) return
    if (info.width && info.height) {
      frame.style.aspectRatio = `${info.width} / ${info.height}`
      img.width = info.width
      img.height = info.height
    }
    if (info.blurhash && !img.complete)
      blurUrl(info.blurhash, info.width || 32, info.height || 32).then(
        (u) => {
          if (alive && !img.complete) frame.style.backgroundImage = `url(${u})`
        },
        () => {},
      )
  }

  const sync = (n: PMNode) => {
    const src: string = n.attrs.src || ''
    if (img.getAttribute('src') !== src) {
      dom.classList.remove('is-broken', 'is-loaded')
      broken.hidden = true
      img.hidden = false
      frame.style.backgroundImage = ''
      frame.style.aspectRatio = ''
      const ss = srcset(src)
      if (ss) {
        img.srcset = ss
        img.sizes = '(max-width: 760px) 100vw, 760px'
      } else img.removeAttribute('srcset')
      if (!src.startsWith('/')) img.referrerPolicy = 'no-referrer'
      img.src = src
      const known = knownImages.get(src)
      if (known) placeholder(known)
      else if (/^\/api\/v1\/(files|media)\//.test(src)) {
        const svc = media(p.editor)
        const c = svc.peek(src)
        if (c) placeholder(info(c))
        else svc.unfurl(src).then((d) => alive && d && placeholder(info(d)), () => {})
      }
    }
    img.alt = n.attrs.alt ?? ''
    if (n.attrs.title) img.title = n.attrs.title
    else img.removeAttribute('title')
    broken.textContent = n.attrs.alt || n.attrs.title || src
    applyWidth(n.attrs.width)
    if (document.activeElement !== caption) caption.value = n.attrs.caption ?? ''
    dom.classList.toggle('has-caption', !!n.attrs.caption)
    caption.readOnly = !p.editor.isEditable
    blockAttrs(dom, n)
  }
  const info = (d: Unfurl) => ({ width: d.width ?? d.thumbnail?.width, height: d.height ?? d.thumbnail?.height, blurhash: d.blurhash ?? d.thumbnail?.blurhash })

  img.addEventListener('load', () => {
    dom.classList.add('is-loaded')
    frame.style.backgroundImage = ''
  })
  img.addEventListener('error', () => {
    dom.classList.add('is-broken')
    img.hidden = true
    broken.hidden = false
  })
  zoom.addEventListener('mousedown', (e) => e.preventDefault())
  zoom.addEventListener('click', () => openLightbox(img.currentSrc || img.src, img.alt))
  img.addEventListener('dblclick', () => openLightbox(img.currentSrc || img.src, img.alt))

  // 拖左右边改宽度：按正文宽度算百分比，两边对称（图片居中）
  const drag = (side: 'left' | 'right') => (e: PointerEvent) => {
    if (!p.editor.isEditable) return
    e.preventDefault()
    e.stopPropagation()
    const handle = side === 'left' ? left : right
    handle.setPointerCapture(e.pointerId)
    const parentW = (dom.parentElement as HTMLElement).getBoundingClientRect().width
    const startW = dom.getBoundingClientRect().width
    const x0 = e.clientX
    let pct = (startW / parentW) * 100
    dom.classList.add('is-resizing')
    const move = (ev: PointerEvent) => {
      const dx = (ev.clientX - x0) * (side === 'left' ? -1 : 1) * 2
      pct = Math.max(10, Math.min(100, ((startW + dx) / parentW) * 100))
      applyWidth(Math.round(pct))
    }
    const up = () => {
      handle.removeEventListener('pointermove', move)
      handle.removeEventListener('pointerup', up)
      dom.classList.remove('is-resizing')
      const w = Math.round(pct)
      setAttr(p, 'width', w >= 100 ? null : w)
    }
    handle.addEventListener('pointermove', move)
    handle.addEventListener('pointerup', up)
  }
  left.addEventListener('pointerdown', drag('left'))
  right.addEventListener('pointerdown', drag('right'))

  const commitCaption = () => setAttr(p, 'caption', caption.value.trim() || null)
  caption.addEventListener('blur', commitCaption)
  caption.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === 'Escape') {
      e.preventDefault()
      commitCaption()
      p.editor.commands.focus()
    }
  })

  sync(node)
  return {
    dom,
    update(n) {
      if (n.type !== node.type) return false
      node = n
      sync(n)
      return true
    },
    selectNode: () => dom.classList.add('is-selected'),
    deselectNode: () => dom.classList.remove('is-selected'),
    stopEvent: (e) => {
      const tg = e.target as HTMLElement
      return tg === caption || tg.closest('.oad-image-handle, .oad-image-zoom') !== null
    },
    ignoreMutation: () => true,
    destroy() {
      alive = false
    },
  }
}
