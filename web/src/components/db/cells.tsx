// 数据库的格子：每种属性怎么显示、点了怎么改。表格、看板的卡片、行页面顶上的属性栏都用它。
// 改好的值按存的样子交给 onSave（选项 id、agent:3、{start, end}）；新的选项名直接交名字，服务器会加上这个选项。
import { createContext, useContext, useEffect, useRef, useState } from 'react'
import { openPage, useStore } from '../../store'
import { t } from '../../i18n'
import { useAuthors, who } from '../../authors'
import { notice } from '../Dialog'
import { Popover } from './Popover'
import { calcText, dbApi, display, fmtNumber, isEmpty, optionOf, readOnly, valueOf, type DbFile, type DbOption, type DbProp, type DbRow } from './db'

/** 格子要的上下文：在哪个库、关联到的行的标题（服务器给的 refs，选了新的行补进来）、点了按钮以后怎么刷新。 */
export type DbCtxValue = { dbId: number; refs: Record<number, string>; addRefs: (m: Record<number, string>) => void; onPressed: () => void }
export const DbCtx = createContext<DbCtxValue>({ dbId: 0, refs: {}, addRefs: () => {}, onPressed: () => {} })

function Links({ ids }: { ids: number[] }) {
  const { refs } = useContext(DbCtx)
  return <span className="db-chips">{ids.filter(id => id in refs).map(id => (
    <button key={id} className="db-link-chip" title={t('打开')} onClick={e => { e.stopPropagation(); openPage(id) }}>{refs[id] || t('无标题')}</button>))}</span>
}

function Files({ v }: { v: DbFile[] }) {
  return <span className="db-chips">{v.map((f, k) => (
    <a key={k} className="db-file" href={f.url} target="_blank" rel="noreferrer" onClick={e => e.stopPropagation()} title={f.name}>
      {/^\/api\/media\//.test(f.url) && /\.(png|jpe?g|webp|gif)$/i.test(f.name) ? <img src={f.url} alt="" /> : <span className="db-file-icon">⎘</span>}
      <span>{f.name}</span></a>))}</span>
}

/** 文字格里的 [标题](地址) 和网址点得开；站内的页面就地打开。 */
function Linked({ text }: { text: string }) {
  const parts: React.ReactNode[] = []
  const re = /\[([^\]\n]+)\]\(((?:https?:\/\/|\/pages\/)[^)\s]+)\)|(https?:\/\/[^\s<>)\]，。；]+)/g
  let last = 0, m: RegExpExecArray | null
  while ((m = re.exec(text))) {
    if (m.index > last) parts.push(text.slice(last, m.index))
    const url = m[2] || m[3], label = m[1] || m[3]
    const page = url.match(/^\/pages\/(\d+)$/)
    parts.push(page
      ? <a key={m.index} href={url} onClick={e => { e.preventDefault(); e.stopPropagation(); openPage(Number(page[1])) }}>{label}</a>
      : <a key={m.index} href={url} target="_blank" rel="noreferrer" onClick={e => e.stopPropagation()}>{label}</a>)
    last = m.index + m[0].length
  }
  if (!parts.length) return <>{text}</>
  if (last < text.length) parts.push(text.slice(last))
  return <>{parts}</>
}

function Calc({ prop, v }: { prop: DbProp; v: any }) {
  if (typeof v === 'boolean') return <span className={'db-check' + (v ? ' on' : '')}>{v ? '✓' : ''}</span>
  if (typeof v === 'number') return <span className="db-num">{fmtNumber(prop, v)}</span>
  return <span className="db-text">{calcText(prop, v)}</span>
}

export function Chip({ o }: { o?: DbOption }) {
  if (!o) return <span className="db-chip">?</span>
  return <span className={'db-chip c-' + (o.color || 'default')}>{o.name}</span>
}

function Person({ by }: { by: string }) {
  const nameOf = useAuthors()
  const w = who(by)
  return <span className="db-person"><i style={{ background: w?.color || 'var(--fg3)' }} />{nameOf(by)}</span>
}

/** 只读的显示。 */
export function Value({ prop, v }: { prop: DbProp; v: any }) {
  if (prop.type === 'formula' || prop.type === 'rollup') return v === null || v === undefined || v === '' ? null : <Calc prop={prop} v={v} />
  if (isEmpty(v) && prop.type !== 'checkbox') return null
  switch (prop.type) {
    case 'relation': return <Links ids={v} />
    case 'files': return <Files v={v} />
    case 'unique_id': return <span className="db-num db-id">{display(prop, v)}</span>
    case 'select': case 'status': return <Chip o={optionOf(prop, v)} />
    case 'multi_select': return <span className="db-chips">{(v as string[]).map(id => <Chip key={id} o={optionOf(prop, id)} />)}</span>
    case 'person': return <span className="db-chips">{(v as string[]).map(x => <Person key={x} by={x} />)}</span>
    case 'created_by': case 'updated_by': return <Person by={v} />
    case 'checkbox': return <span className={'db-check' + (v ? ' on' : '')}>{v ? '✓' : ''}</span>
    case 'number': return <span className="db-num">{fmtNumber(prop, v)}</span>
    case 'url': return <a href={v} target="_blank" rel="noreferrer" onClick={e => e.stopPropagation()}>{v.replace(/^https?:\/\//, '')}</a>
    case 'email': return <a href={'mailto:' + v} onClick={e => e.stopPropagation()}>{v}</a>
    case 'phone': return <a href={'tel:' + v} onClick={e => e.stopPropagation()}>{v}</a>
    case 'text': return <span className="db-text"><Linked text={String(v)} /></span>
    default: return <span className="db-text">{display(prop, v)}</span>
  }
}

const INLINE = new Set(['title', 'text', 'number', 'url', 'email', 'phone'])

/** 能点着改的格子。readOnly：锁定的库、自动算的属性。 */
export function Cell({ prop, row, readOnly: ro, onSave, className = '' }: {
  prop: DbProp; row: DbRow; readOnly?: boolean; onSave: (v: any) => void; className?: string
}) {
  const v = valueOf(row, prop)
  const ref = useRef<HTMLDivElement>(null)
  const [open, setOpen] = useState(false)
  const ctx = useContext(DbCtx)
  const nameOf = useAuthors()
  const locked = ro || readOnly(prop)
  if (prop.type === 'button') {
    const press = async (e: React.MouseEvent) => {
      e.stopPropagation()
      try {
        const r = await dbApi.press(ctx.dbId, row.id, prop.key)
        ctx.onPressed()
        if (r.comment) notice(r.comment.woke?.length ? t('评论写好了，通知了 {names}。', { names: r.comment.woke.map((id: number) => nameOf(`agent:${id}`)).join(t('、')) }) : t('评论写好了。'))
      } catch (err: any) { notice(err.message) }
    }
    return <div className={'db-cell t-button ' + className}><button className="btn db-button" onClick={press}>{prop.label || prop.name}</button></div>
  }
  const click = () => {
    if (locked) return
    if (prop.type === 'checkbox') return onSave(!v)
    setOpen(true)
  }
  if (open && INLINE.has(prop.type)) {
    return <div ref={ref} className={'db-cell editing ' + className}><InlineEdit prop={prop} v={v} onDone={x => { setOpen(false); if (x !== undefined) onSave(x) }} /></div>
  }
  return (
    <div ref={ref} className={'db-cell t-' + prop.type + (locked ? ' ro' : '') + (open ? ' active' : '') + ' ' + className} onClick={click}
      tabIndex={locked ? undefined : 0} onKeyDown={e => { if (e.key === 'Enter' && !open) { e.preventDefault(); click() } }}>
      <Value prop={prop} v={v} />
      {open && <Popover anchor={ref.current} onClose={() => setOpen(false)} width={prop.type === 'date' ? 280 : 250}>
        {(prop.type === 'select' || prop.type === 'status' || prop.type === 'multi_select') &&
          <OptionPicker prop={prop} value={v} onChange={x => { onSave(x); if (prop.type !== 'multi_select') setOpen(false) }} />}
        {prop.type === 'date' && <DatePicker v={v} onChange={onSave} />}
        {prop.type === 'person' && <PersonPicker v={v || []} onChange={onSave} />}
        {prop.type === 'relation' && <RelationPicker prop={prop} v={(v || []).filter((id: number) => id in ctx.refs)} onChange={onSave} />}
        {prop.type === 'files' && <FilesEditor v={v || []} onChange={onSave} />}
      </Popover>}
    </div>
  )
}

/** 文字、数字、网址这些：就地改，回车或点别处存，Esc 不存。undefined 是没改。 */
function InlineEdit({ prop, v, onDone }: { prop: DbProp; v: any; onDone: (v: any) => void }) {
  const [s, setS] = useState(v === null || v === undefined ? '' : String(v))
  const done = useRef(false)
  const finish = (save: boolean) => {
    if (done.current) return
    done.current = true
    const orig = v === null || v === undefined ? '' : String(v)
    onDone(save && s !== orig ? (prop.type === 'number' ? (s.trim() === '' ? null : s) : s.trim() === '' && prop.type !== 'title' ? null : s) : undefined)
  }
  const multi = prop.type === 'text'
  const common = {
    autoFocus: true, value: s, className: 'db-input',
    onChange: (e: any) => setS(e.target.value),
    onBlur: () => finish(true),
    onKeyDown: (e: React.KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); finish(false) }
      if (e.key === 'Enter' && !e.nativeEvent.isComposing && (!multi || !e.shiftKey)) { e.preventDefault(); finish(true) }
    },
  }
  return multi ? <textarea {...common} rows={Math.min(6, Math.max(1, s.split('\n').length))} />
    : <input {...common} inputMode={prop.type === 'number' ? 'decimal' : prop.type === 'phone' ? 'tel' : undefined}
      type={prop.type === 'email' ? 'email' : 'text'} placeholder={prop.type === 'title' ? t('无标题') : ''} />
}

/** 单选、状态、多选：搜索，点选；单选、多选里没有的名字回车就加一个（状态不加）。 */
export function OptionPicker({ prop, value, onChange }: { prop: DbProp; value: any; onChange: (v: any) => void }) {
  const [q, setQ] = useState('')
  const multi = prop.type === 'multi_select'
  const cur: string[] = multi ? value || [] : value ? [value] : []
  const opts = prop.options || []
  const shown = opts.filter(o => o.name.toLowerCase().includes(q.trim().toLowerCase()))
  const exact = opts.find(o => o.name.toLowerCase() === q.trim().toLowerCase())
  const canAdd = prop.type !== 'status' && q.trim() && !exact
  const pick = (id: string) => {
    if (!multi) return onChange(cur[0] === id ? null : id)
    onChange(cur.includes(id) ? cur.filter(x => x !== id) : [...cur, id])
  }
  const add = () => {
    const name = q.trim()
    setQ('')
    onChange(multi ? [...cur, name] : name)
  }
  return (
    <div className="db-picker">
      <input className="db-input" autoFocus value={q} placeholder={prop.type === 'status' ? t('找一个状态') : t('找一个选项，或者输入新的')}
        onChange={e => setQ(e.target.value)} onKeyDown={e => {
          if (e.key !== 'Enter' || e.nativeEvent.isComposing) return
          e.preventDefault()
          if (exact) pick(exact.id)
          else if (canAdd) add()
          else if (shown.length === 1) pick(shown[0].id)
        }} />
      <div className="db-picker-list">
        {shown.map(o => <button key={o.id} className={'db-picker-item' + (cur.includes(o.id) ? ' on' : '')} onClick={() => pick(o.id)}>
          <Chip o={o} />{cur.includes(o.id) && <span className="grow db-tick">✓</span>}</button>)}
        {canAdd && <button className="db-picker-item" onClick={add}>{t('新建')} <span className="db-chip c-gray">{q.trim()}</span></button>}
        {!shown.length && !canAdd && <div className="muted small db-picker-empty">{t('没有选项')}</div>}
      </div>
      {cur.length > 0 && <button className="link-btn small db-clear" onClick={() => onChange(null)}>{t('清除')}</button>}
    </div>
  )
}

/** 日期：开始日期，可以加时间、加结束日期。 */
export function DatePicker({ v, onChange }: { v: { start: string; end?: string } | null; onChange: (v: any) => void }) {
  const split = (s?: string) => (s ? [s.slice(0, 10), s.length > 10 ? s.slice(11, 16) : ''] : ['', ''])
  const [d0, h0] = split(v?.start), [d1, h1] = split(v?.end)
  const [timed, setTimed] = useState(!!h0)
  const [ranged, setRanged] = useState(!!v?.end)
  const save = (sd: string, sh: string, ed: string, eh: string, tm = timed, rg = ranged) => {
    if (!sd) return onChange(null)
    const start = sd + (tm && sh ? ' ' + sh : '')
    const end = rg && ed ? ed + (tm && eh ? ' ' + eh : '') : undefined
    onChange(end && end >= start ? { start, end } : { start })
  }
  return (
    <div className="db-date">
      <div className="db-date-row"><input type="date" value={d0} onChange={e => save(e.target.value, h0, d1, h1)} />
        {timed && <input type="time" value={h0} onChange={e => save(d0, e.target.value, d1, h1)} />}</div>
      {ranged && <div className="db-date-row"><span className="muted small">→</span><input type="date" value={d1} min={d0} onChange={e => save(d0, h0, e.target.value, h1)} />
        {timed && <input type="time" value={h1} onChange={e => save(d0, h0, d1, e.target.value)} />}</div>}
      <label className="db-toggle"><input type="checkbox" checked={ranged} onChange={e => { setRanged(e.target.checked); if (!e.target.checked) save(d0, h0, '', '', timed, false) }} />{t('结束日期')}</label>
      <label className="db-toggle"><input type="checkbox" checked={timed} onChange={e => { setTimed(e.target.checked); if (!e.target.checked) save(d0, '', d1, '', false) }} />{t('带时间')}</label>
      <div className="db-date-row">
        <button className="link-btn small" onClick={() => { const d = new Date(); const s = new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 10); save(s, h0, d1, h1) }}>{t('今天')}</button>
        <span className="grow" />{v && <button className="link-btn small" onClick={() => onChange(null)}>{t('清除')}</button>}
      </div>
    </div>
  )
}

/** 关联：在关联到的库里按名称找行，点了加上；已经选的可以去掉。 */
export function RelationPicker({ prop, v, onChange }: { prop: DbProp; v: number[]; onChange: (v: any) => void }) {
  const { refs, addRefs } = useContext(DbCtx)
  const [q, setQ] = useState('')
  const [found, setFound] = useState<{ id: number; title: string }[] | null>(null)
  useEffect(() => {
    if (!prop.db) return
    const h = setTimeout(() => dbApi.find(prop.db!, q).then(setFound).catch(() => setFound([])), q ? 200 : 0)
    return () => clearTimeout(h)
  }, [q, prop.db])
  const add = (r: { id: number; title: string }) => { addRefs({ [r.id]: r.title }); onChange([...v, r.id]) }
  return (
    <div className="db-picker">
      {v.length > 0 && <div className="db-chips db-picked">{v.map(id => (
        <span key={id} className="db-link-chip">{refs[id] || t('无标题')}
          <button className="icon-btn small" title={t('去掉')} onClick={() => onChange(v.filter(x => x !== id))}>×</button></span>))}</div>}
      <input className="db-input" autoFocus value={q} placeholder={t('找一行')} onChange={e => setQ(e.target.value)} />
      <div className="db-picker-list">
        {found === null ? <div className="muted small db-picker-empty">{t('加载中…')}</div>
          : found.filter(r => !v.includes(r.id)).map(r => <button key={r.id} className="db-picker-item" onClick={() => add(r)}>{r.title || <span className="muted">{t('无标题')}</span>}</button>)}
        {found && !found.filter(r => !v.includes(r.id)).length && <div className="muted small db-picker-empty">{t('没有找到')}</div>}
      </div>
    </div>
  )
}

/** 文件：传上来（任何文件，最多 10 MB）或者贴一个网址；点 × 去掉。 */
export function FilesEditor({ v, onChange }: { v: DbFile[]; onChange: (v: any) => void }) {
  const [url, setUrl] = useState('')
  const [busy, setBusy] = useState(false)
  const pick = useRef<HTMLInputElement>(null)
  const upload = async (files: FileList | null) => {
    if (!files?.length) return
    setBusy(true)
    const out = [...v]
    try {
      for (const f of Array.from(files)) {
        const r = await fetch(`/api/media?file=1&name=${encodeURIComponent(f.name)}`, { method: 'PUT', body: f })
        const d = await r.json().catch(() => ({}))
        if (!r.ok) throw new Error(d.detail || t('上传失败（{status}）', { status: r.status }))
        out.push({ name: d.name, url: `/api/media/${d.id}` })
      }
      onChange(out)
    } catch (e: any) { notice(e.message) } finally { setBusy(false); if (pick.current) pick.current.value = '' }
  }
  const addUrl = () => {
    const u = url.trim()
    if (!u) return
    onChange([...v, { name: u.split('?')[0].replace(/\/+$/, '').split('/').pop() || u, url: /^https?:\/\//i.test(u) ? u : 'https://' + u }])
    setUrl('')
  }
  return (
    <div className="db-picker db-files-edit">
      {v.map((f, k) => <div key={k} className="db-file-row">
        <a href={f.url} target="_blank" rel="noreferrer">{f.name}</a>
        <button className="icon-btn small" title={t('去掉')} onClick={() => onChange(v.filter((_, i) => i !== k))}>×</button></div>)}
      <input ref={pick} type="file" multiple hidden onChange={e => upload(e.target.files)} />
      <button className="btn" disabled={busy} onClick={() => pick.current?.click()}>{busy ? t('上传中…') : t('传文件')}</button>
      <input className="db-input" value={url} placeholder={t('或者贴一个网址，回车')} onChange={e => setUrl(e.target.value)}
        onKeyDown={e => { if (e.key === 'Enter' && !e.nativeEvent.isComposing) { e.preventDefault(); addUrl() } }} />
    </div>
  )
}

/** 人员：你、没撤销的 Agent，多选。 */
export function PersonPicker({ v, onChange }: { v: string[]; onChange: (v: any) => void }) {
  const agents = useStore(s => s.agents)
  const [q, setQ] = useState('')
  const people: [string, string][] = [['user', t('你')], ...agents.filter(a => !a.revoked).map(a => [`agent:${a.id}`, a.name] as [string, string])]
  const shown = people.filter(([, n]) => n.toLowerCase().includes(q.trim().toLowerCase()))
  const toggle = (id: string) => onChange(v.includes(id) ? v.filter(x => x !== id) : [...v, id])
  return (
    <div className="db-picker">
      {people.length > 6 && <input className="db-input" autoFocus value={q} placeholder={t('找人')} onChange={e => setQ(e.target.value)} />}
      <div className="db-picker-list">
        {shown.map(([id]) => <button key={id} className={'db-picker-item' + (v.includes(id) ? ' on' : '')} onClick={() => toggle(id)}>
          <Person by={id} />{v.includes(id) && <span className="grow db-tick">✓</span>}</button>)}
      </div>
      {v.length > 0 && <button className="link-btn small db-clear" onClick={() => onChange(null)}>{t('清除')}</button>}
    </div>
  )
}
