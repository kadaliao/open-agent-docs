// 点一个东西弹出来的小面板（格子的编辑器、表头菜单、筛选……）：贴着那个东西放，放不下就往上、往左挪；点外面或按 Esc 关掉。
// 手机上贴着屏幕底边整宽显示。
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { isPhone } from '../../swipe'

export function Popover({ anchor, onClose, children, width = 260, className = '' }: {
  anchor: HTMLElement | null; onClose: () => void; children: ReactNode; width?: number; className?: string
}) {
  const box = useRef<HTMLDivElement>(null)
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null)
  const phone = isPhone()
  useLayoutEffect(() => {
    if (!anchor || phone) return
    const place = () => {
      const r = anchor.getBoundingClientRect(), h = box.current?.offsetHeight || 0, vw = innerWidth, vh = innerHeight
      const left = Math.max(8, Math.min(r.left, vw - width - 8))
      const top = r.bottom + 4 + h > vh - 8 && r.top - 4 - h > 8 ? r.top - 4 - h : Math.min(r.bottom + 4, Math.max(8, vh - h - 8))
      setPos({ left, top })
    }
    place()
    const ro = new ResizeObserver(place)
    if (box.current) ro.observe(box.current)
    addEventListener('resize', place)
    return () => { ro.disconnect(); removeEventListener('resize', place) }
  }, [anchor, width, phone])
  useEffect(() => {
    const down = (e: MouseEvent | TouchEvent) => {
      const el = e.target as Node
      if (box.current?.contains(el) || anchor?.contains(el)) return
      if ((el as Element).closest?.('.dialog-bg')) return   // 确认框是另一层
      onClose()
    }
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); onClose() } }
    document.addEventListener('mousedown', down, true)
    document.addEventListener('touchstart', down, true)
    document.addEventListener('keydown', key, true)
    return () => {
      document.removeEventListener('mousedown', down, true)
      document.removeEventListener('touchstart', down, true)
      document.removeEventListener('keydown', key, true)
    }
  }, [anchor, onClose])
  const style = phone ? undefined : { left: pos?.left ?? -9999, top: pos?.top ?? -9999, width }
  return createPortal(<div ref={box} className={'db-pop' + (phone ? ' sheet' : '') + (className ? ' ' + className : '')} style={style}
    onMouseDown={e => e.stopPropagation()}>{children}</div>, document.body)
}
