// 点块把手弹出的菜单：转换成…、链接卡片显示成…、复制块链接、复制一份、删除。
import type { Editor } from '@tiptap/core'
import { NodeSelection, TextSelection } from '@tiptap/pm/state'
import { useEffect, useRef, useState } from 'react'
import type { BlockHandleStorage, BlockTarget } from '../extensions/block-handle'
import { media } from '../extensions/media'
import { convertEmbed, type PasteChoice } from '../extensions/paste'
import { t, type EditorStrings } from '../strings'

const TURN: { key: string; label: keyof EditorStrings; icon: string; run: (e: Editor) => void }[] = [
  { key: 'p', label: 'block.paragraph', icon: 'T', run: (e) => e.chain().setParagraph().run() },
  { key: 'h1', label: 'block.h1', icon: 'H1', run: (e) => e.chain().setHeading({ level: 1 }).run() },
  { key: 'h2', label: 'block.h2', icon: 'H2', run: (e) => e.chain().setHeading({ level: 2 }).run() },
  { key: 'h3', label: 'block.h3', icon: 'H3', run: (e) => e.chain().setHeading({ level: 3 }).run() },
  { key: 'ul', label: 'block.bulletList', icon: '•', run: (e) => e.chain().toggleBulletList().run() },
  { key: 'ol', label: 'block.orderedList', icon: '1.', run: (e) => e.chain().toggleOrderedList().run() },
  { key: 'todo', label: 'block.taskList', icon: '☐', run: (e) => e.chain().toggleTaskList().run() },
  { key: 'quote', label: 'block.blockquote', icon: '❝', run: (e) => e.chain().toggleBlockquote().run() },
  { key: 'callout', label: 'block.callout', icon: 'i', run: (e) => e.chain().wrapIn('callout', { kind: 'note' }).run() },
  { key: 'code', label: 'block.codeBlock', icon: '</>', run: (e) => e.chain().setCodeBlock().run() },
]

const VIEWS: { key: PasteChoice; label: keyof EditorStrings }[] = [
  { key: 'embed', label: 'pasteAsEmbed' },
  { key: 'bookmark', label: 'pasteAsBookmark' },
  { key: 'mention', label: 'pasteAsMention' },
  { key: 'link', label: 'pasteAsLink' },
]

export function BlockMenu({ editor }: { editor: Editor }) {
  const [target, setTarget] = useState<BlockTarget | null>(null)
  const [done, setDone] = useState<string | null>(null)
  const box = useRef<HTMLDivElement>(null)
  const st = (editor.storage as unknown as { oadBlockHandle?: BlockHandleStorage }).oadBlockHandle

  useEffect(() => {
    if (!st) return
    st.open = (b) => {
      setTarget(b)
      setDone(null)
      st.locked = !!b
    }
    return () => {
      st.open = () => {}
      st.locked = false
    }
  }, [st])

  useEffect(() => {
    if (!target) return
    const close = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) shut()
    }
    const key = (e: KeyboardEvent) => e.key === 'Escape' && shut()
    document.addEventListener('mousedown', close, true)
    document.addEventListener('keydown', key, true)
    return () => {
      document.removeEventListener('mousedown', close, true)
      document.removeEventListener('keydown', key, true)
    }
  }, [target]) // eslint-disable-line react-hooks/exhaustive-deps

  const shut = () => {
    setTarget(null)
    if (st) st.locked = false
  }
  if (!target) return null
  const node = editor.state.doc.nodeAt(target.pos)
  if (!node) return null
  const isEmbed = node.type.name === 'embed'
  const textual = node.isTextblock || ['bulletList', 'orderedList', 'taskList', 'listItem', 'taskItem', 'blockquote'].includes(node.type.name)

  // 转换：把光标放进这一块再执行命令
  const turn = (run: (e: Editor) => void) => {
    const inside = Math.min(target.pos + 1, editor.state.doc.content.size)
    editor.view.dispatch(editor.state.tr.setSelection(TextSelection.near(editor.state.doc.resolve(inside))))
    run(editor)
    editor.view.focus()
    shut()
  }
  const copyLink = async () => {
    const id = node.attrs.id
    if (!id) return
    try {
      await navigator.clipboard.writeText(`${media(editor).pageUrl()}#${id}`)
      setDone('link')
      setTimeout(shut, 700)
    } catch {
      shut()
    }
  }
  const duplicate = () => {
    const tr = editor.state.tr.insert(target.pos + node.nodeSize, node)
    tr.setSelection(NodeSelection.create(tr.doc, target.pos + node.nodeSize))
    editor.view.dispatch(tr.scrollIntoView())
    shut()
  }
  const remove = () => {
    editor.view.dispatch(editor.state.tr.delete(target.pos, target.pos + node.nodeSize))
    editor.view.focus()
    shut()
  }

  const left = Math.max(8, Math.min(target.rect.left, window.innerWidth - 248))
  const below = window.innerHeight - target.rect.bottom
  const style: React.CSSProperties = below < 380 ? { left, top: target.rect.top - 6, transform: 'translateY(-100%)' } : { left, top: target.rect.bottom + 6 }

  return (
    <div className="oad-popover oad-block-menu" ref={box} style={style} role="menu" onMouseDown={(e) => e.preventDefault()}>
      {isEmbed && (
        <>
          <div className="oad-menu-label">{t('viewAs')}</div>
          <div className="oad-menu-seg">
            {VIEWS.map((v) => (
              <button
                key={v.key}
                type="button"
                role="menuitemradio"
                aria-checked={v.key === node.attrs.view}
                onClick={() => {
                  convertEmbed(editor, target.pos, v.key)
                  shut()
                }}
              >
                {t(v.label)}
              </button>
            ))}
          </div>
          <div className="oad-menu-sep" />
        </>
      )}
      {textual && (
        <>
          <div className="oad-menu-label">{t('turnInto')}</div>
          <div className="oad-menu-grid">
            {TURN.map((x) => (
              <button key={x.key} type="button" role="menuitem" title={t(x.label)} onClick={() => turn(x.run)}>
                <span className="oad-menu-icon">{x.icon}</span>
                <span>{t(x.label)}</span>
              </button>
            ))}
          </div>
          <div className="oad-menu-sep" />
        </>
      )}
      <button type="button" role="menuitem" className="oad-menu-item" onClick={copyLink} disabled={!node.attrs.id}>
        {done === 'link' ? t('copied') : t('copyBlockLink')}
      </button>
      <button type="button" role="menuitem" className="oad-menu-item" onClick={duplicate}>
        {t('duplicate')}
      </button>
      <button type="button" role="menuitem" className="oad-menu-item oad-is-danger" onClick={remove}>
        {t('deleteBlock')}
      </button>
    </div>
  )
}
