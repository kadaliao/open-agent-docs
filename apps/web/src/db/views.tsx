// 看板、日历、画廊、列表四种视图。
import { useVirtualizer } from '@tanstack/react-virtual'
import { useEffect, useMemo, useRef, useState } from 'react'
import { api } from '../api'
import { CellView, cellValue, isEmptyVal, useDb } from './cells'
import { groupDefaults, visibleProps } from './TableView'
import { t } from './strings'
import type { DateValue, Group, Prop, QueryResult, Row, View } from './types'
import { Pill, READONLY, TypeIcon } from './ui'

interface VProps {
  view: View
  result: QueryResult
  onView: (patch: Partial<View>) => void
  maxHeight?: number
}

// 光看值不知道是什么的类型（数字、日期、勾选……）：卡片上值前面带上属性名
const LABELED = new Set<Prop['type']>(['number', 'formula', 'rollup', 'date', 'created_time', 'updated_time', 'checkbox', 'unique_id', 'url', 'email', 'phone', 'text'])
function PropLabel({ p }: { p: Prop }) {
  return (
    <span className="db-card-label">
      <TypeIcon type={p.type} />
      <span className="db-card-label-text">{p.name}</span>
    </span>
  )
}

/** 卡片上显示的属性：视图里没隐藏、有值的 */
function cardProps(props: Prop[], view: View) {
  return visibleProps(props, view).filter((p) => p.type !== 'title' && p.type !== 'button')
}

function Card({ row, props, onOpen, cover, draggable, onDragStart }: { row: Row; props: Prop[]; onOpen: () => void; cover?: string | null; draggable?: boolean; onDragStart?: (e: React.DragEvent) => void }) {
  const { store } = useDb()
  const shown = props.filter((p) => !isEmptyVal(cellValue(store, p, row)))
  return (
    <div className="db-card" role="button" tabIndex={0} draggable={draggable} onDragStart={onDragStart} onClick={onOpen} onKeyDown={(e) => e.key === 'Enter' && onOpen()} data-row={row.id}>
      {cover !== undefined && <div className="db-card-cover">{cover ? <img src={cover} alt="" loading="lazy" decoding="async" referrerPolicy="no-referrer" /> : <span className="db-card-nocover" />}</div>}
      <div className="db-card-body">
        <div className={'db-card-title' + (row.props.title ? '' : ' is-empty')}>{(row.props.title as string) || t('untitled')}</div>
        {shown.map((p) => (
          <div key={p.key} className={'db-card-prop is-' + p.type + (LABELED.has(p.type) ? ' has-label' : '')} title={p.name}>
            {LABELED.has(p.type) && <PropLabel p={p} />}
            <span className="db-card-val">
              <CellView p={p} row={row} />
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

// ---------------- 看板 ----------------
const PAGE = 60

export function BoardView({ view, result }: VProps) {
  const { store, canWrite, openRow } = useDb()
  const groupProp = store.prop(view.group_by ?? '')
  // 卡片上不再重复显示分列用的那个属性
  const props = useMemo(() => cardProps(store.schema.props, view).filter((p) => p.key !== view.group_by), [store.schema, view])
  const [drag, setDrag] = useState<{ id: string; from: string } | null>(null)
  const [over, setOver] = useState<{ col: string; before: string | null } | null>(null)
  const [limit, setLimit] = useState<Record<string, number>>({})
  // 列内拖动排序：服务器回来之前先按本地的顺序显示
  const [local, setLocal] = useState<Record<string, string[]>>({})
  useEffect(() => setLocal({}), [result])

  if (!groupProp) return <div className="db-empty">{t('groupBy')} →</div>
  const groups: Group[] = result.groups ?? []
  const editable = canWrite && !READONLY.has(groupProp.type)

  const drop = (g: Group, before: string | null) => {
    if (!drag) return
    const { id, from } = drag
    if (from !== g.key) {
      const p = groupProp
      let value: unknown = g.key || null
      if (p.type === 'multi_select' || p.type === 'person') {
        const cur = ((store.rows.get(id)?.props[p.key] as string[]) ?? []).filter((x) => x !== from)
        value = g.key ? [...new Set([...cur, g.key])] : cur
      } else if (p.type === 'checkbox') value = g.key === 'true'
      store.setCells([{ row_id: id, key: p.key, value }])
    }
    // 位置：放在 before 前面
    const ids = (local[g.key] ?? g.ids).filter((x) => x !== id)
    const at = before ? ids.indexOf(before) : ids.length
    ids.splice(at < 0 ? ids.length : at, 0, id)
    setLocal((m) => ({ ...m, [g.key]: ids }))
    if (!view.sorts?.length) store.moveRow(id, at > 0 ? ids[at - 1] : null)
    setDrag(null)
    setOver(null)
  }

  return (
    <div className="db-board">
      {groups.map((g) => {
        const ids = local[g.key] ?? g.ids
        const n = limit[g.key] ?? PAGE
        return (
          <div
            key={g.key}
            className={'db-col' + (over?.col === g.key ? ' is-over' : '')}
            onDragOver={(e) => {
              if (!drag) return
              e.preventDefault()
              if (over?.col !== g.key) setOver({ col: g.key, before: null })
            }}
            onDrop={(e) => {
              e.preventDefault()
              drop(g, over?.col === g.key ? over.before : null)
            }}
          >
            <div className="db-col-head">
              {g.key === '' ? <span className="db-group-none">{t('none')}</span> : groupProp.options ? <Pill opt={{ name: g.name, color: (g.color as never) ?? 'default' }} status={groupProp.type === 'status'} /> : <span className="db-group-name">{g.name}</span>}
              <span className="db-group-count">{ids.length}</span>
            </div>
            <div className="db-col-cards">
              {ids.slice(0, n).map((id) => {
                const row = store.rows.get(id)
                if (!row) return null
                return (
                  <div
                    key={id}
                    className={'db-card-slot' + (over?.before === id ? ' is-before' : '') + (drag?.id === id ? ' is-dragging' : '')}
                    onDragOver={(e) => {
                      if (!drag) return
                      e.preventDefault()
                      e.stopPropagation()
                      const r = e.currentTarget.getBoundingClientRect()
                      const before = e.clientY < r.top + r.height / 2 ? id : (ids[ids.indexOf(id) + 1] ?? null)
                      if (over?.before !== before || over?.col !== g.key) setOver({ col: g.key, before })
                    }}
                  >
                    <Card
                      row={row}
                      props={props}
                      onOpen={() => openRow(id)}
                      draggable={editable}
                      onDragStart={(e) => {
                        e.dataTransfer.effectAllowed = 'move'
                        e.dataTransfer.setData('text/plain', id)
                        setDrag({ id, from: g.key })
                      }}
                    />
                  </div>
                )
              })}
              {ids.length > n && (
                <button type="button" className="db-more" onClick={() => setLimit((m) => ({ ...m, [g.key]: n + PAGE }))}>
                  +{ids.length - n}
                </button>
              )}
              {canWrite && (
                <button type="button" className="db-col-add" onClick={() => store.addRow(groupDefaults(store, view, g))}>
                  + {t('newRow')}
                </button>
              )}
            </div>
          </div>
        )
      })}
    </div>
  )
}

// ---------------- 日历 ----------------
const pad = (n: number) => String(n).padStart(2, '0')
const iso = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
const parseDay = (s: string) => new Date(s.slice(0, 10) + 'T00:00:00')
const addDays = (s: string, n: number) => {
  const d = parseDay(s)
  d.setDate(d.getDate() + n)
  return iso(d) + (s.length > 10 ? s.slice(10) : '')
}

export function CalendarView({ view, result }: VProps) {
  const { store, canWrite, openRow } = useDb()
  const key = view.calendar_by ?? store.schema.props.find((p) => p.type === 'date')?.key
  const prop = store.prop(key ?? '')
  const [mode, setMode] = useState<'month' | 'week'>('month')
  const [cursor, setCursor] = useState(() => {
    const d = new Date()
    d.setHours(0, 0, 0, 0)
    return d
  })
  const [drag, setDrag] = useState<string | null>(null)
  const [over, setOver] = useState<string | null>(null)

  const days = useMemo(() => {
    if (mode === 'week') {
      const s = new Date(cursor)
      s.setDate(s.getDate() - ((s.getDay() + 6) % 7))
      return Array.from({ length: 7 }, (_, i) => new Date(s.getFullYear(), s.getMonth(), s.getDate() + i))
    }
    const first = new Date(cursor.getFullYear(), cursor.getMonth(), 1)
    const s = new Date(first)
    s.setDate(1 - ((first.getDay() + 6) % 7))
    return Array.from({ length: 42 }, (_, i) => new Date(s.getFullYear(), s.getMonth(), s.getDate() + i))
  }, [cursor, mode])

  // 每天有哪些行（跨天的在每一天都出现）
  const byDay = useMemo(() => {
    const m = new Map<string, Row[]>()
    if (!prop) return m
    const lo = iso(days[0])
    const hi = iso(days[days.length - 1])
    for (const id of result.ids) {
      const row = store.rows.get(id)
      if (!row) continue
      const v = cellValue(store, prop, row) as DateValue | string | null
      const start = typeof v === 'string' ? v : v?.start
      if (!start) continue
      const end = (typeof v === 'object' && v?.end) || start
      if (end.slice(0, 10) < lo || start.slice(0, 10) > hi) continue
      for (let d = start.slice(0, 10), k = 0; d <= end.slice(0, 10) && k < 62; d = addDays(d, 1), k++) {
        if (d < lo || d > hi) continue
        const list = m.get(d) ?? []
        list.push(row)
        m.set(d, list)
      }
    }
    return m
  }, [result, prop, days, store])

  if (!prop) return <div className="db-empty">{t('calendarBy')} →</div>
  const editable = canWrite && prop.type === 'date'
  const today = iso(new Date())
  const step = (n: number) => {
    const d = new Date(cursor)
    if (mode === 'month') d.setMonth(d.getMonth() + n, 1)
    else d.setDate(d.getDate() + 7 * n)
    setCursor(d)
  }
  const moveTo = (rowId: string, day: string) => {
    const row = store.rows.get(rowId)
    const v = row?.props[prop.key] as DateValue | undefined
    if (!v?.start) return
    const delta = Math.round((parseDay(day).getTime() - parseDay(v.start).getTime()) / 864e5)
    if (!delta) return
    store.setCells([{ row_id: rowId, key: prop.key, value: { ...v, start: addDays(v.start, delta), ...(v.end ? { end: addDays(v.end, delta) } : {}) } }])
  }
  const noDate = result.ids.filter((id) => {
    const r = store.rows.get(id)
    return r && !r.props[prop.key]
  }).length

  return (
    <div className="db-cal">
      <div className="db-cal-head">
        <span className="db-cal-title">
          {t('monthTitle', { y: cursor.getFullYear(), m: cursor.getMonth() + 1 })}
        </span>
        <span className="db-cal-nodate">{noDate ? `${t('noDate')} · ${noDate}` : ''}</span>
        <div className="db-menu-seg db-cal-mode">
          <button type="button" aria-pressed={mode === 'month'} onClick={() => setMode('month')}>
            {t('month')}
          </button>
          <button type="button" aria-pressed={mode === 'week'} onClick={() => setMode('week')}>
            {t('week')}
          </button>
        </div>
        <button type="button" className="db-tool" onClick={() => step(-1)} aria-label={t('prev')}>
          ‹
        </button>
        <button type="button" className="db-tool" onClick={() => setCursor(new Date(new Date().setHours(0, 0, 0, 0)))}>
          {t('today')}
        </button>
        <button type="button" className="db-tool" onClick={() => step(1)} aria-label={t('next')}>
          ›
        </button>
      </div>
      <div className={'db-cal-grid is-' + mode}>
        {t('weekdays')
          .split(',')
          .map((w) => (
            <div key={w} className="db-cal-wd">
              {w}
            </div>
          ))}
        {days.map((d) => {
          const k = iso(d)
          const list = byDay.get(k) ?? []
          const max = mode === 'month' ? 3 : 50
          return (
            <div
              key={k}
              className={'db-cal-day' + (d.getMonth() !== cursor.getMonth() && mode === 'month' ? ' is-out' : '') + (k === today ? ' is-today' : '') + (over === k ? ' is-over' : '')}
              onDragOver={(e) => {
                if (!drag) return
                e.preventDefault()
                setOver(k)
              }}
              onDragLeave={() => over === k && setOver(null)}
              onDrop={(e) => {
                e.preventDefault()
                if (drag) moveTo(drag, k)
                setDrag(null)
                setOver(null)
              }}
            >
              <div className="db-cal-num">
                <span>{d.getDate() === 1 && mode === 'month' ? t('monthDay', { m: d.getMonth() + 1, d: d.getDate() }) : d.getDate()}</span>
                {canWrite && prop.type === 'date' && (
                  <button type="button" className="db-cal-add" aria-label={t('newRow')} onClick={() => store.addRow({ ...groupDefaults(store, view), [prop.key]: { start: k } })}>
                    +
                  </button>
                )}
              </div>
              {list.slice(0, max).map((row) => (
                <button
                  key={row.id}
                  type="button"
                  className={'db-cal-ev' + (drag === row.id ? ' is-dragging' : '')}
                  draggable={editable}
                  onDragStart={(e) => {
                    e.dataTransfer.effectAllowed = 'move'
                    e.dataTransfer.setData('text/plain', row.id)
                    setDrag(row.id)
                  }}
                  onDragEnd={() => setDrag(null)}
                  onClick={() => openRow(row.id)}
                  title={(row.props.title as string) || t('untitled')}
                >
                  {(row.props.title as string) || t('untitled')}
                </button>
              ))}
              {list.length > max && <span className="db-cal-more">+{list.length - max}</span>}
            </div>
          )
        })}
      </div>
    </div>
  )
}

// ---------------- 画廊 ----------------
const bodyCovers = new Map<string, Promise<string | null>>()
function bodyCover(id: string) {
  let p = bodyCovers.get(id)
  if (!p) {
    p = api
      .page(id)
      .then(() => fetch(`/api/v1/pages/${id}/content?format=json`, { credentials: 'same-origin' }))
      .then((r) => (r.ok ? r.json() : null))
      .then((j) => {
        let found: string | null = null
        const walk = (n: { type?: string; attrs?: { src?: string }; content?: unknown[] }) => {
          if (found) return
          if (n.type === 'image' && n.attrs?.src) found = n.attrs.src
          ;(n.content as (typeof n)[] | undefined)?.forEach(walk)
        }
        if (j?.doc) walk(j.doc)
        return found
      })
      .catch(() => null)
    bodyCovers.set(id, p)
  }
  return p
}

function useCover(view: View, row: Row, visible: boolean) {
  const [body, setBody] = useState<string | null>(null)
  const files = view.cover && view.cover !== 'body' ? (row.props[view.cover] as { url: string; name?: string }[] | undefined) : undefined
  const fromFiles = files?.find((f) => /\.(png|jpe?g|gif|webp|avif|svg)(\?|$)/i.test(f.name || f.url))?.url
  useEffect(() => {
    if (view.cover === 'body' && visible) bodyCover(row.id).then(setBody)
  }, [view.cover, row.id, visible])
  if (!view.cover) return undefined
  const src = fromFiles ?? body
  return src ? (src.startsWith('/api/') && !/\.svg(\?|$)/i.test(src) ? src.split('?')[0] + '?w=640' : src) : null
}

function GalleryCard({ row, view, props, visible }: { row: Row; view: View; props: Prop[]; visible: boolean }) {
  const { openRow } = useDb()
  const cover = useCover(view, row, visible)
  return <Card row={row} props={props} cover={cover} onOpen={() => openRow(row.id)} />
}

export function GalleryView({ view, result, maxHeight }: VProps) {
  const { store, canWrite } = useDb()
  const props = useMemo(() => cardProps(store.schema.props, view), [store.schema, view])
  const box = useRef<HTMLDivElement>(null)
  const [w, setW] = useState(900)
  useEffect(() => {
    const el = box.current
    if (!el) return
    const ro = new ResizeObserver(([e]) => setW(e.contentRect.width))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  const size = view.card_size ?? 'medium'
  const min = size === 'small' ? 180 : size === 'large' ? 320 : 240
  const cols = Math.max(1, Math.floor((w + 16) / (min + 16)))
  const rowsN = Math.ceil(result.ids.length / cols)
  const cardH = (view.cover ? (size === 'small' ? 120 : size === 'large' ? 220 : 160) : 0) + 56 + Math.min(4, props.length) * 26
  const rv = useVirtualizer({ count: rowsN + (canWrite ? 1 : 0), getScrollElement: () => box.current, estimateSize: () => cardH + 16, overscan: 3, gap: 16 })
  return (
    <div className="db-gallery" ref={box} style={maxHeight ? { maxHeight } : undefined}>
      <div style={{ height: rv.getTotalSize(), position: 'relative' }}>
        {rv.getVirtualItems().map((vi) => {
          if (vi.index === rowsN)
            return (
              <button key="add" type="button" className="db-gallery-add" style={{ top: vi.start, height: 40 }} onClick={() => store.addRow(groupDefaults(store, view))}>
                + {t('newRow')}
              </button>
            )
          const ids = result.ids.slice(vi.index * cols, vi.index * cols + cols)
          return (
            <div key={vi.index} ref={rv.measureElement} data-index={vi.index} className="db-gallery-row" style={{ top: vi.start, gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}>
              {ids.map((id) => {
                const row = store.rows.get(id)
                return row ? <GalleryCard key={id} row={row} view={view} props={props} visible /> : null
              })}
            </div>
          )
        })}
      </div>
    </div>
  )
}

// ---------------- 列表 ----------------
export function ListView({ view, result, maxHeight }: VProps) {
  const { store, canWrite, openRow } = useDb()
  const props = useMemo(() => cardProps(store.schema.props, view).slice(0, 4), [store.schema, view])
  const box = useRef<HTMLDivElement>(null)
  const rv = useVirtualizer({ count: result.ids.length + (canWrite ? 1 : 0), getScrollElement: () => box.current, estimateSize: () => 40, overscan: 10 })
  return (
    <div className="db-list" ref={box} style={maxHeight ? { maxHeight } : undefined}>
      <div style={{ height: rv.getTotalSize(), position: 'relative' }}>
        {rv.getVirtualItems().map((vi) => {
          if (vi.index === result.ids.length)
            return (
              <button key="add" type="button" className="db-add-row" style={{ top: vi.start, width: '100%' }} onClick={() => store.addRow(groupDefaults(store, view))}>
                <span className="db-add-in">
                  <span className="db-add-plus">+</span>
                  {t('newRow')}
                </span>
              </button>
            )
          const row = store.rows.get(result.ids[vi.index])
          if (!row) return null
          return (
            <button key={row.id} type="button" className="db-list-row" style={{ top: vi.start }} onClick={() => openRow(row.id)}>
              <span className="db-list-ico">▤</span>
              <span className={'db-list-title' + (row.props.title ? '' : ' is-empty')}>{(row.props.title as string) || t('untitled')}</span>
              <span className="db-list-props">
                {props.map((p) =>
                  isEmptyVal(cellValue(store, p, row)) ? null : (
                    <span key={p.key} className="db-list-prop" title={p.name}>
                      {(p.type === 'number' || p.type === 'formula' || p.type === 'rollup' || p.type === 'checkbox') && <span className="db-list-label">{p.name}</span>}
                      <CellView p={p} row={row} />
                    </span>
                  ),
                )}
              </span>
            </button>
          )
        })}
      </div>
    </div>
  )
}
