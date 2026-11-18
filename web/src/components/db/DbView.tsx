// 资料库的数据库页面（open_agent_docs/dbs.py）：视图标签（表格 / 看板）、筛选、排序、显示哪些属性、新建；表格每格点了就改，看板拖动换列。
// 改了服务器发 db_changed（store 转成 oad-db 事件），开着的库重新取一遍：别人（Agent、另一个窗口）改的马上看到。
// 格子先改界面再存；存的时候带着这一行的版本号，同一个属性被别人先改了（409）就提示、重新取。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ApiError, api, openPage } from '../../store'
import { locale, t } from '../../i18n'
import { ask, notice } from '../Dialog'
import { nameOf } from '../../authors'
import { Popover } from './Popover'
import { Cell, Chip, DbCtx, Value } from './cells'
import { AddProp, FieldsEditor, FilterEditor, HeaderMenu, SchemaHistory, SortEditor, ViewMenu, changeText, type Change, type HistoryItem } from './menus'
import { PAGE, dbApi, isEmpty, typeIcon, valueOf, type DbData, type DbProp, type DbRow, type DbView as View } from './db'

type Pop = { kind: 'header' | 'add' | 'filter' | 'sort' | 'fields' | 'view' | 'row' | 'newview' | 'history'; anchor: HTMLElement; prop?: DbProp; row?: DbRow; view?: View }

const when = (ts: number) => new Date(ts * 1000).toLocaleString(locale, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
const stored = (id: number) => { try { return localStorage.getItem(`oad.dbview.${id}`) } catch { return null } }

export default function DbView({ id }: { id: number }) {
  const [data, setData] = useState<DbData | null>(null)
  const [err, setErr] = useState('')
  const [viewId, setViewId] = useState<string | null>(() => stored(id))
  const [pop, setPop] = useState<Pop | null>(null)
  const [fresh, setFresh] = useState<number | null>(null)   // 刚加的行：标题直接进入编辑
  const [hist, setHist] = useState<HistoryItem[] | null>(null)
  const loadHist = useCallback(() => api<HistoryItem[]>(`/api/dbs/${id}/history`).then(setHist).catch(() => {}), [id])
  const [extraRefs, setExtraRefs] = useState<Record<number, string>>({})   // 关联里刚选的行的标题（服务器下次给之前先用着）
  // 一次取 PAGE 行，滚到底再多取一页；刚加的行不在前面这些里也带上（include），免得新建了看不见
  const [limit, setLimit] = useState(PAGE)
  const added = useRef<number[]>([])
  const vid = useRef(viewId)
  vid.current = viewId
  const lim = useRef(limit)
  lim.current = limit
  const load = useCallback(() => dbApi.get(id, vid.current, lim.current, added.current).then(d => { setData(d); setErr('') })
    .catch((e: any) => setErr(e.message)), [id])
  useEffect(() => { setViewId(stored(id)); setLimit(PAGE); added.current = [] }, [id])
  useEffect(() => { load() }, [id, viewId, limit, load])
  useEffect(() => {
    const h = (e: Event) => { if ((e as CustomEvent).detail?.db_id === id) load() }
    const again = () => load()
    window.addEventListener('oad-db', h)
    window.addEventListener('oad-page-resync', again)
    return () => { window.removeEventListener('oad-db', h); window.removeEventListener('oad-page-resync', again) }
  }, [id, load])
  const pick = (v: string) => { try { localStorage.setItem(`oad.dbview.${id}`, v) } catch { /* 记不住就算了 */ } setViewId(v); setLimit(PAGE) }
  // 滚到最下面的那条线露出来：再多取一页
  const more = useRef<HTMLDivElement>(null)
  // 服务器先给前 limit 行，刚加的行在这之外的才接在后面（dbs.public 的 include）：只有前面这段没取全才算还有
  const hasMore = !!data && Math.min(data.rows.length, limit) < data.count
  useEffect(() => {
    const el = more.current
    if (!el || !hasMore) return
    const io = new IntersectionObserver(es => { if (es.some(e => e.isIntersecting)) setLimit(l => Math.max(l, (data?.rows.length || 0)) + PAGE) }, { rootMargin: '400px' })
    io.observe(el)
    return () => io.disconnect()
  }, [hasMore, data?.rows.length])

  if (err && !data) return <div className="error-line">{err}</div>
  if (!data) return <div className="muted">{t('加载中…')}</div>
  const view = data.views.find(v => v.id === data.view) || data.views[0]
  const schema = data.schema

  const failed = (e: any) => {
    if (e instanceof ApiError && e.status === 409) {
      const c = e.data?.detail?.conflicts?.[0]
      notice(c ? t('「{name}」刚被{who}改成了「{value}」，已经换成现在的样子。', { name: c.name, who: nameOf(c.by), value: c.current || t('空') }) : e.message)
    } else notice(e.message)
    load()
  }
  const save = (row: DbRow, prop: DbProp, value: any) => {
    setData(d => d && { ...d, rows: d.rows.map(r => r.id !== row.id ? r : prop.type === 'title' ? { ...r, title: value }
      : { ...r, props: isEmpty(value) ? Object.fromEntries(Object.entries(r.props).filter(([k]) => k !== prop.key)) : { ...r.props, [prop.key]: value } }) })
    dbApi.update(id, row.id, { [prop.key]: value }, row.version)
      .then(r => setData(d => d && { ...d, rows: d.rows.map(x => (x.id === r.id ? r : x)) })).catch(failed)
  }
  const putView = (v: View) => {
    const views = data.views.map(x => (x.id === v.id ? v : x))
    setData({ ...data, views })
    dbApi.views(id, views, view.id, limit).then(setData).catch(failed)
  }
  // 改表结构：清掉了值的告诉一声，能在“历史”里撤销
  const told = (d: DbData & { last_change?: Change[] }) => {
    setData(d)
    const lost = (d.last_change || []).filter(c => c.cleared)
    if (lost.length) notice(lost.map(changeText).join('；') + t('。点“历史”能撤销。'))
    if (pop?.kind === 'history') loadHist()
  }
  const schemaOps = (ops: any[]) => dbApi.schema(id, ops, view.id, limit).then(told).catch(failed)
  const undo = async (h: HistoryItem) => {
    if (!(await ask(t('撤销这次改动？{what}', { what: h.changes.map(changeText).join('；') }), { ok: t('撤销') }))) return
    api<DbData>(`/api/dbs/${id}/history/${h.id}/undo`, { method: 'POST', body: { view: view.id, limit } }).then(d => { setData(d); loadHist() }).catch(failed)
  }
  const addView = (type: View['type']) => {
    const group = schema.find(p => p.type === 'status') || schema.find(p => p.type === 'select')
    const v = { id: '', name: type === 'board' ? t('看板') : t('表格'), type, ...(type === 'board' ? { group_by: group!.key } : {}) } as View
    dbApi.views(id, [...data.views, v]).then(d => { setData(d); pick(d.views[d.views.length - 1].id) }).catch(failed)
  }
  const delView = async (v: View) => {
    setPop(null)
    if (!(await ask(t('删除视图「{name}」？', { name: v.name }), { ok: t('删除'), danger: true }))) return
    const rest = data.views.filter(x => x.id !== v.id)
    dbApi.views(id, rest, rest[0].id).then(d => { setData(d); pick(rest[0].id) }).catch(failed)
  }
  // 新建一行：筛选条件里“是某个选项 / 含某人 / 勾了”的值先填上，免得新行一出来就被筛掉
  const prefill = () => {
    const out: Record<string, any> = {}
    for (const f of view.filter || []) {
      const p = schema.find(x => x.key === f.prop)
      if (!p || isEmpty(f.value)) continue
      if ((p.type === 'select' || p.type === 'status') && f.op === 'eq') out[p.key] = f.value
      if ((p.type === 'multi_select' || p.type === 'person') && f.op === 'contains') out[p.key] = [f.value]
      if (p.type === 'checkbox' && f.op === 'eq' && f.value) out[p.key] = true
    }
    return out
  }
  const addRow = async (props: Record<string, any> = {}) => {
    try {
      const r = await dbApi.add(id, { props: { ...prefill(), ...props } })
      added.current = [...added.current, r.id].slice(-50)
      setFresh(r.id)
      await load()
    } catch (e: any) { failed(e) }
  }
  const delRow = async (r: DbRow) => {
    setPop(null)
    if (!(await ask(t('删除「{title}」这一行？（先放进回收站，30 天内能恢复）', { title: r.title || t('无标题') }), { ok: t('删除'), danger: true }))) return
    setData({ ...data, rows: data.rows.filter(x => x.id !== r.id), total: data.total - 1, count: data.count - 1 })
    dbApi.remove(id, r.id).catch(failed)
  }
  const open = (kind: Pop['kind'], extra: Partial<Pop> = {}) => (e: React.MouseEvent<HTMLElement>) => {
    e.stopPropagation()
    setPop(pop?.kind === kind && pop.anchor === e.currentTarget ? null : { kind, anchor: e.currentTarget, ...extra })
  }
  const nf = view.filter?.length || 0, ns = view.sort?.length || 0
  const hidden = new Set(view.hidden || [])
  const cols = schema.filter(p => p.type === 'title' || !hidden.has(p.key))

  const ctx = { dbId: id, refs: { ...extraRefs, ...data.refs }, addRefs: (m: Record<number, string>) => setExtraRefs(x => ({ ...x, ...m })), onPressed: load }
  return (
    <DbCtx.Provider value={ctx}>
    <div className="db">
      <div className="db-bar">
        <div className="db-tabs">
          {data.views.map(v => (
            <button key={v.id} className={'db-tab' + (v.id === view.id ? ' on' : '')}
              onClick={e => (v.id === view.id ? open('view', { view: v })(e) : pick(v.id))}
              title={v.id === view.id ? t('视图设置') : undefined}>
              <span className="db-tab-icon">{v.type === 'board' ? '▥' : '▤'}</span>{v.name}</button>))}
          <button className="db-tab add" title={t('加一个视图')} onClick={open('newview')}>＋</button>{/* i18n-ignore */}
        </div>
        <span className="grow" />
        <button className={'db-tool' + (nf ? ' on' : '')} onClick={open('filter')}>{t('筛选')}{nf ? ` ${nf}` : ''}</button>
        <button className={'db-tool' + (ns ? ' on' : '')} onClick={open('sort')}>{t('排序')}{ns ? ` ${ns}` : ''}</button>
        <button className="db-tool" onClick={open('fields')}>{t('属性')}</button>
        <button className="db-tool" onClick={e => { loadHist(); open('history')(e) }}>{t('历史')}</button>
        <button className="btn primary db-new" onClick={() => addRow()}>{t('新建')}</button>
      </div>
      {data.locked && <div className="muted small db-note">{t('已锁定：Agent 只能读，你照样能改')}</div>}

      {view.type === 'board'
        ? <Board data={data} view={view} save={save} addRow={addRow} fresh={fresh} setFresh={setFresh} failed={failed} reload={load} />
        : <div className="db-table-wrap">
          <table className="db-table">
            <thead><tr>
              {cols.map(p => <Th key={p.key} prop={p} width={view.widths?.[p.key]} onMenu={open('header', { prop: p })}
                onWidth={w => putView({ ...view, widths: { ...(view.widths || {}), [p.key]: w } })} />)}
              <th className="db-th-add"><button title={t('加一个属性')} onClick={open('add')}>＋</button></th>{/* i18n-ignore */}
            </tr></thead>
            <tbody>
              {data.rows.map(r => (
                <tr key={r.id}>
                  {cols.map(p => (
                    <td key={p.key} className={p.type === 'title' ? 'db-td-title' : ''}>
                      {p.type === 'title'
                        ? <TitleCell row={r} auto={fresh === r.id} onSave={v => { setFresh(null); save(r, p, v) }} onMenu={open('row', { row: r })} />
                        : <Cell prop={p} row={r} onSave={v => save(r, p, v)} />}
                    </td>))}
                  <td />
                </tr>))}
            </tbody>
          </table>
          {hasMore ? <div ref={more} className="muted small db-note db-more">{t('还有 {n} 行，往下滚接着加载…', { n: data.count - data.rows.length })}</div>
            : <button className="db-add-row" onClick={() => addRow()}>＋ {t('新建')}</button>}{/* i18n-ignore */}
          {data.count < data.total && <div className="muted small db-note">{t('筛掉了 {n} 行', { n: data.total - data.count })}</div>}
        </div>}
      {view.type === 'board' && hasMore && <div ref={more} className="muted small db-note db-more">{t('还有 {n} 张卡片，往下滚接着加载…', { n: data.count - data.rows.length })}</div>}

      {pop && <Popover anchor={pop.anchor} onClose={() => setPop(null)} width={pop.kind === 'filter' ? 420 : pop.kind === 'header' || pop.kind === 'history' || pop.kind === 'add' ? 340 : 240}>
        {pop.kind === 'header' && pop.prop && <HeaderMenu key={pop.prop.key} prop={schema.find(p => p.key === pop.prop!.key) || pop.prop} schema={schema} view={view}
          onSchema={schemaOps} onView={putView} onClose={() => setPop(null)} />}
        {pop.kind === 'add' && <AddProp schema={schema} onAdd={op => { setPop(null); schemaOps([{ op: 'add', ...op }]) }} />}
        {pop.kind === 'filter' && <FilterEditor schema={schema} view={view} onView={putView} />}
        {pop.kind === 'sort' && <SortEditor schema={schema} view={view} onView={putView} />}
        {pop.kind === 'fields' && <FieldsEditor schema={schema} view={view} onView={putView} />}
        {pop.kind === 'view' && pop.view && <ViewMenu schema={schema} view={view} canDelete={data.views.length > 1} onView={putView} onDelete={() => delView(view)} />}
        {pop.kind === 'history' && <SchemaHistory list={hist} nameOf={nameOf} when={when} onUndo={undo} />}
        {pop.kind === 'newview' && <div className="db-menu">
          <button onClick={() => { setPop(null); addView('table') }}>▤ {t('表格')}</button>
          <button disabled={!schema.some(p => p.type === 'select' || p.type === 'status')} onClick={() => { setPop(null); addView('board') }}>▥ {t('看板')}</button>
        </div>}
        {pop.kind === 'row' && pop.row && <div className="db-menu">
          <button onClick={() => openPage(pop.row!.id)}>{t('打开')}</button>
          <button className="danger-text" onClick={() => delRow(pop.row!)}>{t('删除')}</button>
        </div>}
      </Popover>}
    </div>
    </DbCtx.Provider>
  )
}

/** 表头：点了出菜单；右边缘拖动改列宽。 */
function Th({ prop, width, onMenu, onWidth }: { prop: DbProp; width?: number; onMenu: (e: React.MouseEvent<HTMLElement>) => void; onWidth: (w: number) => void }) {
  const ref = useRef<HTMLTableCellElement>(null)
  const drag = (e: React.MouseEvent) => {
    e.preventDefault()
    e.stopPropagation()
    const th = ref.current!, x0 = e.clientX, w0 = th.offsetWidth
    let w = w0
    const move = (m: MouseEvent) => { w = Math.max(60, Math.min(800, w0 + m.clientX - x0)); th.style.width = w + 'px' }
    const up = () => { removeEventListener('mousemove', move); removeEventListener('mouseup', up); if (w !== w0) onWidth(Math.round(w)) }
    addEventListener('mousemove', move)
    addEventListener('mouseup', up)
  }
  return (
    <th ref={ref} style={{ width: width || (prop.type === 'title' ? 260 : prop.type === 'checkbox' ? 80 : 170) }}>
      <button className="db-th" onClick={onMenu}><span className="db-type-icon">{typeIcon(prop.type)}</span>{prop.name}</button>
      <span className="db-resize" onMouseDown={drag} />
    </th>
  )
}

/** 名称格：点了就地改；右边的“打开”进这一行的页面，“⋯”是这一行的菜单。 */
function TitleCell({ row, auto, onSave, onMenu }: { row: DbRow; auto: boolean; onSave: (v: string) => void; onMenu: (e: React.MouseEvent<HTMLElement>) => void }) {
  const [edit, setEdit] = useState(auto)
  useEffect(() => { if (auto) setEdit(true) }, [auto])
  if (edit) return <TitleInput value={row.title} onDone={v => { setEdit(false); if (v !== undefined) onSave(v) }} />
  return (
    <div className="db-title-cell">
      <span className="db-title-text" onClick={() => setEdit(true)}>{row.title || <span className="muted">{t('无标题')}</span>}</span>
      <button className="db-open" onClick={() => openPage(row.id)}>{t('打开')}</button>
      <button className="db-row-more" title={t('更多')} onClick={onMenu}>⋯</button>
    </div>
  )
}

function TitleInput({ value, onDone }: { value: string; onDone: (v: string | undefined) => void }) {
  const [s, setS] = useState(value)
  const done = useRef(false)
  const finish = (ok: boolean) => { if (done.current) return; done.current = true; onDone(ok && s !== value ? s : undefined) }
  return <input className="db-input" autoFocus value={s} placeholder={t('无标题')} onChange={e => setS(e.target.value)} onBlur={() => finish(true)}
    onKeyDown={e => { if (e.key === 'Escape') finish(false); if (e.key === 'Enter' && !e.nativeEvent.isComposing) finish(true) }} />
}

/** 看板：按单选 / 状态分列（没填的放在最前面的“没有”一列，没有卡片时不显示）。拖动卡片换列；没设排序时也能调顺序。 */
function Board({ data, view, save, addRow, fresh, setFresh, failed, reload }: {
  data: DbData; view: View; save: (r: DbRow, p: DbProp, v: any) => void; addRow: (props?: Record<string, any>) => void
  fresh: number | null; setFresh: (id: number | null) => void; failed: (e: any) => void; reload: () => void
}) {
  const group = data.schema.find(p => p.key === view.group_by)
  const [drag, setDrag] = useState<number | null>(null)
  const [over, setOver] = useState<{ col: string; index: number } | null>(null)
  const [menu, setMenu] = useState<{ row: number; anchor: HTMLElement } | null>(null)
  const shownProps = data.schema.filter(p => p.type !== 'title' && p.key !== group?.key && !(view.hidden || []).includes(p.key))
  const columns = useMemo(() => {
    if (!group) return []
    const byCol = new Map<string, DbRow[]>()
    for (const r of data.rows) {
      const v = valueOf(r, group)
      const k = v && group.options?.some(o => o.id === v) ? v : ''
      byCol.set(k, [...(byCol.get(k) || []), r])
    }
    const cols = (group.options || []).map(o => ({ id: o.id, o, rows: byCol.get(o.id) || [] }))
    return byCol.get('')?.length || data.groups?.[''] ? [{ id: '', o: undefined, rows: byCol.get('') || [] }, ...cols] : cols
  }, [data.rows, data.groups, group])
  // 手机上长按卡片拖动（HTML5 拖放在 iPhone 上不灵）：长按 350 毫秒拿起，跟着手指走，松手放下；拖到边上自动滚
  const boardRef = useRef<HTMLDivElement>(null)
  const latest = useRef<{ place: (r: DbRow, col: string, index: number) => void; rows: DbRow[] } | null>(null)
  useEffect(() => {
    const el = boardRef.current
    if (!el) return
    type St = { id: number; card: HTMLElement; x: number; y: number; on: boolean; timer: ReturnType<typeof setTimeout>; ghost?: HTMLElement
      dx: number; dy: number; at?: { col: string; index: number }; raf?: number }
    let st: St | null = null
    const scroller = () => el.closest('.scroller') as HTMLElement | null
    const target = (x: number, y: number) => {   // 按横向位置找列：列只有卡片那么高，手指在短的列下面也算放进这一列
      const col = [...el.querySelectorAll<HTMLElement>('.db-col')].find(c => { const r = c.getBoundingClientRect(); return x >= r.left - 6 && x <= r.right + 6 })
      if (!col) return undefined
      const cards = [...col.querySelectorAll<HTMLElement>('.db-card:not(.dragging)')]
      const k = cards.findIndex(c => { const r = c.getBoundingClientRect(); return y < r.top + r.height / 2 })
      return { col: col.dataset.col || '', index: k < 0 ? cards.length : k }
    }
    const tick = () => {   // 手指在边上：横着滚看板、竖着滚页面
      if (!st?.on) return
      const b = el.getBoundingClientRect(), sc = scroller(), edge = 48
      const x0 = el.scrollLeft, y0 = sc?.scrollTop
      if (st.x < b.left + edge) el.scrollLeft -= 10
      else if (st.x > b.right - edge) el.scrollLeft += 10
      if (sc) {
        const r = sc.getBoundingClientRect()
        if (st.y < r.top + edge) sc.scrollTop -= 10
        else if (st.y > r.bottom - edge) sc.scrollTop += 10
      }
      if (el.scrollLeft !== x0 || sc?.scrollTop !== y0) {   // 滚了：手指没动，底下的列变了
        st.at = target(st.x, st.y)
        setOver(st.at || null)
      }
      st.raf = requestAnimationFrame(tick)
    }
    const lift = () => {
      if (!st) return
      st.on = true
      const r = st.card.getBoundingClientRect()
      st.dx = st.x - r.left; st.dy = st.y - r.top
      const g = st.card.cloneNode(true) as HTMLElement
      g.className = 'db-card db-ghost'
      Object.assign(g.style, { width: r.width + 'px', left: r.left + 'px', top: r.top + 'px' })
      document.body.appendChild(g)
      st.ghost = g
      setDrag(st.id)
      navigator.vibrate?.(10)
      st.raf = requestAnimationFrame(tick)
    }
    const start = (e: TouchEvent) => {
      const t0 = e.touches[0]
      const card = (e.target as HTMLElement).closest?.('.db-card') as HTMLElement | null
      if (e.touches.length !== 1 || !card || (e.target as HTMLElement).closest('button, input, textarea, a')) return
      st = { id: Number(card.dataset.id), card, x: t0.clientX, y: t0.clientY, on: false, dx: 0, dy: 0, timer: setTimeout(lift, 350) }
    }
    const move = (e: TouchEvent) => {
      if (!st) return
      const t0 = e.touches[0]
      if (!st.on) {   // 还没拿起来就动了：是在滚动，不拖
        if (Math.hypot(t0.clientX - st.x, t0.clientY - st.y) > 8) { clearTimeout(st.timer); st = null }
        return
      }
      e.preventDefault()
      st.x = t0.clientX; st.y = t0.clientY
      if (st.ghost) Object.assign(st.ghost.style, { left: st.x - st.dx + 'px', top: st.y - st.dy + 'px' })
      st.at = target(st.x, st.y)
      setOver(st.at || null)
    }
    const end = (e: TouchEvent) => {
      if (!st) return
      clearTimeout(st.timer)
      const s0 = st
      st = null
      if (!s0.on) return
      e.preventDefault()   // 不要再算成一次点击（会打开这一行）
      if (s0.raf) cancelAnimationFrame(s0.raf)
      s0.ghost?.remove()
      setDrag(null)
      setOver(null)
      const r = latest.current?.rows.find(x => x.id === s0.id)
      if (r && s0.at && e.type === 'touchend') latest.current!.place(r, s0.at.col, s0.at.index)
    }
    el.addEventListener('touchstart', start, { passive: true })
    el.addEventListener('touchmove', move, { passive: false })
    el.addEventListener('touchend', end)
    el.addEventListener('touchcancel', end)
    return () => {
      el.removeEventListener('touchstart', start)
      el.removeEventListener('touchmove', move)
      el.removeEventListener('touchend', end)
      el.removeEventListener('touchcancel', end)
      if (st) { clearTimeout(st.timer); st.ghost?.remove(); if (st.raf) cancelAnimationFrame(st.raf) }
    }
  }, [!!group])
  if (!group) return <div className="muted">{t('看板要按一个单选或状态属性分列：在视图设置里选一个。')}</div>
  const sorted = !!view.sort?.length
  const indexAt = (colEl: HTMLElement, y: number) => {
    const cards = [...colEl.querySelectorAll<HTMLElement>('.db-card:not(.dragging)')]
    const k = cards.findIndex(c => { const r = c.getBoundingClientRect(); return y < r.top + r.height / 2 })
    return k < 0 ? cards.length : k
  }
  // 放到某一列的第 index 张（不算它自己）：拖动和卡片菜单的“挪到…”“往上挪”都走这里
  const place = async (r: DbRow, colId: string, index: number) => {
    const col = columns.find(c => c.id === colId)
    const rest = (col?.rows || []).filter(x => x.id !== r.id)
    try {
      if ((valueOf(r, group) || '') !== colId) save(r, group, colId || null)
      if (!sorted && rest.length) {   // 手动顺序：排在这一列里前一张卡片后面；放在最前就排在这一列第一张的前面
        const all = data.rows.filter(x => x.id !== r.id)
        const after = index > 0 ? rest[Math.min(index, rest.length) - 1].id : (all[all.indexOf(rest[0]) - 1]?.id ?? 0)
        await dbApi.move(data.id, r.id, after)
        reload()
      }
    } catch (e: any) { failed(e) }
  }
  latest.current = { place, rows: data.rows }
  const drop = (colId: string, index: number) => {
    const r = data.rows.find(x => x.id === drag)
    setDrag(null)
    setOver(null)
    if (r) place(r, colId, index)
  }
  // 手机上 HTML5 拖放不灵：卡片右上角的“⋯”里换列、调顺序（桌面上悬停时也有）
  const colOf = (r: DbRow) => { const v = valueOf(r, group); return v && group.options?.some(o => o.id === v) ? v : '' }
  const menuRow = menu && data.rows.find(x => x.id === menu.row)
  const menuAt = menuRow ? (columns.find(c => c.id === colOf(menuRow))?.rows || []).indexOf(menuRow) : -1
  const menuLen = menuRow ? (columns.find(c => c.id === colOf(menuRow))?.rows.length || 0) : 0
  const pick = (fn: () => void) => { setMenu(null); fn() }
  return (
    <div className="db-board" ref={boardRef}>
      {columns.map(c => (
        <div key={c.id || 'none'} data-col={c.id} className={'db-col' + (over?.col === c.id ? ' over' : '')}
          onDragOver={e => { if (drag == null) return; e.preventDefault(); setOver({ col: c.id, index: indexAt(e.currentTarget, e.clientY) }) }}
          onDragLeave={e => { if (!e.currentTarget.contains(e.relatedTarget as Node)) setOver(null) }}
          onDrop={e => { e.preventDefault(); drop(c.id, indexAt(e.currentTarget, e.clientY)) }}>
          <div className="db-col-head">{c.o ? <Chip o={c.o} /> : <span className="db-chip">{t('没有{name}', { name: group.name })}</span>}
            <span className="muted small">{data.groups?.[c.id] ?? c.rows.length}</span></div>
          {c.rows.map(r => {
            const at = r.id === drag ? -1 : c.rows.filter(x => x.id !== drag).indexOf(r)   // 被拖的那张不算位置
            return <div key={r.id}>
              {over?.col === c.id && over.index === at && <div className="db-drop-line" />}
              <div className={'db-card' + (drag === r.id ? ' dragging' : '')} draggable data-id={r.id}
                onDragStart={e => { e.dataTransfer.effectAllowed = 'move'; e.dataTransfer.setData('text/plain', String(r.id)); setDrag(r.id) }}
                onDragEnd={() => { setDrag(null); setOver(null) }}
                onClick={() => fresh !== r.id && openPage(r.id)}>
                {fresh !== r.id && <button className="db-card-more" title={t('挪到…')}
                  onClick={e => { e.stopPropagation(); setMenu(menu?.row === r.id ? null : { row: r.id, anchor: e.currentTarget }) }}>⋯</button>}
                {fresh === r.id
                  ? <TitleInput value={r.title} onDone={v => { setFresh(null); if (v !== undefined) save(r, data.schema[0], v) }} />
                  : <b className="db-card-title">{r.title || <span className="muted">{t('无标题')}</span>}</b>}
                {shownProps.map(p => { const v = valueOf(r, p); return isEmpty(v) ? null : <div key={p.key} className="db-card-prop"><Value prop={p} v={v} /></div> })}
              </div>
            </div>
          })}
          {over?.col === c.id && over.index >= c.rows.filter(x => x.id !== drag).length && <div className="db-drop-line" />}
          <button className="db-col-add" onClick={() => addRow({ [group.key]: c.id || null })}>＋ {t('新建')}</button>{/* i18n-ignore */}
        </div>))}
      {menu && menuRow && <Popover anchor={menu.anchor} onClose={() => setMenu(null)} width={220}>
        <div className="db-menu db-card-menu">
          <div className="muted small">{t('挪到')}</div>
          {[...(group.options || []).map(o => ({ id: o.id, o })), { id: '', o: undefined }].map(c => (
            <button key={c.id || 'none'} className="db-card-to" onClick={() => pick(() => colOf(menuRow) !== c.id && place(menuRow, c.id, Infinity))}>
              {c.o ? <Chip o={c.o} /> : <span className="db-chip">{t('没有{name}', { name: group.name })}</span>}
              {colOf(menuRow) === c.id && <span className="muted">✓</span>}
            </button>))}
          {!sorted && <>
            <div className="db-menu-sep" />
            <button disabled={menuAt <= 0} onClick={() => pick(() => place(menuRow, colOf(menuRow), menuAt - 1))}>↑ {t('往上挪')}</button>
            <button disabled={menuAt < 0 || menuAt >= menuLen - 1} onClick={() => pick(() => place(menuRow, colOf(menuRow), menuAt + 1))}>↓ {t('往下挪')}</button>
          </>}
        </div>
      </Popover>}
    </div>
  )
}
