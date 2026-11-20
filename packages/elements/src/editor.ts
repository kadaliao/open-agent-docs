// <oad-editor page-id="…" base-url="…" token="…" comments readonly>：标题 + 协作编辑器（+ 评论栏）。
// 界面（React、Tiptap、Yjs）在 editor-app.tsx，第一次用到时才加载。
import type { PageConn } from '@open-agent-docs/client/core'
import { OadBaseElement } from './base.ts'
import type { OadEditorElement as Api } from './types.ts'

export class OadEditorElement extends OadBaseElement implements Api {
  static get observedAttributes() {
    return [...super.observedAttributes, 'page-id', 'comments', 'readonly']
  }
  private rerender: (() => void) | null = null
  private conn: PageConn | null = null

  get pageId(): string {
    return this.getAttribute('page-id') ?? ''
  }
  set pageId(v: string) {
    this.setAttribute('page-id', v)
  }
  get comments(): boolean {
    return this.hasAttribute('comments')
  }
  set comments(v: boolean) {
    this.toggleAttribute('comments', !!v)
  }
  get readonly(): boolean {
    return this.hasAttribute('readonly')
  }
  set readonly(v: boolean) {
    this.toggleAttribute('readonly', !!v)
  }
  protected upgradeProps() {
    return [...super.upgradeProps(), 'pageId', 'comments', 'readonly']
  }

  protected key() {
    return this.pageId || null
  }

  protected attributeUpdated(name: string) {
    // 评论栏、只读：重画就行，不用重连
    if ((name === 'comments' || name === 'readonly') && this.rerender) this.rerender()
    else this.schedule()
  }

  protected onToken(token: string | null) {
    // WebSocket 断了重连时用新的 token（y-websocket 每次连都按 params 拼地址）
    const p = this.conn?.provider as unknown as { params?: Record<string, string> } | undefined
    if (p?.params) {
      if (token) p.params.token = token
      else delete p.params.token
    }
  }

  protected start() {
    const pageId = this.pageId
    // 只读时点内部链接直接跳（编辑时链接不跳，免得点进去改字时跑掉）；按住 Cmd / Ctrl 也跳
    const onClick = (e: MouseEvent) => {
      const a = e.composedPath().find((n): n is HTMLAnchorElement => (n as Element).tagName === 'A')
      const href = a?.getAttribute('href')
      if (!href || !(this.readonly || e.metaKey || e.ctrlKey)) return
      const pid = this.pageIdOf(href)
      if (!pid) return
      e.preventDefault()
      this.navigate(href, pid)
    }
    this.mount.addEventListener('click', onClick)
    const stop = this.lazyStart(async () => {
      const { mountEditor } = await import('./editor-app.tsx')
      return () => {
        const m = mountEditor(this.host(), {
          pageId,
          opts: () => ({ comments: this.comments, readonly: this.readonly }),
          setConn: (c) => {
            this.conn = c
            if (c) this.onToken(this.token)
          },
        })
        this.rerender = m.rerender
        return m.stop
      }
    })
    return () => {
      this.rerender = null
      this.conn = null
      this.mount.removeEventListener('click', onClick)
      stop()
    }
  }
}
