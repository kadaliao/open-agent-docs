// 三个元素共用的部分：Shadow DOM 和样式、登录（token 属性 / el.token / el.getToken）、客户端、事件、属性变了重连、移除时释放。
// 只引客户端的 core 和编辑器的 i18n（很小）：<oad-viewer> 用不到 React 编辑器，编辑器、数据库视图的代码在 start() 里按需加载。
import { createClient, isApiError, type OadClient } from '@open-agent-docs/client/core'
import { setLocale } from '@open-agent-docs/editor/i18n'
import { clientUrlResolver, type UrlResolver } from '@open-agent-docs/editor/resolve'
import { adoptStyles, releaseStyles } from './styles.ts'
import type { OadErrorDetail, OadNavigateDetail, TokenProvider } from './types.ts'

const BASE_ATTRS = ['base-url', 'token', 'theme', 'lang']

export interface ElementHost {
  el: OadBaseElement
  shadow: ShadowRoot
  mount: HTMLDivElement
  client: OadClient
  baseUrl: string
  /** 显示服务器上的图片、文件前换地址（token 登录时签名、跨域时拼服务器地址）；同域 cookie 登录时 null */
  resolveUrl: UrlResolver | null
  emit: <D>(name: string, detail: D, cancelable?: boolean) => boolean
  fail: (e: unknown) => void
  navigate: (href: string, pageId?: string) => void
  pageIdOf: (href: string) => string | null
}

/** JWT 的 exp（秒）；不是 JWT（Agent token）返回 null */
export function jwtExp(token: string): number | null {
  const part = token.split('.')[1]
  if (!part || token.split('.').length !== 3) return null
  try {
    const json = atob(part.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(part.length / 4) * 4, '='))
    const exp = JSON.parse(json)?.exp
    return typeof exp === 'number' ? exp : null
  } catch {
    return null
  }
}

/** 离过期不到 60 秒就算快过期 */
const expiresSoon = (t: string, now = Date.now()) => {
  const exp = jwtExp(t)
  return exp !== null && exp * 1000 - now < 60_000
}

export abstract class OadBaseElement extends HTMLElement {
  static get observedAttributes() {
    return BASE_ATTRS
  }

  protected readonly shadow: ShadowRoot
  /** React 或者 HTML 画在这里面（.oad-root：编辑器通用控件的样式只在它里面生效） */
  protected readonly mount: HTMLDivElement
  /** 出错时显示的那一行（和 mount 分开：mount 里是 React 管的，不能从外面换掉） */
  private readonly errorBox: HTMLDivElement
  private _token: string | null = null
  private _getToken: TokenProvider | null = null
  private _client: OadClient | null = null
  private _resolver: { client: OadClient; r: UrlResolver | null } | null = null
  private running: { key: string; stop: () => void } | null = null
  private scheduled = false
  private refreshing: Promise<void> | null = null
  private refreshTimer: ReturnType<typeof setTimeout> | null = null
  /** 上一次是因为没登录（401）失败的：换了 token 就重来 */
  private authFailed = false

  constructor() {
    super()
    this.shadow = this.attachShadow({ mode: 'open' })
    this.mount = document.createElement('div')
    this.mount.className = 'oad-root oad-el'
    this.mount.setAttribute('part', 'root')
    const own = document.createElement('style')
    own.setAttribute('data-oad-own', '')
    own.textContent = HOST_CSS
    this.errorBox = document.createElement('div')
    this.errorBox.className = 'oad-el-error'
    this.errorBox.setAttribute('role', 'alert')
    this.errorBox.setAttribute('part', 'error')
    this.errorBox.hidden = true
    this.shadow.append(own, this.mount, this.errorBox)
    // 元素定义之前宿主就设了 el.token = …：那是挂在实例上的普通属性，挪到 setter 上
    for (const p of this.upgradeProps()) {
      if (Object.prototype.hasOwnProperty.call(this, p)) {
        const v = (this as Record<string, unknown>)[p]
        delete (this as Record<string, unknown>)[p]
        ;(this as Record<string, unknown>)[p] = v
      }
    }
  }

  /** 子类加上自己的属性名 */
  protected upgradeProps(): string[] {
    return ['token', 'getToken', 'baseUrl', 'theme']
  }

  // ---------- 属性 ----------
  /** 服务器地址；空是同源 */
  get baseUrl(): string {
    return (this.getAttribute('base-url') ?? '').trim().replace(/\/+$/, '')
  }
  set baseUrl(v: string) {
    this.setAttribute('base-url', v)
  }
  /** JWT 或 Agent token；也可以用 token 属性。不给就靠 cookie（同源、登录过） */
  get token(): string | null {
    return this._token ?? this.getAttribute('token')
  }
  set token(v: string | null) {
    this._token = v || null
    this.tokenChanged()
  }
  /** 快过期、收到 401 时调它换一个新的 token（返回 JWT / Agent token） */
  get getToken(): TokenProvider | null {
    return this._getToken
  }
  set getToken(f: TokenProvider | null) {
    this._getToken = f
    this.planRefresh()
  }
  get theme(): string {
    return this.getAttribute('theme') ?? 'auto'
  }
  set theme(v: string) {
    this.setAttribute('theme', v)
  }
  /** 这个元素用的客户端（@open-agent-docs/client）：想自己调接口时拿来用，登录方式和元素一样 */
  get client(): OadClient {
    return (this._client ??= this.makeClient())
  }
  /** 图片、文件的地址解析（跟着客户端换）：用 token 时批量签名（POST /files/sign，缓存到过期前 60 秒），跨域时拼上服务器地址 */
  private get urlResolver(): UrlResolver | null {
    const c = this.client
    if (this._resolver?.client !== c) this._resolver = { client: c, r: clientUrlResolver(c, { token: !!(this.token || this.getToken) }) }
    return this._resolver.r
  }

  // ---------- 生命周期 ----------
  connectedCallback() {
    adoptStyles(this.shadow)
    this.applyLang()
    this.schedule()
  }
  disconnectedCallback() {
    this.schedule()
  }
  attributeChangedCallback(name: string, old: string | null, v: string | null) {
    if (old === v) return
    if (name === 'token') this.tokenChanged()
    else if (name === 'lang') this.applyLang()
    else if (name === 'theme') {
      /* 只是样式（:host([theme=dark])） */
    } else this.attributeUpdated(name)
  }

  /** 子类：除了 key 里的以外的属性变了（不用重连，重画就行） */
  protected attributeUpdated(_name: string) {
    this.schedule()
  }

  /** 子类：这次连接由哪些东西决定（变了就断开重连）；还不能连（比如没给 page-id）返回 null */
  protected abstract key(): string | null
  /** 子类：开始（画出来、连上），返回清理函数 */
  protected abstract start(): () => void

  /** 属性、连接状态变了：放到微任务里统一处理（移动元素时会先断开再连上，合并成一次，不重连） */
  protected schedule() {
    if (this.scheduled) return
    this.scheduled = true
    queueMicrotask(() => {
      this.scheduled = false
      this.sync()
    })
  }

  private sync() {
    const key = this.isConnected ? this.key() : null
    const k = key === null ? null : this.baseUrl + '\n' + key
    if (this.running && this.running.key === k) return
    this.stopNow()
    if (k === null) {
      if (!this.isConnected) releaseStyles(this.shadow)
      return
    }
    this.authFailed = false
    this.errorBox.hidden = true
    this.mount.hidden = false
    this.mount.replaceChildren()
    this._client = this.makeClient()
    this.running = { key: k, stop: this.start() }
    this.planRefresh()
  }

  private stopNow() {
    if (!this.running) return
    const { stop } = this.running
    this.running = null
    if (this.refreshTimer) clearTimeout(this.refreshTimer)
    this.refreshTimer = null
    try {
      stop()
    } catch (e) {
      console.error('[oad] 清理出错', e)
    }
  }

  /** 强制重来（比如出错以后宿主换了配置） */
  reload() {
    this.stopNow()
    this.schedule()
  }

  private applyLang() {
    const l = this.getAttribute('lang')
    if (l === 'zh' || l === 'en') setLocale(l)
  }

  // ---------- 登录 ----------
  private currentToken(): string | null {
    return this.token
  }

  /** 没有 token、或者快过期：问 getToken 要一个 */
  private async ensureToken() {
    if (!this.getToken) return
    const t = this.currentToken()
    if (!t || expiresSoon(t)) await this.refreshToken()
  }

  /** 调 getToken 换新的（同时只调一次） */
  refreshToken(): Promise<void> {
    if (!this.getToken) return Promise.resolve()
    this.refreshing ??= (async () => {
      try {
        const t = await this.getToken!()
        if (t) {
          this._token = t
          this.tokenChanged()
        }
      } catch (e) {
        this.fail(e)
      } finally {
        this.refreshing = null
      }
    })()
    return this.refreshing
  }

  /** token 变了：协作连接换上新的；之前因为没登录失败的，重来 */
  private tokenChanged() {
    this.onToken(this.currentToken())
    this.planRefresh()
    if (this.authFailed) this.reload()
  }

  /** 子类：拿到新 token（编辑器要换 WebSocket 的参数） */
  protected onToken(_token: string | null) {}

  /** JWT 过期前 60 秒自动换（WebSocket 断了重连时要用新的） */
  private planRefresh() {
    if (this.refreshTimer) clearTimeout(this.refreshTimer)
    this.refreshTimer = null
    const t = this.currentToken()
    const exp = t ? jwtExp(t) : null
    if (!this.getToken || exp === null || !this.running) return
    const wait = Math.max(0, exp * 1000 - Date.now() - 60_000)
    // setTimeout 最多等 24.8 天
    this.refreshTimer = setTimeout(() => this.refreshToken(), Math.min(wait, 2 ** 31 - 1))
  }

  private authFetch = async (input: RequestInfo | URL, init: RequestInit = {}): Promise<Response> => {
    await this.ensureToken()
    const go = () => {
      const h = new Headers(init.headers)
      const t = this.currentToken()
      if (t) h.set('authorization', 'Bearer ' + t)
      return fetch(input, { ...init, headers: h })
    }
    let r = await go()
    if (r.status === 401 && this.getToken && !init.signal?.aborted) {
      await this.refreshToken()
      r = await go()
    }
    return r
  }

  private makeClient(): OadClient {
    const base = this.baseUrl
    const bearer = !!(this.token || this.getToken)
    return createClient({
      baseUrl: base,
      token: () => this.currentToken() ?? undefined,
      // 用 token 时不带 cookie（跨域时不用服务器另开 Allow-Credentials）
      credentials: bearer ? 'omit' : undefined,
      fetch: this.authFetch,
      locale: () => (this.getAttribute('lang') ?? (typeof navigator !== 'undefined' ? navigator.language : 'zh')).slice(0, 2),
    })
  }

  /** 给按需加载的界面代码用的（React 那部分不在这个文件里） */
  host(): ElementHost {
    return {
      el: this,
      shadow: this.shadow,
      mount: this.mount,
      client: this.client,
      baseUrl: this.baseUrl,
      resolveUrl: this.urlResolver,
      emit: (n, d, c) => this.emit(n, d, c),
      fail: (e) => this.fail(e),
      navigate: (href, pageId) => this.navigate(href, pageId),
      pageIdOf: (href) => this.pageIdOf(href),
    }
  }

  /** 按需加载界面代码再画：返回的清理函数在加载完之前调也没关系 */
  protected lazyStart(load: () => Promise<() => () => void>): () => void {
    let stop: (() => void) | null = null
    let dead = false
    this.mount.innerHTML = '<div class="oad-el-loading" aria-busy="true"></div>'
    load().then(
      (run) => {
        if (!dead) stop = run()
      },
      (e) => !dead && this.fail(e),
    )
    return () => {
      dead = true
      stop?.()
    }
  }

  // ---------- 事件 ----------
  /** 发事件（冒泡、穿过 Shadow DOM）；返回宿主有没有 preventDefault */
  protected emit<D>(name: string, detail: D, cancelable = false): boolean {
    return this.dispatchEvent(new CustomEvent(name, { detail, bubbles: true, composed: true, cancelable }))
  }

  /** 出错：发 oad-error，界面上也写一行 */
  protected fail(e: unknown, show = true) {
    const status = isApiError(e) ? e.status : undefined
    if (status === 401) this.authFailed = true
    const detail: OadErrorDetail = {
      error: e,
      status,
      code: isApiError(e) ? e.code : undefined,
      message: e instanceof Error ? e.message : String(e),
    }
    this.emit('oad-error', detail)
    if (show) {
      this.errorBox.textContent = status === 401 ? '没有登录，或者登录过期了' : status === 403 ? '没有权限' : status === 404 ? '找不到' : detail.message
      this.errorBox.hidden = false
      this.mount.hidden = true
    }
  }

  /** 去另一页：发 oad-navigate（可以 preventDefault 自己处理），没人拦就跳过去 */
  protected navigate(href: string, pageId?: string) {
    const abs = new URL(href, (this.baseUrl || location.origin) + '/').href
    const detail: OadNavigateDetail = { href: abs, pageId }
    if (this.emit('oad-navigate', detail, true)) location.assign(abs)
  }

  /** 内部页面的链接（/p/<id>）：拿到 id */
  protected pageIdOf(href: string): string | null {
    try {
      const u = new URL(href, (this.baseUrl || location.origin) + '/')
      const base = new URL((this.baseUrl || location.origin) + '/')
      if (u.origin !== base.origin) return null
      const m = /\/p\/([^/?#]+)/.exec(u.pathname.slice(base.pathname.length - 1))
      return m ? decodeURIComponent(m[1]) : null
    } catch {
      return null
    }
  }
}

// 元素自己的样式（不在编辑器的样式表里）
const HOST_CSS = `
:host { display: block; position: relative; container-type: inline-size; color: var(--oad-fg); font-family: var(--oad-font, system-ui, sans-serif); }
:host([hidden]) { display: none; }
.oad-el { min-height: 1em; }
.oad-el-loading { min-height: 120px; }
.oad-el-error[hidden], .oad-el[hidden] { display: none; }
.oad-el-error { padding: 12px 16px; border-radius: var(--oad-radius, 8px); background: var(--oad-bg-2, #f7f7f5); color: var(--oad-fg-2, #5f5e5b); font-size: 14px; }
.oad-viewer-title { font-size: 2em; font-weight: 700; line-height: 1.25; margin: 0 0 0.6em; }
.oad-el-split { display: flex; gap: 24px; align-items: flex-start; }
.oad-el-split > .oad-el-doc { flex: 1; min-width: 0; }
.oad-el-split > .oad-el-comments { width: 320px; flex: none; position: sticky; top: 0; max-height: 100vh; overflow: auto; }
/* 按元素自己的宽度（不是窗口）：窄了评论栏放到下面 */
@container (max-width: 760px) { .oad-el-split { flex-direction: column; } .oad-el-split > .oad-el-comments { width: 100%; position: static; max-height: none; } }
`
