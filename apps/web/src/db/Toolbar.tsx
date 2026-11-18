// 库页面的工具栏：筛选（可嵌套的且 / 或）、排序、分组（日历依据、画廊封面）、属性显隐、搜索、新建。
import { useState } from 'react'
import { useDb } from './cells'
import { DbHistory } from './History'
import { COND_NAMES, t, TYPE_NAMES, WITHIN_NAMES } from './strings'
import type { Cond, Filter, Prop, View } from './types'
import { isGroup } from './types'
import { condsFor, NO_VALUE, Pill, Popover, rectOf, TypeIcon, type Anchor } from './ui'

interface Props {
  view: View
  onView: (patch: Partial<View>) => void
  search: string
  onSearch: (s: string) => void
  onNew: () => void
}

export function Toolbar({ view, onView, search, onSearch, onNew }: Props) {
  const { store, canWrite } = useDb()
  const [open, setOpen] = useState<{ kind: 'filter' | 'sort' | 'group' | 'props' | 'history'; anchor: Anchor } | null>(null)
  const [searching, setSearching] = useState(!!search)
  const nFilter = countConds(view.filter)
  const nSort = view.sorts?.length ?? 0
  const groupProp = store.prop(view.group_by ?? '')
  const tool = (kind: 'filter' | 'sort' | 'group' | 'props', label: string, active: boolean) => (
    <button type="button" data-db-tool={kind} title={label} aria-label={label} className={'db-tool' + (active ? ' is-on' : '')} onClick={(e) => setOpen({ kind, anchor: rectOf(e.currentTarget) })}>
      <svg className="db-tool-ico" viewBox="0 0 24 24" aria-hidden>
        <path fill="currentColor" d={TOOL_ICONS[kind]} />
      </svg>
      <span className="db-tool-label">{label}</span>
    </button>
  )
  return (
    <div className="db-toolbar">
      {tool('filter', nFilter ? `${t('filter')} · ${nFilter}` : t('filter'), !!nFilter)}
      {tool('sort', nSort ? `${t('sort')} · ${nSort}` : t('sort'), !!nSort)}
      {view.type !== 'calendar' && tool('group', groupProp ? `${t('group')} · ${groupProp.name}` : t('group'), !!groupProp)}
      {view.type === 'calendar' && tool('group', `${t('calendarBy')} · ${store.prop(view.calendar_by ?? '')?.name ?? '—'}`, true)}
      {tool('props', (view.hidden?.length ?? 0) ? `${t('props')} · ${t('hiddenCount', { n: view.hidden!.length })}` : t('props'), false)}
      <span className="db-toolbar-gap" />
      {searching ? (
        <input
          autoFocus
          className="db-search"
          value={search}
          placeholder={t('search')}
          onChange={(e) => onSearch(e.target.value)}
          onBlur={() => !search && setSearching(false)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              onSearch('')
              setSearching(false)
            }
          }}
        />
      ) : (
        <button type="button" className="db-tool db-tool-icon" aria-label={t('search')} title={t('search')} onClick={() => setSearching(true)}>
          <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden>
            <path fill="currentColor" d="M10 3a7 7 0 0 1 5.6 11.2l4.6 4.6-1.4 1.4-4.6-4.6A7 7 0 1 1 10 3zm0 2a5 5 0 1 0 0 10 5 5 0 0 0 0-10z" />
          </svg>
        </button>
      )}
      <button type="button" className={'db-tool db-tool-icon' + (open?.kind === 'history' ? ' is-on' : '')} data-db-tool="history" aria-label={t('history')} title={t('history')} onClick={(e) => setOpen({ kind: 'history', anchor: rectOf(e.currentTarget) })}>
        <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden>
          <path fill="currentColor" d="M13 3a9 9 0 1 1-8.5 12h2.1A7 7 0 1 0 6 9.3L8.7 12H2V5.3l2.6 2.6A9 9 0 0 1 13 3zm-1 4h2v5.2l3.4 2-1 1.7-4.4-2.6z" />
        </svg>
      </button>
      {canWrite && (
        <button type="button" className="db-new" onClick={onNew}>
          {t('newRow')}
        </button>
      )}
      {open?.kind === 'history' && (
        <Popover anchor={open.anchor} onClose={() => setOpen(null)} width={440} align="right" className="db-menu db-history-pop">
          <DbHistory />
        </Popover>
      )}
      {open?.kind === 'filter' && (
        <Popover anchor={open.anchor} onClose={() => setOpen(null)} width={560} className="db-menu db-filter-pop">
          <FilterGroup f={view.filter ?? { op: 'and', items: [] }} depth={0} onChange={(f) => onView({ filter: f.items.length ? f : null })} />
        </Popover>
      )}
      {open?.kind === 'sort' && (
        <Popover anchor={open.anchor} onClose={() => setOpen(null)} width={400} className="db-menu">
          <SortEditor view={view} onView={onView} />
        </Popover>
      )}
      {open?.kind === 'group' && (
        <Popover anchor={open.anchor} onClose={() => setOpen(null)} width={280} className="db-menu">
          <GroupEditor view={view} onView={onView} close={() => setOpen(null)} />
        </Popover>
      )}
      {open?.kind === 'props' && (
        <Popover anchor={open.anchor} onClose={() => setOpen(null)} width={280} className="db-menu" align="right">
          <PropsEditor view={view} onView={onView} />
        </Popover>
      )}
    </div>
  )
}

const TOOL_ICONS = {
  filter: 'M3 5h18l-7 8.5V19l-4 2v-7.5z',
  sort: 'M7 4 3 8h3v12h2V8h3zm10 16 4-4h-3V4h-2v12h-3z',
  group: 'M3 4h8v7H3zm10 0h8v4h-8zM3 13h8v7H3zm10-3h8v10h-8z',
  props: 'M12 7a5 5 0 1 1 0 10 5 5 0 0 1 0-10zm0 2a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM2 12c2.2-4.4 5.8-7 10-7s7.8 2.6 10 7c-2.2 4.4-5.8 7-10 7S4.2 16.4 2 12zm2.3 0c1.9 3.3 4.6 5 7.7 5s5.8-1.7 7.7-5C17.8 8.7 15.1 7 12 7s-5.8 1.7-7.7 5z',
}

const countConds = (f?: Filter | null): number => (f ? f.items.reduce((n, x) => n + (isGroup(x) ? countConds(x) : 1), 0) : 0)

function FilterGroup({ f, depth, onChange, onRemove }: { f: Filter; depth: number; onChange: (f: Filter) => void; onRemove?: () => void }) {
  const { store } = useDb()
  const props = store.schema.props.filter((p) => p.type !== 'button')
  const set = (i: number, x: Cond | Filter | null) => onChange({ ...f, items: x ? f.items.map((y, j) => (j === i ? x : y)) : f.items.filter((_, j) => j !== i) })
  return (
    <div className={'db-fgroup' + (depth ? ' is-nested' : '')}>
      {f.items.map((it, i) => (
        <div key={i} className="db-frow">
          <span className="db-fjoin">
            {i === 0 ? (
              t('where')
            ) : i === 1 ? (
              <select value={f.op} onChange={(e) => onChange({ ...f, op: e.target.value as 'and' | 'or' })}>
                <option value="and">{t('and')}</option>
                <option value="or">{t('or')}</option>
              </select>
            ) : (
              f.op === 'and' ? t('and') : t('or')
            )}
          </span>
          {isGroup(it) ? (
            <FilterGroup f={it} depth={depth + 1} onChange={(g) => set(i, g)} onRemove={() => set(i, null)} />
          ) : (
            <CondRow c={it} props={props} onChange={(c) => set(i, c)} onRemove={() => set(i, null)} />
          )}
        </div>
      ))}
      <div className="db-fadd">
        <button
          type="button"
          className="db-link-btn"
          onClick={() => {
            const p = props[0]
            onChange({ ...f, items: [...f.items, { prop: p.key, cond: condsFor(p)[0] }] })
          }}
        >
          + {t('addFilter')}
        </button>
        {depth < 2 && (
          <button type="button" className="db-link-btn" onClick={() => onChange({ ...f, items: [...f.items, { op: f.op === 'and' ? 'or' : 'and', items: [{ prop: props[0].key, cond: condsFor(props[0])[0] }] }] })}>
            + {t('addFilterGroup')}
          </button>
        )}
        {onRemove && (
          <button type="button" className="db-link-btn is-danger" onClick={onRemove}>
            ✕
          </button>
        )}
      </div>
    </div>
  )
}

function CondRow({ c, props, onChange, onRemove }: { c: Cond; props: Prop[]; onChange: (c: Cond) => void; onRemove: () => void }) {
  const p = props.find((x) => x.key === c.prop) ?? props[0]
  const conds = condsFor(p)
  return (
    <div className="db-cond">
      <select
        value={p.key}
        onChange={(e) => {
          const np = props.find((x) => x.key === e.target.value)!
          onChange({ prop: np.key, cond: condsFor(np)[0] })
        }}
      >
        {props.map((x) => (
          <option key={x.key} value={x.key}>
            {x.name}
          </option>
        ))}
      </select>
      <select value={c.cond} onChange={(e) => onChange({ ...c, cond: e.target.value })}>
        {conds.map((k) => (
          <option key={k} value={k}>
            {COND_NAMES[k] ?? k}
          </option>
        ))}
      </select>
      {!NO_VALUE.has(c.cond) && <CondValue p={p} c={c} onChange={onChange} />}
      <button type="button" className="db-pill-x" onClick={onRemove} aria-label="remove">
        ×
      </button>
    </div>
  )
}

function CondValue({ p, c, onChange }: { p: Prop; c: Cond; onChange: (c: Cond) => void }) {
  const { people } = useDb()
  if (p.options) {
    return (
      <select value={String(c.value ?? '')} onChange={(e) => onChange({ ...c, value: e.target.value || undefined })}>
        <option value="">—</option>
        {p.options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.name}
          </option>
        ))}
      </select>
    )
  }
  if (p.type === 'person' || p.type === 'created_by' || p.type === 'updated_by')
    return (
      <select value={String(c.value ?? '')} onChange={(e) => onChange({ ...c, value: e.target.value || undefined })}>
        <option value="">—</option>
        {[...people.values()].map((x) => (
          <option key={x.id} value={`${x.kind === 'agent' ? 'agent' : 'user'}:${x.id}`}>
            {x.name}
          </option>
        ))}
      </select>
    )
  if (c.cond === 'is_within')
    return (
      <select value={String(c.value ?? 'past_week')} onChange={(e) => onChange({ ...c, value: e.target.value })}>
        {Object.entries(WITHIN_NAMES).map(([k, n]) => (
          <option key={k} value={k}>
            {n}
          </option>
        ))}
      </select>
    )
  if (p.type === 'date' || p.type === 'created_time' || p.type === 'updated_time')
    return <input type="date" value={String((c.value as { start?: string })?.start ?? c.value ?? '').slice(0, 10)} onChange={(e) => onChange({ ...c, value: e.target.value ? { start: e.target.value } : undefined })} />
  return (
    <input
      className="db-cond-input"
      type={p.type === 'number' ? 'number' : 'text'}
      value={String(c.value ?? '')}
      placeholder="…"
      onChange={(e) => onChange({ ...c, value: e.target.value === '' ? undefined : p.type === 'number' ? Number(e.target.value) : e.target.value })}
      onKeyDown={(e) => e.stopPropagation()}
    />
  )
}

function SortEditor({ view, onView }: { view: View; onView: (p: Partial<View>) => void }) {
  const { store } = useDb()
  const sorts = view.sorts ?? []
  const props = store.schema.props.filter((p) => p.type !== 'button')
  return (
    <div className="db-sorts">
      {sorts.map((s, i) => (
        <div key={i} className="db-cond">
          <select value={s.prop} onChange={(e) => onView({ sorts: sorts.map((x, j) => (j === i ? { ...x, prop: e.target.value } : x)) })}>
            {props.map((p) => (
              <option key={p.key} value={p.key}>
                {p.name}
              </option>
            ))}
          </select>
          <select value={s.dir} onChange={(e) => onView({ sorts: sorts.map((x, j) => (j === i ? { ...x, dir: e.target.value as 'asc' | 'desc' } : x)) })}>
            <option value="asc">{t('asc')}</option>
            <option value="desc">{t('desc')}</option>
          </select>
          <button type="button" className="db-pill-x" onClick={() => onView({ sorts: sorts.filter((_, j) => j !== i) })}>
            ×
          </button>
        </div>
      ))}
      <button type="button" className="db-link-btn" onClick={() => onView({ sorts: [...sorts, { prop: props.find((p) => !sorts.some((s) => s.prop === p.key))?.key ?? props[0].key, dir: 'asc' }] })}>
        + {t('addSort')}
      </button>
    </div>
  )
}

const GROUPABLE = new Set(['select', 'status', 'multi_select', 'person', 'checkbox', 'date', 'text', 'number', 'url', 'created_by', 'formula'])

function GroupEditor({ view, onView, close }: { view: View; onView: (p: Partial<View>) => void; close: () => void }) {
  const { store } = useDb()
  if (view.type === 'calendar') {
    const dates = store.schema.props.filter((p) => p.type === 'date' || p.type === 'created_time' || p.type === 'updated_time')
    return (
      <>
        <div className="db-menu-label">{t('calendarBy')}</div>
        {dates.map((p) => (
          <button key={p.key} type="button" className={'db-menu-item' + (view.calendar_by === p.key ? ' is-on' : '')} onClick={() => (onView({ calendar_by: p.key }), close())}>
            <TypeIcon type={p.type} />
            {p.name}
          </button>
        ))}
      </>
    )
  }
  const props = store.schema.props.filter((p) => GROUPABLE.has(p.type))
  return (
    <>
      <div className="db-menu-label">{t('groupBy')}</div>
      {view.type !== 'board' && (
        <button type="button" className={'db-menu-item' + (!view.group_by ? ' is-on' : '')} onClick={() => (onView({ group_by: null }), close())}>
          {t('noGroup')}
        </button>
      )}
      {props.map((p) => (
        <button key={p.key} type="button" className={'db-menu-item' + (view.group_by === p.key ? ' is-on' : '')} onClick={() => (onView({ group_by: p.key, collapsed: [] }), close())}>
          <TypeIcon type={p.type} />
          {p.name}
          <span className="db-menu-aside">{TYPE_NAMES[p.type]}</span>
        </button>
      ))}
      {view.type === 'gallery' && <CoverPicker view={view} onView={onView} />}
    </>
  )
}

function CoverPicker({ view, onView }: { view: View; onView: (p: Partial<View>) => void }) {
  const { store } = useDb()
  return (
    <>
      <div className="db-menu-sep" />
      <div className="db-menu-label">{t('cover')}</div>
      <div className="db-menu-seg">
        {[{ k: '', n: t('noCover') }, { k: 'body', n: t('bodyCover') }, ...store.schema.props.filter((p) => p.type === 'files').map((p) => ({ k: p.key, n: p.name }))].map((x) => (
          <button key={x.k} type="button" aria-pressed={(view.cover ?? '') === x.k} onClick={() => onView({ cover: x.k || null })}>
            {x.n}
          </button>
        ))}
      </div>
      <div className="db-menu-label">{t('cardSize')}</div>
      <div className="db-menu-seg">
        {(['small', 'medium', 'large'] as const).map((s) => (
          <button key={s} type="button" aria-pressed={(view.card_size ?? 'medium') === s} onClick={() => onView({ card_size: s })}>
            {t(s)}
          </button>
        ))}
      </div>
    </>
  )
}

function PropsEditor({ view, onView }: { view: View; onView: (p: Partial<View>) => void }) {
  const { store } = useDb()
  const hidden = new Set(view.hidden ?? [])
  const props = store.schema.props.filter((p) => p.type !== 'title')
  return (
    <>
      <div className="db-menu-row">
        <button type="button" className="db-link-btn" onClick={() => onView({ hidden: [] })}>
          {t('showAll')}
        </button>
        <button type="button" className="db-link-btn" onClick={() => onView({ hidden: props.map((p) => p.key) })}>
          {t('hideAll')}
        </button>
      </div>
      <div className="db-menu-scroll">
        {props.map((p) => (
          <label key={p.key} className="db-menu-item db-prop-toggle">
            <TypeIcon type={p.type} />
            <span className="db-prop-name">{p.name}</span>
            <input
              type="checkbox"
              className="db-switch"
              checked={!hidden.has(p.key)}
              onChange={(e) => onView({ hidden: e.target.checked ? [...hidden].filter((k) => k !== p.key) : [...hidden, p.key] })}
            />
          </label>
        ))}
      </div>
    </>
  )
}

// 视图上方：筛选条件的小标签（点开就是筛选面板）
export function FilterChips({ view, onOpen }: { view: View; onOpen: () => void }) {
  const { store } = useDb()
  if (!view.filter?.items.length) return null
  const flat = view.filter.items.filter((x): x is Cond => !isGroup(x))
  return (
    <div className="db-chips">
      {flat.map((c, i) => {
        const p = store.prop(c.prop)
        if (!p) return null
        const opt = p.options?.find((o) => o.id === c.value)
        return (
          <button key={i} type="button" className="db-chip" onClick={onOpen}>
            <TypeIcon type={p.type} />
            {p.name} {COND_NAMES[c.cond]} {opt ? <Pill opt={opt} /> : NO_VALUE.has(c.cond) ? '' : typeof c.value === 'object' ? (c.value as { start?: string })?.start : String(c.value ?? '…')}
          </button>
        )
      })}
      {view.filter.items.length > flat.length && <span className="db-chip is-more">+{view.filter.items.length - flat.length}</span>}
    </div>
  )
}
