// 数据库界面的小零件：浮层、选项色块、类型图标、每种类型能用的筛选条件和汇总。
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import type { Color, Prop, PropType, SelectOption } from './types'

// ---------- 浮层 ----------
export interface Anchor {
  left: number
  top: number
  bottom: number
  right?: number
  width?: number
}

export const rectOf = (el: Element | null): Anchor => {
  const r = el?.getBoundingClientRect()
  return r ? { left: r.left, top: r.top, bottom: r.bottom, right: r.right, width: r.width } : { left: 0, top: 0, bottom: 0 }
}

export function Popover({ anchor, onClose, children, className = '', width, align = 'left' }: { anchor: Anchor; onClose: () => void; children: ReactNode; className?: string; width?: number; align?: 'left' | 'right' }) {
  const box = useRef<HTMLDivElement>(null)
  const [pos, setPos] = useState<{ left: number; top: number; up: boolean }>({ left: anchor.left, top: anchor.bottom + 4, up: false })
  useLayoutEffect(() => {
    const el = box.current
    if (!el) return
    const h = el.offsetHeight
    const w = el.offsetWidth
    const up = anchor.bottom + 4 + h > window.innerHeight - 8 && anchor.top - h - 4 > 8
    let left = align === 'right' && anchor.right !== undefined ? anchor.right - w : anchor.left
    left = Math.max(8, Math.min(left, window.innerWidth - w - 8))
    setPos({ left, top: up ? anchor.top - h - 4 : Math.min(anchor.bottom + 4, window.innerHeight - h - 8), up })
  }, [anchor.left, anchor.top, anchor.bottom, align]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    const down = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node) && !(e.target as HTMLElement).closest?.('.db-pop')) onClose()
    }
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation()
        onClose()
      }
    }
    // 下一轮再挂，免得打开它的那次点击把它关掉
    const t = setTimeout(() => document.addEventListener('mousedown', down, true))
    document.addEventListener('keydown', key, true)
    return () => {
      clearTimeout(t)
      document.removeEventListener('mousedown', down, true)
      document.removeEventListener('keydown', key, true)
    }
  }, [onClose])
  return createPortal(
    <div ref={box} className={'db-pop ' + className} style={{ left: pos.left, top: pos.top, width }} onMouseDown={(e) => e.stopPropagation()}>
      {children}
    </div>,
    document.body,
  )
}

// ---------- 选项 ----------
export const COLORS: Color[] = ['default', 'gray', 'brown', 'orange', 'yellow', 'green', 'blue', 'purple', 'pink', 'red']
export const COLOR_NAMES: Record<Color, string> = { default: '默认', gray: '灰', brown: '棕', orange: '橙', yellow: '黄', green: '绿', blue: '蓝', purple: '紫', pink: '粉', red: '红' }

export function Pill({ opt, status, onRemove }: { opt: Pick<SelectOption, 'name' | 'color'>; status?: boolean; onRemove?: () => void }) {
  return (
    <span className={'db-pill c-' + (opt.color || 'default') + (status ? ' is-status' : '')}>
      {status && <i className="db-pill-dot" />}
      <span className="db-pill-text">{opt.name}</span>
      {onRemove && (
        <button
          type="button"
          className="db-pill-x"
          onMouseDown={(e) => e.preventDefault()}
          onClick={(e) => {
            e.stopPropagation()
            onRemove()
          }}
        >
          ×
        </button>
      )}
    </span>
  )
}

export const randomColor = (i: number): Color => COLORS[1 + (i % (COLORS.length - 1))]
export const optId = () => 'o' + Math.random().toString(36).slice(2, 7)

// ---------- 类型图标 ----------
const ICONS: Record<PropType, string> = {
  title: 'M4 5h16v2H13v12h-2V7H4z',
  text: 'M4 6h16v2H4zm0 5h16v2H4zm0 5h10v2H4z',
  number: 'M9 3 8 8H4v2h3.6l-.8 4H3v2h3.4L5.6 21h2l.8-5h4l-.8 5h2l.8-5H18v-2h-3.4l.8-4H20V8h-4.2l1-5h-2l-1 5h-4l1-5zm.4 7h4l-.8 4h-4z',
  select: 'M12 3a9 9 0 1 1 0 18 9 9 0 0 1 0-18zm0 2a7 7 0 1 0 0 14 7 7 0 0 0 0-14zm-3.5 5h7L12 15z',
  multi_select: 'M4 6h3v3H4zm5 1h11v1H9zM4 11h3v3H4zm5 1h11v1H9zm-5 4h3v3H4zm5 1h11v1H9z',
  status: 'M12 3a9 9 0 1 1 0 18 9 9 0 0 1 0-18zm0 2v14a7 7 0 0 0 0-14z',
  date: 'M7 2h2v2h6V2h2v2h3v17H4V4h3zm-1 7v10h12V9zm2 2h3v3H8z',
  person: 'M12 4a4 4 0 1 1 0 8 4 4 0 0 1 0-8zm0 10c4.4 0 8 2 8 4.5V21H4v-2.5C4 16 7.6 14 12 14z',
  checkbox: 'M5 4h14a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1zm1 2v12h12V6zm9.3 2.3 1.4 1.4-5.7 5.7-3.4-3.4 1.4-1.4 2 2z',
  url: 'M10.6 13.4a1 1 0 0 1 0-1.4l3-3a1 1 0 1 1 1.4 1.4l-3 3a1 1 0 0 1-1.4 0zm-2.1 4.9a3.5 3.5 0 0 1-5-5l2.9-2.8 1.4 1.4-2.8 2.8a1.5 1.5 0 1 0 2.1 2.1l2.8-2.8 1.4 1.4zm9.2-5.2-1.4-1.4 2.8-2.8a1.5 1.5 0 1 0-2.1-2.1l-2.8 2.8-1.4-1.4 2.8-2.9a3.5 3.5 0 1 1 5 5z',
  email: 'M3 5h18v14H3zm2 2v.5l7 4.5 7-4.5V7zm14 2.9-7 4.5-7-4.5V17h14z',
  phone: 'M7 2h10a1 1 0 0 1 1 1v18a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1zm1 2v14h8V4zm3 15h2v1h-2z',
  files: 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zm-1 1.5L18.5 9H13zM6 20V4h6v6h6v10z',
  created_time: 'M12 3a9 9 0 1 1 0 18 9 9 0 0 1 0-18zm0 2a7 7 0 1 0 0 14 7 7 0 0 0 0-14zm-1 2h2v5l4 2-1 1.7-5-2.7z',
  updated_time: 'M12 3a9 9 0 1 1 0 18 9 9 0 0 1 0-18zm0 2a7 7 0 1 0 0 14 7 7 0 0 0 0-14zm-1 2h2v5l4 2-1 1.7-5-2.7z',
  created_by: 'M12 4a4 4 0 1 1 0 8 4 4 0 0 1 0-8zm0 10c4.4 0 8 2 8 4.5V21H4v-2.5C4 16 7.6 14 12 14z',
  updated_by: 'M12 4a4 4 0 1 1 0 8 4 4 0 0 1 0-8zm0 10c4.4 0 8 2 8 4.5V21H4v-2.5C4 16 7.6 14 12 14z',
  relation: 'M7 7h6v2H7a3 3 0 0 0 0 6h6v2H7A5 5 0 0 1 7 7zm4 4h6a5 5 0 0 1 0 10h-6v-2h6a3 3 0 0 0 0-6h-6z',
  rollup: 'M3 5h18v2H3zm3 6h12v2H6zm3 6h6v2H9z',
  formula: 'M8 4h9v2H9.7l-.6 5H13v2H8.9l-.8 6c-.2 1.7-1.5 3-3.1 3H4v-2h1c.6 0 1-.4 1.1-1l.8-6H5v-2h2.1l.7-5.4C8 3.8 9.3 2.6 11 2.6z',
  unique_id: 'M4 7h3v10H4zm5 0h3v10H9zm5 0h1v10h-1zm3 0h3v10h-3z',
  button: 'M3 7a3 3 0 0 1 3-3h12a3 3 0 0 1 3 3v6a3 3 0 0 1-3 3h-3l-3 4-3-4H6a3 3 0 0 1-3-3z',
}
export function TypeIcon({ type }: { type: PropType }) {
  return (
    <svg className="db-type-ico" viewBox="0 0 24 24" aria-hidden>
      <path d={ICONS[type] ?? ICONS.text} fill="currentColor" />
    </svg>
  )
}

export const VIEW_ICONS: Record<string, string> = {
  table: 'M3 4h18v16H3zm2 2v3h14V6zm0 5v3h6v-3zm8 0v3h6v-3zm-8 5v2h6v-2zm8 0v2h6v-2z',
  board: 'M3 4h5v16H3zm6.5 0h5v10h-5zM16 4h5v13h-5z',
  calendar: 'M7 2h2v2h6V2h2v2h3v17H4V4h3zm-1 7v10h12V9z',
  gallery: 'M3 4h8v7H3zm10 0h8v7h-8zM3 13h8v7H3zm10 0h8v7h-8z',
  list: 'M4 5h2v2H4zm4 0h12v2H8zm-4 6h2v2H4zm4 0h12v2H8zm-4 6h2v2H4zm4 0h12v2H8z',
}
export function ViewIcon({ type }: { type: string }) {
  return (
    <svg className="db-view-ico" viewBox="0 0 24 24" aria-hidden>
      <path d={VIEW_ICONS[type] ?? VIEW_ICONS.table} fill="currentColor" />
    </svg>
  )
}

// ---------- 每种类型能用的筛选条件、汇总 ----------
export const READONLY = new Set<PropType>(['formula', 'rollup', 'created_time', 'updated_time', 'created_by', 'updated_by', 'unique_id', 'button'])

export function condsFor(p: Prop): string[] {
  switch (p.type) {
    case 'number':
    case 'unique_id':
      return ['eq', 'neq', 'gt', 'gte', 'lt', 'lte', 'empty', 'not_empty']
    case 'select':
    case 'status':
      return ['eq', 'neq', 'empty', 'not_empty']
    case 'multi_select':
    case 'person':
    case 'relation':
    case 'files':
    case 'created_by':
    case 'updated_by':
      return ['contains', 'not_contains', 'empty', 'not_empty']
    case 'checkbox':
      return ['checked', 'unchecked']
    case 'date':
    case 'created_time':
    case 'updated_time':
      return ['eq', 'lt', 'gt', 'lte', 'gte', 'is_within', 'empty', 'not_empty']
    default:
      return ['contains', 'not_contains', 'eq', 'neq', 'starts_with', 'ends_with', 'empty', 'not_empty']
  }
}
export const NO_VALUE = new Set(['empty', 'not_empty', 'checked', 'unchecked'])

export function totalsFor(p: Prop): string[] {
  const base = ['count', 'count_values', 'count_empty', 'count_unique', 'percent_empty', 'percent_not_empty']
  if (p.type === 'number' || p.type === 'formula' || p.type === 'rollup' || p.type === 'unique_id') return [...base, 'sum', 'avg', 'median', 'min', 'max', 'range']
  if (p.type === 'checkbox') return ['count', 'checked', 'unchecked', 'percent_checked']
  if (p.type === 'date' || p.type === 'created_time' || p.type === 'updated_time') return [...base, 'earliest', 'latest']
  return base
}

export const ADDABLE_TYPES: PropType[] = ['text', 'number', 'select', 'multi_select', 'status', 'date', 'person', 'checkbox', 'url', 'email', 'phone', 'files', 'relation', 'rollup', 'formula', 'created_time', 'updated_time', 'created_by', 'updated_by', 'unique_id', 'button']

export function useMeasure<T extends HTMLElement>() {
  const ref = useRef<T>(null)
  const [size, setSize] = useState({ width: 0, height: 0 })
  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const ro = new ResizeObserver(([e]) => setSize({ width: e.contentRect.width, height: e.contentRect.height }))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  return [ref, size] as const
}
