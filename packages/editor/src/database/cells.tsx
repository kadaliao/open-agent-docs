// 格子：按类型显示，按类型编辑（文字、数字、选项、日期、人员、勾选、文件、关联、网址）。
import { createContext, useContext, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react'
import type { Person } from '@open-agent-docs/client'
import type { MediaServices } from '../extensions/media'
import { colorFor, initials } from '../react/util'
import { tryNormalize } from '@open-agent-docs/client'
import type { DbStore } from '@open-agent-docs/client'
import { t } from './strings'
import type { DateValue, Prop, Row, SelectOption } from '@open-agent-docs/client'
import { optId, Pill, Popover, randomColor, READONLY, type Anchor } from './ui'

export interface DbCtx {
  store: DbStore
  ws: string
  people: Map<string, Person>
  canWrite: boolean
  openRow: (id: string) => void
  // 工作区里的库（关联选目标库用）
  dbs: { id: string; title: string }[]
  /** 去别的页面（网页里是前端路由；别的宿主默认改 location） */
  navigate: (href: string) => void
  pageHref: (id: string) => string
  /** 行正文编辑器用的服务（链接卡片、上传……） */
  media?: Partial<MediaServices>
}
export const DbContext = createContext<DbCtx | null>(null)
export const useDb = () => useContext(DbContext)!

/** 一格的值：算出来的属性从引擎拿 */
export function cellValue(store: DbStore, p: Prop, row: Row): unknown {
  if (READONLY.has(p.type) || (p.type === 'relation' && p.relation?.mirror_of)) {
    if (p.type === 'button') return null
    return store.engine?.computed(row.id)[p.key] ?? null
  }
  return row.props[p.key] ?? null
}

export const isEmptyVal = (v: unknown) => v === null || v === undefined || v === '' || (Array.isArray(v) && v.length === 0)

function personName(people: Map<string, Person>, ref: string) {
  const id = ref.replace(/^(user|agent):/, '')
  return people.get(id)?.name ?? people.get(ref)?.name ?? id
}

function relatedTitle(store: DbStore, dbId: string | undefined, id: string) {
  if (!dbId) return id
  const s = store.client.database(dbId)
  return s.rowTitle(s.rows.get(id)) || t('untitled')
}

/** 只显示 */
export function CellView({ p, row, wrap }: { p: Prop; row: Row; wrap?: boolean }) {
  const { store, people, openRow } = useDb()
  const v = cellValue(store, p, row)
  const disp = () => store.engine?.display(p.key, v) ?? String(v ?? '')
  switch (p.type) {
    case 'title':
      return <span className={'db-title-text' + (v ? '' : ' is-empty')}>{(v as string) || t('untitled')}</span>
    case 'select':
    case 'status': {
      const o = p.options?.find((x) => x.id === v)
      return o ? <Pill opt={o} status={p.type === 'status'} /> : null
    }
    case 'multi_select':
      return (
        <span className={'db-pills' + (wrap ? ' is-wrap' : '')}>
          {(Array.isArray(v) ? v : []).map((id) => {
            const o = p.options?.find((x) => x.id === id)
            return o ? <Pill key={id} opt={o} /> : null
          })}
        </span>
      )
    case 'checkbox':
      return <span className={'db-check' + (v ? ' is-on' : '')} aria-checked={!!v} role="checkbox" />
    case 'number':
    case 'unique_id':
      return <span className="db-num">{disp()}</span>
    case 'url':
      return v ? (
        <a className="db-link" href={String(v)} target="_blank" rel="noopener noreferrer nofollow" onClick={(e) => e.stopPropagation()}>
          {String(v).replace(/^https?:\/\/(www\.)?/, '')}
        </a>
      ) : null
    case 'email':
      return v ? (
        <a className="db-link" href={`mailto:${v}`} onClick={(e) => e.stopPropagation()}>
          {String(v)}
        </a>
      ) : null
    case 'person':
    case 'created_by':
    case 'updated_by': {
      const list = (Array.isArray(v) ? v : v ? [v] : []) as string[]
      return (
        <span className="db-people">
          {list.map((ref) => {
            const name = personName(people, ref)
            const agent = ref.startsWith('agent:') || people.get(ref.replace(/^\w+:/, ''))?.kind === 'agent'
            return (
              <span key={ref} className="db-person">
                <span className={'avatar avatar-xs' + (agent ? ' is-agent' : '')} style={{ background: colorFor(ref) }}>
                  {agent ? '✦' : initials(name)}
                </span>
                {name}
              </span>
            )
          })}
        </span>
      )
    }
    case 'files':
      return (
        <span className="db-files">
          {(Array.isArray(v) ? (v as { url: string; name?: string }[]) : []).map((f, i) =>
            /\.(png|jpe?g|gif|webp|avif|svg)$/i.test(f.name || f.url) ? (
              <img key={i} className="db-file-thumb" src={f.url.startsWith('/api/') && !/\.svg$/i.test(f.name || f.url) ? f.url + '?w=160' : f.url} alt={f.name ?? ''} loading="lazy" referrerPolicy="no-referrer" />
            ) : (
              <span key={i} className="db-file-chip">
                {f.name || f.url}
              </span>
            ),
          )}
        </span>
      )
    case 'relation':
      return (
        <span className="db-pills">
          {(Array.isArray(v) ? (v as string[]) : []).map((id) => (
            <button
              key={id}
              type="button"
              className="db-rel"
              onClick={(e) => {
                e.stopPropagation()
                if (p.relation?.db_id === store.id) openRow(id)
              }}
            >
              <span className="db-rel-ico">↗</span>
              {relatedTitle(store, p.relation?.db_id, id)}
            </button>
          ))}
        </span>
      )
    case 'button':
      return <ButtonCell p={p} row={row} />
    default:
      return <span className="db-text">{disp()}</span>
  }
}

function ButtonCell({ p, row }: { p: Prop; row: Row }) {
  const { store, canWrite } = useDb()
  const [done, setDone] = useState(false)
  return (
    <button
      type="button"
      className="db-btn-cell"
      disabled={!canWrite}
      onClick={async (e) => {
        e.stopPropagation()
        // 先在本地按配置改属性，再告诉服务器
        if (p.button?.set) store.setCells(Object.entries(p.button.set).map(([key, value]) => ({ row_id: row.id, key, value })))
        store.client.pressButton(store.id, row.id, p.key).catch(() => {})
        setDone(true)
        setTimeout(() => setDone(false), 1200)
      }}
    >
      {done ? t('pressed') : p.button?.label || p.name}
    </button>
  )
}

// ---------------- 编辑 ----------------
export interface EditProps {
  p: Prop
  row: Row
  anchor: Anchor
  // 打开编辑时按的那个键（直接开始打字）
  initial?: string
  onDone: (move?: 'down' | 'right' | 'left' | 'none') => void
}

export function CellEditor(props: EditProps) {
  switch (props.p.type) {
    case 'select':
    case 'status':
    case 'multi_select':
      return <OptionsEditor {...props} />
    case 'date':
      return <DateEditor {...props} />
    case 'person':
      return <PersonEditor {...props} />
    case 'files':
      return <FilesEditor {...props} />
    case 'relation':
      return <RelationEditor {...props} />
    default:
      return <TextEditor {...props} />
  }
}

function TextEditor({ p, row, anchor, initial, onDone }: EditProps) {
  const { store } = useDb()
  const v = row.props[p.key]
  const [text, setText] = useState(initial ?? (v === null || v === undefined ? '' : p.type === 'number' ? String(v) : String(v)))
  const ref = useRef<HTMLTextAreaElement>(null)
  useEffect(() => {
    const el = ref.current!
    el.focus()
    el.setSelectionRange(el.value.length, el.value.length)
  }, [])
  const [error, setError] = useState<string | null>(null)
  const commit = (move: 'down' | 'right' | 'left' | 'none') => {
    let val: unknown
    if (p.type === 'title' || p.type === 'text') val = text.trim() || null
    else {
      const n = tryNormalize(store.engine, p.key, text.trim())
      // 认不出（比如数字格里写了字）：回车时提示，失焦时放弃
      if ('error' in n) {
        if (move === 'none') return onDone('none')
        return setError(n.error)
      }
      val = n.value
    }
    if (JSON.stringify(val ?? null) !== JSON.stringify(v ?? null)) store.setCells([{ row_id: row.id, key: p.key, value: val }])
    onDone(move)
  }
  const multi = p.type === 'text'
  return (
    <>
    {error && (
      <div className="db-cell-err" role="alert" style={{ left: anchor.left, top: anchor.bottom + 4 }}>
        {error}
      </div>
    )}
    <textarea
      ref={ref}
      className={'db-cell-input' + (p.type === 'number' ? ' is-num' : '')}
      style={{ left: anchor.left, top: anchor.top, width: Math.max(anchor.width ?? 160, p.type === 'text' || p.type === 'title' ? 260 : 120), minHeight: anchor.bottom - anchor.top }}
      value={text}
      rows={1}
      inputMode={p.type === 'number' ? 'decimal' : p.type === 'email' ? 'email' : p.type === 'phone' ? 'tel' : undefined}
      onChange={(e) => {
        setText(e.target.value)
        setError(null)
        e.target.style.height = 'auto'
        e.target.style.height = e.target.scrollHeight + 'px'
      }}
      onBlur={() => commit('none')}
      onKeyDown={(e) => {
        if (e.nativeEvent.isComposing) return
        if (e.key === 'Escape') {
          e.preventDefault()
          onDone('none')
        } else if (e.key === 'Enter' && !(multi && e.shiftKey)) {
          e.preventDefault()
          commit('down')
        } else if (e.key === 'Tab') {
          e.preventDefault()
          commit(e.shiftKey ? 'left' : 'right')
        }
        e.stopPropagation()
      }}
    />
    </>
  )
}

function OptionsEditor({ p, row, anchor, initial, onDone }: EditProps) {
  const { store } = useDb()
  const multi = p.type === 'multi_select'
  const cur = row.props[p.key]
  const selected: string[] = multi ? ((cur as string[]) ?? []) : cur ? [cur as string] : []
  const [q, setQ] = useState(initial ?? '')
  const [active, setActive] = useState(0)
  const opts = p.options ?? []
  const shown = opts.filter((o) => o.name.toLowerCase().includes(q.trim().toLowerCase()))
  const canCreate = q.trim() && !opts.some((o) => o.name === q.trim())
  const set = (ids: string[]) => store.setCells([{ row_id: row.id, key: p.key, value: multi ? ids : (ids[0] ?? null) }])
  const pick = (o: SelectOption) => {
    if (multi) set(selected.includes(o.id) ? selected.filter((x) => x !== o.id) : [...selected, o.id])
    else {
      set(selected[0] === o.id ? [] : [o.id])
      onDone('none')
    }
    setQ('')
  }
  const create = async () => {
    const name = q.trim()
    const o: SelectOption = { id: optId(), name, color: randomColor(opts.length) }
    if (p.type === 'status') o.group = 'todo'
    await store.setOptions(p.key, [...opts, o])
    if (multi) set([...selected, o.id])
    else {
      set([o.id])
      onDone('none')
    }
    setQ('')
  }
  const items = [...shown.map((o) => ({ kind: 'opt' as const, o })), ...(canCreate ? [{ kind: 'new' as const }] : [])]
  return (
    <Popover anchor={anchor} onClose={() => onDone('none')} width={Math.max(260, anchor.width ?? 0)} className="db-opt-pop">
      <div className="db-opt-head">
        {selected.map((id) => {
          const o = opts.find((x) => x.id === id)
          return o ? <Pill key={id} opt={o} status={p.type === 'status'} onRemove={() => set(selected.filter((x) => x !== id))} /> : null
        })}
        <input
          autoFocus
          className="db-opt-search"
          value={q}
          placeholder={t('search')}
          onChange={(e) => {
            setQ(e.target.value)
            setActive(0)
          }}
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing) return
            if (e.key === 'ArrowDown') {
              e.preventDefault()
              setActive((a) => Math.min(items.length - 1, a + 1))
            } else if (e.key === 'ArrowUp') {
              e.preventDefault()
              setActive((a) => Math.max(0, a - 1))
            } else if (e.key === 'Enter') {
              e.preventDefault()
              const it = items[active]
              if (it?.kind === 'opt') pick(it.o)
              else if (it?.kind === 'new') create()
            } else if (e.key === 'Backspace' && !q && selected.length) set(selected.slice(0, -1))
            else if (e.key === 'Escape') onDone('none')
            e.stopPropagation()
          }}
        />
      </div>
      <div className="db-opt-list">
        {p.type === 'status'
          ? (['todo', 'doing', 'done'] as const).map((g) => {
              const inG = items.filter((it) => it.kind === 'opt' && (it.o.group ?? 'todo') === g)
              if (!inG.length) return null
              return (
                <div key={g}>
                  <div className="db-opt-group">{g === 'todo' ? t('statusTodo') : g === 'doing' ? t('statusDoing') : t('statusDone')}</div>
                  {inG.map((it) => it.kind === 'opt' && <OptRow key={it.o.id} o={it.o} on={selected.includes(it.o.id)} active={items.indexOf(it) === active} status onPick={() => pick(it.o)} />)}
                </div>
              )
            })
          : items.map((it, i) => (it.kind === 'opt' ? <OptRow key={it.o.id} o={it.o} on={selected.includes(it.o.id)} active={i === active} onPick={() => pick(it.o)} /> : null))}
        {canCreate && (
          <button type="button" className={'db-opt-row' + (active === items.length - 1 ? ' is-active' : '')} onClick={create}>
            <span className="db-opt-new">{t('newOptionLabel')}</span>
            <Pill opt={{ name: q.trim(), color: randomColor(opts.length) }} />
          </button>
        )}
        {!items.length && <div className="db-empty-note">{t('noResults')}</div>}
      </div>
    </Popover>
  )
}

function OptRow({ o, on, active, status, onPick }: { o: SelectOption; on: boolean; active: boolean; status?: boolean; onPick: () => void }) {
  return (
    <button type="button" className={'db-opt-row' + (active ? ' is-active' : '')} onClick={onPick}>
      <Pill opt={o} status={status} />
      {on && <span className="db-opt-check">✓</span>}
    </button>
  )
}

// ---------- 日期 ----------
const pad = (n: number) => String(n).padStart(2, '0')
const iso = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
const parseIso = (s?: string) => (s ? new Date(s.length === 10 ? s + 'T00:00:00' : s) : null)

export function MonthGrid({ month, onMonth, isSel, inRange, onPick }: { month: Date; onMonth: (d: Date) => void; isSel: (d: Date) => boolean; inRange?: (d: Date) => boolean; onPick: (d: Date) => void }) {
  const first = new Date(month.getFullYear(), month.getMonth(), 1)
  const start = new Date(first)
  start.setDate(1 - ((first.getDay() + 6) % 7))
  const days = Array.from({ length: 42 }, (_, i) => new Date(start.getFullYear(), start.getMonth(), start.getDate() + i))
  const today = iso(new Date())
  return (
    <div className="db-month">
      <div className="db-month-head">
        <button type="button" onClick={() => onMonth(new Date(month.getFullYear(), month.getMonth() - 1, 1))} aria-label={t('prev')}>
          ‹
        </button>
        <span>
          {t('monthTitle', { y: month.getFullYear(), m: month.getMonth() + 1 })}
        </span>
        <button type="button" onClick={() => onMonth(new Date(month.getFullYear(), month.getMonth() + 1, 1))} aria-label={t('next')}>
          ›
        </button>
      </div>
      <div className="db-month-grid">
        {t('weekdays')
          .split(',')
          .map((w) => (
            <span key={w} className="db-month-wd">
              {w}
            </span>
          ))}
        {days.map((d) => (
          <button
            key={iso(d)}
            type="button"
            className={'db-month-day' + (d.getMonth() !== month.getMonth() ? ' is-out' : '') + (isSel(d) ? ' is-sel' : '') + (inRange?.(d) ? ' is-range' : '') + (iso(d) === today ? ' is-today' : '')}
            onClick={() => onPick(d)}
          >
            {d.getDate()}
          </button>
        ))}
      </div>
    </div>
  )
}

function DateEditor({ p, row, anchor, onDone }: EditProps) {
  const { store } = useDb()
  const v = (row.props[p.key] as DateValue | undefined) ?? null
  const [val, setVal] = useState<DateValue | null>(v)
  const [ranged, setRanged] = useState(!!v?.end)
  const [month, setMonth] = useState(() => parseIso(v?.start) ?? new Date())
  const [picking, setPicking] = useState<'start' | 'end'>('start')
  const [text, setText] = useState(v ? store.engine?.display(p.key, v) ?? '' : '')
  const save = (x: DateValue | null) => {
    setVal(x)
    setText(x ? store.engine?.display(p.key, x) ?? '' : '')
    store.setCells([{ row_id: row.id, key: p.key, value: x }])
  }
  const timeOf = (s?: string) => (s && s.includes('T') ? s.slice(11, 16) : '09:00')
  const withTime = (day: string, s?: string) => (val?.time ? `${day}T${timeOf(s)}` : day)
  return (
    <Popover anchor={anchor} onClose={() => onDone('none')} width={264} className="db-date-pop">
      <input
        autoFocus
        className="db-date-text"
        value={text}
        placeholder="2026-11-20"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            const r = tryNormalize(store.engine, p.key, text)
            const n = 'value' in r ? (r.value as DateValue | null) : null
            if (n) {
              save(n)
              setMonth(parseIso(n.start)!)
            }
          }
          e.stopPropagation()
        }}
      />
      <MonthGrid
        month={month}
        onMonth={setMonth}
        isSel={(d) => iso(d) === val?.start?.slice(0, 10) || iso(d) === val?.end?.slice(0, 10)}
        inRange={(d) => !!(val?.end && iso(d) > val.start.slice(0, 10) && iso(d) < val.end.slice(0, 10))}
        onPick={(d) => {
          const day = iso(d)
          if (!ranged || picking === 'start' || !val) {
            save({ ...(val ?? {}), start: withTime(day, val?.start), ...(ranged && val?.end && val.end.slice(0, 10) >= day ? {} : { end: undefined }) } as DateValue)
            if (ranged) setPicking('end')
          } else {
            const [a, b] = day < val.start.slice(0, 10) ? [day, val.start.slice(0, 10)] : [val.start.slice(0, 10), day]
            save({ ...val, start: withTime(a, val.start), end: withTime(b, val.end) })
            setPicking('start')
          }
        }}
      />
      {val?.time && (
        <div className="db-date-times">
          <input type="time" value={timeOf(val.start)} onChange={(e) => save({ ...val, start: `${val.start.slice(0, 10)}T${e.target.value}` })} />
          {val.end && <input type="time" value={timeOf(val.end)} onChange={(e) => save({ ...val, end: `${val.end!.slice(0, 10)}T${e.target.value}` })} />}
        </div>
      )}
      <div className="db-date-opts">
        <label>
          <input
            type="checkbox"
            checked={ranged}
            onChange={(e) => {
              setRanged(e.target.checked)
              if (!e.target.checked && val?.end) save({ ...val, end: undefined })
              setPicking(e.target.checked ? 'end' : 'start')
            }}
          />
          {t('endDate')}
        </label>
        <label>
          <input
            type="checkbox"
            checked={!!val?.time}
            onChange={(e) =>
              val &&
              save(
                e.target.checked
                  ? { ...val, time: true, start: `${val.start.slice(0, 10)}T09:00`, ...(val.end ? { end: `${val.end.slice(0, 10)}T10:00` } : {}) }
                  : { start: val.start.slice(0, 10), ...(val.end ? { end: val.end.slice(0, 10) } : {}) },
              )
            }
          />
          {t('includeTime')}
        </label>
        <button type="button" className="db-link-btn" onClick={() => (save(null), onDone('none'))}>
          {t('clear')}
        </button>
      </div>
    </Popover>
  )
}

// ---------- 人员 ----------
function PersonEditor({ p, row, anchor, onDone }: EditProps) {
  const { store, people } = useDb()
  const cur = ((row.props[p.key] as string[]) ?? []).slice()
  const [q, setQ] = useState('')
  const list = [...people.values()].filter((x) => x.name.toLowerCase().includes(q.toLowerCase()))
  const ref = (x: Person) => `${x.kind === 'agent' ? 'agent' : 'user'}:${x.id}`
  const toggle = (x: Person) => {
    const r = ref(x)
    store.setCells([{ row_id: row.id, key: p.key, value: cur.includes(r) ? cur.filter((y) => y !== r) : [...cur, r] }])
  }
  return (
    <Popover anchor={anchor} onClose={() => onDone('none')} width={260} className="db-opt-pop">
      <div className="db-opt-head">
        <input autoFocus className="db-opt-search" value={q} placeholder={t('search')} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => e.stopPropagation()} />
      </div>
      <div className="db-opt-list">
        {list.map((x) => (
          <button key={x.kind + x.id} type="button" className="db-opt-row" onClick={() => toggle(x)}>
            <span className="db-person">
              <span className={'avatar avatar-xs' + (x.kind === 'agent' ? ' is-agent' : '')} style={{ background: colorFor(ref(x)) }}>
                {x.kind === 'agent' ? '✦' : initials(x.name)}
              </span>
              {x.name}
            </span>
            {cur.includes(ref(x)) && <span className="db-opt-check">✓</span>}
          </button>
        ))}
        {!list.length && <div className="db-empty-note">{t('noResults')}</div>}
      </div>
    </Popover>
  )
}

// ---------- 文件 ----------
function FilesEditor({ p, row, anchor, onDone }: EditProps) {
  const { store, ws } = useDb()
  const files = ((row.props[p.key] as { url: string; name?: string }[]) ?? []).slice()
  const [busy, setBusy] = useState(0)
  const [link, setLink] = useState('')
  const set = (v: { url: string; name?: string }[]) => store.setCells([{ row_id: row.id, key: p.key, value: v }])
  const up = useMemo(() => (f: File, prog: (x: number) => void) => store.client.upload(ws, f, prog), [ws, store])
  return (
    <Popover anchor={anchor} onClose={() => onDone('none')} width={300} className="db-opt-pop">
      <div className="db-opt-list">
        {files.map((f, i) => (
          <div key={i} className="db-file-row">
            <a href={f.url} target="_blank" rel="noopener noreferrer">
              {f.name || f.url}
            </a>
            <button type="button" className="db-pill-x" onClick={() => set(files.filter((_, j) => j !== i))}>
              ×
            </button>
          </div>
        ))}
      </div>
      <div className="db-file-actions">
        <label className="btn btn-sm">
          {busy ? `${t('upload')}…` : t('upload')}
          <input
            type="file"
            multiple
            hidden
            onChange={async (e) => {
              const list = Array.from(e.target.files ?? [])
              setBusy(list.length)
              const got: { url: string; name: string }[] = []
              for (const f of list) {
                try {
                  const r = await up(f, () => {})
                  got.push({ url: r.url, name: r.name })
                } catch {
                  /* 跳过传不上的 */
                }
              }
              setBusy(0)
              if (got.length) set([...files, ...got])
            }}
          />
        </label>
        <input
          className="db-opt-search"
          value={link}
          placeholder={t('addLink')}
          onChange={(e) => setLink(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && link.trim()) {
              const url = link.trim()
              set([...files, { url, name: url.split('/').pop() || url }])
              setLink('')
            }
            e.stopPropagation()
          }}
        />
      </div>
    </Popover>
  )
}

// ---------- 关联 ----------
function RelationEditor({ p, row, anchor, onDone }: EditProps) {
  const { store } = useDb()
  const target = store.client.database(p.relation?.db_id ?? store.id)
  useSyncExternalStore(target.subscribe, target.getVersion)
  useEffect(() => {
    target.open()
  }, [target])
  const cur = ((row.props[p.key] as string[]) ?? []).slice()
  const [q, setQ] = useState('')
  const all = [...target.rows.values()]
  const shown = useMemo(() => {
    const s = q.trim().toLowerCase()
    return all.filter((r) => !s || target.rowTitle(r).toLowerCase().includes(s)).slice(0, 60)
  }, [q, all.length]) // eslint-disable-line react-hooks/exhaustive-deps
  const toggle = (id: string) => store.setCells([{ row_id: row.id, key: p.key, value: cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id] }])
  return (
    <Popover anchor={anchor} onClose={() => onDone('none')} width={320} className="db-opt-pop">
      <div className="db-opt-head">
        <input autoFocus className="db-opt-search" value={q} placeholder={t('searchRows')} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => e.stopPropagation()} />
      </div>
      <div className="db-opt-list">
        {!target.meta && <div className="db-empty-note">{t('noRelatedDb')}</div>}
        {shown.map((r) => (
          <button key={r.id} type="button" className="db-opt-row" onClick={() => toggle(r.id)}>
            <span className="db-rel">
              <span className="db-rel-ico">↗</span>
              {target.rowTitle(r) || t('untitled')}
            </span>
            {cur.includes(r.id) && <span className="db-opt-check">✓</span>}
          </button>
        ))}
      </div>
    </Popover>
  )
}
