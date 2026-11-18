// 表格视图：行、列都虚拟滚动（TanStack Virtual），名称列固定在左边；键盘导航、多格复制粘贴、拖列宽、拖列顺序、每列底部汇总、分组折叠。
import { useVirtualizer } from '@tanstack/react-virtual'
import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { CellEditor, CellView, cellValue, isEmptyVal, useDb } from './cells'
import { ColumnMenu, AddPropMenu } from './ColumnMenu'
import { useLocale } from '../i18n'
import { tryNormalize } from './engine'
import { t, TOTAL_NAMES } from './strings'
import type { Group, Prop, QueryResult, Row, View } from './types'
import { Pill, Popover, READONLY, rectOf, totalsFor, TypeIcon, type Anchor } from './ui'

const ROW_H = 34
const GROUP_H = 42
const HEAD_H = 34
const FOOT_H = 34

export function defaultWidth(p: Prop) {
  switch (p.type) {
    case 'title':
      return 280
    case 'checkbox':
      return 96
    case 'number':
    case 'unique_id':
      return 120
    case 'date':
    case 'created_time':
    case 'updated_time':
      return 170
    case 'text':
    case 'url':
    case 'relation':
    case 'multi_select':
      return 220
    default:
      return 160
  }
}

/** 视图里要显示的列（名称在第一列） */
export function visibleProps(props: Prop[], view: View) {
  const hidden = new Set(view.hidden ?? [])
  const order = view.order ?? []
  const pos = new Map(order.map((k, i) => [k, i]))
  const rest = props.filter((p) => p.type !== 'title')
  rest.sort((a, b) => (pos.get(a.key) ?? 1e6 + props.indexOf(a)) - (pos.get(b.key) ?? 1e6 + props.indexOf(b)))
  const title = props.find((p) => p.type === 'title')
  return [...(title ? [title] : []), ...rest.filter((p) => !hidden.has(p.key))]
}

type Item = { t: 'g'; g: Group } | { t: 'r'; id: string } | { t: 'add'; g?: Group }

interface Props {
  view: View
  result: QueryResult
  onView: (patch: Partial<View>) => void
  maxHeight?: number
}

export function TableView({ view, result, onView, maxHeight }: Props) {
  const { store, canWrite, openRow } = useDb()
  const schema = store.schema
  const cols = useMemo(() => visibleProps(schema.props, view), [schema, view])
  const [localW, setLocalW] = useState<Record<string, number>>({})
  const width = useCallback((p: Prop) => localW[p.key] ?? view.widths?.[p.key] ?? defaultWidth(p), [localW, view.widths])
  const collapsed = useMemo(() => new Set(view.collapsed ?? []), [view.collapsed])

  // 平铺成一列：分组标题、行、每组最后的“新建”
  const items: Item[] = useMemo(() => {
    if (!result.groups) return [...result.ids.map((id) => ({ t: 'r' as const, id })), { t: 'add' as const }]
    const out: Item[] = []
    for (const g of result.groups) {
      if (!g.count && view.type === 'table' && g.key === '') continue
      out.push({ t: 'g', g })
      if (!collapsed.has(g.key)) {
        for (const id of g.ids) out.push({ t: 'r', id })
        out.push({ t: 'add', g })
      }
    }
    return out
  }, [result, collapsed, view.type])
  const rowItems = useMemo(() => items.map((it, i) => [it, i] as const).filter(([it]) => it.t === 'r'), [items])
  const rowIndexOf = useMemo(() => new Map(rowItems.map(([it, i], k) => [i, k])), [rowItems])

  const scroll = useRef<HTMLDivElement>(null)
  // 第一帧只画看得见的（打开快）；画出来以后再多备几行，滚动时不白
  const [warm, setWarm] = useState(false)
  useEffect(() => {
    const id = requestAnimationFrame(() => setTimeout(() => setWarm(true), 0))
    return () => cancelAnimationFrame(id)
  }, [])
  const initialRect = useMemo(() => ({ width: window.innerWidth, height: maxHeight ?? window.innerHeight }), []) // eslint-disable-line react-hooks/exhaustive-deps
  const rv = useVirtualizer({
    count: items.length,
    getScrollElement: () => scroll.current,
    estimateSize: (i) => (items[i].t === 'g' ? GROUP_H : ROW_H),
    initialRect,
    overscan: warm ? 12 : 0,
    paddingStart: HEAD_H,
    paddingEnd: FOOT_H + 8,
  })
  useEffect(() => rv.measure(), [items, rv])
  const rest = useMemo(() => cols.slice(1), [cols])
  // 手机上名称列窄一点，别的列才露得出来
  const titleW = cols[0] ? (window.innerWidth < 760 ? Math.min(width(cols[0]), 168) : width(cols[0])) : 0
  const cv = useVirtualizer({
    horizontal: true,
    count: rest.length,
    getScrollElement: () => scroll.current,
    estimateSize: (i) => width(rest[i]),
    initialRect,
    overscan: warm ? 4 : 0,
    paddingStart: titleW,
  })
  useEffect(() => cv.measure(), [rest, localW, view.widths, cv])
  const vcols = cv.getVirtualItems()
  const totalW = titleW + rest.reduce((s, p) => s + width(p), 0) + 140

  // ---- 选中、编辑 ----
  const [sel, setSel] = useState<{ r: number; c: number } | null>(null)
  const [anchor, setAnchorSel] = useState<{ r: number; c: number } | null>(null)
  const [editing, setEditing] = useState<{ rowId: string; key: string; anchor: Anchor; initial?: string } | null>(null)
  const [colMenu, setColMenu] = useState<{ p: Prop; anchor: Anchor } | null>(null)
  const [addProp, setAddProp] = useState<Anchor | null>(null)
  const [totalMenu, setTotalMenu] = useState<{ p: Prop; anchor: Anchor } | null>(null)
  const [toast, setToast] = useState<string | null>(null)

  const range = useMemo(() => {
    if (!sel) return null
    const a = anchor ?? sel
    return { r0: Math.min(a.r, sel.r), r1: Math.max(a.r, sel.r), c0: Math.min(a.c, sel.c), c1: Math.max(a.c, sel.c) }
  }, [sel, anchor])

  const rowAt = (r: number) => {
    const it = rowItems[r]?.[0]
    return it && it.t === 'r' ? store.rows.get(it.id) : undefined
  }

  // 选中格子时焦点在一个看不见的输入框上（就在格子上面）：中文输入法要有能输入的地方，组字在它里面完成，打完再开编辑器
  const sink = useRef<HTMLTextAreaElement>(null)
  const [composing, setComposing] = useState<Anchor | null>(null)
  const focusGrid = () => (sink.current ?? scroll.current)?.focus({ preventScroll: true })
  const cellEl = (r: number, c: number) => scroll.current?.querySelector(`[data-cell="${r}:${c}"]`) as HTMLElement | null

  const reveal = (r: number, c: number) => {
    const itemIndex = rowItems[r]?.[1]
    if (itemIndex !== undefined) rv.scrollToIndex(itemIndex, { align: 'auto' })
    if (c > 0) cv.scrollToIndex(c - 1, { align: 'auto' })
  }

  const move = (dr: number, dc: number, extend = false) => {
    const cur = sel ?? { r: 0, c: 0 }
    const r = Math.max(0, Math.min(rowItems.length - 1, cur.r + dr))
    const c = Math.max(0, Math.min(cols.length - 1, cur.c + dc))
    if (extend) setAnchorSel(anchor ?? cur)
    else setAnchorSel(null)
    setSel({ r, c })
    reveal(r, c)
  }

  const startEdit = (r: number, c: number, initial?: string) => {
    const row = rowAt(r)
    const p = cols[c]
    if (!row || !p || !canWrite) return
    if (READONLY.has(p.type) || (p.type === 'relation' && p.relation?.mirror_of)) return
    if (p.type === 'checkbox') {
      store.setCells([{ row_id: row.id, key: p.key, value: !row.props[p.key] }])
      return
    }
    // 格子在屏幕上就马上开（双击后紧接着打字不会丢字）；不在就先滚过去，下一帧再开
    const now = cellEl(r, c)
    const box = scroll.current?.getBoundingClientRect()
    const rect = now?.getBoundingClientRect()
    if (now && box && rect && rect.top >= box.top + HEAD_H && rect.bottom <= box.bottom - FOOT_H && rect.left >= box.left && rect.right <= box.right) {
      setEditing({ rowId: row.id, key: p.key, anchor: rectOf(now), initial })
      return
    }
    reveal(r, c)
    requestAnimationFrame(() => {
      const el = cellEl(r, c)
      if (!el) return
      setEditing({ rowId: row.id, key: p.key, anchor: rectOf(el), initial })
    })
  }

  const doneEdit = (dir?: 'down' | 'right' | 'left' | 'none') => {
    setEditing(null)
    focusGrid()
    if (dir === 'down') move(1, 0)
    else if (dir === 'right') move(0, 1)
    else if (dir === 'left') move(0, -1)
  }

  // ---- 复制粘贴：选中的格子 ↔ 制表符分隔的文字 ----
  const copy = (e: ClipboardEvent) => {
    if (!range || editing) return
    const lines: string[] = []
    for (let r = range.r0; r <= range.r1; r++) {
      const row = rowAt(r)
      if (!row) continue
      const cells: string[] = []
      for (let c = range.c0; c <= range.c1; c++) {
        const p = cols[c]
        const v = cellValue(store, p, row)
        cells.push(p.type === 'person' ? String(v ?? '') : (store.engine?.display(p.key, v) ?? '').replace(/[\t\n]/g, ' '))
      }
      lines.push(cells.join('\t'))
    }
    e.clipboardData?.setData('text/plain', lines.join('\n'))
    e.preventDefault()
    const n = (range.r1 - range.r0 + 1) * (range.c1 - range.c0 + 1)
    setToast(t('copyCells', { n }))
    setTimeout(() => setToast(null), 1400)
  }
  const paste = (e: ClipboardEvent) => {
    if (!sel || editing || !canWrite) return
    const text = e.clipboardData?.getData('text/plain')
    if (!text) return
    e.preventDefault()
    const grid = text.replace(/\r/g, '').replace(/\n$/, '').split('\n').map((l) => l.split('\t'))
    const start = range ? { r: range.r0, c: range.c0 } : sel
    // 只贴一个值、选了一片：每格都填它
    const fill = grid.length === 1 && grid[0].length === 1 && range && (range.r1 > range.r0 || range.c1 > range.c0)
    const ops: { row_id: string; key: string; value: unknown }[] = []
    let bad = 0
    const rows = fill ? range!.r1 - range!.r0 + 1 : grid.length
    const colsN = fill ? range!.c1 - range!.c0 + 1 : Math.max(...grid.map((g) => g.length))
    for (let i = 0; i < rows; i++) {
      const row = rowAt(start.r + i)
      if (!row) break
      for (let j = 0; j < colsN; j++) {
        const p = cols[start.c + j]
        if (!p || READONLY.has(p.type)) continue
        const raw = fill ? grid[0][0] : grid[i]?.[j]
        if (raw === undefined) continue
        if (p.type === 'title' || p.type === 'text') {
          ops.push({ row_id: row.id, key: p.key, value: raw.trim() || null })
          continue
        }
        // 认不出的格子跳过
        const n = tryNormalize(store.engine, p.key, raw.trim())
        if ('value' in n) ops.push({ row_id: row.id, key: p.key, value: n.value })
        else bad++
      }
    }
    store.setCells(ops)
    if (bad) {
      setToast(t('pasteSkipped', { n: bad }))
      setTimeout(() => setToast(null), 2400)
    }
  }
  useEffect(() => {
    const el = scroll.current
    if (!el) return
    const mine = () => document.activeElement === el || document.activeElement === sink.current
    const c = (e: ClipboardEvent) => mine() && copy(e)
    const p = (e: ClipboardEvent) => mine() && paste(e)
    document.addEventListener('copy', c)
    document.addEventListener('paste', p)
    return () => {
      document.removeEventListener('copy', c)
      document.removeEventListener('paste', p)
    }
  })

  const onKey = (e: React.KeyboardEvent) => {
    if (editing || e.nativeEvent.isComposing) return
    const k = e.key
    if (k === 'ArrowDown' || k === 'ArrowUp' || k === 'ArrowLeft' || k === 'ArrowRight') {
      e.preventDefault()
      const big = e.metaKey || e.ctrlKey
      const dr = k === 'ArrowDown' ? (big ? 1e9 : 1) : k === 'ArrowUp' ? (big ? -1e9 : -1) : 0
      const dc = k === 'ArrowRight' ? (big ? 1e9 : 1) : k === 'ArrowLeft' ? (big ? -1e9 : -1) : 0
      move(dr, dc, e.shiftKey)
    } else if (k === 'Tab') {
      e.preventDefault()
      move(0, e.shiftKey ? -1 : 1)
    } else if (k === 'Enter' && sel) {
      e.preventDefault()
      if (e.metaKey || e.ctrlKey) {
        const row = rowAt(sel.r)
        if (row) openRow(row.id)
      } else startEdit(sel.r, sel.c)
    } else if (k === 'Escape') {
      setAnchorSel(null)
    } else if ((k === 'Backspace' || k === 'Delete') && range && canWrite) {
      e.preventDefault()
      const ops: { row_id: string; key: string; value: unknown }[] = []
      for (let r = range.r0; r <= range.r1; r++)
        for (let c = range.c0; c <= range.c1; c++) {
          const row = rowAt(r)
          const p = cols[c]
          if (row && p && !READONLY.has(p.type) && p.type !== 'title') ops.push({ row_id: row.id, key: p.key, value: null })
        }
      store.setCells(ops)
    } else if (k === ' ' && sel && cols[sel.c]?.type === 'checkbox') {
      e.preventDefault()
      startEdit(sel.r, sel.c)
    } else if (sel && k.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault()
      startEdit(sel.r, sel.c, k)
    }
  }

  // ---- 拖列宽、拖列顺序 ----
  const resize = (p: Prop) => (e: React.PointerEvent) => {
    e.preventDefault()
    e.stopPropagation()
    const x0 = e.clientX
    const w0 = width(p)
    const el = e.currentTarget as HTMLElement
    el.setPointerCapture(e.pointerId)
    let w = w0
    const mv = (ev: PointerEvent) => {
      w = Math.max(64, Math.min(800, w0 + ev.clientX - x0))
      setLocalW((m) => ({ ...m, [p.key]: w }))
    }
    const up = () => {
      el.removeEventListener('pointermove', mv)
      el.removeEventListener('pointerup', up)
      if (w !== w0) onView({ widths: { ...(view.widths ?? {}), [p.key]: Math.round(w) } })
      setTimeout(() => setLocalW((m) => Object.fromEntries(Object.entries(m).filter(([k]) => k !== p.key))), 0)
    }
    el.addEventListener('pointermove', mv)
    el.addEventListener('pointerup', up)
  }
  const [dragCol, setDragCol] = useState<string | null>(null)
  const [dropCol, setDropCol] = useState<string | null>(null)
  const dropOn = (target: Prop) => {
    if (!dragCol || dragCol === target.key || target.type === 'title') return
    const keys = visibleProps(schema.props, { ...view, hidden: [] })
      .filter((p) => p.type !== 'title')
      .map((p) => p.key)
    const from = keys.indexOf(dragCol)
    const to = keys.indexOf(target.key)
    keys.splice(from, 1)
    keys.splice(to, 0, dragCol)
    onView({ order: keys })
  }

  const headCell = (p: Prop, idx: number, style: React.CSSProperties) => (
    <div
      key={p.key}
      className={'db-th' + (p.type === 'title' ? ' is-sticky' : '') + (dropCol === p.key ? ' is-drop' : '')}
      style={style}
      draggable={canWrite && p.type !== 'title'}
      onDragStart={(e) => {
        setDragCol(p.key)
        e.dataTransfer.effectAllowed = 'move'
      }}
      onDragOver={(e) => {
        if (dragCol) {
          e.preventDefault()
          setDropCol(p.key)
        }
      }}
      onDragLeave={() => setDropCol(null)}
      onDrop={() => {
        dropOn(p)
        setDragCol(null)
        setDropCol(null)
      }}
      onDragEnd={() => {
        setDragCol(null)
        setDropCol(null)
      }}
      onClick={(e) => setColMenu({ p, anchor: rectOf(e.currentTarget) })}
      data-col={idx}
    >
      <TypeIcon type={p.type} />
      <span className="db-th-name">{p.name}</span>
      {view.sorts?.some((s) => s.prop === p.key) && <span className="db-th-sort">{view.sorts.find((s) => s.prop === p.key)!.dir === 'asc' ? '↑' : '↓'}</span>}
      {canWrite && <span className="db-th-resize" onPointerDown={resize(p)} onClick={(e) => e.stopPropagation()} />}
    </div>
  )

  // 行组件拿到的回调保持不变（行用 memo），里面再调最新的逻辑
  const h = useRef({ down: (_r: number, _c: number, _e: React.MouseEvent) => {}, edit: (_r: number, _c: number) => {} })
  h.current = {
    down: (r, c, ev) => {
      if (ev.shiftKey && sel) {
        setAnchorSel(anchor ?? sel)
        setSel({ r, c })
      } else {
        setAnchorSel(null)
        setSel({ r, c })
      }
      // 按下的默认动作会把焦点给外面的表格，接键盘的输入框要在它之后拿焦点
      if ((ev.target as HTMLElement).closest('a, button, input')) return
      ev.preventDefault()
      focusGrid()
    },
    edit: (r, c) => startEdit(r, c),
  }
  const onCellDown = useCallback((r: number, c: number, e: React.MouseEvent) => h.current.down(r, c, e), [])
  const onCellEdit = useCallback((r: number, c: number) => h.current.edit(r, c), [])

  const visibleCols = useMemo(() => vcols.map((v) => ({ v, p: rest[v.index] })), [vcols.map((v) => v.index).join(','), rest]) // eslint-disable-line react-hooks/exhaustive-deps
  const leftPad = vcols.length ? vcols[0].start - titleW : 0

  const totals = result.totals ?? {}
  const empty = !result.ids.length

  return (
    <div className="db-table-wrap" style={maxHeight ? { maxHeight } : undefined}>
      <div
        ref={scroll}
        className="db-table"
        tabIndex={0}
        onKeyDown={onKey}
        role="grid"
        aria-rowcount={result.ids.length}
        aria-colcount={cols.length}
        style={maxHeight ? { maxHeight } : undefined}
      >
        <div className="db-table-inner" style={{ width: totalW }}>
          {/* 表头 */}
          <div className="db-thead" style={{ width: totalW }}>
            {cols[0] && headCell(cols[0], 0, { width: titleW })}
            <div style={{ width: leftPad, flex: 'none' }} />
            {visibleCols.map(({ v, p }) => headCell(p, v.index + 1, { width: width(p) }))}
            {canWrite && (
              <button type="button" className="db-th db-th-add" onClick={(e) => setAddProp(rectOf(e.currentTarget))} aria-label={t('addProp')}>
                +
              </button>
            )}
          </div>
          {/* 行 */}
          {rv.getVirtualItems().map((vi) => {
            const it = items[vi.index]
            if (it.t === 'g')
              return (
                <GroupRow
                  key={'g' + it.g.key}
                  g={it.g}
                  top={vi.start}
                  width={totalW}
                  collapsed={collapsed.has(it.g.key)}
                  prop={store.prop(view.group_by ?? '')}
                  onToggle={() => onView({ collapsed: collapsed.has(it.g.key) ? [...collapsed].filter((k) => k !== it.g.key) : [...collapsed, it.g.key] })}
                />
              )
            if (it.t === 'add')
              return canWrite ? (
                <button
                  key={'add' + (it.g?.key ?? '')}
                  type="button"
                  className="db-add-row"
                  style={{ top: vi.start, width: totalW }}
                  onClick={() => store.addRow(groupDefaults(store, view, it.g))}
                >
                  <span className="db-add-in">
                    <span className="db-add-plus">+</span>
                    {t('newRow')}
                  </span>
                </button>
              ) : null
            const row = store.rows.get(it.id)
            if (!row) return null
            const r = rowIndexOf.get(vi.index)!
            return (
              <TableRow
                key={it.id}
                row={row}
                r={r}
                top={vi.start}
                cols={cols}
                visibleCols={visibleCols}
                leftPad={leftPad}
                titleW={titleW}
                width={width}
                totalW={totalW}
                sel={sel}
                range={range}
                version={row}
                onCell={onCellDown}
                onEdit={onCellEdit}
              />
            )
          })}
          {/* 行都是绝对定位的，这里占住它们的高度；汇总行跟在后面、贴着底边 */}
          <div style={{ height: Math.max(0, rv.getTotalSize() - HEAD_H - FOOT_H - 8) }} />
          {/* 汇总 */}
          <div className="db-tfoot" style={{ width: totalW }}>
            {cols[0] && (
              <div className="db-tf is-sticky" style={{ width: titleW }}>
                <span className="db-tf-count">{t('rows', { n: result.ids.length })}</span>
              </div>
            )}
            <div style={{ width: leftPad, flex: 'none' }} />
            {visibleCols.map(({ p }) => (
              <button
                type="button"
                key={p.key}
                className={'db-tf' + (view.totals?.[p.key] ? ' is-set' : '')}
                style={{ width: width(p) }}
                onClick={(e) => canWrite && setTotalMenu({ p, anchor: rectOf(e.currentTarget) })}
              >
                {view.totals?.[p.key] ? (
                  <>
                    <span className="db-tf-label">{TOTAL_NAMES[view.totals[p.key]]}</span>
                    <span className="db-tf-val">{fmtTotal(store, p, view.totals[p.key], totals[p.key])}</span>
                  </>
                ) : (
                  <span className="db-tf-hint">{t('calc')}</span>
                )}
              </button>
            ))}
          </div>
        </div>
        {empty && <div className="db-empty">{t('emptyDb')}</div>}
        <textarea
          ref={sink}
          className={'db-ime-sink' + (composing ? ' is-on' : '')}
          style={composing ? { left: composing.left, top: composing.top, width: composing.width } : undefined}
          aria-hidden
          tabIndex={-1}
          autoComplete="off"
          spellCheck={false}
          onCompositionStart={() => {
            const el = sel && cellEl(sel.r, sel.c)
            if (!sel || !canWrite || !el) return
            setComposing(rectOf(el))
          }}
          onCompositionEnd={(e) => {
            const text = (e.target as HTMLTextAreaElement).value
            ;(e.target as HTMLTextAreaElement).value = ''
            setComposing(null)
            if (sel && text) startEdit(sel.r, sel.c, text)
          }}
          onInput={(e) => {
            // 没经过组字直接进来的文字（有的输入法、手机键盘）
            const el = e.target as HTMLTextAreaElement
            if ((e.nativeEvent as InputEvent).isComposing || composing) return
            const text = el.value
            el.value = ''
            if (sel && text) startEdit(sel.r, sel.c, text)
          }}
        />
      </div>
      {editing &&
        (() => {
          const row = store.rows.get(editing.rowId)
          const p = store.prop(editing.key)
          return row && p ? <CellEditor p={p} row={row} anchor={editing.anchor} initial={editing.initial} onDone={doneEdit} /> : null
        })()}
      {colMenu && <ColumnMenu p={colMenu.p} anchor={colMenu.anchor} view={view} onView={onView} onClose={() => setColMenu(null)} />}
      {addProp && <AddPropMenu anchor={addProp} view={view} onView={onView} onClose={() => setAddProp(null)} />}
      {totalMenu && (
        <Popover anchor={totalMenu.anchor} onClose={() => setTotalMenu(null)} width={180} className="db-menu">
          <button
            type="button"
            className="db-menu-item"
            onClick={() => {
              const next = { ...(view.totals ?? {}) }
              delete next[totalMenu.p.key]
              onView({ totals: next })
              setTotalMenu(null)
            }}
          >
            {t('totalsNone')}
          </button>
          {totalsFor(totalMenu.p).map((fn) => (
            <button
              key={fn}
              type="button"
              className={'db-menu-item' + (view.totals?.[totalMenu.p.key] === fn ? ' is-on' : '')}
              onClick={() => {
                onView({ totals: { ...(view.totals ?? {}), [totalMenu.p.key]: fn } })
                setTotalMenu(null)
              }}
            >
              {TOTAL_NAMES[fn]}
            </button>
          ))}
        </Popover>
      )}
      {toast && <div className="db-toast">{toast}</div>}
    </div>
  )
}

function fmtTotal(store: ReturnType<typeof useDb>['store'], p: Prop, fn: string, v: unknown) {
  if (v === null || v === undefined) return '—'
  if (fn.startsWith('percent')) return `${Math.round((v as number) * 1000) / 10}%`
  if (['count', 'count_values', 'count_empty', 'count_unique', 'checked', 'unchecked'].includes(fn)) return String(v)
  if (typeof v === 'number' && (p.type === 'number' || p.type === 'formula' || p.type === 'rollup')) return store.engine?.display(p.key, Math.round(v * 100) / 100) || String(v)
  if (typeof v === 'object') return store.engine?.display(p.key, v) ?? ''
  return String(v)
}

/** 在某个分组里新建：带上这个分组的值 */
export function groupDefaults(store: ReturnType<typeof useDb>['store'], view: View, g?: Group): Record<string, unknown> {
  const props: Record<string, unknown> = {}
  // 筛选里“是某值”的条件也带上，免得新行一建就被筛掉
  for (const it of view.filter?.op === 'and' ? view.filter.items : []) {
    if ('prop' in it && (it.cond === 'eq' || it.cond === 'contains') && it.value !== undefined && !READONLY.has(store.prop(it.prop)?.type ?? 'formula')) {
      const p = store.prop(it.prop)!
      props[it.prop] = p.type === 'multi_select' || p.type === 'person' ? [it.value] : p.type === 'checkbox' ? true : it.value
    }
  }
  if (g && view.group_by) {
    const p = store.prop(view.group_by)
    if (p && !READONLY.has(p.type) && g.key) {
      props[p.key] = p.type === 'multi_select' || p.type === 'person' ? [g.key] : p.type === 'checkbox' ? g.key === 'true' : p.type === 'select' || p.type === 'status' ? g.key : g.key
    }
  }
  return props
}

function GroupRow({ g, top, width, collapsed, prop, onToggle }: { g: Group; top: number; width: number; collapsed: boolean; prop?: Prop; onToggle: () => void }) {
  return (
    <div className="db-group-row" style={{ top, width }}>
      <div className="db-group-in">
      <button type="button" className={'db-group-toggle' + (collapsed ? '' : ' is-open')} onClick={onToggle} aria-label="toggle" aria-expanded={!collapsed}>
        <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden>
          <path d="M6 3.5 10.5 8 6 12.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {g.key === '' ? <span className="db-group-none">{t('none')}</span> : prop?.options ? <Pill opt={{ name: g.name, color: (g.color as never) ?? 'default' }} status={prop.type === 'status'} /> : <span className="db-group-name">{g.name}</span>}
      <span className="db-group-count">{g.count}</span>
      </div>
    </div>
  )
}

interface RowProps {
  row: Row
  r: number
  top: number
  cols: Prop[]
  visibleCols: { v: { index: number; start: number }; p: Prop }[]
  leftPad: number
  titleW: number
  width: (p: Prop) => number
  totalW: number
  sel: { r: number; c: number } | null
  range: { r0: number; r1: number; c0: number; c1: number } | null
  version: unknown
  onCell: (r: number, c: number, e: React.MouseEvent) => void
  onEdit: (r: number, c: number) => void
}

const TableRow = memo(
  function TableRow({ row, r, top, cols, visibleCols, leftPad, titleW, width, totalW, sel, range, onCell, onEdit }: RowProps) {
    const { openRow } = useDb()
    useLocale()
    const cls = (c: number) => {
      let s = 'db-td'
      if (sel && sel.r === r && sel.c === c) s += ' is-active'
      if (range && r >= range.r0 && r <= range.r1 && c >= range.c0 && c <= range.c1 && (range.r1 > range.r0 || range.c1 > range.c0)) s += ' is-range'
      return s
    }
    const title = cols[0]
    return (
      <div className="db-tr" style={{ top, width: totalW }} role="row">
        {title && (
          <div className={cls(0) + ' is-sticky is-title'} style={{ width: titleW }} data-cell={`${r}:0`} onMouseDown={(e) => onCell(r, 0, e)} onDoubleClick={() => onEdit(r, 0)}>
            <CellView p={title} row={row} />
            <button
              type="button"
              className="db-open"
              onMouseDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation()
                openRow(row.id)
              }}
            >
              {t('openRow')}
            </button>
          </div>
        )}
        <div style={{ width: leftPad, flex: 'none' }} />
        {visibleCols.map(({ v, p }) => {
          const c = v.index + 1
          return (
            <div
              key={p.key}
              className={cls(c) + (p.type === 'number' || p.type === 'unique_id' ? ' is-num' : '') + (isEmptyVal(row.props[p.key]) ? ' is-empty' : '')}
              style={{ width: width(p) }}
              data-cell={`${r}:${c}`}
              onMouseDown={(e) => onCell(r, c, e)}
              onDoubleClick={() => onEdit(r, c)}
              onClick={(e) => {
                // 勾选、按钮单击就生效
                if (p.type === 'checkbox' && (e.target as HTMLElement).classList.contains('db-check')) onEdit(r, c)
              }}
            >
              <CellView p={p} row={row} />
            </div>
          )
        })}
      </div>
    )
  },
  (a, b) =>
    a.row === b.row &&
    a.top === b.top &&
    a.cols === b.cols &&
    a.visibleCols === b.visibleCols &&
    a.leftPad === b.leftPad &&
    a.width === b.width &&
    a.r === b.r &&
    (a.sel?.r === a.r) === (b.sel?.r === b.r) &&
    (a.sel?.r !== a.r || a.sel?.c === b.sel?.c) &&
    a.range === b.range &&
    a.onCell === b.onCell,
)
