// 行内提及（linkMention{url}）：网站图标 + 标题。标题等解析结果来了再换上，先显示网址的域名。
import type { NodeViewRenderer } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { media } from '../extensions/media'
import { host } from '../media/dom'
import { resolveDeep } from '../media/resolve'
import type { Unfurl } from '../media/types'

function label(d: Unfurl | null, url: string) {
  if (!d) return host(url)
  if (d.embed?.type === 'github_issue') return `${d.embed.owner}/${d.embed.repo}#${d.embed.number}`
  if (d.embed?.type === 'github_repo') return `${d.embed.owner}/${d.embed.repo}`
  if (d.embed?.type === 'x') return `${d.author?.name ?? 'X'}：${d.embed.text.slice(0, 40)}${d.embed.text.length > 40 ? '…' : ''}`
  return d.title || host(url)
}

export const linkMentionView: NodeViewRenderer = (p) => {
  let node: PMNode = p.node
  const dom = document.createElement('a')
  dom.className = 'oad-link-mention'
  dom.target = '_blank'
  dom.rel = 'noopener noreferrer nofollow'
  dom.contentEditable = 'false'
  dom.setAttribute('data-type', 'linkMention')
  const ico = document.createElement('img')
  ico.className = 'oad-link-mention-icon'
  ico.alt = ''
  ico.setAttribute('referrerpolicy', 'no-referrer')
  ico.addEventListener('error', () => (ico.style.display = 'none'))
  const text = document.createElement('span')
  text.className = 'oad-link-mention-text'
  dom.append(ico, text)
  let alive = true

  const show = (d: Unfurl | null) => {
    const url = node.attrs.url || ''
    text.textContent = label(d, url)
    dom.title = d?.title ? `${d.title}\n${url}` : url
    let fav = d?.favicon ?? ''
    if (!fav && /^https?:/i.test(url)) {
      try {
        fav = `${new URL(url).origin}/favicon.ico`
      } catch {
        fav = ''
      }
    }
    // 本站的文件、转存的图标：跨域、token 登录时换成能直接打开的地址
    const got = resolveDeep({ href: /^(https?:|\/)/i.test(url) ? url : '#', fav }, media(p.editor).resolveUrl)
    const apply = (x: { href: string; fav: string }) => {
      if (!alive || node.attrs.url !== url) return
      dom.href = x.href
      if (x.fav) {
        ico.style.display = ''
        ico.src = x.fav
      } else ico.removeAttribute('src')
    }
    if (got instanceof Promise) got.then(apply, () => {})
    else apply(got)
  }
  const start = () => {
    const url = node.attrs.url || ''
    const svc = media(p.editor)
    const c = svc.peek(url)
    show(c)
    if (!c)
      svc.unfurl(url).then(
        (d) => alive && node.attrs.url === url && d && show(d),
        () => {},
      )
  }
  start()
  dom.addEventListener('click', (e) => {
    if (p.editor.isEditable && !(e.metaKey || e.ctrlKey)) e.preventDefault()
  })
  return {
    dom,
    update(n) {
      if (n.type !== node.type) return false
      const changed = n.attrs.url !== node.attrs.url
      node = n
      if (changed) start()
      return true
    },
    selectNode: () => dom.classList.add('oad-is-selected'),
    deselectNode: () => dom.classList.remove('oad-is-selected'),
    ignoreMutation: () => true,
    destroy() {
      alive = false
    },
  }
}
