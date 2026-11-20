// 选中文字时的格式菜单：粗体、斜体、下划线、删除线、行内代码、高亮、链接、行内公式。
import type { Editor } from '@tiptap/core'
import { useEditorState } from '@tiptap/react'
import { BubbleMenu } from '@tiptap/react/menus'
import { useEffect, useState } from 'react'
import type { KeysStorage } from '../extensions/keys'
import { t, type EditorStrings } from '../strings'
import { normalizeUrl } from './UrlPrompt'

const MARKS: { mark: string; label: keyof EditorStrings; glyph: React.ReactNode; key: string }[] = [
  { mark: 'bold', label: 'bold', glyph: <b>B</b>, key: '⌘B' },
  { mark: 'italic', label: 'italic', glyph: <i style={{ fontFamily: 'Georgia, serif' }}>I</i>, key: '⌘I' },
  { mark: 'underline', label: 'underline', glyph: <u>U</u>, key: '⌘U' },
  { mark: 'strike', label: 'strike', glyph: <s>S</s>, key: '⌘⇧S' },
  { mark: 'code', label: 'code', glyph: <span className="oad-glyph-code">{'</>'}</span>, key: '⌘E' },
  { mark: 'highlight', label: 'highlight', glyph: <span className="oad-glyph-hl">A</span>, key: '⌘⇧H' },
]

export function BubbleToolbar({ editor, onComment }: { editor: Editor; onComment?: () => void }) {
  const [link, setLink] = useState<string | null>(null)
  // ⌘K（选中了文字）：打开链接输入框
  useEffect(() => {
    const st = (editor.storage as unknown as { oadKeys?: KeysStorage }).oadKeys
    if (!st) return
    st.link = () => setLink((editor.getAttributes('link').href as string | undefined) ?? '')
    st.comment = onComment ?? null
    return () => {
      st.link = null
      st.comment = null
    }
  }, [editor, onComment])
  const s = useEditorState({
    editor,
    selector: ({ editor: e }) => {
      if (!e || e.state.selection.empty) return null
      const out: Record<string, boolean | string> = {}
      for (const m of MARKS) out[m.mark] = e.isActive(m.mark)
      out.link = (e.getAttributes('link').href as string | undefined) ?? ''
      return out
    },
  })

  const applyLink = () => {
    const href = normalizeUrl(link ?? '')
    const chain = editor.chain().focus().extendMarkRange('link')
    if (href) chain.setLink({ href }).run()
    else chain.unsetLink().run()
    setLink(null)
  }

  const toMath = () => {
    const { from, to } = editor.state.selection
    const latex = editor.state.doc.textBetween(from, to, ' ')
    if (!latex.trim()) return
    editor.chain().focus().insertContentAt({ from, to }, { type: 'mathInline', attrs: { latex } }).run()
  }

  return (
    <BubbleMenu
      editor={editor}
      className="oad-popover oad-bubble"
      shouldShow={({ editor: e, state, view, element }) => {
        if (!e.isEditable || state.selection.empty) return false
        if (e.isActive('codeBlock')) return false
        // 选中的是整块（图片、公式块）不弹
        if ((state.selection as { node?: unknown }).node) return false
        return view.hasFocus() || element.contains((element.getRootNode() as Document | ShadowRoot).activeElement)
      }}
      options={{ placement: 'top', offset: 8, onHide: () => setLink(null) }}
    >
      {link !== null ? (
        <span className="oad-bubble-link">
          <input
            autoFocus
            value={link}
            placeholder={t('linkPlaceholder')}
            onChange={(e) => setLink(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault()
                applyLink()
              }
              if (e.key === 'Escape') {
                e.preventDefault()
                setLink(null)
                editor.commands.focus()
              }
            }}
          />
          <button type="button" className="oad-btn-primary" onMouseDown={(e) => e.preventDefault()} onClick={applyLink}>
            {t('confirm')}
          </button>
        </span>
      ) : (
        <>
          {MARKS.map((m) => (
            <button
              key={m.mark}
              type="button"
              className="oad-bubble-btn"
              aria-pressed={!!s?.[m.mark]}
              title={`${t(m.label)}  ${m.key}`}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => editor.chain().focus().toggleMark(m.mark).run()}
            >
              {m.glyph}
            </button>
          ))}
          <span className="oad-bubble-sep" />
          <button
            type="button"
            className="oad-bubble-btn oad-bubble-wide"
            aria-pressed={!!s?.link}
            title={t('link')}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setLink((s?.link as string) || '')}
          >
            {t('link')}
          </button>
          {onComment && (
            <>
              <span className="oad-bubble-sep" />
              <button type="button" className="oad-bubble-btn oad-bubble-wide" title={t('comment')} onMouseDown={(e) => e.preventDefault()} onClick={onComment}>
                {t('comment')}
              </button>
            </>
          )}
          <button type="button" className="oad-bubble-btn" title={t('mathInline')} onMouseDown={(e) => e.preventDefault()} onClick={toMath}>
            <span style={{ fontFamily: 'KaTeX_Math, Georgia, serif', fontStyle: 'italic' }}>√x</span>
          </button>
        </>
      )}
    </BubbleMenu>
  )
}
