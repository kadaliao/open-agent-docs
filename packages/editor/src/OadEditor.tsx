// 可嵌入的编辑器：给一个 Y.Doc（正文在 XmlFragment "default"）和可选的 provider（有 awareness 就显示别人的光标）。
import type { Editor } from '@tiptap/core'
import { EditorContent, useEditor, useEditorState } from '@tiptap/react'
import { Selection, TextSelection } from '@tiptap/pm/state'
import { lazy, Suspense, useContext, useEffect, useMemo, useRef, useState } from 'react'
import type { Awareness } from 'y-protocols/awareness'
import type * as Y from 'yjs'
import { editorExtensions, type OadUser } from './extensions'
import type { AuthorInfo } from './extensions/block-author'
import { selectionToAnchor, type CommentAnchor } from './extensions/comments'
import type { MediaServices } from './extensions/media'
import { OadPortalRoot } from './react/context'
import { BlockMenu } from './ui/BlockMenu'
import { PasteMenu } from './ui/PasteMenu'
import type { SlashStorage } from './extensions/slash'
import { findCell } from './schema/tables'
import { BubbleToolbar } from './ui/BubbleToolbar'
import { LinkCard } from './ui/LinkCard'
import { openHref } from './extensions/link-open'
import { SlashMenu } from './ui/SlashMenu'
import { UrlPrompt, type UrlPromptState } from './ui/UrlPrompt'

/** 光标放到正文开头并立刻聚焦（Tiptap 的 focus 命令要等下一帧，紧接着打的字会落到别处）。 */
export function focusStart(editor: Editor) {
  const { state, view } = editor
  view.dispatch(state.tr.setSelection(Selection.atStart(state.doc)).scrollIntoView())
  view.focus()
}

const TableToolbar = lazy(() => import('./ui/TableToolbar'))

export interface OadEditorProps {
  ydoc: Y.Doc
  provider?: { awareness: Awareness } | null
  user: OadUser
  field?: string
  editable?: boolean
  autofocus?: boolean
  className?: string
  onReady?: (editor: Editor) => void
  // 给了才在格式菜单里出现“评论”：选中文字 → 锚点
  onComment?: (anchor: CommentAnchor) => void
  // 点了正文里有评论的文字
  onOpenThreads?: (ids: string[]) => void
  // 块作者对照表（服务器 /pages/{id}/authors 的 clients）
  authors?: Record<string, AuthorInfo>
  onUnknownAuthor?: (clientId: string) => void
  // 链接解析、上传、Markdown 解析（见 extensions/media.ts）
  media?: Partial<MediaServices>
  // 打开正文里的链接（⌘ 点、链接卡片上的“打开”、只读时点）：返回 true 表示宿主处理了，否则在新标签页打开
  onOpenLink?: (href: string) => boolean
}

export function OadEditor(props: OadEditorProps) {
  const { ydoc, provider, user, field = 'default', editable = true, autofocus = false, className, onReady } = props
  const awareness = provider?.awareness ?? null
  // 回调和作者表放 ref 里，变了不用重建编辑器
  const live = useRef(props)
  live.current = props
  // 浮层挂在哪（<OadPortalRoot>）：节点视图（大图预览）从扩展选项里取，也放 ref
  const portal = useRef<HTMLElement | null>(null)
  portal.current = useContext(OadPortalRoot)
  const extensions = useMemo(
    () =>
      editorExtensions({
        ydoc,
        field,
        awareness,
        user,
        onOpenThreads: (ids) => live.current.onOpenThreads?.(ids),
        authors: () => live.current.authors ?? {},
        onUnknownAuthor: (c) => live.current.onUnknownAuthor?.(c),
        media: () => live.current.media ?? {},
        portalRoot: () => portal.current,
        openLink: (href) => live.current.onOpenLink?.(href) ?? false,
      }),
    // user 变了走 updateUser，不重建编辑器
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ydoc, field, awareness],
  )
  const editor = useEditor(
    {
      extensions,
      editable,
      autofocus,
      immediatelyRender: true,
      shouldRerenderOnTransaction: false,
      editorProps: {
        attributes: { class: 'oad-prose', spellcheck: 'true', 'data-oad-editor': '' },
      },
    },
    [extensions],
  )

  useEffect(() => {
    if (editor && editor.isEditable !== editable) editor.setEditable(editable)
  }, [editor, editable])

  useEffect(() => {
    if (!editor || !awareness) return
    editor.commands.updateUser({ kind: 'user', ...user })
  }, [editor, awareness, user.id, user.name, user.color]) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (editor) onReady?.(editor)
  }, [editor]) // eslint-disable-line react-hooks/exhaustive-deps

  // 斜杠菜单要网址时弹的输入框
  const [prompt, setPrompt] = useState<UrlPromptState | null>(null)
  useEffect(() => {
    if (!editor) return
    const st = (editor.storage as unknown as { slashCommands?: SlashStorage }).slashCommands
    if (!st) return
    st.ui = {
      askUrl: (kind) =>
        new Promise((resolve) => {
          const c = editor.view.coordsAtPos(editor.state.selection.from)
          setPrompt({ kind, rect: { left: c.left, bottom: c.bottom }, resolve })
        }),
    }
  }, [editor])

  const inTable = useEditorState({ editor, selector: ({ editor: e }) => (e ? !!findCell(e.state) : false) })

  // 点正文下面的空白：光标放到最后；最后一块不是段落就补一个空段落
  const onTail = (e: React.MouseEvent) => {
    if (!editor || !editor.isEditable) return
    e.preventDefault()
    const { doc } = editor.state
    const last = doc.lastChild
    if (last && (last.type.name !== 'paragraph' || last.content.size > 0)) {
      const tr = editor.state.tr.insert(doc.content.size, editor.schema.nodes.paragraph.create())
      tr.setSelection(TextSelection.near(tr.doc.resolve(tr.doc.content.size - 1)))
      editor.view.dispatch(tr.scrollIntoView())
      editor.view.focus()
    } else {
      // 不用 commands.focus('end')：它等下一帧才设光标，紧接着的输入和选择会被它冲掉
      editor.view.dispatch(editor.state.tr.setSelection(Selection.atEnd(doc)).scrollIntoView())
      editor.view.focus()
    }
  }

  if (!editor) return null
  return (
    <div className={'oad-editor' + (className ? ' ' + className : '')}>
      <EditorContent editor={editor} />
      <div className="oad-editor-tail" onMouseDown={onTail} />
      <SlashMenu editor={editor} />
      <BubbleToolbar
        editor={editor}
        onComment={
          props.onComment
            ? () => {
                const a = selectionToAnchor(editor)
                if (!a) return
                // 选区收到结尾，格式菜单收起；正在写的这段由宿主用草稿高亮
                editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, editor.state.selection.to)))
                live.current.onComment?.(a)
              }
            : undefined
        }
      />
      <LinkCard editor={editor} open={(href) => openHref(href, (h) => live.current.onOpenLink?.(h) ?? false)} />
      <PasteMenu editor={editor} />
      <BlockMenu editor={editor} />
      {prompt && <UrlPrompt state={prompt} onClose={() => setPrompt(null)} />}
      {inTable && (
        <Suspense fallback={null}>
          <TableToolbar editor={editor} />
        </Suspense>
      )}
    </div>
  )
}
