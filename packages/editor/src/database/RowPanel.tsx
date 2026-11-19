// 一行：上面是属性（可编辑），下面是正文（和普通页面一样协作）。表格里打开是右侧滑出的面板，能展开成整页。
import type { Editor } from '@tiptap/core'
import { focusStart, OadEditor } from '../OadEditor'
import { OadTitle } from '../OadTitle'
import type { OadUser } from '../extensions'
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react'
import type { Me } from '@oad/client'
import type { EventsLike, OadClient, PageConn } from '@oad/client'
import { colorFor } from '../react/util'
import { CellEditor, CellView, cellValue, DbContext, isEmptyVal, useDb, type DbCtx } from './cells'
import { AddPropMenu } from './ColumnMenu'
import { useDbPeople } from './DatabaseView'
import { DbHistory } from './History'
import { t } from './strings'
import type { Prop } from '@oad/client'
import { READONLY, rectOf, TypeIcon, type Anchor } from './ui'

/** 一行的属性列表 */
export function RowProps({ rowId }: { rowId: string }) {
  const { store, canWrite } = useDb()
  const row = store.rows.get(rowId)
  const [editing, setEditing] = useState<{ p: Prop; anchor: Anchor } | null>(null)
  const [adding, setAdding] = useState<Anchor | null>(null)
  const [showEmpty, setShowEmpty] = useState(true)
  const [history, setHistory] = useState(false)
  if (!row) return null
  const props = store.schema.props.filter((p) => p.type !== 'title')
  const empty = props.filter((p) => isEmptyVal(cellValue(store, p, row)) && p.type !== 'button' && p.type !== 'checkbox')
  const shown = showEmpty ? props : props.filter((p) => !empty.includes(p))
  const view = store.views[0]
  return (
    <div className="db-rowprops">
      {shown.map((p) => {
        const ro = !canWrite || READONLY.has(p.type) || !!p.relation?.mirror_of
        return (
          <div key={p.key} className="db-rowprop">
            <div className="db-rowprop-name" title={p.name}>
              <TypeIcon type={p.type} />
              <span>{p.name}</span>
            </div>
            <div
              className={'db-rowprop-val' + (ro ? ' is-ro' : '') + (isEmptyVal(cellValue(store, p, row)) ? ' is-empty' : '')}
              role={ro ? undefined : 'button'}
              tabIndex={ro ? undefined : 0}
              data-prop={p.key}
              onClick={(e) => {
                if (ro) return
                if (p.type === 'checkbox') return store.setCells([{ row_id: rowId, key: p.key, value: !row.props[p.key] }])
                if (p.type === 'button') return
                setEditing({ p, anchor: rectOf(e.currentTarget) })
              }}
              onKeyDown={(e) => {
                if (!ro && e.key === 'Enter') setEditing({ p, anchor: rectOf(e.currentTarget) })
              }}
            >
              {isEmptyVal(cellValue(store, p, row)) && p.type !== 'checkbox' && p.type !== 'button' ? <span className="db-placeholder">{t('empty')}</span> : <CellView p={p} row={row} wrap />}
            </div>
          </div>
        )
      })}
      <div className="db-rowprops-foot">
        {empty.length > 0 && (
          <button type="button" className="db-link-btn" onClick={() => setShowEmpty((v) => !v)}>
            {showEmpty ? t('hideEmptyProps', { n: empty.length }) : t('showEmptyProps', { n: empty.length })}
          </button>
        )}
        {canWrite && (
          <button type="button" className="db-link-btn" onClick={(e) => setAdding(rectOf(e.currentTarget))}>
            + {t('addProp')}
          </button>
        )}
        <button type="button" className={'db-link-btn' + (history ? ' is-on' : '')} onClick={() => setHistory((v) => !v)} aria-expanded={history}>
          ↺ {t('history')}
        </button>
      </div>
      {history && <DbHistory rowId={rowId} onClose={() => setHistory(false)} />}
      {editing && <CellEditor p={store.prop(editing.p.key) ?? editing.p} row={store.rows.get(rowId) ?? row} anchor={editing.anchor} onDone={() => setEditing(null)} />}
      {adding && view && <AddPropMenu anchor={adding} view={view} onView={(patch) => store.updateView(view.id, patch)} onClose={() => setAdding(null)} />}
    </div>
  )
}

/** 一行的标题 + 正文（自己连一个协作文档） */
function RowDoc({ rowId, me, canWrite, ws, onTitle }: { rowId: string; me: Me; canWrite: boolean; ws: string; onTitle: (s: string) => void }) {
  const user = useMemo<OadUser>(() => ({ id: me.user.id, name: me.user.name, color: colorFor(me.user.id), kind: 'user' }), [me.user.id, me.user.name])
  const { store, media } = useDb()
  void ws
  const [conn, setConn] = useState<PageConn | null>(null)
  const [ready, setReady] = useState(false)
  const editor = useRef<Editor | null>(null)
  const services = media
  useEffect(() => {
    const c = store.client.openPage(rowId)
    c.provider.awareness.setLocalStateField('user', user)
    setConn(c)
    setReady(false)
    let alive = true
    c.ready.then(() => alive && setReady(true))
    return () => {
      alive = false
      c.destroy()
    }
  }, [rowId]) // eslint-disable-line react-hooks/exhaustive-deps
  const status = useSyncExternalStore(
    (l) => conn?.onStatus(l) ?? (() => {}),
    () => conn?.status() ?? 'connecting',
  )
  const editable = canWrite && status !== 'denied'
  if (!conn) return null
  return (
    <>
      <div className="db-row-title">
        <OadTitle ydoc={conn.ydoc} readOnly={!editable} placeholder={t('untitled')} onChange={onTitle} onEnter={() => editor.current && focusStart(editor.current)} />
      </div>
      <RowProps rowId={rowId} />
      <div className={'db-row-body' + (ready ? '' : ' is-pending')}>
        <OadEditor ydoc={conn.ydoc} provider={conn.provider} user={user} editable={editable} media={services} onReady={(e) => (editor.current = e)} />
      </div>
    </>
  )
}

/** 右侧滑出的面板 */
export function RowPanel({ rowId, me, onClose }: { rowId: string; me: Me; onClose: () => void }) {
  const { store, canWrite, ws, navigate, pageHref } = useDb()
  useSyncExternalStore(store.subscribe, store.getVersion)
  const panel = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || document.querySelector('.db-pop')) return
      // 正文、标题里按 Esc 不关
      if ((e.target as HTMLElement).closest?.('.ProseMirror, .oad-title')) return
      onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])
  return (
    <aside className="db-peek oad-root" ref={panel} role="dialog" aria-label={t('openRow')} data-row={rowId}>
      <div className="db-peek-bar">
        <button type="button" className="icon-btn" onClick={onClose} aria-label={t('closePanel')} title={t('closePanel')}>
          <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden>
            <path fill="currentColor" d="M13.6 5.4 20.2 12l-6.6 6.6-1.4-1.4 4.2-4.2H4v-2h12.4L12.2 6.8z" />
          </svg>
        </button>
        <button type="button" className="icon-btn" onClick={() => navigate(pageHref(rowId))} aria-label={t('openFullPage')} title={t('openFullPage')}>
          <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden>
            <path fill="currentColor" d="M5 5h6v2H8.4l4.3 4.3-1.4 1.4L7 8.4V11H5zm14 14h-6v-2h2.6l-4.3-4.3 1.4-1.4 4.3 4.3V13h2z" />
          </svg>
        </button>
        <span className="db-peek-crumb">{store.meta?.page.title || t('untitled')}</span>
        <span className="db-toolbar-gap" />
        {canWrite && (
          <button
            type="button"
            className="db-link-btn is-danger"
            onClick={() => {
              store.deleteRows([rowId])
              onClose()
            }}
          >
            {t('deleteRow')}
          </button>
        )}
      </div>
      <div className="db-peek-scroll">
        <article className="db-peek-doc">
          <RowDoc key={rowId} rowId={rowId} me={me} canWrite={canWrite} ws={ws} onTitle={(s) => store.localTitle(rowId, s)} />
        </article>
      </div>
    </aside>
  )
}

/** 整页打开一行时，标题和正文之间的属性（PageView 用） */
export function RowHeader({
  client,
  rowId,
  dbId,
  me,
  ws,
  canWrite,
  events,
  navigate = (href) => location.assign(href),
  pageHref = (id) => `/p/${id}`,
}: {
  client: OadClient
  rowId: string
  dbId: string
  me: Me
  ws: string
  canWrite: boolean
  events?: EventsLike
  navigate?: (href: string) => void
  pageHref?: (id: string) => string
}) {
  const store = client.database(dbId)
  useSyncExternalStore(store.subscribe, store.getVersion)
  useEffect(() => {
    store.open(events)
  }, [store, events])
  const people = useDbPeople(client, ws, me)
  const ctx: DbCtx = useMemo(() => ({ store, ws, people, canWrite, dbs: [], navigate, pageHref, openRow: (id) => navigate(pageHref(id)) }), [store, ws, people, canWrite]) // eslint-disable-line react-hooks/exhaustive-deps
  if (!store.rows.has(rowId)) return <div className="db-rowprops is-loading" />
  return (
    <DbContext.Provider value={ctx}>
      <RowProps rowId={rowId} />
    </DbContext.Provider>
  )
}

export default RowHeader
