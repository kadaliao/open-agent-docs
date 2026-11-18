// 空行里贴网址以后弹出的小菜单：嵌入 / 书签 / 提及 / 链接。回车 = 嵌入（默认），方向键选，Esc 收起。
import type { Editor } from '@tiptap/core'
import { useEffect, useRef, useState } from 'react'
import { closePasteMenu, convertEmbed, type PasteChoice, type PasteMenuState, type PasteStorage } from '../extensions/paste'
import { t, type EditorStrings } from '../strings'

const ITEMS: { key: PasteChoice; label: keyof EditorStrings; icon: string }[] = [
  { key: 'embed', label: 'pasteAsEmbed', icon: '▣' },
  { key: 'bookmark', label: 'pasteAsBookmark', icon: '▭' },
  { key: 'mention', label: 'pasteAsMention', icon: '@' },
  { key: 'link', label: 'pasteAsLink', icon: '↗' },
]

export function PasteMenu({ editor }: { editor: Editor }) {
  const [state, setState] = useState<PasteMenuState | null>(null)
  const [cur, setCur] = useState(0)
  const ref = useRef({ state, cur })
  ref.current = { state, cur }

  useEffect(() => {
    const st = (editor.storage as unknown as { oadPaste?: PasteStorage }).oadPaste
    if (!st) return
    st.show = (s) => {
      setState(s)
      setCur(0)
    }
    st.key = (e) => {
      const { state: s, cur: c } = ref.current
      if (!s) return false
      if (e.key === 'ArrowDown' || e.key === 'ArrowRight') {
        setCur((c + 1) % ITEMS.length)
        return true
      }
      if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') {
        setCur((c - 1 + ITEMS.length) % ITEMS.length)
        return true
      }
      if (e.key === 'Enter') {
        pick(ITEMS[c].key)
        return true
      }
      return false
    }
    return () => {
      st.show = () => {}
      st.key = () => false
    }
  }, [editor]) // eslint-disable-line react-hooks/exhaustive-deps

  const pick = (k: PasteChoice) => {
    const s = ref.current.state
    if (!s) return
    closePasteMenu(editor)
    if (k !== 'embed') convertEmbed(editor, s.pos, k)
    else {
      // 嵌入就是现在的样子：光标挪到卡片后面接着写
      const after = s.pos + (editor.state.doc.nodeAt(s.pos)?.nodeSize ?? 1)
      editor.chain().focus().setTextSelection(Math.min(after + 1, editor.state.doc.content.size)).run()
    }
  }

  if (!state) return null
  const dom = editor.view.nodeDOM(state.pos) as HTMLElement | null
  if (!dom) return null
  const r = dom.getBoundingClientRect()
  const top = Math.min(r.top + 8, window.innerHeight - 60)
  return (
    <div className="oad-popover oad-paste-menu" style={{ left: Math.max(8, r.left), top: Math.max(8, top - 46) }} role="menu" onMouseDown={(e) => e.preventDefault()}>
      {ITEMS.map((it, i) => (
        <button key={it.key} type="button" role="menuitem" data-active={i === cur} onMouseMove={() => i !== cur && setCur(i)} onClick={() => pick(it.key)}>
          <span className="oad-paste-icon">{it.icon}</span>
          {t(it.label)}
        </button>
      ))}
      <span className="oad-paste-hint">{t('pasteHint')}</span>
    </div>
  )
}
