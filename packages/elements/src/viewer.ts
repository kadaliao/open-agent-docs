// <oad-viewer page-id="…" base-url="…" token="…">：只读的正文。不用 React、不连 WebSocket，打开最快。
// 正文用服务器的 GET /pages/{id}/content?format=html（{ html, version }，标签和属性是白名单过的）；
// 服务器还没有 format=html（旧版本）时，取 Markdown 用 @open-agent-docs/markdown 在浏览器里转（WASM，用到才下载）。
// 样式：服务器的 /embed/content.css（content-css 属性可以换成别的地址）。
import { isApiError } from '@open-agent-docs/client/core'
import { OadBaseElement } from './base.ts'
import type { OadViewerElement as Api } from './types.ts'

/** 旧服务器的退路：Markdown → HTML。@open-agent-docs/markdown 有 toHtml 就用它（和服务器同一份代码），没有就原样显示 */
export async function markdownToHtml(md: string): Promise<string> {
  try {
    const m = (await import('@open-agent-docs/markdown')) as unknown as { default?: () => Promise<unknown>; toHtml?: (md: string) => string }
    if (typeof m.toHtml === 'function') {
      await m.default?.()
      return m.toHtml(md)
    }
  } catch (e) {
    console.warn('[oad] @open-agent-docs/markdown 加载失败，按纯文本显示', e)
  }
  return `<div class="oad-content"><pre style="white-space:pre-wrap">${escapeHtml(md)}</pre></div>`
}

const escapeHtml = (s: string) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!)

export class OadViewerElement extends OadBaseElement implements Api {
  static get observedAttributes() {
    return [...super.observedAttributes, 'page-id', 'content-css']
  }
  private link: HTMLLinkElement | null = null

  get pageId(): string {
    return this.getAttribute('page-id') ?? ''
  }
  set pageId(v: string) {
    this.setAttribute('page-id', v)
  }
  get contentCss(): string {
    return this.getAttribute('content-css') ?? this.baseUrl + '/embed/content.css'
  }
  set contentCss(v: string) {
    this.setAttribute('content-css', v)
  }
  protected upgradeProps() {
    return [...super.upgradeProps(), 'pageId', 'contentCss']
  }

  protected key() {
    return this.pageId || null
  }

  protected attributeUpdated(name: string) {
    if (name === 'content-css' && this.link) this.link.href = this.contentCss
    else this.schedule()
  }

  protected start() {
    const id = this.pageId
    const client = this.client
    const base = this.baseUrl
    let alive = true
    const link = document.createElement('link')
    link.rel = 'stylesheet'
    link.href = this.contentCss
    link.setAttribute('data-oad-own', '')
    this.shadow.insertBefore(link, this.mount)
    this.link = link
    this.mount.innerHTML = '<div class="oad-el-loading" aria-busy="true"></div>'

    const onClick = (e: MouseEvent) => {
      if (e.defaultPrevented || e.button !== 0) return
      const a = e.composedPath().find((n): n is HTMLAnchorElement => (n as Element).tagName === 'A')
      const href = a?.getAttribute('href')
      if (!href || href.startsWith('#')) return
      const pid = this.pageIdOf(href)
      if (pid) {
        e.preventDefault()
        this.navigate(href, pid)
      } else if (!a!.target) a!.target = '_blank' // 外面的链接开新窗口，别把宿主页面换掉
    }
    this.mount.addEventListener('click', onClick)

    ;(async () => {
      const [page, html] = await Promise.all([client.page(id), this.fetchHtml(id)])
      if (!alive) return
      const title = document.createElement('h1')
      title.className = 'oad-viewer-title'
      title.textContent = page.title || '无标题'
      const body = document.createElement('div')
      body.className = 'oad-viewer-body'
      body.innerHTML = html
      if (base) absolutize(body, base)
      this.mount.replaceChildren(title, body)
      this.emit('oad-ready', { id, title: page.title ?? '' })
    })().catch((e) => alive && this.fail(e))

    return () => {
      alive = false
      this.mount.removeEventListener('click', onClick)
      link.remove()
      this.link = null
    }
  }

  private async fetchHtml(id: string): Promise<string> {
    const r = await this.client.content(id, 'html').catch((e) => {
      // 旧服务器不认 format=html：400；别的错误照抛
      if (isApiError(e) && e.status === 400) return null
      throw e
    })
    if (r && typeof r.html === 'string') return r.html
    const md = (r as { markdown?: string | null } | null)?.markdown ?? (await this.client.content(id)).markdown ?? ''
    return markdownToHtml(md)
  }
}

/** 服务器给的是 /api/v1/files/… 这样的绝对路径：宿主不同域时补上服务器地址 */
function absolutize(root: HTMLElement, base: string) {
  for (const [sel, attr] of [
    ['[src]', 'src'],
    ['a[href]', 'href'],
    ['[poster]', 'poster'],
  ] as const) {
    root.querySelectorAll(sel).forEach((el) => {
      const v = el.getAttribute(attr)
      if (v && v.startsWith('/') && !v.startsWith('//')) el.setAttribute(attr, base + v)
    })
  }
}
