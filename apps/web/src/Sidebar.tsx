// 侧栏：页面树。新建、重命名、拖动排序和移进别的页面、删除；别人（或 Agent）改了会通过 SSE 实时更新。
import { memo, useEffect, useMemo, useRef, useState } from 'react'
import { api, type Me, type Page } from './api'
import { SidebarIcon } from './icons'
import { applyTheme, colorFor, getTheme, initials, navigate, pagePath, storageGet, storageSet, type Theme } from './lib'
import { buildTree, usePages, type PageStore, type TreeNode } from './store'
import { t } from './strings'

type Zone = 'before' | 'inside' | 'after'
const EXPANDED_KEY = 'oad:expanded'

interface Props {
  store: PageStore
  me: Me
  workspaceName: string
  currentId: string | null
  canWrite: boolean
  canLogout: boolean
  onLogout: () => void
  onCollapse: () => void
}

export function Sidebar({ store, me, workspaceName, currentId, canWrite, canLogout, onLogout, onCollapse }: Props) {
  const pages = usePages(store)
  const tree = useMemo(() => buildTree(pages), [pages])
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(storageGet<string[]>(EXPANDED_KEY) ?? []))
  const [renaming, setRenaming] = useState<string | null>(null)
  const [menu, setMenu] = useState<{ id: string; x: number; y: number } | null>(null)
  const [drag, setDrag] = useState<{ id: string; over: string | null; zone: Zone } | null>(null)

  const toggle = (id: string, open?: boolean) =>
    setExpanded((s) => {
      const n = new Set(s)
      if (open ?? !n.has(id)) n.add(id)
      else n.delete(id)
      storageSet(EXPANDED_KEY, [...n])
      return n
    })

  // 打开的页面所在的分支展开
  useEffect(() => {
    if (!currentId) return
    const byId = new Map(pages.map((p) => [p.id, p]))
    let p = byId.get(currentId)
    const need: string[] = []
    while (p?.parent_id && byId.has(p.parent_id)) {
      if (!expanded.has(p.parent_id)) need.push(p.parent_id)
      p = byId.get(p.parent_id)
    }
    if (need.length) need.forEach((id) => toggle(id, true))
  }, [currentId, pages.length]) // eslint-disable-line react-hooks/exhaustive-deps

  const create = async (parent: string | null) => {
    const p = await api.createPage(store.ws, parent ? { parent_id: parent } : {})
    store.upsert(p)
    if (parent) toggle(parent, true)
    navigate(pagePath(p.id))
  }

  const remove = async (id: string) => {
    const page = store.pages.get(id)
    if (!window.confirm(t('deleteConfirm', { title: page?.title || t('untitled') }))) return
    const gone = store.removeTree(id)
    if (currentId && gone.includes(currentId)) navigate('/')
    await api.deletePage(id).catch(() => store.load())
  }

  const move = async (id: string, b: { parent_id: string | null; after?: string | null }) => {
    const p = await api.patchPage(id, b).catch(() => null)
    if (p) store.upsert(p)
    else store.load().catch(() => {})
    if (b.parent_id) toggle(b.parent_id, true)
  }

  const siblingsOf = (parent: string | null) =>
    pages.filter((p) => (p.parent_id ?? null) === parent).sort((a, b) => (a.position < b.position ? -1 : a.position > b.position ? 1 : 0))

  const onDrop = (target: Page, zone: Zone, dragged: string) => {
    if (dragged === target.id || store.descendants(dragged).includes(target.id)) return
    if (zone === 'inside') return move(dragged, { parent_id: target.id })
    const parent = target.parent_id ?? null
    if (zone === 'after') return move(dragged, { parent_id: parent, after: target.id })
    const sibs = siblingsOf(parent).filter((p) => p.id !== dragged)
    const i = sibs.findIndex((p) => p.id === target.id)
    move(dragged, { parent_id: parent, after: i > 0 ? sibs[i - 1].id : null })
  }

  const [theme, setTheme] = useState<Theme>(getTheme)
  const nextTheme = () => {
    const order: Theme[] = ['system', 'light', 'dark']
    const n = order[(order.indexOf(theme) + 1) % order.length]
    applyTheme(n)
    setTheme(n)
  }

  return (
    <aside className="sidebar" onClick={() => setMenu(null)}>
      <div className="side-head">
        <div className="ws">
          <span className="ws-mark" aria-hidden>
            ✦
          </span>
          <span className="ws-name">{workspaceName}</span>
        </div>
        <button className="icon-btn" onClick={onCollapse} aria-label={t('toggleSidebar')} title={t('toggleSidebar')}>
          <SidebarIcon />
        </button>
      </div>

      {canWrite && (
        <button className="side-new" onClick={() => create(null)}>
          <span className="side-new-plus" aria-hidden>
            +
          </span>
          {t('newPage')}
        </button>
      )}

      <div className="side-label">{t('pages')}</div>
      <nav className="tree" aria-label={t('pages')} onDragEnd={() => setDrag(null)}>
        {tree.length === 0 ? (
          <div className="tree-empty">{t('noPages')}</div>
        ) : (
          tree.map((n) => (
            <TreeItem
              key={n.page.id}
              node={n}
              depth={0}
              currentId={currentId}
              expanded={expanded}
              renaming={renaming}
              drag={drag}
              canWrite={canWrite}
              onToggle={toggle}
              onCreate={create}
              onMenu={(id, x, y) => setMenu({ id, x, y })}
              onRename={setRenaming}
              onRenameDone={async (id, title) => {
                setRenaming(null)
                const p = store.pages.get(id)
                if (!p || p.title === title) return
                store.upsert({ ...p, title })
                const r = await api.patchPage(id, { title }).catch(() => null)
                if (r) store.upsert(r)
              }}
              setDrag={setDrag}
              onDrop={onDrop}
            />
          ))
        )}
      </nav>

      <div className="side-foot">
        <div className="me" title={me.user.email ?? undefined}>
          <span className="avatar" style={{ background: colorFor(me.user.id) }}>
            {initials(me.user.name)}
          </span>
          <span className="me-name">{me.user.name}</span>
        </div>
        <button className="icon-btn" onClick={nextTheme} title={theme === 'light' ? t('themeLight') : theme === 'dark' ? t('themeDark') : t('themeSystem')}>
          {theme === 'light' ? '☀︎' : theme === 'dark' ? '☾' : '◐'}
        </button>
        {canLogout && (
          <button className="icon-btn" onClick={onLogout} title={t('logout')} aria-label={t('logout')}>
            ⎋
          </button>
        )}
      </div>

      {menu && (
        <div className="menu" style={{ left: menu.x, top: menu.y }} onClick={(e) => e.stopPropagation()} role="menu">
          <button
            role="menuitem"
            onClick={() => {
              setRenaming(menu.id)
              setMenu(null)
            }}
          >
            {t('rename')}
          </button>
          <button
            role="menuitem"
            onClick={() => {
              create(menu.id)
              setMenu(null)
            }}
          >
            {t('newSubpage')}
          </button>
          <button
            role="menuitem"
            onClick={() => {
              const p = store.pages.get(menu.id)
              move(menu.id, { parent_id: p?.parent_id ?? null, after: null })
              setMenu(null)
            }}
          >
            {t('moveToTop')}
          </button>
          {store.pages.get(menu.id)?.parent_id && (
            <button
              role="menuitem"
              onClick={() => {
                const p = store.pages.get(menu.id)!
                const parent = store.pages.get(p.parent_id!)
                move(menu.id, { parent_id: parent?.parent_id ?? null, after: parent?.id })
                setMenu(null)
              }}
            >
              {t('moveOut')}
            </button>
          )}
          <div className="menu-sep" />
          <button
            role="menuitem"
            className="is-danger"
            onClick={() => {
              remove(menu.id)
              setMenu(null)
            }}
          >
            {t('delete')}
          </button>
        </div>
      )}
    </aside>
  )
}

interface ItemProps {
  node: TreeNode
  depth: number
  currentId: string | null
  expanded: Set<string>
  renaming: string | null
  drag: { id: string; over: string | null; zone: Zone } | null
  canWrite: boolean
  onToggle: (id: string) => void
  onCreate: (parent: string) => void
  onMenu: (id: string, x: number, y: number) => void
  onRenameDone: (id: string, title: string) => void
  onRename: (id: string) => void
  setDrag: (d: { id: string; over: string | null; zone: Zone } | null) => void
  onDrop: (target: Page, zone: Zone, dragged: string) => void
}

const TreeItem = memo(function TreeItem(p: ItemProps) {
  const { page, children } = p.node
  const open = p.expanded.has(page.id)
  const active = p.currentId === page.id
  const row = useRef<HTMLDivElement>(null)
  const over = p.drag && p.drag.over === page.id && p.drag.id !== page.id ? p.drag.zone : null

  const zoneAt = (y: number): Zone => {
    const r = row.current!.getBoundingClientRect()
    const f = (y - r.top) / r.height
    return f < 0.28 ? 'before' : f > 0.72 ? 'after' : 'inside'
  }

  return (
    <div className="tree-node" role="treeitem" aria-expanded={children.length ? open : undefined} aria-selected={active}>
      <div
        ref={row}
        className={'tree-row' + (active ? ' is-active' : '') + (over ? ' drop-' + over : '') + (p.drag?.id === page.id ? ' is-dragging' : '')}
        style={{ paddingLeft: 8 + p.depth * 16 }}
        data-page-id={page.id}
        draggable={p.canWrite && p.renaming !== page.id}
        onDragStart={(e) => {
          e.dataTransfer.effectAllowed = 'move'
          e.dataTransfer.setData('text/x-oad-page', page.id)
          p.setDrag({ id: page.id, over: null, zone: 'inside' })
        }}
        onDragOver={(e) => {
          if (!p.drag) return
          e.preventDefault()
          const zone = zoneAt(e.clientY)
          if (p.drag.over !== page.id || p.drag.zone !== zone) p.setDrag({ ...p.drag, over: page.id, zone })
        }}
        onDrop={(e) => {
          e.preventDefault()
          const id = e.dataTransfer.getData('text/x-oad-page') || p.drag?.id
          if (id) p.onDrop(page, zoneAt(e.clientY), id)
          p.setDrag(null)
        }}
        onClick={() => navigate(pagePath(page.id))}
        onDoubleClick={() => p.canWrite && p.onRename(page.id)}
        onContextMenu={(e) => {
          if (!p.canWrite) return
          e.preventDefault()
          p.onMenu(page.id, e.clientX, e.clientY)
        }}
      >
        <button
          className={'tree-chevron' + (children.length ? '' : ' is-leaf') + (open ? ' is-open' : '')}
          aria-label={open ? t('collapse') : t('expand')}
          onClick={(e) => {
            e.stopPropagation()
            p.onToggle(page.id)
          }}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <path d="M3.5 2l3 3-3 3" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
        <span className="tree-icon" aria-hidden>
          {page.icon || <DocIcon />}
        </span>
        {p.renaming === page.id ? (
          <RenameInput initial={page.title} onDone={(v) => p.onRenameDone(page.id, v)} />
        ) : (
          <span className={'tree-title' + (page.title ? '' : ' is-untitled')}>{page.title || t('untitled')}</span>
        )}
        {p.canWrite && (
          <span className="tree-actions" onClick={(e) => e.stopPropagation()}>
            <button
              className="tree-act"
              aria-label={t('moreActions')}
              title={t('moreActions')}
              onClick={(e) => {
                const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
                p.onMenu(page.id, r.left, r.bottom + 4)
              }}
            >
              ⋯
            </button>
            <button className="tree-act" aria-label={t('newSubpage')} title={t('newSubpage')} onClick={() => p.onCreate(page.id)}>
              +
            </button>
          </span>
        )}
      </div>
      {open && children.length > 0 && (
        <div role="group">
          {children.map((c) => (
            <TreeItem key={c.page.id} {...p} node={c} depth={p.depth + 1} />
          ))}
        </div>
      )}
    </div>
  )
})

function RenameInput({ initial, onDone }: { initial: string; onDone: (v: string) => void }) {
  const [v, setV] = useState(initial)
  const done = useRef(false)
  const finish = (val: string) => {
    if (done.current) return
    done.current = true
    onDone(val)
  }
  return (
    <input
      className="tree-rename"
      autoFocus
      value={v}
      onFocus={(e) => e.currentTarget.select()}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => setV(e.target.value)}
      onBlur={() => finish(v.trim())}
      onKeyDown={(e) => {
        if (e.key === 'Enter') finish(v.trim())
        if (e.key === 'Escape') finish(initial)
      }}
    />
  )
}

function DocIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <path d="M4 1.75h5.2L12.5 5v8.25c0 .55-.45 1-1 1H4c-.55 0-1-.45-1-1v-10.5c0-.55.45-1 1-1z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
      <path d="M9 1.9V5.2h3.3M5.5 8h5M5.5 10.5h3.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  )
}
