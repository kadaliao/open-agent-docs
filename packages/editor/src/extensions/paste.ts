// 粘贴和拖放：
// - 空行里贴网址 → 链接卡片块（立刻出骨架）+ 小菜单（嵌入 / 书签 / 提及 / 链接）；文字中间贴网址 → 链接；选中文字后贴网址 → 给选中的字加链接
// - 贴 / 拖图片和文件 → 上传：上传时用 decoration 画本地预览和进度（不往共享文档里写临时地址），传完再插入真正的块
// - 贴像 Markdown 的纯文本 → 用 oad-markdown（WASM）解析成块
import { Extension, type Editor } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { NodeSelection, Plugin, PluginKey, TextSelection, type EditorState } from '@tiptap/pm/state'
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view'
import { bytes } from '../media/dom'
import { t } from '../strings'
import { knownImages } from '../views/image'
import { media } from './media'

export type PasteChoice = 'embed' | 'bookmark' | 'mention' | 'link'

export interface PasteMenuState {
  pos: number
  url: string
}

export interface PasteStorage {
  // 宿主（React）注册：显示 / 隐藏粘贴菜单；菜单开着时的按键
  show: (s: PasteMenuState | null) => void
  key: (e: KeyboardEvent) => boolean
  open: PasteMenuState | null
}

const URL_RE = /^https?:\/\/[^\s<>"']+$/i
export const isUrl = (s: string) => URL_RE.test(s.trim()) && s.trim().length < 2048

export function looksLikeMarkdown(text: string) {
  const s = text.trim()
  if (!s || s.length > 500_000) return false
  if (/^(#{1,6} |[-*+] |\d+[.)] |> |```|~~~|\$\$|\|.+\||- \[[ xX]\] |> \[!)/m.test(s)) return true
  const inline = (s.match(/\*\*[^*\n]+\*\*|`[^`\n]+`|\[[^\]\n]+\]\([^)\s]+\)|!\[[^\]\n]*\]\([^)\s]+\)/g) ?? []).length
  return inline >= 1 && s.includes('\n') ? true : inline >= 2
}

// HTML 里有结构（标题、列表、表格、图片）就交给 ProseMirror 按 HTML 处理
const richHtml = (html: string) => /<(h[1-6]|ul|ol|table|img|pre|blockquote)[\s>]/i.test(html) && !/vscode|monaco|<meta name="?generator"?[^>]*(sublime|jetbrains)/i.test(html)

// ---------- 上传 ----------
interface Pending {
  id: string
  pos: number
  name: string
  size: number
  preview: string | null
  progress: number
  error: boolean
}

const uploadKey = new PluginKey<DecorationSet>('oadUploads')
let seq = 0

function placeholderDom(u: Pending) {
  const box = document.createElement('div')
  box.className = 'oad-upload' + (u.error ? ' is-error' : '')
  box.contentEditable = 'false'
  if (u.preview) {
    const im = document.createElement('img')
    im.src = u.preview
    im.alt = ''
    box.append(im)
  } else {
    const row = document.createElement('div')
    row.className = 'oad-upload-row'
    row.textContent = `${u.name} · ${bytes(u.size)}`
    box.append(row)
  }
  const bar = document.createElement('div')
  bar.className = 'oad-upload-bar'
  const fill = document.createElement('i')
  fill.style.width = `${Math.round(u.progress * 100)}%`
  bar.append(fill)
  const label = document.createElement('span')
  label.className = 'oad-upload-label'
  label.textContent = u.error ? t('uploadFailed') : `${t('uploading')} ${Math.round(u.progress * 100)}%`
  box.append(bar, label)
  return box
}

function setPending(view: EditorView, u: Pending | null, remove?: string) {
  view.dispatch(view.state.tr.setMeta(uploadKey, { u, remove }))
}

/** 插入位置在空段落里就换掉它，否则插在当前块后面 */
function insertBlock(view: EditorView, pos: number, node: PMNode) {
  const { state } = view
  const $p = state.doc.resolve(Math.min(pos, state.doc.content.size))
  const tr = state.tr
  let at: number
  if ($p.parent.isTextblock && $p.parent.content.size === 0 && $p.depth > 0) {
    at = $p.before()
    tr.replaceWith(at, $p.after(), node)
  } else {
    at = $p.depth > 0 ? $p.after(1) : $p.pos
    tr.insert(at, node)
  }
  const end = at + node.nodeSize
  if (!tr.doc.resolve(end).nodeAfter) tr.insert(end, state.schema.nodes.paragraph.create())
  tr.setSelection(TextSelection.near(tr.doc.resolve(Math.min(end + 1, tr.doc.content.size))))
  view.dispatch(tr.scrollIntoView())
}

export function uploadFiles(editor: Editor, files: File[], pos: number) {
  const svc = media(editor)
  if (!svc.upload) return false
  const view = editor.view
  for (const file of files) {
    const id = 'u' + ++seq
    const isImage = file.type.startsWith('image/')
    const u: Pending = { id, pos, name: file.name || 'file', size: file.size, preview: isImage ? URL.createObjectURL(file) : null, progress: 0, error: false }
    setPending(view, u)
    svc.upload(file, (f) => {
      u.progress = f
      if (!view.isDestroyed) setPending(view, { ...u })
    }).then(
      (r) => {
        if (view.isDestroyed) return
        const cur = (uploadKey.getState(view.state)?.find(undefined, undefined, (s) => s.id === id) ?? [])[0]
        const at = cur ? cur.from : pos
        setPending(view, null, id)
        if (u.preview) URL.revokeObjectURL(u.preview)
        const schema = view.state.schema
        if (r.mime.startsWith('image/') && r.mime !== 'image/svg+xml') {
          knownImages.set(r.url, { width: r.width, height: r.height, blurhash: r.blurhash })
          insertBlock(view, at, schema.nodes.image.create({ src: r.url, alt: r.name.replace(/\.[^.]+$/, '') || null }))
        } else {
          svc.remember(r.url, { kind: 'file', provider: 'generic', url: r.url, name: r.name, mime: r.mime, size: r.size, duration_ms: r.duration_ms, width: r.width, height: r.height })
          insertBlock(view, at, schema.nodes.embed.create({ url: r.url, view: 'embed' }))
        }
      },
      () => {
        if (view.isDestroyed) return
        setPending(view, { ...u, error: true })
        setTimeout(() => !view.isDestroyed && setPending(view, null, id), 4000)
      },
    )
  }
  return true
}

// ---------- 把链接卡片换成别的样子 ----------
export function convertEmbed(editor: Editor, pos: number, to: PasteChoice) {
  const { state, view } = editor
  const node = state.doc.nodeAt(pos)
  if (!node) return
  const url: string = node.type.name === 'embed' ? node.attrs.url : node.attrs.url
  const s = state.schema
  const tr = state.tr
  if (node.type.name === 'embed' && (to === 'embed' || to === 'bookmark')) {
    tr.setNodeAttribute(pos, 'view', to)
    tr.setSelection(NodeSelection.create(tr.doc, pos))
  } else {
    let para: PMNode
    if (to === 'mention') para = s.nodes.paragraph.create({ id: node.attrs.id }, s.nodes.linkMention.create({ url }))
    else if (to === 'link') para = s.nodes.paragraph.create({ id: node.attrs.id }, s.text(url, [s.marks.link.create({ href: url })]))
    else para = s.nodes.embed.create({ id: node.attrs.id, url, view: to })
    tr.replaceWith(pos, pos + node.nodeSize, para)
    tr.setSelection(TextSelection.near(tr.doc.resolve(pos + para.nodeSize - 1)))
  }
  view.dispatch(tr)
  view.focus()
}

const menuKey = new PluginKey<PasteMenuState | null>('oadPasteMenu')

export const Paste = Extension.create<object, PasteStorage>({
  name: 'oadPaste',
  // 比 Tiptap 自带的粘贴处理（链接、Markdown 输入规则）先跑
  priority: 200,
  addStorage() {
    return { show: () => {}, key: () => false, open: null }
  },
  addProseMirrorPlugins() {
    const editor = this.editor
    const storage = this.storage
    return [
      // 上传中的占位
      new Plugin<DecorationSet>({
        key: uploadKey,
        state: {
          init: () => DecorationSet.empty,
          apply(tr, set) {
            let next = set.map(tr.mapping, tr.doc)
            const m = tr.getMeta(uploadKey) as { u: Pending | null; remove?: string } | undefined
            if (m) {
              const id = m.remove ?? m.u?.id
              if (id) next = next.remove(next.find(undefined, undefined, (s) => s.id === id))
              if (m.u) {
                const old = set.find(undefined, undefined, (s) => s.id === m.u!.id)[0]
                const at = Math.min(old ? tr.mapping.map(old.from) : m.u.pos, tr.doc.content.size)
                const u = m.u
                next = next.add(tr.doc, [Decoration.widget(at, () => placeholderDom(u), { id: u.id, side: -1 })])
              }
            }
            return next
          },
        },
        props: { decorations: (s) => uploadKey.getState(s) },
      }),
      // 粘贴菜单
      new Plugin<PasteMenuState | null>({
        key: menuKey,
        state: {
          init: () => null,
          apply(tr, cur) {
            const m = tr.getMeta(menuKey)
            if (m !== undefined) return m
            if (!cur) return null
            // 别处改了文档或者光标走了：收起
            if (tr.docChanged && !tr.getMeta('oadKeepPasteMenu')) {
              const pos = tr.mapping.map(cur.pos)
              const n = tr.doc.nodeAt(pos)
              return n?.type.name === 'embed' && n.attrs.url === cur.url ? { ...cur, pos } : null
            }
            if (tr.selectionSet) {
              const sel = tr.selection
              if (!(sel instanceof NodeSelection && sel.from === cur.pos) && !(sel.from > cur.pos && sel.from <= cur.pos + 3)) return null
            }
            return cur
          },
        },
        view: () => ({
          update(v: EditorView) {
            const s = menuKey.getState(v.state) ?? null
            if (s?.pos !== storage.open?.pos || s?.url !== storage.open?.url) {
              storage.open = s
              storage.show(s)
            }
          },
        }),
        props: {
          handleKeyDown(view, e) {
            if (!menuKey.getState(view.state)) return false
            if (e.key === 'Escape') {
              view.dispatch(view.state.tr.setMeta(menuKey, null))
              return true
            }
            return storage.key(e)
          },
          handlePaste(view, event) {
            const cd = event.clipboardData
            if (!cd) return false
            const svc = media(editor)
            // 文件
            const files = Array.from(cd.files ?? [])
            const html = cd.getData('text/html')
            if (files.length && svc.upload && !(html && !files.some((f) => f.type.startsWith('image/')))) {
              event.preventDefault()
              return uploadFiles(editor, files, view.state.selection.from)
            }
            const text = cd.getData('text/plain')
            const { state } = view
            const { $from, empty } = state.selection
            // 网址
            if (text && isUrl(text)) {
              const url = text.trim()
              if (!empty) return false // 交给 Link：给选中的字加链接
              const p = $from.parent
              if (p.type.name === 'paragraph' && p.content.size === 0 && $from.depth > 0 && !['tableCell', 'tableHeader'].includes($from.node(-1).type.name)) {
                const pos = $from.before()
                const node = state.schema.nodes.embed.create({ url, view: 'embed' })
                const tr = state.tr.replaceWith(pos, $from.after(), node)
                const end = pos + node.nodeSize
                if (!tr.doc.resolve(end).nodeAfter) tr.insert(end, state.schema.nodes.paragraph.create())
                tr.setSelection(NodeSelection.create(tr.doc, pos))
                tr.setMeta(menuKey, { pos, url })
                view.dispatch(tr.scrollIntoView())
                // 顺手开始解析
                svc.unfurl(url).catch(() => {})
                return true
              }
              return false
            }
            // Markdown
            if (text && svc.parseMarkdown && !(html && richHtml(html)) && looksLikeMarkdown(text) && !$from.parent.type.spec.code) {
              event.preventDefault()
              svc.parseMarkdown(text).then(
                (doc) => {
                  if (view.isDestroyed) return
                  const content = (doc?.content ?? []) as any[]
                  if (!content.length) return editor.commands.insertContent(text)
                  // 只有一段：当行内内容插进来，不拆段
                  if (content.length === 1 && content[0].type === 'paragraph') editor.commands.insertContent(content[0].content ?? [])
                  else editor.commands.insertContent(content)
                },
                () => editor.commands.insertContent(text),
              )
              return true
            }
            return false
          },
          handleDrop(view, event, _slice, moved) {
            if (moved) return false
            const files = Array.from(event.dataTransfer?.files ?? [])
            if (!files.length || !media(editor).upload) return false
            event.preventDefault()
            const at = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos ?? view.state.selection.from
            return uploadFiles(editor, files, at)
          },
        },
      }),
    ]
  },
})

export const closePasteMenu = (editor: Editor) => editor.view.dispatch(editor.state.tr.setMeta(menuKey, null))
export const pasteMenuOpen = (state: EditorState) => menuKey.getState(state)
