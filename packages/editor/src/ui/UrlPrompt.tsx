import { useEffect, useRef, useState } from 'react'
import { t } from '../strings'

export interface UrlPromptState {
  kind: 'image' | 'embed'
  rect: { left: number; bottom: number }
  resolve: (v: string | null) => void
}

const URL_OK = /^(https?:\/\/|\/(?!\/))/i

export function normalizeUrl(v: string): string | null {
  const s = v.trim()
  if (!s) return null
  if (URL_OK.test(s)) return s
  if (/^[\w-]+(\.[\w-]+)+(\/|$)/.test(s)) return 'https://' + s
  return null
}

// 插入图片、链接卡片时要网址的小输入框
export function UrlPrompt({ state, onClose }: { state: UrlPromptState; onClose: () => void }) {
  const [v, setV] = useState('')
  const [bad, setBad] = useState(false)
  const input = useRef<HTMLInputElement>(null)
  const closed = useRef(false)
  useEffect(() => input.current?.focus(), [])
  const done = (val: string | null) => {
    if (closed.current) return
    closed.current = true
    state.resolve(val)
    onClose()
  }
  const submit = () => {
    const u = normalizeUrl(v)
    if (!u) return setBad(true)
    done(u)
  }
  const left = Math.max(8, Math.min(state.rect.left, window.innerWidth - 368))
  return (
    <div className="oad-popover oad-url-prompt" style={{ left, top: state.rect.bottom + 8 }}>
      <input
        ref={input}
        value={v}
        aria-invalid={bad}
        placeholder={state.kind === 'image' ? t('imagePlaceholder') : t('embedPlaceholder')}
        onChange={(e) => {
          setV(e.target.value)
          setBad(false)
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault()
            submit()
          } else if (e.key === 'Escape') {
            e.preventDefault()
            done(null)
          }
        }}
        onBlur={() => done(null)}
      />
      <button type="button" className="oad-btn-primary" onMouseDown={(e) => e.preventDefault()} onClick={submit}>
        {t('confirm')}
      </button>
    </div>
  )
}
