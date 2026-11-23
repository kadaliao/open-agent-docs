// 一个库：视图标签、工具栏、当前视图、行的侧边面板。整页和页面里的内嵌都用它。
import { useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react'
import type { EventsLike, Me, OadClient, Person } from '@open-agent-docs/client'
import type { MediaServices } from '../extensions/media'
import { useAgents } from '../react/people'
import { DbContext, useDb as useDbCtx, type DbCtx } from './cells'
import { RowPanel } from './RowPanel'
import { safeQuery } from '@open-agent-docs/client'
import { t, VIEW_NAMES } from './strings'
import { TableView } from './TableView'
import { FilterChips, Toolbar } from './Toolbar'
import type { QueryResult, View, ViewType } from '@open-agent-docs/client'
import { Popover, rectOf, ViewIcon, type Anchor } from './ui'
import { BoardView, CalendarView, GalleryView, ListView } from './views'

const memberCache = new Map<string, Person[]>()
/** 人员属性用的：工作区成员 + Agent */
export function useDbPeople(client: OadClient, ws: string, me: Me) {
  const agents = useAgents(client, ws)
  const [members, setMembers] = useState<Person[]>(() => memberCache.get(ws) ?? [])
  useEffect(() => {
    client.members(ws).then(
      (ms) => {
        const list = ms.map((m): Person => ({ kind: 'user', id: m.user.id, name: m.user.name }))
        memberCache.set(ws, list)
        setMembers(list)
      },
      () => {},
    )
  }, [client, ws])
  return useMemo(() => {
    const m = new Map<string, Person>()
    m.set(me.user.id, { kind: 'user', id: me.user.id, name: me.user.name })
    for (const x of members) m.set(x.id, x)
    for (const a of agents) if (!a.revoked_at) m.set(a.id, { kind: 'agent', id: a.id, name: a.name })
    return m
  }, [agents, members, me])
}

export interface DatabaseViewProps {
  client: OadClient
  dbId: string
  me: Me
  ws: string
  canWrite: boolean
  /** 工作区事件（client.events(ws)，或者宿主自己的）：别人改了马上拉增量 */
  events?: EventsLike
  // 只看这个视图（内嵌时地址里的 ?view=）
  viewId?: string | null
  embedded?: boolean
  // 工作区里的库（关联用）
  dbs?: { id: string; title: string }[]
  /** 去别的页面（默认改 location）；pageHref 决定一页的网址（默认 /p/<id>） */
  navigate?: (href: string) => void
  pageHref?: (id: string) => string
  /** 行正文编辑器用的服务（链接卡片、上传） */
  media?: Partial<MediaServices>
}

const defaultNavigate = (href: string) => location.assign(href)
const defaultHref = (id: string) => `/p/${id}`

const viewFromUrl = () => new URLSearchParams(location.search).get('view')

export function DatabaseView({ client, dbId, me, ws, canWrite, events, viewId, embedded, dbs = [], navigate = defaultNavigate, pageHref = defaultHref, media }: DatabaseViewProps) {
  const store = client.database(dbId)
  const pagePath = pageHref
  const version = useSyncExternalStore(store.subscribe, store.getVersion)
  useEffect(() => {
    store.open(events)
  }, [store, events])
  const people = useDbPeople(client, ws, me)
  // 引擎按名字筛选、显示人员时要知道谁是谁
  useEffect(() => {
    store.engine?.setPeople?.([...people.values()].map((p) => ({ id: `${p.kind === 'agent' ? 'agent' : 'user'}:${p.id}`, name: p.name })))
  }, [people, store.engine])
  const [vid, setVid] = useState<string | null>(() => viewId ?? (embedded ? null : viewFromUrl()))
  const views = store.views
  const view = views.find((v) => v.id === vid) ?? views[0]
  const [search, setSearch] = useState('')
  const [peek, setPeek] = useState<string | null>(() => (embedded ? null : new URLSearchParams(location.search).get('row')))

  // 整页：当前视图、打开的行记在地址里（刷新还在）
  useEffect(() => {
    if (embedded) return
    const u = new URL(location.href)
    if (view && view.id !== views[0]?.id) u.searchParams.set('view', view.id)
    else u.searchParams.delete('view')
    if (peek) u.searchParams.set('row', peek)
    else u.searchParams.delete('row')
    if (u.href !== location.href) history.replaceState(history.state, '', u)
  }, [view?.id, peek, embedded]) // eslint-disable-line react-hooks/exhaustive-deps

  // 库的根元素：找自己工具栏里的按钮（不用 document.querySelector：页面上可能有好几个库，也可能在 Shadow DOM 里）
  const box = useRef<HTMLDivElement>(null)
  const ctx: DbCtx = useMemo(
    () => ({
      store,
      ws,
      people,
      canWrite,
      dbs,
      navigate,
      pageHref,
      media,
      openFilter: () => box.current?.querySelector<HTMLButtonElement>('[data-db-tool="filter"]')?.click(),
      openRow: (id: string) => {
        if (id.startsWith('tmp_')) return
        if (embedded) navigate(pagePath(dbId) + `?row=${id}` + (view ? `&view=${view.id}` : ''))
        else setPeek(id)
      },
    }),
    [store, ws, people, canWrite, dbs, embedded, dbId, view, navigate, pageHref, media], // eslint-disable-line react-hooks/exhaustive-deps
  )

  // 查询：每次数据或视图变了就算一遍（引擎里 1 万行十几毫秒）
  const result: QueryResult = useMemo(() => {
    if (!store.engine || !view) return { ids: [] }
    const t0 = performance.now()
    const r = safeQuery(store.engine, { ...view, search: search.trim() || undefined })
    store.timing.query = performance.now() - t0
    return r
  }, [version, view, search, store]) // eslint-disable-line react-hooks/exhaustive-deps
  // 性能测试用：结果画到屏幕上的时间
  useLayoutEffect(() => {
    store.timing.commit = performance.now()
  })

  const onView = (patch: Partial<View>) => view && canWrite && store.updateView(view.id, patch)
  // 只读的人改视图（筛选、排序）只在本地生效
  const [localPatch, setLocalPatch] = useState<Partial<View>>({})
  const shownView = useMemo(() => (view ? { ...view, ...(canWrite ? {} : localPatch) } : undefined), [view, canWrite, localPatch])
  const patchView = (p: Partial<View>) => (canWrite ? onView(p) : setLocalPatch((x) => ({ ...x, ...p })))
  const shownResult: QueryResult = useMemo(() => {
    if (canWrite || !Object.keys(localPatch).length || !store.engine || !shownView) return result
    return safeQuery(store.engine, { ...shownView, search: search.trim() || undefined })
  }, [result, canWrite, localPatch, shownView, search, store.engine])

  if (store.status === 'loading' && !store.meta) return <DbSkeleton embedded={embedded} />
  if (store.status === 'error' && !store.meta)
    return (
      <div className="oad-db-error">
        {t('loadFailed')}
        <span className="oad-muted">{store.error}</span>
        <button type="button" className="oad-btn oad-btn-sm" onClick={() => location.reload()}>
          {t('retry')}
        </button>
      </div>
    )
  if (!shownView) return null

  const newRow = () => {
    const id = store.addRow({}, null, (real) => !embedded && setPeek(real))
    void id
  }

  return (
    <DbContext.Provider value={ctx}>
      <div ref={box} className={'oad-db oad-root' + (embedded ? ' oad-is-embedded' : '')} data-db={dbId} data-status={store.status}>
        {embedded && (
          <a
            className="oad-db-embed-title"
            href={pagePath(dbId)}
            onClick={(e) => {
              e.preventDefault()
              navigate(pagePath(dbId))
            }}
          >
            <span className="oad-db-embed-ico">▦</span>
            {store.meta?.page.title || t('untitled')}
          </a>
        )}
        <div className="oad-db-bar">
          <ViewTabs views={views} current={shownView} onPick={setVid} embedded={embedded} />
          <Toolbar view={shownView} onView={patchView} search={search} onSearch={setSearch} onNew={newRow} />
        </div>
        <FilterChips view={shownView} onOpen={() => ctx.openFilter?.()} />
        <div className={'oad-db-view oad-is-' + shownView.type}>
          {shownView.type === 'table' && <TableView view={shownView} result={shownResult} onView={patchView} maxHeight={embedded ? 420 : undefined} />}
          {shownView.type === 'board' && <BoardView view={shownView} result={shownResult} onView={patchView} />}
          {shownView.type === 'calendar' && <CalendarView view={shownView} result={shownResult} onView={patchView} />}
          {shownView.type === 'gallery' && <GalleryView view={shownView} result={shownResult} onView={patchView} maxHeight={embedded ? 520 : undefined} />}
          {shownView.type === 'list' && <ListView view={shownView} result={shownResult} onView={patchView} maxHeight={embedded ? 420 : undefined} />}
        </div>
        {store.error && store.meta && (
          <div className="oad-db-toast oad-is-error" role="alert">
            {store.status === 'cache' ? t('offline') : t('saveFailed') + store.error}
          </div>
        )}
        {peek && !embedded && <RowPanel rowId={peek} me={me} onClose={() => setPeek(null)} />}
      </div>
    </DbContext.Provider>
  )
}

function ViewTabs({ views, current, onPick, embedded }: { views: View[]; current: View; onPick: (id: string) => void; embedded?: boolean }) {
  const { store, canWrite } = useDbCtx()
  const [menu, setMenu] = useState<{ v: View; anchor: Anchor } | null>(null)
  const [adding, setAdding] = useState<Anchor | null>(null)
  const [renaming, setRenaming] = useState<string | null>(null)
  const add = async (type: ViewType) => {
    setAdding(null)
    const props = store.schema.props
    const base: Omit<View, 'id'> = { name: VIEW_NAMES[type], type }
    if (type === 'board') base.group_by = (props.find((p) => p.type === 'status') ?? props.find((p) => p.type === 'select') ?? props.find((p) => p.type === 'person'))?.key ?? null
    if (type === 'calendar') base.calendar_by = props.find((p) => p.type === 'date')?.key ?? null
    if (type === 'gallery') base.cover = props.find((p) => p.type === 'files')?.key ?? 'body'
    const id = await store.addView(base)
    if (id) onPick(id)
  }
  return (
    <div className="oad-db-tabs" role="tablist">
      {views.map((v) =>
        renaming === v.id ? (
          <input
            key={v.id}
            autoFocus
            className="oad-db-tab-rename"
            defaultValue={v.name}
            onBlur={(e) => {
              const n = e.target.value.trim()
              if (n && n !== v.name) store.updateView(v.id, { name: n })
              setRenaming(null)
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
              if (e.key === 'Escape') setRenaming(null)
            }}
          />
        ) : (
          <button
            key={v.id}
            type="button"
            role="tab"
            aria-selected={v.id === current.id}
            className={'oad-db-tab' + (v.id === current.id ? ' oad-is-on' : '')}
            onClick={(e) => (v.id === current.id && canWrite ? setMenu({ v, anchor: rectOf(e.currentTarget) }) : onPick(v.id))}
            onDoubleClick={() => canWrite && setRenaming(v.id)}
          >
            <ViewIcon type={v.type} />
            {v.name}
          </button>
        ),
      )}
      {canWrite && !embedded && (
        <button type="button" className="oad-db-tab oad-db-tab-add" aria-label={t('addView')} title={t('addView')} onClick={(e) => setAdding(rectOf(e.currentTarget))}>
          +
        </button>
      )}
      {adding && (
        <Popover anchor={adding} onClose={() => setAdding(null)} width={200} className="oad-db-menu">
          <div className="oad-db-menu-label">{t('addView')}</div>
          {(Object.keys(VIEW_NAMES) as ViewType[]).map((ty) => (
            <button key={ty} type="button" className="oad-db-menu-item" onClick={() => add(ty)}>
              <ViewIcon type={ty} />
              {VIEW_NAMES[ty]}
            </button>
          ))}
        </Popover>
      )}
      {menu && (
        <Popover anchor={menu.anchor} onClose={() => setMenu(null)} width={200} className="oad-db-menu">
          <button type="button" className="oad-db-menu-item" onClick={() => (setRenaming(menu.v.id), setMenu(null))}>
            ✎ {t('renameView')}
          </button>
          <button
            type="button"
            className="oad-db-menu-item"
            onClick={async () => {
              setMenu(null)
              const { id: _, ...rest } = menu.v
              const id = await store.addView({ ...rest, name: menu.v.name + t('copySuffix') })
              if (id) onPick(id)
            }}
          >
            ⧉ {t('duplicateView')}
          </button>
          {views.length > 1 && (
            <button
              type="button"
              className="oad-db-menu-item oad-is-danger"
              onClick={() => {
                setMenu(null)
                store.deleteView(menu.v.id)
              }}
            >
              ✕ {t('deleteView')}
            </button>
          )}
        </Popover>
      )}
    </div>
  )
}


function DbSkeleton({ embedded }: { embedded?: boolean }) {
  return (
    <div className={'oad-db oad-root oad-db-skeleton' + (embedded ? ' oad-is-embedded' : '')} aria-busy="true">
      <div className="oad-db-bar">
        <span className="oad-sk" style={{ width: 160, height: 22 }} />
      </div>
      {Array.from({ length: embedded ? 4 : 10 }, (_, i) => (
        <div key={i} className="oad-sk-row">
          <span className="oad-sk" style={{ width: '30%' }} />
          <span className="oad-sk" style={{ width: '18%' }} />
          <span className="oad-sk" style={{ width: '22%' }} />
        </div>
      ))}
    </div>
  )
}
