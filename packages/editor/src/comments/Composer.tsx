// 写评论的输入框：输入 @ 弹出可选的人和 Agent，存的时候换成 @[名字](agent:<id>)。⌘/Ctrl + 回车发送。
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { Person } from '@oad/client/core'
import { colorFor, initials } from '../react/util'
import { t } from './strings'
import { bodyToDraft, draftToBody } from './body'

export interface Candidate extends Person {
  about?: string | null
}

interface Props {
  candidates: Candidate[]
  initial?: string
  placeholder?: string
  submitLabel?: string
  autoFocus?: boolean
  onSubmit: (body: string) => Promise<unknown> | void
  onCancel?: () => void
  compact?: boolean
}

export function Composer({ candidates, initial = '', placeholder, submitLabel, autoFocus, onSubmit, onCancel, compact }: Props) {
  const start = useMemo(() => bodyToDraft(initial), [initial])
  const [text, setText] = useState(start.text)
  const mentions = useRef(new Map(start.mentions))
  const [picker, setPicker] = useState<{ from: number; query: string; cur: number } | null>(null)
  const [busy, setBusy] = useState(false)
  const ta = useRef<HTMLTextAreaElement>(null)

  const fit = () => {
    const el = ta.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = Math.min(240, el.scrollHeight) + 'px'
  }
  useLayoutEffect(fit, [text])
  useEffect(() => {
    if (autoFocus) {
      const el = ta.current!
      el.focus()
      el.setSelectionRange(el.value.length, el.value.length)
    }
  }, [autoFocus])

  const matches = useMemo(() => {
    if (!picker) return []
    const q = picker.query.toLowerCase()
    return candidates.filter((c) => !q || c.name.toLowerCase().includes(q)).slice(0, 8)
  }, [picker, candidates])

  // 光标前面是 @xxx（@ 在开头或空白后面）就弹候选
  const updatePicker = (value: string, caret: number) => {
    const before = value.slice(0, caret)
    const m = /(^|\s)@([^\s@]{0,32})$/.exec(before)
    if (m) setPicker((p) => ({ from: caret - m[2].length - 1, query: m[2], cur: p?.from === caret - m[2].length - 1 ? Math.min(p.cur, 7) : 0 }))
    else setPicker(null)
  }

  const pick = (c: Candidate) => {
    if (!picker) return
    const el = ta.current!
    const caret = el.selectionStart
    const insert = '@' + c.name + ' '
    const next = text.slice(0, picker.from) + insert + text.slice(caret)
    mentions.current.set(c.name, `${c.kind}:${c.id}`)
    setText(next)
    setPicker(null)
    requestAnimationFrame(() => {
      const p = picker.from + insert.length
      el.focus()
      el.setSelectionRange(p, p)
    })
  }

  const submit = async () => {
    const body = draftToBody(text.trim(), mentions.current)
    if (!body || busy) return
    setBusy(true)
    try {
      await onSubmit(body)
      setText('')
      mentions.current = new Map()
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className={'composer' + (compact ? ' is-compact' : '')}>
      <textarea
        ref={ta}
        value={text}
        rows={compact ? 1 : 2}
        placeholder={placeholder ?? t('writeComment')}
        onChange={(e) => {
          setText(e.target.value)
          updatePicker(e.target.value, e.target.selectionStart)
        }}
        onKeyUp={(e) => {
          if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) updatePicker(text, e.currentTarget.selectionStart)
        }}
        onBlur={() => setTimeout(() => setPicker(null), 120)}
        onKeyDown={(e) => {
          if (e.nativeEvent.isComposing) return
          if (picker && matches.length) {
            if (e.key === 'ArrowDown') {
              e.preventDefault()
              setPicker({ ...picker, cur: (picker.cur + 1) % matches.length })
              return
            }
            if (e.key === 'ArrowUp') {
              e.preventDefault()
              setPicker({ ...picker, cur: (picker.cur - 1 + matches.length) % matches.length })
              return
            }
            if (e.key === 'Enter' || e.key === 'Tab') {
              e.preventDefault()
              pick(matches[picker.cur])
              return
            }
          }
          if (e.key === 'Escape') {
            if (picker) setPicker(null)
            else onCancel?.()
            return
          }
          if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
            e.preventDefault()
            submit()
          }
        }}
      />
      {picker && (
        <div className="mention-picker" role="listbox" onMouseDown={(e) => e.preventDefault()}>
          {matches.length === 0 ? (
            <div className="mention-empty">{t('mentionEmpty')}</div>
          ) : (
            matches.map((c, i) => (
              <button
                key={c.kind + c.id}
                type="button"
                role="option"
                aria-selected={i === picker.cur}
                className="mention-item"
                onMouseMove={() => i !== picker.cur && setPicker({ ...picker, cur: i })}
                onClick={() => pick(c)}
              >
                <span className={'avatar' + (c.kind === 'agent' ? ' is-agent' : '')} style={{ background: colorFor(c.id) }}>
                  {c.kind === 'agent' ? '✦' : initials(c.name)}
                </span>
                <span className="mention-name">{c.name}</span>
                {c.kind === 'agent' && <span className="mention-tag">{t('agentTag')}</span>}
                {c.about && <span className="mention-about">{c.about}</span>}
              </button>
            ))
          )}
        </div>
      )}
      <div className="composer-bar">
        <span className="composer-hint">{t('sendHint')}</span>
        {onCancel && (
          <button type="button" className="btn btn-sm" onClick={onCancel}>
            {t('cancel')}
          </button>
        )}
        <button type="button" className="btn btn-sm btn-primary" disabled={!text.trim() || busy} onClick={submit}>
          {submitLabel ?? t('send')}
        </button>
      </div>
    </div>
  )
}
