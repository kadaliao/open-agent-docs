// 对外的类型：元素的属性、事件。只用这里的类型，不引别的包（发出去的 elements.d.ts 就是从这个文件生成的）。

/** 换 token：返回新的 JWT / Agent token（可以是 Promise） */
export type TokenProvider = () => string | null | undefined | Promise<string | null | undefined>

export type OadTheme = 'light' | 'dark' | 'auto'
export type OadLang = 'zh' | 'en'

/** 三个元素都有的 */
export interface OadElementBase extends HTMLElement {
  /** 服务器地址（属性 base-url），空是同源 */
  baseUrl: string
  /** JWT 或 Agent token（属性 token）；不给就靠 cookie */
  token: string | null
  /** 快过期（JWT 的 exp 前 60 秒）、收到 401 时调它换新的 */
  getToken: TokenProvider | null
  /** 'light' | 'dark' | 'auto'（属性 theme，默认跟系统） */
  theme: string
  /** 强制断开重连 */
  reload(): void
  /** 调 getToken 换一个 token */
  refreshToken(): Promise<void>
}

/** <oad-editor page-id base-url token theme lang comments readonly> */
export interface OadEditorElement extends OadElementBase {
  pageId: string
  /** 显示评论栏（属性 comments） */
  comments: boolean
  /** 只读（属性 readonly）；只有查看权限时自动只读 */
  readonly: boolean
}

/** <oad-viewer page-id base-url token theme content-css> */
export interface OadViewerElement extends OadElementBase {
  pageId: string
  /** 正文的样式表地址（属性 content-css），默认 <base-url>/embed/content.css */
  contentCss: string
}

/** <oad-database db-id base-url token theme lang view-id readonly> */
export interface OadDatabaseElement extends OadElementBase {
  dbId: string
  /** 只显示这个视图（属性 view-id） */
  viewId: string | null
  readonly: boolean
}

export interface OadReadyDetail {
  /** 页面（或库）的 id */
  id: string
  title: string
}
export interface OadChangeDetail {
  id: string
  /** 'title'：标题变了；'content'：正文变了（合并 300 毫秒内的改动） */
  kind: 'title' | 'content'
  title: string
  /** true：自己改的；false：别人（协作者、Agent）改的 */
  local: boolean
}
export interface OadNavigateDetail {
  /** 完整地址 */
  href: string
  /** 是本服务器的页面时有 */
  pageId?: string
}
export interface OadErrorDetail {
  error: unknown
  /** HTTP 状态（服务器回的错误才有） */
  status?: number
  code?: string
  message: string
}

export interface OadElementEventMap extends HTMLElementEventMap {
  'oad-ready': CustomEvent<OadReadyDetail>
  'oad-change': CustomEvent<OadChangeDetail>
  'oad-navigate': CustomEvent<OadNavigateDetail>
  'oad-error': CustomEvent<OadErrorDetail>
}

declare global {
  interface HTMLElementTagNameMap {
    'oad-editor': OadEditorElement
    'oad-viewer': OadViewerElement
    'oad-database': OadDatabaseElement
  }
  // el.addEventListener('oad-change', (e) => e.detail.title) 有提示
  interface HTMLElementEventMap {
    'oad-ready': CustomEvent<OadReadyDetail>
    'oad-change': CustomEvent<OadChangeDetail>
    'oad-navigate': CustomEvent<OadNavigateDetail>
    'oad-error': CustomEvent<OadErrorDetail>
  }
}
