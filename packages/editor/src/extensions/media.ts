// 编辑器用到的外部服务：链接解析、上传、文件信息。宿主（apps/web）通过 OadEditor 的 props 填进来，节点视图从 editor.storage.oadMedia 取。
import { Extension, type Editor } from '@tiptap/core'
import type { Unfurl, Uploaded } from '../media/types'

export interface MediaServices {
  /** 解析链接；没有服务器时返回 null（卡片按网址自己拼一个最简单的） */
  unfurl: (url: string) => Promise<Unfurl | null>
  /** 已经在内存里的解析结果，同步拿（打开页面时直接画，不闪骨架） */
  peek: (url: string) => Unfurl | null
  /** 上传文件；onProgress 0–1 */
  upload: ((file: File, onProgress: (f: number) => void) => Promise<Uploaded>) | null
  /** 记下一个解析结果（上传完就知道文件信息，不用再问服务器） */
  remember: (url: string, d: Unfurl) => void
  /** Markdown → 文档 JSON（浏览器里的 WASM 版 oad-markdown）；没有就当纯文本粘贴 */
  parseMarkdown: ((md: string) => Promise<{ type: 'doc'; content?: unknown[] } | null>) | null
  /** 复制块链接用：这一页的地址 */
  pageUrl: () => string
}

const defaults: MediaServices = {
  unfurl: async () => null,
  peek: () => null,
  upload: null,
  remember: () => {},
  parseMarkdown: null,
  pageUrl: () => location.href.split('#')[0],
}

// 宿主给的服务放在选项里（取的时候才读，宿主后来换了也能拿到最新的）
export const MediaServicesExt = Extension.create<{ get: () => Partial<MediaServices> }>({
  name: 'oadMedia',
  addOptions() {
    return { get: () => ({}) }
  },
})

export function media(editor: Editor): MediaServices {
  const ext = editor.extensionManager.extensions.find((e) => e.name === 'oadMedia')
  const got = (ext?.options as { get?: () => Partial<MediaServices> } | undefined)?.get?.() ?? {}
  return { ...defaults, ...Object.fromEntries(Object.entries(got).filter(([, v]) => v !== undefined)) }
}
