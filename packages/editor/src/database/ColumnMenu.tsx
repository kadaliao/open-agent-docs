// 列头菜单：改名、改类型、排序、筛选、隐藏、删除、左右插入；公式、数字格式、选项、关联 / 汇总的设置。
import { useState, useSyncExternalStore } from 'react'
import { useDb } from './cells'
import { FormulaEditor } from './FormulaEditor'
import { DATE_FORMAT_NAMES, FORMAT_NAMES, t, TOTAL_NAMES, TYPE_NAMES } from './strings'
import { getLocale } from '../i18n'
import type { Prop, PropType, SelectOption, View } from '@open-agent-docs/client'
import { ADDABLE_TYPES, COLOR_NAMES, COLORS, condsFor, optId, Pill, Popover, randomColor, TypeIcon, type Anchor } from './ui'

interface Props {
  p: Prop
  anchor: Anchor
  view: View
  onView: (patch: Partial<View>) => void
  onClose: () => void
}

export function ColumnMenu({ p, anchor, view, onView, onClose }: Props) {
  const { store, canWrite } = useDb()
  const [name, setName] = useState(p.name)
  const [sub, setSub] = useState<null | 'type' | 'options' | 'formula' | 'format' | 'datefmt' | 'relation' | 'rollup' | 'insert-left' | 'insert-right'>(null)
  const commitName = () => {
    const n = name.trim()
    if (n && n !== p.name) store.renameProp(p.key, n)
  }
  const sort = (dir: 'asc' | 'desc') => {
    onView({ sorts: [{ prop: p.key, dir }, ...(view.sorts ?? []).filter((s) => s.prop !== p.key)] })
    onClose()
  }
  const filter = () => {
    const cond = condsFor(p)[0]
    const items = [...(view.filter?.items ?? []), { prop: p.key, cond, value: undefined }]
    onView({ filter: { op: view.filter?.op ?? 'and', items } })
    onClose()
    // 打开工具栏里的筛选
    setTimeout(() => document.querySelector<HTMLButtonElement>('[data-db-tool="filter"]')?.click(), 0)
  }
  const hide = () => {
    onView({ hidden: [...new Set([...(view.hidden ?? []), p.key])] })
    onClose()
  }

  if (sub === 'formula') return <FormulaEditor p={p} onClose={onClose} />
  if (sub === 'options') return <OptionsMenu p={p} anchor={anchor} onClose={onClose} />
  if (sub === 'insert-left' || sub === 'insert-right')
    return (
      <AddPropMenu
        anchor={anchor}
        view={view}
        onView={onView}
        onClose={onClose}
        at={(() => {
          const order = view.order?.length ? view.order : store.schema.props.filter((x) => x.type !== 'title').map((x) => x.key)
          const i = order.indexOf(p.key)
          return p.type === 'title' ? 0 : sub === 'insert-left' ? Math.max(0, i) : i + 1
        })()}
      />
    )

  return (
    <Popover anchor={anchor} onClose={() => (commitName(), onClose())} width={260} className="db-menu">
      {canWrite && (
        <input
          autoFocus
          className="db-menu-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              commitName()
              onClose()
            }
            e.stopPropagation()
          }}
        />
      )}
      {sub === 'type' ? (
        <div className="db-menu-scroll">
          {(p.type === 'title' ? [] : ADDABLE_TYPES).map((ty) => (
            <button
              key={ty}
              type="button"
              className={'db-menu-item' + (ty === p.type ? ' is-on' : '')}
              onClick={() => {
                store.changeType(p.key, ty, defaultsFor(ty, p, store.id, store.schema.props))
                onClose()
              }}
            >
              <TypeIcon type={ty} />
              {TYPE_NAMES[ty]}
            </button>
          ))}
        </div>
      ) : sub === 'format' ? (
        Object.entries(FORMAT_NAMES).map(([k, n]) => (
          <button
            key={k}
            type="button"
            className={'db-menu-item' + ((p.format ?? 'number') === k ? ' is-on' : '')}
            onClick={() => {
              store.updateProp(p.key, { format: k })
              onClose()
            }}
          >
            {n}
          </button>
        ))
      ) : sub === 'datefmt' ? (
        Object.entries(DATE_FORMAT_NAMES).map(([k, n]) => (
          <button
            key={k}
            type="button"
            className={'db-menu-item' + ((p.date_format ?? 'iso') === k ? ' is-on' : '')}
            onClick={() => {
              store.updateProp(p.key, { date_format: k })
              onClose()
            }}
          >
            {n}
          </button>
        ))
      ) : sub === 'relation' ? (
        <RelationSettings p={p} onClose={onClose} />
      ) : sub === 'rollup' ? (
        <RollupSettings p={p} onClose={onClose} />
      ) : (
        <>
          {canWrite && p.type !== 'title' && (
            <button type="button" className="db-menu-item" onClick={() => setSub('type')}>
              <TypeIcon type={p.type} />
              {t('changeType')}
              <span className="db-menu-aside">{TYPE_NAMES[p.type]} ›</span>
            </button>
          )}
          {canWrite && p.type === 'formula' && (
            <button type="button" className="db-menu-item" onClick={() => setSub('formula')}>
              <TypeIcon type="formula" />
              {t('editFormula')}
            </button>
          )}
          {canWrite && (p.type === 'select' || p.type === 'multi_select' || p.type === 'status') && (
            <button type="button" className="db-menu-item" onClick={() => setSub('options')}>
              <TypeIcon type="select" />
              {t('options')}
              <span className="db-menu-aside">{p.options?.length ?? 0} ›</span>
            </button>
          )}
          {canWrite && (p.type === 'number' || p.type === 'formula' || p.type === 'rollup') && (
            <button type="button" className="db-menu-item" onClick={() => setSub('format')}>
              <TypeIcon type="number" />
              {t('format')}
              <span className="db-menu-aside">{FORMAT_NAMES[p.format ?? 'number']} ›</span>
            </button>
          )}
          {canWrite && (p.type === 'date' || p.type === 'created_time' || p.type === 'updated_time') && (
            <button type="button" className="db-menu-item" onClick={() => setSub('datefmt')}>
              <TypeIcon type="date" />
              {t('dateFormat')}
              <span className="db-menu-aside">{DATE_FORMAT_NAMES[p.date_format ?? 'iso']} ›</span>
            </button>
          )}
          {canWrite && p.type === 'relation' && (
            <button type="button" className="db-menu-item" onClick={() => setSub('relation')}>
              <TypeIcon type="relation" />
              {TYPE_NAMES.relation}
              <span className="db-menu-aside">›</span>
            </button>
          )}
          {canWrite && p.type === 'rollup' && (
            <button type="button" className="db-menu-item" onClick={() => setSub('rollup')}>
              <TypeIcon type="rollup" />
              {TYPE_NAMES.rollup}
              <span className="db-menu-aside">›</span>
            </button>
          )}
          <div className="db-menu-sep" />
          <button type="button" className="db-menu-item" onClick={() => sort('asc')}>
            ↑ {t('sortAsc')}
          </button>
          <button type="button" className="db-menu-item" onClick={() => sort('desc')}>
            ↓ {t('sortDesc')}
          </button>
          <button type="button" className="db-menu-item" onClick={filter}>
            ⌕ {t('filter')}
          </button>
          {p.type !== 'title' && (
            <button type="button" className="db-menu-item" onClick={hide}>
              ◌ {t('hide')}
            </button>
          )}
          {canWrite && (
            <>
              <div className="db-menu-sep" />
              <button type="button" className="db-menu-item" onClick={() => setSub('insert-left')}>
                ⇤ {t('insertLeft')}
              </button>
              <button type="button" className="db-menu-item" onClick={() => setSub('insert-right')}>
                ⇥ {t('insertRight')}
              </button>
              {p.type !== 'title' && (
                <button
                  type="button"
                  className="db-menu-item is-danger"
                  onClick={() => {
                    if (window.confirm(t('deletePropConfirm', { name: p.name }))) store.deleteProp(p.key)
                    onClose()
                  }}
                >
                  ✕ {t('deleteProp')}
                </button>
              )}
            </>
          )}
        </>
      )}
    </Popover>
  )
}

function defaultsFor(ty: PropType, p: Prop, dbId: string, props: Prop[] = []): Partial<Prop> {
  // 关联默认连到自己这个库；汇总默认沿第一个关联数个数
  if (ty === 'relation' && !p.relation) return { relation: { db_id: dbId } }
  if (ty === 'rollup' && !p.rollup) {
    const rel = props.find((x) => x.type === 'relation')
    if (rel) return { rollup: { relation: rel.key, prop: 'title', fn: 'count' } }
  }
  if ((ty === 'select' || ty === 'multi_select') && !p.options?.length) return { options: [] }
  if (ty === 'status' && !p.options?.some((o) => o.group))
    return {
      options: [
        { id: optId(), name: t('statusTodo'), color: 'gray', group: 'todo' },
        { id: optId(), name: t('statusDoing'), color: 'blue', group: 'doing' },
        { id: optId(), name: t('statusDone'), color: 'green', group: 'done' },
      ],
    }
  if (ty === 'formula') return { formula: p.formula ?? '' }
  return {}
}

export function AddPropMenu({ anchor, view, onView, onClose, at }: { anchor: Anchor; view: View; onView: (p: Partial<View>) => void; onClose: () => void; at?: number }) {
  const { store } = useDb()
  const [name, setName] = useState('')
  const add = async (type: PropType) => {
    const props = store.schema.props
    const base = name.trim() || TYPE_NAMES[type]
    let n = base
    for (let i = 2; props.some((p) => p.name === n); i++) n = `${base} ${i}`
    const extra = defaultsFor(type, { key: '', name: n, type }, store.id, props)
    onClose()
    const key = await store.addProp({ name: n, type, ...extra })
    if (!key) return
    // 放到视图列顺序里对应的位置
    const order = (view.order?.length ? view.order : props.filter((x) => x.type !== 'title').map((x) => x.key)).filter((k) => k !== key)
    order.splice(at ?? order.length, 0, key)
    onView({ order })
  }
  return (
    <Popover anchor={anchor} onClose={onClose} width={240} className="db-menu" align="right">
      <input autoFocus className="db-menu-name" value={name} placeholder={t('newProp')} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => (e.key === 'Enter' && add('text'), e.stopPropagation())} />
      <div className="db-menu-scroll">
        {ADDABLE_TYPES.map((ty) => (
          <button key={ty} type="button" className="db-menu-item" onClick={() => add(ty)}>
            <TypeIcon type={ty} />
            {TYPE_NAMES[ty]}
          </button>
        ))}
      </div>
    </Popover>
  )
}

function OptionsMenu({ p, anchor, onClose }: { p: Prop; anchor: Anchor; onClose: () => void }) {
  const { store } = useDb()
  const [opts, setOpts] = useState<SelectOption[]>(p.options ?? [])
  const [editing, setEditing] = useState<string | null>(null)
  const save = (next: SelectOption[]) => {
    setOpts(next)
    store.setOptions(p.key, next)
  }
  return (
    <Popover anchor={anchor} onClose={onClose} width={280} className="db-menu">
      <div className="db-menu-label">{t('options')}</div>
      <div className="db-menu-scroll">
        {opts.map((o, i) => (
          <div key={o.id} className="db-optedit">
            {editing === o.id ? (
              <div className="db-optedit-box">
                <input
                  autoFocus
                  className="db-menu-name"
                  defaultValue={o.name}
                  onBlur={(e) => save(opts.map((x) => (x.id === o.id ? { ...x, name: e.target.value.trim() || x.name } : x)))}
                  onKeyDown={(e) => (e.key === 'Enter' && (e.target as HTMLInputElement).blur(), e.stopPropagation())}
                />
                <div className="db-colors">
                  {COLORS.map((c) => (
                    <button key={c} type="button" title={COLOR_NAMES[c]} className={'db-color c-' + c + (o.color === c ? ' is-on' : '')} onClick={() => save(opts.map((x) => (x.id === o.id ? { ...x, color: c } : x)))} />
                  ))}
                </div>
                {p.type === 'status' && (
                  <div className="db-menu-seg">
                    {(['todo', 'doing', 'done'] as const).map((g) => (
                      <button key={g} type="button" aria-pressed={(o.group ?? 'todo') === g} onClick={() => save(opts.map((x) => (x.id === o.id ? { ...x, group: g } : x)))}>
                        {g === 'todo' ? t('statusTodo') : g === 'doing' ? t('statusDoing') : t('statusDone')}
                      </button>
                    ))}
                  </div>
                )}
                <button type="button" className="db-menu-item is-danger" onClick={() => save(opts.filter((x) => x.id !== o.id))}>
                  {t('deleteOption')}
                </button>
              </div>
            ) : (
              <button type="button" className="db-menu-item" onClick={() => setEditing(o.id)}>
                <Pill opt={o} status={p.type === 'status'} />
                <span className="db-menu-aside">{i + 1}</span>
              </button>
            )}
          </div>
        ))}
      </div>
      <button
        type="button"
        className="db-menu-item"
        onClick={() => {
          const o: SelectOption = { id: optId(), name: t('optionN', { n: opts.length + 1 }), color: randomColor(opts.length), ...(p.type === 'status' ? { group: 'todo' as const } : {}) }
          save([...opts, o])
          setEditing(o.id)
        }}
      >
        + {t('addOption')}
      </button>
    </Popover>
  )
}

function useDbList() {
  const { store, dbs } = useDb()
  return dbs.filter((d) => d.id !== store.id)
}

function RelationSettings({ p, onClose }: { p: Prop; onClose: () => void }) {
  const { store } = useDb()
  const dbs = useDbList()
  return (
    <>
      <div className="db-menu-label">{TYPE_NAMES.relation}</div>
      <button
        type="button"
        className={'db-menu-item' + (p.relation?.db_id === store.id ? ' is-on' : '')}
        onClick={() => {
          store.updateProp(p.key, { relation: { db_id: store.id } })
          onClose()
        }}
      >
        ↺ {store.meta?.page.title || t('untitled')}
      </button>
      {dbs.map((d) => (
        <button
          key={d.id}
          type="button"
          className={'db-menu-item' + (p.relation?.db_id === d.id ? ' is-on' : '')}
          onClick={() => {
            store.updateProp(p.key, { relation: { db_id: d.id } })
            onClose()
          }}
        >
          ↗ {d.title || t('untitled')}
        </button>
      ))}
    </>
  )
}

function RollupSettings({ p, onClose }: { p: Prop; onClose: () => void }) {
  const { store } = useDb()
  const rels = store.schema.props.filter((x) => x.type === 'relation')
  const relKey = p.rollup?.relation ?? rels[0]?.key
  const rel = rels.find((x) => x.key === relKey)
  const target = store.client.database(rel?.relation?.db_id ?? store.id)
  useSyncExternalStore(target.subscribe, target.getVersion)
  const FNS = ['show_original', 'count', 'count_values', 'sum', 'average', 'min', 'max', 'earliest', 'latest', 'percent_checked']
  const NAMES: Record<string, string> = { ...TOTAL_NAMES, show_original: getLocale() === 'en' ? 'Show original' : '显示原值', count: TOTAL_NAMES.count, average: TOTAL_NAMES.avg }
  const set = (patch: Partial<NonNullable<Prop['rollup']>>) => store.updateProp(p.key, { rollup: { relation: relKey ?? '', prop: p.rollup?.prop ?? 'title', fn: p.rollup?.fn ?? 'count', ...patch } })
  return (
    <div className="db-rollup">
      <label>
        {TYPE_NAMES.relation}
        <select value={relKey} onChange={(e) => set({ relation: e.target.value })}>
          {rels.map((r) => (
            <option key={r.key} value={r.key}>
              {r.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t('props')}
        <select value={p.rollup?.prop ?? 'title'} onChange={(e) => set({ prop: e.target.value })}>
          {(target.meta?.schema.props ?? []).map((x) => (
            <option key={x.key} value={x.key}>
              {x.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t('calc')}
        <select value={p.rollup?.fn ?? 'count'} onChange={(e) => set({ fn: e.target.value })}>
          {FNS.map((f) => (
            <option key={f} value={f}>
              {NAMES[f]}
            </option>
          ))}
        </select>
      </label>
      <button type="button" className="btn btn-sm" onClick={onClose}>
        {t('save')}
      </button>
    </div>
  )
}
