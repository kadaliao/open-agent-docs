// 表格的命令：插入、增删行列、Tab 跳格。表格没有合并单元格，第一行是表头（和 Markdown 一致）。
import { Extension, type CommandProps } from '@tiptap/core'
import type { Node as PMNode, NodeType, Schema } from '@tiptap/pm/model'
import { TextSelection, type EditorState } from '@tiptap/pm/state'

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    oadTable: {
      insertTable: (opts?: { rows?: number; cols?: number }) => ReturnType
      addRowBefore: () => ReturnType
      addRowAfter: () => ReturnType
      addColumnBefore: () => ReturnType
      addColumnAfter: () => ReturnType
      deleteRow: () => ReturnType
      deleteColumn: () => ReturnType
      deleteTable: () => ReturnType
      goToNextCell: () => ReturnType
      goToPreviousCell: () => ReturnType
    }
  }
}

export interface CellInfo {
  table: PMNode
  tablePos: number
  row: number
  col: number
}

export function findCell(state: EditorState): CellInfo | null {
  const { $from } = state.selection
  for (let d = $from.depth; d > 2; d--) {
    const n = $from.node(d)
    if (n.type.name === 'tableCell' || n.type.name === 'tableHeader') {
      const table = $from.node(d - 2)
      if (table.type.name !== 'table') return null
      return { table, tablePos: $from.before(d - 2), row: $from.index(d - 2), col: $from.index(d - 1) }
    }
  }
  return null
}

const cell = (type: NodeType, schema: Schema) => type.create(null, schema.nodes.paragraph.create())

function cellType(schema: Schema, row: number) {
  return row === 0 ? schema.nodes.tableHeader : schema.nodes.tableCell
}

// 第 r 行开头、第 c 格开头的位置
function rowPos(table: PMNode, tablePos: number, r: number) {
  let pos = tablePos + 1
  for (let i = 0; i < r; i++) pos += table.child(i).nodeSize
  return pos
}
function cellPos(table: PMNode, tablePos: number, r: number, c: number) {
  let pos = rowPos(table, tablePos, r) + 1
  const row = table.child(r)
  for (let i = 0; i < c; i++) pos += row.child(i).nodeSize
  return pos
}

function focusCell(tr: CommandProps['tr'], tablePos: number, r: number, c: number) {
  const table = tr.doc.nodeAt(tablePos)
  if (!table || r < 0 || r >= table.childCount) return
  const row = table.child(r)
  if (c < 0 || c >= row.childCount) return
  const pos = cellPos(table, tablePos, r, c)
  tr.setSelection(TextSelection.near(tr.doc.resolve(pos + 1)))
}

const addRow = (offset: 0 | 1) => ({ state, tr, dispatch }: CommandProps) => {
  const info = findCell(state)
  if (!info) return false
  // 表头上面不加行
  const at = Math.max(1, info.row + offset)
  if (dispatch) {
    const cols = info.table.child(Math.min(at, info.table.childCount - 1)).childCount
    const cells = Array.from({ length: cols }, () => cell(state.schema.nodes.tableCell, state.schema))
    const row = state.schema.nodes.tableRow.create(null, cells)
    const pos = at >= info.table.childCount ? info.tablePos + info.table.nodeSize - 1 : rowPos(info.table, info.tablePos, at)
    tr.insert(pos, row)
    focusCell(tr, info.tablePos, at, info.col)
  }
  return true
}

const addCol = (offset: 0 | 1) => ({ state, tr, dispatch }: CommandProps) => {
  const info = findCell(state)
  if (!info) return false
  if (dispatch) {
    const at = info.col + offset
    // 从后往前插，前面的位置不受影响
    for (let r = info.table.childCount - 1; r >= 0; r--) {
      const row = info.table.child(r)
      const c = Math.min(at, row.childCount)
      const pos = c >= row.childCount ? rowPos(info.table, info.tablePos, r) + row.nodeSize - 1 : cellPos(info.table, info.tablePos, r, c)
      tr.insert(pos, cell(cellType(state.schema, r), state.schema))
    }
    focusCell(tr, info.tablePos, info.row, at)
  }
  return true
}

export const TableCommands = Extension.create({
  name: 'oadTable',
  addCommands() {
    return {
      insertTable:
        (opts = {}) =>
        ({ state, tr, dispatch }) => {
          const rows = Math.max(2, opts.rows ?? 3)
          const cols = Math.max(1, opts.cols ?? 3)
          const s = state.schema
          const table = s.nodes.table.create(
            null,
            Array.from({ length: rows }, (_, r) =>
              s.nodes.tableRow.create(null, Array.from({ length: cols }, () => cell(cellType(s, r), s))),
            ),
          )
          if (dispatch) {
            const { $from } = tr.selection
            // 空段落里就替换它，否则插在当前块后面
            const parent = $from.parent
            let pos: number
            if (parent.isTextblock && parent.content.size === 0 && $from.depth > 0) {
              pos = $from.before()
              tr.replaceWith(pos, $from.after(), table)
            } else {
              pos = $from.depth > 0 ? $from.after(1) : $from.pos
              tr.insert(pos, table)
            }
            tr.setSelection(TextSelection.near(tr.doc.resolve(pos + 3)))
            tr.scrollIntoView()
          }
          return true
        },
      addRowBefore: () => addRow(0),
      addRowAfter: () => addRow(1),
      addColumnBefore: () => addCol(0),
      addColumnAfter: () => addCol(1),
      deleteRow:
        () =>
        ({ state, tr, dispatch, commands }) => {
          const info = findCell(state)
          if (!info) return false
          if (info.table.childCount <= 2 || info.row === 0) {
            // 表头不能单独删；只剩一行内容时删整张表
            if (info.row === 0) return false
            return commands.deleteTable()
          }
          if (dispatch) {
            const pos = rowPos(info.table, info.tablePos, info.row)
            tr.delete(pos, pos + info.table.child(info.row).nodeSize)
            focusCell(tr, info.tablePos, Math.min(info.row, info.table.childCount - 2), info.col)
          }
          return true
        },
      deleteColumn:
        () =>
        ({ state, tr, dispatch, commands }) => {
          const info = findCell(state)
          if (!info) return false
          if (info.table.child(0).childCount <= 1) return commands.deleteTable()
          if (dispatch) {
            for (let r = info.table.childCount - 1; r >= 0; r--) {
              const row = info.table.child(r)
              if (info.col >= row.childCount) continue
              const pos = cellPos(info.table, info.tablePos, r, info.col)
              tr.delete(pos, pos + row.child(info.col).nodeSize)
            }
            focusCell(tr, info.tablePos, info.row, Math.max(0, info.col - 1))
          }
          return true
        },
      deleteTable:
        () =>
        ({ state, tr, dispatch }) => {
          const info = findCell(state)
          if (!info) return false
          if (dispatch) {
            const p = state.schema.nodes.paragraph.create()
            tr.replaceWith(info.tablePos, info.tablePos + info.table.nodeSize, p)
            tr.setSelection(TextSelection.near(tr.doc.resolve(info.tablePos + 1)))
          }
          return true
        },
      goToNextCell:
        () =>
        ({ state, tr, dispatch, commands }) => {
          const info = findCell(state)
          if (!info) return false
          const row = info.table.child(info.row)
          if (info.col + 1 < row.childCount) {
            if (dispatch) focusCell(tr, info.tablePos, info.row, info.col + 1)
            return true
          }
          if (info.row + 1 < info.table.childCount) {
            if (dispatch) focusCell(tr, info.tablePos, info.row + 1, 0)
            return true
          }
          // 最后一格按 Tab：加一行
          return commands.addRowAfter() && commands.command(({ tr: t2 }) => (focusCell(t2, info.tablePos, info.row + 1, 0), true))
        },
      goToPreviousCell:
        () =>
        ({ state, tr, dispatch }) => {
          const info = findCell(state)
          if (!info) return false
          if (dispatch) {
            if (info.col > 0) focusCell(tr, info.tablePos, info.row, info.col - 1)
            else if (info.row > 0) focusCell(tr, info.tablePos, info.row - 1, info.table.child(info.row - 1).childCount - 1)
          }
          return true
        },
    }
  },
  addKeyboardShortcuts() {
    return {
      Tab: () => (findCell(this.editor.state) ? this.editor.commands.goToNextCell() : false),
      'Shift-Tab': () => (findCell(this.editor.state) ? this.editor.commands.goToPreviousCell() : false),
    }
  },
})
