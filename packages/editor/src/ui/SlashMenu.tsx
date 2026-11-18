import type { Editor } from '@tiptap/core'
import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { SlashState, SlashStorage } from '../extensions/slash'
import { t } from '../strings'

// 贴着光标显示，下面放不下就往上翻
export function SlashMenu({ editor }: { editor: Editor }) {
  const [state, setState] = useState<SlashState | null>(null)
  const [cur, setCur] = useState(0)
  const box = useRef<HTMLDivElement>(null)
  const ref = useRef({ state, cur })
  ref.current = { state, cur }

  useEffect(() => {
    const st = (editor.storage as unknown as { slashCommands?: SlashStorage }).slashCommands
    if (!st) return
    st.show = (s) => {
      setState(s)
      setCur(0)
    }
    st.key = (e) => {
      const { state: s, cur: c } = ref.current
      if (!s || !s.items.length) return false
      if (e.key === 'ArrowDown') {
        setCur((c + 1) % s.items.length)
        return true
      }
      if (e.key === 'ArrowUp') {
        setCur((c - 1 + s.items.length) % s.items.length)
        return true
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        s.pick(s.items[c])
        return true
      }
      return false
    }
    return () => {
      st.show = () => {}
      st.key = () => false
    }
  }, [editor])

  useLayoutEffect(() => {
    box.current?.querySelector('[data-active="true"]')?.scrollIntoView({ block: 'nearest' })
  }, [cur, state])

  if (!state?.rect) return null
  const vv = window.visualViewport
  const visTop = vv?.offsetTop ?? 0
  const visBottom = vv ? vv.offsetTop + vv.height : window.innerHeight
  const below = visBottom - state.rect.bottom - 8
  const above = state.rect.top - visTop - 8
  const up = below < 320 && above > below
  const left = Math.max(8, Math.min(state.rect.left, window.innerWidth - 288))
  const maxHeight = Math.max(160, Math.min(360, up ? above : below))
  const style: React.CSSProperties = up
    ? { left, top: state.rect.top - 6, transform: 'translateY(-100%)', maxHeight }
    : { left, top: state.rect.bottom + 6, maxHeight }

  return (
    <div className="oad-popover oad-slash" ref={box} style={style} role="listbox" onMouseDown={(e) => e.preventDefault()}>
      {state.items.length === 0 ? (
        <div className="oad-slash-empty">{t('slashEmpty')}</div>
      ) : (
        state.items.map((it, k) => (
          <button
            key={it.key}
            type="button"
            role="option"
            aria-selected={k === cur}
            data-active={k === cur}
            className="oad-slash-item"
            onMouseMove={() => k !== cur && setCur(k)}
            onClick={() => state.pick(it)}
          >
            <span className="oad-slash-icon" data-key={it.key}>
              {it.icon}
            </span>
            <span className="oad-slash-text">
              <span className="oad-slash-label">{t(it.label)}</span>
              <span className="oad-slash-hint">{t(it.hint)}</span>
            </span>
          </button>
        ))
      )}
    </div>
  )
}
