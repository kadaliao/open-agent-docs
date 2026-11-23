// 链接卡片：编辑时光标停在链接里（没有选中文字）就在下面弹一张小卡片：网址（点了打开）、打开、改网址、去掉链接。
// 选中文字时是格式菜单（BubbleToolbar），两个不同时出现。
import type { Editor } from '@tiptap/core'
import { useEditorState } from '@tiptap/react'
import { BubbleMenu } from '@tiptap/react/menus'
import { useState } from 'react'
import { t } from '../strings'
import { normalizeUrl } from './UrlPrompt'

/** 卡片上显示的网址：去掉协议和结尾的斜杠，太长截掉中间 */
export function shortUrl(href: string) {
  const s = href.replace(/^https?:\/\//i, '').replace(/^mailto:/i, '').replace(/\/$/, '')
  return s.length > 48 ? s.slice(0, 30) + '…' + s.slice(-14) : s
}

export function LinkCard({ editor, open }: { editor: Editor; open: (href: string) => void }) {
  const [edit, setEdit] = useState<string | null>(null)
  const href = useEditorState({
    editor,
    selector: ({ editor: e }) => (e && e.isEditable && e.state.selection.empty && e.isActive('link') ? ((e.getAttributes('link').href as string | undefined) ?? '') : null),
  })
  const apply = () => {
    const h = normalizeUrl(edit ?? '')
    const chain = editor.chain().focus().extendMarkRange('link')
    if (h) chain.setLink({ href: h }).run()
    else chain.unsetLink().run()
    setEdit(null)
  }
  return (
    <BubbleMenu
      editor={editor}
      pluginKey="oadLinkCard"
      className="oad-popover oad-bubble oad-link-card"
      shouldShow={({ editor: e, state, view }) => e.isEditable && state.selection.empty && e.isActive('link') && view.hasFocus()}
      options={{ placement: 'bottom-start', offset: 6, onHide: () => setEdit(null) }}
    >
      {edit !== null ? (
        <span className="oad-bubble-link">
          <input
            autoFocus
            value={edit}
            placeholder={t('linkPlaceholder')}
            onChange={(e) => setEdit(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault()
                apply()
              }
              if (e.key === 'Escape') {
                e.preventDefault()
                setEdit(null)
                editor.commands.focus()
              }
            }}
          />
          <button type="button" className="oad-btn-primary" onMouseDown={(e) => e.preventDefault()} onClick={apply}>
            {t('confirm')}
          </button>
        </span>
      ) : (
        <>
          <a
            className="oad-link-card-url"
            href={href ?? ''}
            title={href ?? ''}
            onMouseDown={(e) => e.preventDefault()}
            onClick={(e) => {
              e.preventDefault()
              if (href) open(href)
            }}
          >
            {shortUrl(href ?? '')}
          </a>
          <span className="oad-bubble-sep" />
          <button type="button" className="oad-bubble-btn oad-bubble-wide" onMouseDown={(e) => e.preventDefault()} onClick={() => href && open(href)}>
            {t('openLink')}
          </button>
          <button type="button" className="oad-bubble-btn oad-bubble-wide" onMouseDown={(e) => e.preventDefault()} onClick={() => setEdit(href ?? '')}>
            {t('editLink')}
          </button>
          <button
            type="button"
            className="oad-bubble-btn oad-bubble-wide"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().extendMarkRange('link').unsetLink().run()}
          >
            {t('removeLink')}
          </button>
        </>
      )}
    </BubbleMenu>
  )
}
