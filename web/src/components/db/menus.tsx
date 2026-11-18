// 数据库的菜单：表头（改名、改类型、选项、格式、排序、隐藏、删列）、加属性、筛选、排序、显示哪些属性、视图设置。
// 表结构的改动交给 onSchema（ops，见 open_agent_docs/dbs.change_schema），视图的改动交给 onView（这个视图改成什么样）。
import { useContext, useEffect, useState } from 'react'
import { api, useStore } from '../../store'
import { t } from '../../i18n'
import { ask } from '../Dialog'
import { Cell, DbCtx } from './cells'
import { COLORS, COLOR_NAMES, FORMATS, GROUPS, OPTIONED, ROLLUPS, TYPES, dbApi, isMirror, opName, opsOf, readOnly, typeIcon, typeName,
  type DbOption, type DbProp, type DbRow, type DbView, type Filter, type PropType, type Sort } from './db'

type SchemaFn = (ops: any[]) => Promise<void>
type ViewFn = (v: DbView) => void

// 所有数据库的名字（关联选库、显示关联到哪个库用），取一次
let dbList: Promise<{ id: number; title: string }[]> | null = null
function useDbs() {
  const [list, setList] = useState<{ id: number; title: string }[]>([])
  useEffect(() => {
    dbList = dbList || api<any[]>('/api/pages').then(ps => ps.filter(p => p.kind === 'db').map(p => ({ id: p.id, title: p.title }))).catch(() => [])
    dbList.then(setList)
  }, [])
  return list
}
export const forgetDbs = () => { dbList = null }

/** 关联到的库的表结构（汇总选“取哪个属性”用）。 */
function useSchemaOf(id?: number) {
  const [schema, setSchema] = useState<DbProp[] | null>(null)
  useEffect(() => { if (id) dbApi.get(id, null, 1).then(d => setSchema(d.schema)).catch(() => setSchema([])) }, [id])
  return schema
}

/** 公式、关联、汇总、自动编号、按钮自己的设置。draft 是现在的样子，onChange 交出改了的字段（表头菜单里马上存，加属性时攒着一起交）。 */
export function ConfigFields({ draft, schema, onChange, adding }: {
  draft: Partial<DbProp> & { type: PropType; twoway?: boolean }; schema: DbProp[]; onChange: (patch: Record<string, any>) => void; adding?: boolean
}) {
  const { dbId } = useContext(DbCtx)
  const dbs = useDbs()
  const rels = schema.filter(p => p.type === 'relation')
  const rel = rels.find(p => p.key === draft.relation) || (adding ? rels[0] : undefined)
  const other = useSchemaOf(draft.type === 'rollup' ? rel?.db : undefined)
  const [formula, setFormula] = useState(draft.formula || '')
  const dbName = (id?: number) => id === dbId ? t('这个库自己') : dbs.find(d => d.id === id)?.title || `#${id}`
  const numFormat = (draft.type === 'formula' || draft.type === 'rollup') && <label className="db-menu-row"><span>{t('数字格式')}</span>
    <select value={draft.format || 'number'} onChange={e => onChange({ format: e.target.value })}>
      {FORMATS.map(([k, n]) => <option key={k} value={k}>{n}</option>)}</select></label>
  switch (draft.type) {
    case 'formula': {
      const insert = (name: string) => { const x = `${formula}${formula && !formula.endsWith(' ') ? ' ' : ''}prop("${name}")`; setFormula(x); if (adding) onChange({ formula: x }) }
      return <div className="db-config">
        <textarea className="db-input db-formula" rows={3} value={formula} placeholder={'prop("单价") * prop("数量")'}   /* i18n-ignore：公式的例子 */
          onChange={e => { setFormula(e.target.value); if (adding) onChange({ formula: e.target.value }) }}
          onBlur={() => { if (!adding && formula !== (draft.formula || '')) onChange({ formula }) }} spellCheck={false} />
        <div className="db-chips">{schema.filter(p => p.key !== draft.key && p.type !== 'button').map(p =>
          <button key={p.key} className="db-chip c-gray" onMouseDown={e => e.preventDefault()} onClick={() => insert(p.name)}>{p.name}</button>)}</div>
        <div className="muted small">{t('加减乘除、比大小、and / or，条件 ? 甲 : 乙；函数有 if、concat、round、dateBetween(日期1, 日期2, "days")、dateAdd、today()、formatDate、length、contains、empty 等。')}</div>
        {numFormat}
      </div>
    }
    case 'relation':
      if (isMirror(draft as DbProp)) return <div className="muted small">{t('这是「{db}」里的关联在这边的一半：在这里改，改的是那边的行。', { db: dbName(draft.db) })}</div>
      if (!adding) return <div className="muted small">{t('关联到：{db}', { db: dbName(draft.db) })}{draft.pair ? ' · ' + t('双向') : ''}</div>
      return <div className="db-config">
        <label className="db-menu-row"><span>{t('关联到')}</span>
          <select value={draft.db ?? dbId} onChange={e => onChange({ db: Number(e.target.value) })}>
            <option value={dbId}>{t('这个库自己')}</option>
            {dbs.filter(d => d.id !== dbId).map(d => <option key={d.id} value={d.id}>{d.title}</option>)}</select></label>
        <label className="db-toggle"><input type="checkbox" checked={!!draft.twoway} onChange={e => onChange({ twoway: e.target.checked })} />
          {t('双向：对方库里也加一列，两边都能改')}</label>
      </div>
    case 'rollup':
      if (!rels.length) return <div className="muted small">{t('汇总要沿一个关联属性取值：先加一个关联。')}</div>
      return <div className="db-config">
        <label className="db-menu-row"><span>{t('沿着')}</span>
          <select value={rel?.key || ''} onChange={e => onChange({ relation: e.target.value, target: undefined })}>
            {!rel && <option value="" disabled>{t('选一个关联')}</option>}
            {rels.map(p => <option key={p.key} value={p.key}>{p.name}</option>)}</select></label>
        <label className="db-menu-row"><span>{t('取')}</span>
          <select value={draft.target || ''} onChange={e => onChange({ relation: rel?.key, target: e.target.value })}>
            {!draft.target && <option value="">{other?.[0]?.name || t('名称')}</option>}
            {(other || []).filter(p => p.type !== 'button').map(p => <option key={p.key} value={p.key}>{p.name}</option>)}</select></label>
        <label className="db-menu-row"><span>{t('算')}</span>
          <select value={draft.fn || 'count'} onChange={e => onChange({ relation: rel?.key, fn: e.target.value })}>
            {ROLLUPS.map(([k, n]) => <option key={k} value={k}>{n}</option>)}</select></label>
        {numFormat}
      </div>
    case 'unique_id':
      return <label className="db-menu-row"><span>{t('前缀')}</span>
        <input className="db-input" defaultValue={draft.prefix || ''} placeholder="TASK-" maxLength={12}   /* i18n-ignore：前缀的例子 */
          onBlur={e => { if (e.target.value !== (draft.prefix || '')) onChange({ prefix: e.target.value }) }} /></label>
    case 'button':
      return <ButtonConfig draft={draft} schema={schema} onChange={onChange} />
    default:
      return null
  }
}

/** 按钮：上面的字、点了把哪些属性改成什么、写一条评论通知哪个 Agent。 */
function ButtonConfig({ draft, schema, onChange }: { draft: Partial<DbProp>; schema: DbProp[]; onChange: (patch: Record<string, any>) => void }) {
  const agents = useStore(s => s.agents).filter(a => !a.revoked || a.id === draft.agent)
  const set = draft.set || {}
  const fake: DbRow = { id: 0, title: set.title ?? '', version: 0, pos: 0, created: 0, updated: 0, created_by: '', updated_by: '', props: set, locked: false }
  const can = schema.filter(p => !readOnly(p) && !isMirror(p) && p.key !== draft.key)
  const unused = can.filter(p => !(p.key in set))
  const put = (next: Record<string, any>) => onChange({ set: next })
  return <div className="db-config">
    <label className="db-menu-row"><span>{t('按钮上的字')}</span>
      <input className="db-input" defaultValue={draft.label || ''} placeholder={draft.name} onBlur={e => onChange({ label: e.target.value })} /></label>
    <div className="muted small">{t('点了把这些属性改成：')}</div>
    {Object.keys(set).map(k => { const p = schema.find(x => x.key === k); return p && <div key={k} className="db-menu-row db-set-row">
      <span>{p.name}</span><Cell prop={p} row={fake} onSave={v => put({ ...set, [k]: v })} className="db-set-value" />
      <button className="icon-btn small" title={t('去掉')} onClick={() => { const n = { ...set }; delete n[k]; put(n) }}>×</button></div> })}
    {!!unused.length && <select value="" onChange={e => { const p = schema.find(x => x.key === e.target.value)!; put({ ...set, [p.key]: p.type === 'checkbox' ? true : p.type === 'title' ? '' : null }) }}>
      <option value="" disabled>{t('＋ 改一个属性')}</option>{unused.map(p => <option key={p.key} value={p.key}>{p.name}</option>)}</select>}
    <div className="muted small">{t('再写一条评论（会通知 Agent 处理这一行，空着就不写）：')}</div>
    <textarea className="db-input" rows={2} defaultValue={draft.comment || ''} placeholder={t('比如：这一行做完了，帮我写个总结')}
      onBlur={e => { if (e.target.value !== (draft.comment || '')) onChange({ comment: e.target.value }) }} />
    <label className="db-menu-row"><span>{t('通知哪个 Agent')}</span>
      <select value={draft.agent ?? ''} onChange={e => onChange({ agent: e.target.value ? Number(e.target.value) : null })}>
        <option value="">{t('默认（评论里 @ 到的，或者这一行的作者）')}</option>
        {agents.map(a => <option key={a.id} value={a.id}>{a.name}</option>)}</select></label>
  </div>
}

function TypeSelect({ value, onChange, title }: { value: PropType | ''; onChange: (t: PropType) => void; title?: boolean }) {
  return <select value={value} onChange={e => onChange(e.target.value as PropType)}>
    {title && <option value="title">{typeName('title')}</option>}
    {!value && <option value="" disabled>{t('类型')}</option>}
    {TYPES.map(([k, name, icon]) => <option key={k} value={k}>{icon}  {name}</option>)}
  </select>
}

/** 表头点开的菜单。 */
export function HeaderMenu({ prop, schema, view, onSchema, onView, onClose }: {
  prop: DbProp; schema: DbProp[]; view: DbView; onSchema: SchemaFn; onView: ViewFn; onClose: () => void
}) {
  const [name, setName] = useState(prop.name)
  const rename = () => { if (name.trim() && name.trim() !== prop.name) onSchema([{ op: 'update', prop: prop.key, name: name.trim() }]) }
  const sortBy = (desc: boolean) => { onView({ ...view, sort: [{ prop: prop.key, desc }] }); onClose() }
  const hide = () => { onView({ ...view, hidden: [...(view.hidden || []), prop.key] }); onClose() }
  const del = async () => {
    if (!(await ask(t('删除属性「{name}」？每一行里这一列的值都会清掉。', { name: prop.name }), { ok: t('删除'), danger: true }))) return
    onClose()
    onSchema([{ op: 'delete', prop: prop.key }])
  }
  return (
    <div className="db-menu">
      <input className="db-input" value={name} onChange={e => setName(e.target.value)} onBlur={rename}
        onKeyDown={e => { if (e.key === 'Enter' && !e.nativeEvent.isComposing) { rename(); onClose() } }} aria-label={t('属性名')} />
      {prop.type !== 'title' && !isMirror(prop) && <label className="db-menu-row"><span>{t('类型')}</span>
        <TypeSelect value={prop.type} onChange={type => onSchema([{ op: 'update', prop: prop.key, type }])} /></label>}
      <ConfigFields key={prop.type} draft={prop} schema={schema} onChange={patch => onSchema([{ op: 'update', prop: prop.key, ...patch }])} />
      {prop.type === 'number' && <label className="db-menu-row"><span>{t('格式')}</span>
        <select value={prop.format || 'number'} onChange={e => onSchema([{ op: 'update', prop: prop.key, format: e.target.value }])}>
          {FORMATS.map(([k, n]) => <option key={k} value={k}>{n}</option>)}</select></label>}
      {OPTIONED.has(prop.type) && <OptionsEditor prop={prop} onSave={options => onSchema([{ op: 'update', prop: prop.key, options }])} />}
      <div className="db-menu-sep" />
      {prop.type !== 'button' && <button onClick={() => sortBy(false)}>↑ {t('升序')}</button>}
      {prop.type !== 'button' && <button onClick={() => sortBy(true)}>↓ {t('降序')}</button>}
      {prop.type !== 'title' && <button onClick={hide}>{t('在这个视图里隐藏')}</button>}
      {prop.type !== 'title' && <button className="danger-text" onClick={del}>{t('删除属性')}</button>}
    </div>
  )
}

/** 选项：改名、颜色、（状态的）分组、删、加。改完一项就存。 */
function OptionsEditor({ prop, onSave }: { prop: DbProp; onSave: (o: DbOption[]) => void }) {
  const [list, setList] = useState<DbOption[]>(prop.options || [])
  const [add, setAdd] = useState('')
  const save = (next: DbOption[]) => { setList(next); onSave(next) }
  const set = (k: number, patch: Partial<DbOption>) => save(list.map((o, i) => (i === k ? { ...o, ...patch } : o)))
  return (
    <div className="db-options">
      <div className="muted small">{t('选项')}</div>
      {list.map((o, k) => (
        <div key={o.id} className="db-option-row">
          <select className={'db-color c-' + o.color} value={o.color} title={t('颜色')} onChange={e => set(k, { color: e.target.value })}>
            {COLORS.map(c => <option key={c} value={c}>{COLOR_NAMES[c]}</option>)}</select>
          <input className="db-input" defaultValue={o.name} onBlur={e => { const v = e.target.value.trim(); if (v && v !== o.name) set(k, { name: v }) }}
            onKeyDown={e => { if (e.key === 'Enter' && !e.nativeEvent.isComposing) (e.target as HTMLInputElement).blur() }} />
          {prop.type === 'status' && <select value={o.group || 'todo'} title={t('分组')} onChange={e => set(k, { group: e.target.value as DbOption['group'] })}>
            {GROUPS.map(([g, n]) => <option key={g} value={g}>{n}</option>)}</select>}
          <button className="icon-btn small" title={t('删除这个选项')} onClick={async () => {
            if (await ask(t('删除选项「{name}」？用了它的行会清掉这个值。', { name: o.name }), { ok: t('删除'), danger: true })) save(list.filter((_, i) => i !== k))
          }}>×</button>
        </div>))}
      <input className="db-input" value={add} placeholder={t('加一个选项')} onChange={e => setAdd(e.target.value)}
        onKeyDown={e => {
          if (e.key !== 'Enter' || e.nativeEvent.isComposing || !add.trim()) return
          if (list.some(o => o.name.toLowerCase() === add.trim().toLowerCase())) return
          save([...list, { id: '', name: add.trim(), color: COLORS[1 + (list.length % (COLORS.length - 1))] } as DbOption])
          setAdd('')
        }} />
    </div>
  )
}

/** 表头最后的“＋”：加一个属性。 */
export function AddProp({ schema, onAdd }: { schema: DbProp[]; onAdd: (op: Record<string, any>) => void }) {
  const [name, setName] = useState('')
  const [type, setType] = useState<PropType>('text')
  const [conf, setConf] = useState<Record<string, any>>({})
  const go = () => { if (name.trim()) onAdd({ ...conf, name: name.trim(), type }) }
  const blocked = type === 'rollup' && !schema.some(p => p.type === 'relation')
  return (
    <div className="db-menu">
      <input className="db-input" autoFocus value={name} placeholder={t('属性名')} onChange={e => setName(e.target.value)}
        onKeyDown={e => { if (e.key === 'Enter' && !e.nativeEvent.isComposing) go() }} />
      <TypeSelect value={type} onChange={x => { setType(x); setConf({}) }} />
      <ConfigFields key={type} adding draft={{ ...conf, type, name }} schema={schema} onChange={patch => setConf(c => ({ ...c, ...patch }))} />
      <button className="btn primary" disabled={!name.trim() || blocked} onClick={go}>{t('添加')}</button>
    </div>
  )
}

/** 筛选条件的值：按属性的类型给合适的输入。 */
function FilterValue({ prop, f, onChange }: { prop: DbProp; f: Filter; onChange: (v: any) => void }) {
  const agents = useStore(s => s.agents)
  if (f.op === 'empty' || f.op === 'not_empty') return null
  if (OPTIONED.has(prop.type)) {
    return <select value={f.value ?? ''} onChange={e => onChange(e.target.value)}>
      <option value="" disabled>{t('选一个')}</option>{prop.options?.map(o => <option key={o.id} value={o.id}>{o.name}</option>)}</select>
  }
  if (prop.type === 'person' || prop.type === 'created_by' || prop.type === 'updated_by') {
    return <select value={f.value ?? ''} onChange={e => onChange(e.target.value)}>
      <option value="" disabled>{t('选一个人')}</option><option value="user">{t('你')}</option>
      {agents.map(a => <option key={a.id} value={`agent:${a.id}`}>{a.name}</option>)}</select>
  }
  if (prop.type === 'checkbox') {
    return <select value={f.value === false ? 'no' : 'yes'} onChange={e => onChange(e.target.value === 'yes')}>
      <option value="yes">{t('勾了')}</option><option value="no">{t('没勾')}</option></select>
  }
  if (prop.type === 'relation' || prop.type === 'files') {
    return <input className="db-input" defaultValue={f.value ?? ''} placeholder={prop.type === 'relation' ? t('名称里有') : t('文件名里有')}
      onBlur={e => onChange(e.target.value)} onKeyDown={e => { if (e.key === 'Enter' && !e.nativeEvent.isComposing) (e.target as HTMLInputElement).blur() }} />
  }
  if (prop.type === 'date' || prop.type === 'created_time' || prop.type === 'updated_time') {
    return <input type="date" value={f.value ?? ''} onChange={e => onChange(e.target.value)} />
  }
  return <input className="db-input" defaultValue={f.value ?? ''} type={prop.type === 'number' ? 'number' : 'text'} placeholder={t('值')}
    onBlur={e => onChange(e.target.value)} onKeyDown={e => { if (e.key === 'Enter' && !e.nativeEvent.isComposing) (e.target as HTMLInputElement).blur() }} />
}

export function FilterEditor({ schema, view, onView }: { schema: DbProp[]; view: DbView; onView: ViewFn }) {
  const list = view.filter || []
  const put = (next: Filter[]) => onView({ ...view, filter: next })
  const set = (k: number, patch: Partial<Filter>) => put(list.map((f, i) => (i === k ? { ...f, ...patch } : f)))
  const byKey = (k: string) => schema.find(p => p.key === k)!
  const add = () => { const p = schema[0]; put([...list, { prop: p.key, op: opsOf(p.type)[0], value: '' }]) }
  schema = schema.filter(p => p.type !== 'button')
  return (
    <div className="db-menu">
      {!list.length && <div className="muted small">{t('还没有筛选条件：都显示')}</div>}
      {list.length > 1 && <div className="muted small">{t('下面的条件都要满足')}</div>}
      {list.map((f, k) => {
        const p = byKey(f.prop)
        if (!p) return null
        return (
          <div key={k} className="db-filter-row">
            <select value={f.prop} onChange={e => { const np = byKey(e.target.value); set(k, { prop: np.key, op: opsOf(np.type)[0], value: np.type === 'checkbox' ? true : '' }) }}>
              {schema.map(x => <option key={x.key} value={x.key}>{x.name}</option>)}</select>
            <select value={f.op} onChange={e => set(k, { op: e.target.value })}>
              {opsOf(p.type).map(op => <option key={op} value={op}>{opName(op, p.type)}</option>)}</select>
            <FilterValue key={f.prop + f.op} prop={p} f={f} onChange={value => set(k, { value })} />
            <button className="icon-btn small" title={t('去掉')} onClick={() => put(list.filter((_, i) => i !== k))}>×</button>
          </div>)
      })}
      <button className="link-btn small" onClick={add}>＋ {t('加一个条件')}</button>{/* i18n-ignore */}
    </div>
  )
}

export function SortEditor({ schema, view, onView }: { schema: DbProp[]; view: DbView; onView: ViewFn }) {
  schema = schema.filter(p => p.type !== 'button')
  const list = view.sort || []
  const put = (next: Sort[]) => onView({ ...view, sort: next })
  return (
    <div className="db-menu">
      {!list.length && <div className="muted small">{t('没有排序：按手动排的顺序')}</div>}
      {list.map((s, k) => (
        <div key={k} className="db-filter-row">
          <select value={s.prop} onChange={e => put(list.map((x, i) => (i === k ? { ...x, prop: e.target.value } : x)))}>
            {schema.map(x => <option key={x.key} value={x.key}>{x.name}</option>)}</select>
          <select value={s.desc ? 'desc' : 'asc'} onChange={e => put(list.map((x, i) => (i === k ? { ...x, desc: e.target.value === 'desc' } : x)))}>
            <option value="asc">{t('升序')}</option><option value="desc">{t('降序')}</option></select>
          <button className="icon-btn small" title={t('去掉')} onClick={() => put(list.filter((_, i) => i !== k))}>×</button>
        </div>))}
      {list.length < 5 && <button className="link-btn small" onClick={() => put([...list, { prop: schema[0].key, desc: false }])}>＋ {t('加一个排序')}</button>}{/* i18n-ignore */}
    </div>
  )
}

export function FieldsEditor({ schema, view, onView }: { schema: DbProp[]; view: DbView; onView: ViewFn }) {
  const hidden = new Set(view.hidden || [])
  const toggle = (k: string) => onView({ ...view, hidden: hidden.has(k) ? [...hidden].filter(x => x !== k) : [...hidden, k] })
  return (
    <div className="db-menu">
      {schema.filter(p => p.type !== 'title').map(p => (
        <label key={p.key} className="db-toggle"><input type="checkbox" checked={!hidden.has(p.key)} onChange={() => toggle(p.key)} />
          <span className="db-type-icon">{typeIcon(p.type)}</span>{p.name}</label>))}
    </div>
  )
}

/** 视图设置：改名、表格 / 看板、看板按哪个属性分列、删掉。 */
export function ViewMenu({ schema, view, canDelete, onView, onDelete }: {
  schema: DbProp[]; view: DbView; canDelete: boolean; onView: ViewFn; onDelete: () => void
}) {
  const [name, setName] = useState(view.name)
  const groupable = schema.filter(p => p.type === 'select' || p.type === 'status')
  const rename = () => { if (name.trim() && name.trim() !== view.name) onView({ ...view, name: name.trim() }) }
  return (
    <div className="db-menu">
      <input className="db-input" value={name} onChange={e => setName(e.target.value)} onBlur={rename}
        onKeyDown={e => { if (e.key === 'Enter' && !e.nativeEvent.isComposing) rename() }} aria-label={t('视图名')} />
      <label className="db-menu-row"><span>{t('显示成')}</span>
        <select value={view.type} onChange={e => {
          const type = e.target.value as DbView['type']
          if (type === 'board' && !groupable.length) return
          onView({ ...view, type, group_by: type === 'board' ? view.group_by || groupable[0].key : undefined })
        }}>
          <option value="table">{t('表格')}</option>
          <option value="board" disabled={!groupable.length}>{t('看板')}{!groupable.length ? t('（要先有单选或状态属性）') : ''}</option>
        </select></label>
      {view.type === 'board' && <label className="db-menu-row"><span>{t('按这个分列')}</span>
        <select value={view.group_by} onChange={e => onView({ ...view, group_by: e.target.value })}>
          {groupable.map(p => <option key={p.key} value={p.key}>{p.name}</option>)}</select></label>}
      {canDelete && <button className="danger-text" onClick={onDelete}>{t('删除这个视图')}</button>}
    </div>
  )
}

export const editable = (p: DbProp) => !readOnly(p)

// ---------- 表结构的历史（open_agent_docs/dbs.history）：每次加列、删列、改类型、改选项一条，能撤销 ----------
export type Change = { kind: string; key?: string; name?: string; from?: string; to?: string; type?: string; cleared?: number; kept?: number; items?: Change[]
  added?: string[]; removed?: string[]; changed?: string[] }
const names = (l?: string[]) => (l || []).map(x => x === '顺序' ? t('顺序') : `「${x}」`).join(t('、'))   // i18n-ignore：服务器记的“顺序”
export type HistoryItem = { id: number; version: number; by: string; ts: number; changes: Change[]; undo_of?: number | null; undone_by?: number }

export function changeText(c: Change): string {
  const cleared = c.cleared ? t('，清掉了 {n} 行的值', { n: c.cleared }) : ''
  switch (c.kind) {
    case 'add': return t('加了属性「{name}」', { name: c.name || '' })
    case 'delete': return t('删了属性「{name}」', { name: c.name || '' }) + cleared
    case 'rename': return t('把「{from}」改名为「{name}」', { from: c.from || '', name: c.name || '' })
    case 'type': return t('把「{name}」从{from}改成{to}', { name: c.name || '', from: typeName(c.from as PropType), to: typeName(c.to as PropType) })
      + (c.cleared ? t('，{n} 行的值转不过去，清掉了', { n: c.cleared }) : '')
    case 'options': return t('改了「{name}」的选项', { name: c.name || '' }) + (c.cleared ? t('，{n} 行用了去掉的选项，清掉了', { n: c.cleared }) : '')
    case 'format': return t('改了「{name}」的数字格式', { name: c.name || '' })
    case 'config': return t('改了「{name}」的设置', { name: c.name || '' })
    case 'views': return [c.added?.length ? t('加了视图{names}', { names: names(c.added) }) : '', c.removed?.length ? t('删了视图{names}', { names: names(c.removed) }) : '',
      c.changed?.length ? t('改了视图{names}', { names: names(c.changed) }) : ''].filter(Boolean).join(t('；')) || t('改了视图')
    case 'undo': return t('撤销了：{what}', { what: (c.items || []).map(changeText).join('；') })
      + (c.kept ? t('（{n} 格之后被改过，留着现在的值）', { n: c.kept }) : '')
    default: return c.kind
  }
}

export function SchemaHistory({ list, nameOf, when, onUndo }: {
  list: HistoryItem[] | null; nameOf: (by: string) => string; when: (ts: number) => string; onUndo: (h: HistoryItem) => void
}) {
  if (!list) return <div className="muted small">{t('加载中…')}</div>
  if (!list.length) return <div className="muted small">{t('还没有改过。加列、删列、改类型、改选项、调视图都会记在这里，能撤销。')}</div>
  return (
    <div className="db-menu db-history">
      {list.map(h => (
        <div key={h.id} className={'db-history-item' + (h.undone_by ? ' undone' : '')}>
          <div className="muted small">{nameOf(h.by)} · {when(h.ts)}{h.undone_by ? ' · ' + t('已撤销') : ''}</div>
          {h.changes.map((c, k) => <div key={k}>{changeText(c)}</div>)}
          {!h.undone_by && <button className="link-btn small" onClick={() => onUndo(h)}>{t('撤销')}</button>}
        </div>))}
    </div>
  )
}
