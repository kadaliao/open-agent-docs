// 斜杠菜单（打 / 或中文输入法下的 、）和选中文字时的格式菜单。菜单项都是 Markdown 存得下的格式。
import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { Extension, type Editor } from '@tiptap/core'
import { PluginKey, TextSelection, type EditorState } from '@tiptap/pm/state'
import type { Node as PMNode } from '@tiptap/pm/model'
import Suggestion from '@tiptap/suggestion'
import { BubbleMenu } from '@tiptap/react/menus'
import { t } from '../../i18n'
import { notice } from '../Dialog'
import { shrinkImage, uploadMedia } from '../Media'

export type SlashItem = { key: string; label: string; icon: string; kw: string; run: (e: Editor) => void }
export type SlashState = { items: SlashItem[]; rect: DOMRect | null; pick: (it: SlashItem) => void } | null

// 关键词：英文和拼音首字母，中文不在这里（界面文字都要过 t()）
export const slashItems = (askImage: () => void): SlashItem[] => [
  { key: 'p', icon: 'T', label: t('正文'), kw: 'text paragraph zw', run: e => e.chain().focus().setParagraph().run() },
  { key: 'h1', icon: 'H1', label: t('标题 1'), kw: 'h1 heading bt', run: e => e.chain().focus().setHeading({ level: 1 }).run() },
  { key: 'h2', icon: 'H2', label: t('标题 2'), kw: 'h2 heading bt', run: e => e.chain().focus().setHeading({ level: 2 }).run() },
  { key: 'h3', icon: 'H3', label: t('标题 3'), kw: 'h3 heading bt', run: e => e.chain().focus().setHeading({ level: 3 }).run() },
  { key: 'ul', icon: '•', label: t('无序列表'), kw: 'bullet list ul wxlb', run: e => e.chain().focus().toggleBulletList().run() },
  { key: 'ol', icon: '1.', label: t('有序列表'), kw: 'ordered number list ol yxlb', run: e => e.chain().focus().toggleOrderedList().run() },
  { key: 'todo', icon: '☐', label: t('待办'), kw: 'todo task check db', run: e => e.chain().focus().toggleTaskList().run() },
  { key: 'quote', icon: '❝', label: t('引用'), kw: 'quote blockquote yy', run: e => e.chain().focus().toggleBlockquote().run() },
  { key: 'callout', icon: 'ℹ︎', label: t('提示框'), kw: 'callout note tip tsk', run: e => e.chain().focus().wrapIn('callout', { kind: 'NOTE' }).run() },
  { key: 'toggle', icon: '▸', label: t('折叠块'), kw: 'toggle details fold zdk', run: e => e.chain().focus().setDetails().run() },
  { key: 'code', icon: '</>', label: t('代码块'), kw: 'code dmk', run: e => e.chain().focus().toggleCodeBlock().run() },
  { key: 'table', icon: '▦', label: t('表格'), kw: 'table bg', run: e => e.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run() },
  { key: 'hr', icon: '—', label: t('分割线'), kw: 'divider hr line fgx', run: e => e.chain().focus().setHorizontalRule().run() },
  { key: 'image', icon: '🖼', label: t('图片'), kw: 'image picture tp', run: () => askImage() },   // i18n-ignore
]

type Storage = { show: (s: SlashState) => void; key: (e: KeyboardEvent) => boolean; items: () => SlashItem[] }

export const SlashCommands = Extension.create<object, Storage>({
  name: 'slashCommands',
  addStorage() { return { show: () => {}, key: () => false, items: () => [] } },
  addProseMirrorPlugins() {
    const storage = this.storage
    // / 要在段首或空格后面打（a/b 这种网址不弹）；、是中文里常用的顿号，只在段首打才弹
    const make = (char: string, name: string, startOfLine: boolean) => Suggestion<SlashItem, SlashItem>({
      editor: this.editor,
      char,
      pluginKey: new PluginKey(name),
      allowSpaces: false,
      startOfLine,
      allow: ({ state, range }) => state.doc.resolve(range.from).parent.type.name === 'paragraph',
      items: ({ query }) => {
        const q = query.toLowerCase()
        return storage.items().filter(it => !q || it.label.toLowerCase().includes(q) || it.kw.includes(q))
      },
      command: ({ editor, range, props }) => {
        editor.chain().focus().deleteRange(range).run()
        props.run(editor)
      },
      render: () => {
        const show = (p: { items: SlashItem[]; clientRect?: (() => DOMRect | null) | null; command: (it: SlashItem) => void }) =>
          storage.show(p.items.length ? { items: p.items, rect: p.clientRect?.() ?? null, pick: p.command } : null)
        return {
          onStart: show,
          onUpdate: show,
          onExit: () => storage.show(null),
          onKeyDown: ({ event }) => {
            if (event.key === 'Escape') { storage.show(null); return true }
            return storage.key(event)
          },
        }
      },
    })
    return [make('/', 'slash', false), make('、', 'slash-zh', true)]
  },
})

/** 斜杠菜单：贴着光标显示，下面放不下就往上翻。
 *  “放不下”按看得见的区域（visualViewport）算：手机上键盘弹起时它比页面矮一大截。往上翻用 top + translateY(-100%)，
 *  不用 bottom：fixed 的 bottom 是从整个页面的底边量的，键盘挡着的那块也算在里面，菜单会落到键盘后面。 */
export function SlashMenu({ editor }: { editor: Editor }) {
  const [state, setState] = useState<SlashState>(null)
  const [cur, setCur] = useState(0)
  const box = useRef<HTMLDivElement>(null)
  const ref = useRef({ state, cur })
  ref.current = { state, cur }
  useEffect(() => {
    const st = (editor.storage as any).slashCommands as Storage | undefined
    if (!st) return   // 编辑器已经销毁了
    st.show = s => { setState(s); setCur(0) }
    st.key = e => {
      const { state: s, cur: c } = ref.current
      if (!s) return false
      if (e.key === 'ArrowDown') { setCur((c + 1) % s.items.length); return true }
      if (e.key === 'ArrowUp') { setCur((c - 1 + s.items.length) % s.items.length); return true }
      if (e.key === 'Enter' || e.key === 'Tab') { s.pick(s.items[c]); return true }
      return false
    }
    return () => { st.show = () => {}; st.key = () => false }
  }, [editor])
  useLayoutEffect(() => { box.current?.querySelector('.on')?.scrollIntoView({ block: 'nearest' }) }, [cur, state])
  if (!state?.rect) return null
  const vv = window.visualViewport
  const visTop = vv?.offsetTop ?? 0, visBottom = vv ? vv.offsetTop + vv.height : window.innerHeight
  const below = visBottom - state.rect.bottom - 8, above = state.rect.top - visTop - 8
  const up = below < 300 && above > below
  const left = Math.max(4, Math.min(state.rect.left, window.innerWidth - 224))
  const maxHeight = Math.max(120, Math.min(300, up ? above : below))
  const style = up ? { left, top: state.rect.top - 4, transform: 'translateY(-100%)', maxHeight } : { left, top: state.rect.bottom + 4, maxHeight }
  return (
    <div className="slash-menu" ref={box} style={style} data-no-swipe="" onMouseDown={e => e.preventDefault()}>
      {state.items.map((it, k) => (
        <button key={it.key} className={k === cur ? 'on' : ''} onMouseEnter={() => setCur(k)} onClick={() => state.pick(it)}>
          <span className="slash-icon">{it.icon}</span>{it.label}
        </button>
      ))}
    </div>
  )
}

const LINK_OK = /^(https?:\/\/|mailto:|\/(?!\/)|#)/i
export const safeHref = (href: string) => {
  const h = href.trim()
  if (!h) return ''
  return LINK_OK.test(h) ? h : /^[\w-]+(\.[\w-]+)+/.test(h) ? 'https://' + h : ''
}

const TOUCH = typeof matchMedia !== 'undefined' && matchMedia('(pointer: coarse)').matches

/** 选中文字时的格式菜单：粗体、斜体、删除线、行内代码、链接。 */
export function Toolbar({ editor, onComment }: { editor: Editor; onComment?: () => void }) {
  const [link, setLink] = useState<string | null>(null)
  const apply = () => {
    const href = safeHref(link || '')
    if (href) editor.chain().focus().extendMarkRange('link').setLink({ href }).run()
    else editor.chain().focus().extendMarkRange('link').unsetLink().run()
    setLink(null)
  }
  return (
    <BubbleMenu editor={editor} className="doc-toolbar" data-no-swipe=""
      // 焦点去了别处（比如点“评论”以后在评论框里打字）就收起；填链接的输入框在菜单里，算菜单自己的焦点
      shouldShow={({ editor: e, state, view, element }) => e.isEditable && !state.selection.empty && !e.isActive('codeBlock') && !e.isActive('rawBlock')
        && (view.hasFocus() || element.contains(document.activeElement))}
      // 手机上离选区远一点：iOS 选区两头的拖动把手伸出来一截，贴太近一拖把手就点到了菜单上的按钮
      options={{ placement: 'top', offset: TOUCH ? 20 : 8, onHide: () => setLink(null) }}>
      {link !== null ? (
        <span className="doc-link-box">
          <input autoFocus value={link} placeholder="https://" onChange={e => setLink(e.target.value)}   // i18n-ignore
            onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); apply() } if (e.key === 'Escape') setLink(null) }} />
          <button onMouseDown={e => e.preventDefault()} onClick={apply}>{t('确定')}</button>
        </span>
      ) : (
        <>
          <button className={editor.isActive('bold') ? 'on' : ''} title={t('粗体')} onClick={() => editor.chain().focus().toggleBold().run()}><b>B</b></button>
          <button className={editor.isActive('italic') ? 'on' : ''} title={t('斜体')} onClick={() => editor.chain().focus().toggleItalic().run()}><i>I</i></button>
          <button className={editor.isActive('strike') ? 'on' : ''} title={t('删除线')} onClick={() => editor.chain().focus().toggleStrike().run()}><s>S</s></button>
          <button className={editor.isActive('code') ? 'on' : ''} title={t('行内代码')} onClick={() => editor.chain().focus().toggleCode().run()}>{'</>'}</button>
          <button className={editor.isActive('link') ? 'on' : ''} title={t('链接')} onClick={() => setLink(editor.getAttributes('link').href || '')}>🔗</button>
          {onComment && <button title={t('评论')} onClick={onComment}>💬</button>}
        </>
      )}
    </BubbleMenu>
  )
}

/** 传图片：浏览器里先缩（和对话里发图一样），存进 media，插成 ![文件名](/api/media/编号)。pos 不给就插在光标处。 */
export async function insertImages(editor: Editor, files: File[], pos?: number) {
  for (const f of files.filter(x => /^image\/(png|jpe?g|webp|gif)$/.test(x.type))) {
    try {
      const m = await uploadMedia(await shrinkImage(f), f.name)
      const alt = f.name.replace(/\.[a-z0-9]+$/i, '').slice(0, 60)
      const chain = editor.chain().focus()
      ;(pos !== undefined ? chain.insertContentAt(pos, { type: 'image', attrs: { src: `/api/media/${m.id}`, alt } }) : chain.setImage({ src: `/api/media/${m.id}`, alt })).run()
    } catch (e: any) { notice(e.message) }
  }
}

/** 插图片：在光标那里弹一个小框，填网址或者传图片（手机 App 里没有 prompt()）。 */
export function ImageBox({ editor, onClose }: { editor: Editor; onClose: () => void }) {
  const [url, setUrl] = useState('')
  const pick = useRef<HTMLInputElement>(null)
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => {   // 点别处就关
    const h = (e: MouseEvent) => { if (!box.current?.contains(e.target as Node)) onClose() }
    document.addEventListener('mousedown', h)
    return () => document.removeEventListener('mousedown', h)
  }, [onClose])
  const at = editor.view.coordsAtPos(editor.state.selection.from)
  const done = () => {
    const src = safeHref(url)
    if (src) editor.chain().focus().setImage({ src }).run()
    onClose()
  }
  return (
    <div ref={box} className="doc-url-box" style={{ left: at.left, top: at.bottom + 6 }} data-no-swipe="">
      <input autoFocus value={url} placeholder={t('图片网址')} onChange={e => setUrl(e.target.value)}
        onKeyDown={e => { if (e.key === 'Enter') done(); if (e.key === 'Escape') onClose() }} />
      <input ref={pick} type="file" accept="image/png,image/jpeg,image/webp,image/gif" multiple hidden
        onChange={async e => { const fs = Array.from(e.target.files || []); onClose(); await insertImages(editor, fs) }} />
      <button className="btn" onClick={() => pick.current?.click()}>{t('传图片')}</button>
      <button className="icon-btn small" title={t('关闭')} onClick={onClose}>×</button>
    </div>
  )
}


// ---------- 段的菜单（手机上用）：光标所在的段左边一个“⋮”，点开上移、下移、删除 ----------
// 段和服务器的切法一样：列表的每一项、顶层的段落 / 标题 / 整个代码块……（表格的行不在这里挪）。挪的是节点本身，段编号跟着走，保存时是一次“挪位置”。
type Unit = { from: number; node: PMNode; parent: PMNode; index: number; item: boolean }

function unitAt(state: EditorState): Unit | null {
  const sel = state.selection, $f = sel.$from
  if ($f.depth === 0) {   // 选中了整个顶层节点（分割线、图片块）
    const node = state.doc.nodeAt(sel.from)
    return node ? { from: sel.from, node, parent: state.doc, index: $f.index(0), item: false } : null
  }
  const top = $f.node(1)
  if (top.type.name === 'table') return null
  if ($f.depth >= 2 && ['bulletList', 'orderedList', 'taskList'].includes(top.type.name)) {
    return { from: $f.before(2), node: $f.node(2), parent: top, index: $f.index(1), item: true }
  }
  return { from: $f.before(1), node: top, parent: state.doc, index: $f.index(0), item: false }
}

export function moveBlock(editor: Editor, dir: -1 | 1): boolean {
  const { state } = editor
  const u = unitAt(state)
  if (!u) return false
  const other = u.parent.maybeChild(u.index + dir)
  if (!other) return false
  const offset = state.selection.from - u.from
  const tr = state.tr.delete(u.from, u.from + u.node.nodeSize)
  const at = dir < 0 ? u.from - other.nodeSize : u.from + other.nodeSize
  tr.insert(at, u.node)
  tr.setSelection(TextSelection.near(tr.doc.resolve(Math.min(at + Math.max(offset, 1), tr.doc.content.size))))
  editor.view.dispatch(tr.scrollIntoView())
  return true
}

export function deleteBlock(editor: Editor): boolean {
  const { state } = editor
  const u = unitAt(state)
  if (!u) return false
  let from = u.from, to = u.from + u.node.nodeSize
  if (u.item && u.parent.childCount === 1) {   // 列表里只剩这一项：整个列表去掉
    from = state.selection.$from.before(1)
    to = from + u.parent.nodeSize
  }
  const tr = state.tr
  if (from === 0 && to === state.doc.content.size) tr.replaceWith(0, to, state.schema.nodes.paragraph.create())   // 页面不能是空的：留一个空段
  else tr.delete(from, to)
  tr.setSelection(TextSelection.near(tr.doc.resolve(Math.min(from, tr.doc.content.size))))
  editor.view.dispatch(tr.scrollIntoView())
  return true
}

export function BlockHandle({ editor }: { editor: Editor }) {
  const [at, setAt] = useState<{ top: number; left: number; up: boolean; down: boolean } | null>(null)
  const [open, setOpen] = useState(false)
  const box = useRef<HTMLDivElement>(null)
  const opened = useRef(false)
  opened.current = open
  useEffect(() => {
    if (!TOUCH) return
    const place = () => {
      const u = editor.isEditable ? unitAt(editor.state) : null
      const dom = u && (editor.view.nodeDOM(u.from) as HTMLElement | null)
      if (!u || !dom?.getBoundingClientRect || (!editor.isFocused && !opened.current)) { setAt(null); return }
      const r = dom.getBoundingClientRect()
      setAt({ top: r.top, left: r.left, up: !!u.parent.maybeChild(u.index - 1), down: !!u.parent.maybeChild(u.index + 1) })
    }
    const blur = () => setTimeout(() => { if (!opened.current && !box.current?.contains(document.activeElement)) setAt(null) }, 150)
    editor.on('selectionUpdate', place)
    editor.on('update', place)
    editor.on('focus', place)
    editor.on('blur', blur)
    addEventListener('scroll', place, true)
    addEventListener('resize', place)
    return () => {
      editor.off('selectionUpdate', place); editor.off('update', place); editor.off('focus', place); editor.off('blur', blur)
      removeEventListener('scroll', place, true); removeEventListener('resize', place)
    }
  }, [editor])
  if (!at) return null
  const run = (fn: () => void) => { fn(); setOpen(false); editor.commands.focus() }
  return (
    <div ref={box} className="doc-block-handle" style={{ top: at.top, left: Math.max(2, at.left - 26) }} data-no-swipe="">
      <button className="doc-bh-btn" title={t('这一段')} onMouseDown={e => e.preventDefault()} onClick={() => setOpen(!open)}>⋮</button>
      {open && <div className="doc-toolbar doc-bh-menu">
        <button disabled={!at.up} onMouseDown={e => e.preventDefault()} onClick={() => run(() => moveBlock(editor, -1))}>↑ {t('上移')}</button>
        <button disabled={!at.down} onMouseDown={e => e.preventDefault()} onClick={() => run(() => moveBlock(editor, 1))}>↓ {t('下移')}</button>
        <button className="danger-text" onMouseDown={e => e.preventDefault()} onClick={() => run(() => deleteBlock(editor))}>{t('删除这一段')}</button>
      </div>}
    </div>
  )
}

