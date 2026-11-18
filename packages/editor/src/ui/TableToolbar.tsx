// 光标在表格里时，表格上方的一排按钮。单独打包，用到表格才加载。
import type { Editor } from '@tiptap/core'
import { useEditorState } from '@tiptap/react'
import { useLayoutEffect, useState } from 'react'
import { findCell } from '../schema/tables'
import { t, type EditorStrings } from '../strings'

const ACTIONS: { key: keyof EditorStrings; run: (e: Editor) => boolean; danger?: boolean }[] = [
  { key: 'addRowAbove', run: (e) => e.chain().focus().addRowBefore().run() },
  { key: 'addRowBelow', run: (e) => e.chain().focus().addRowAfter().run() },
  { key: 'addColLeft', run: (e) => e.chain().focus().addColumnBefore().run() },
  { key: 'addColRight', run: (e) => e.chain().focus().addColumnAfter().run() },
  { key: 'deleteRow', run: (e) => e.chain().focus().deleteRow().run(), danger: true },
  { key: 'deleteCol', run: (e) => e.chain().focus().deleteColumn().run(), danger: true },
  { key: 'deleteTable', run: (e) => e.chain().focus().deleteTable().run(), danger: true },
]

export default function TableToolbar({ editor }: { editor: Editor }) {
  const tablePos = useEditorState({ editor, selector: ({ editor: e }) => (e ? (findCell(e.state)?.tablePos ?? null) : null) })
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null)

  useLayoutEffect(() => {
    if (tablePos === null) return setPos(null)
    const place = () => {
      const dom = editor.view.nodeDOM(tablePos) as HTMLElement | null
      if (!dom) return setPos(null)
      const r = dom.getBoundingClientRect()
      // 贴在表格右下角外面，不挡标题和表格内容
      setPos({ left: r.right, top: r.bottom })
    }
    place()
    // 加了行列、表格变高变宽也要跟着挪
    editor.on('update', place)
    window.addEventListener('scroll', place, true)
    window.addEventListener('resize', place)
    return () => {
      editor.off('update', place)
      window.removeEventListener('scroll', place, true)
      window.removeEventListener('resize', place)
    }
  }, [editor, tablePos])

  if (!pos) return null
  return (
    <div className="oad-popover oad-table-bar" style={{ left: pos.left, top: pos.top + 6 }} onMouseDown={(e) => e.preventDefault()}>
      {ACTIONS.map((a) => (
        <button key={a.key} type="button" className={'oad-table-btn' + (a.danger ? ' is-danger' : '')} onClick={() => a.run(editor)}>
          {t(a.key)}
        </button>
      ))}
    </div>
  )
}
