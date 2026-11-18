// 数据库的一行打开成页面时，正文上面的属性栏：每个属性一行，点了就改（和表格里的格子一样）。
// 别人改了这一行（db_changed 里带着它）就重新取；存的时候带版本号，同一个属性被别人先改了就提示、重新取。
import { useCallback, useEffect, useState } from 'react'
import { ApiError, api } from '../../store'
import { t } from '../../i18n'
import { notice } from '../Dialog'
import { nameOf } from '../../authors'
import { Cell, DbCtx } from './cells'
import { dbApi, typeIcon, type DbProp, type DbRow } from './db'

export default function PropsBar({ dbId, rowId }: { dbId: number; rowId: number }) {
  const [d, setD] = useState<{ db: { id: number; title: string; locked: boolean; schema: DbProp[] }; row: DbRow; refs: Record<number, string> } | null>(null)
  const [extra, setExtra] = useState<Record<number, string>>({})
  const load = useCallback(() => api(`/api/dbs/${dbId}/rows/${rowId}`).then(setD).catch(() => {}), [dbId, rowId])
  useEffect(() => { load() }, [load])
  useEffect(() => {
    const h = (e: Event) => { const x = (e as CustomEvent).detail; if (x?.db_id === dbId && (x.schema || !x.rows?.length || x.rows.includes(rowId))) load() }
    window.addEventListener('oad-db', h)
    window.addEventListener('oad-page-resync', load)
    return () => { window.removeEventListener('oad-db', h); window.removeEventListener('oad-page-resync', load) }
  }, [dbId, rowId, load])
  if (!d) return null
  const save = (p: DbProp, v: any) => {
    setD({ ...d, row: { ...d.row, props: { ...d.row.props, [p.key]: v } } })
    dbApi.update(dbId, rowId, { [p.key]: v }, d.row.version).then(row => setD(x => x && { ...x, row })).then(() => { if (p.type === 'relation') load() }).catch((e: any) => {
      const c = e instanceof ApiError && e.status === 409 ? e.data?.detail?.conflicts?.[0] : null
      notice(c ? t('「{name}」刚被{who}改成了「{value}」，已经换成现在的样子。', { name: c.name, who: nameOf(c.by), value: c.current || t('空') }) : e.message)
      load()
    })
  }
  return (
    <DbCtx.Provider value={{ dbId, refs: { ...extra, ...d.refs }, addRefs: m => setExtra(x => ({ ...x, ...m })), onPressed: load }}>
    <div className="db-props">
      {d.db.schema.filter(p => p.type !== 'title').map(p => (
        <div key={p.key} className="db-prop">
          <span className="db-prop-name"><span className="db-type-icon">{typeIcon(p.type)}</span>{p.name}</span>
          <Cell prop={p} row={d.row} onSave={v => save(p, v)} className="db-prop-value" />
        </div>))}
    </div>
    </DbCtx.Provider>
  )
}
