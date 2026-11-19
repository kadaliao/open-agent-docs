// 公式编辑器：属性、函数补全，实时检查（引擎的 checkFormula），预览前几行算出来的值。
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { useDb } from './cells'
import { FUNCTIONS } from './functions'
import { t } from './strings'
import type { Prop } from '@oad/client'
import { TypeIcon } from './ui'

interface Item {
  label: string
  insert: string
  hint: string
  kind: 'prop' | 'fn'
  type?: Prop['type']
}

export function FormulaEditor({ p, onClose }: { p: Prop; onClose: () => void }) {
  const { store } = useDb()
  const [src, setSrc] = useState(p.formula ?? '')
  const [caret, setCaret] = useState(src.length)
  const [active, setActive] = useState(0)
  const ta = useRef<HTMLTextAreaElement>(null)
  const check = useMemo(() => (src.trim() ? (store.engine?.checkFormula(src, p.key) ?? { ok: true as const }) : { ok: true as const }), [src, store.engine, p.key])

  // 光标前正在打的词
  const word = /[\p{L}\p{N}_."]*$/u.exec(src.slice(0, caret))?.[0] ?? ''
  const items: Item[] = useMemo(() => {
    const w = word.replace(/^prop\("?/, '').replace(/^\.?/, '').toLowerCase()
    const props: Item[] = store.schema.props
      .filter((x) => x.key !== p.key)
      .map((x) => ({ label: x.name, insert: `prop("${x.name}")`, hint: x.type, kind: 'prop', type: x.type }))
    const fns: Item[] = Object.entries(FUNCTIONS).map(([k, f]) => ({ label: f.sig.split('(')[0], insert: f.sig.split('(')[0] + '(', hint: f.sig, kind: 'fn' }))
    if (!w) return [...props, ...fns].slice(0, 40)
    return [...props, ...fns].filter((x) => x.label.toLowerCase().includes(w) || x.insert.toLowerCase().includes(w)).slice(0, 40)
  }, [word, store.schema, p.key])

  const insert = (it: Item) => {
    const start = caret - word.length
    const next = src.slice(0, start) + it.insert + src.slice(caret)
    setSrc(next)
    const c = start + it.insert.length
    pending.current = c
    setCaret(c)
  }
  // 补全插进去以后，光标放在插入的东西后面（和这次渲染同步，紧接着打字不会乱）
  const pending = useRef<number | null>(null)
  useLayoutEffect(() => {
    if (pending.current === null || !ta.current) return
    ta.current.focus()
    ta.current.setSelectionRange(pending.current, pending.current)
    pending.current = null
  }, [src])

  // 预览：拿前 3 行，用一个临时公式算
  const preview = useMemo(() => {
    if (!('ok' in check) || !src.trim() || !store.engine) return []
    const rows = [...store.rows.values()].slice(0, 3)
    return rows.map((r) => {
      try {
        const eng = store.engine!
        const schema = store.schema
        eng.setSchema({ ...schema, props: schema.props.map((x) => (x.key === p.key ? { ...x, type: 'formula' as const, formula: src } : x)) })
        const v = eng.computed(r.id)[p.key]
        const d = eng.display(p.key, v)
        eng.setSchema(schema)
        return { title: (r.props.title as string) || t('untitled'), v: d || '—' }
      } catch {
        return { title: String(r.props.title ?? ''), v: '—' }
      }
    })
  }, [check, src, store])

  useEffect(() => {
    ta.current?.focus()
  }, [])

  const save = () => {
    if (!('ok' in check)) return
    store.setFormula(p.key, src)
    onClose()
  }

  return createPortal(
    <div className="modal-mask oad-root" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal db-formula" role="dialog" aria-label={t('formulaTitle')}>
        <div className="modal-head">
          <h2>
            {t('formulaTitle')} · {p.name}
          </h2>
        </div>
        <div className="modal-body">
          <div className="db-formula-hint">{t('formulaHint')}</div>
          <div className={'db-formula-box' + ('error' in check ? ' is-error' : '')}>
            <textarea
              ref={ta}
              spellCheck={false}
              value={src}
              rows={4}
              onChange={(e) => {
                setSrc(e.target.value)
                setCaret(e.target.selectionStart)
                setActive(0)
              }}
              onSelect={(e) => setCaret((e.target as HTMLTextAreaElement).selectionStart)}
              onKeyDown={(e) => {
                if (items.length && word && (e.key === 'Tab' || (e.key === 'Enter' && !e.metaKey && !e.ctrlKey))) {
                  e.preventDefault()
                  insert(items[active])
                } else if (items.length && word && e.key === 'ArrowDown') {
                  e.preventDefault()
                  setActive((a) => Math.min(items.length - 1, a + 1))
                } else if (items.length && word && e.key === 'ArrowUp') {
                  e.preventDefault()
                  setActive((a) => Math.max(0, a - 1))
                } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                  e.preventDefault()
                  save()
                } else if (e.key === 'Escape') onClose()
                e.stopPropagation()
              }}
            />
            {'error' in check && (
              <div className="db-formula-err" role="alert">
                <span className="db-formula-at">
                  {src.slice(Math.max(0, check.at - 12), check.at)}
                  <mark>{src.slice(check.at, check.at + 1) || ' '}</mark>
                  {src.slice(check.at + 1, check.at + 12)}
                </span>
                {check.error}
              </div>
            )}
            {'ok' in check && src.trim() && <div className="db-formula-ok">✓ {t('formulaOk')}</div>}
          </div>
          <div className="db-formula-cols">
            <div className="db-formula-list" role="listbox">
              {items.map((it, i) => (
                <button key={it.kind + it.label} type="button" role="option" aria-selected={i === active} className="db-formula-item" onMouseMove={() => setActive(i)} onMouseDown={(e) => e.preventDefault()} onClick={() => insert(it)}>
                  {it.kind === 'prop' ? <TypeIcon type={it.type!} /> : <span className="db-fn-ico">ƒ</span>}
                  <span className="db-formula-label">{it.label}</span>
                </button>
              ))}
            </div>
            <div className="db-formula-side">
              {items[active] && (
                <div className="db-formula-doc">
                  <code>{items[active].kind === 'prop' ? items[active].insert : items[active].hint}</code>
                </div>
              )}
              {!!preview.length && (
                <>
                  <div className="db-menu-label">{t('preview')}</div>
                  {preview.map((x, i) => (
                    <div key={i} className="db-formula-prev">
                      <span>{x.title}</span>
                      <b>{x.v}</b>
                    </div>
                  ))}
                </>
              )}
            </div>
          </div>
          <div className="modal-foot">
            <button type="button" className="btn" onClick={onClose}>
              {t('cancel')}
            </button>
            <button type="button" className="btn btn-primary" disabled={!('ok' in check)} onClick={save}>
              {t('save')}
            </button>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  )
}
