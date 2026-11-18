// 一页：标题 + 正文。本地优先：先从 IndexedDB 读出上次的内容马上显示，再连 WebSocket 和服务器同步。
import { OadEditor, OadTitle, focusStart, type Editor, type OadTitleHandle, type OadUser } from '@oad/editor'
import { useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react'
import type { WebsocketProvider } from 'y-websocket'
import { api, ApiError, type Me } from './api'
import { closePage, takeConn, type Conn, type Status } from './conn'
import { SidebarIcon } from './icons'
import { colorFor, initials, navigate, pagePath } from './lib'
import { readSnapshot, saveSnapshot, takeSnapshot } from './snapshot'
import { usePages, type PageStore } from './store'
import { t } from './strings'

interface Props {
  pageId: string
  me: Me
  store: PageStore
  canWrite: boolean
  sidebarCollapsed: boolean
  onExpandSidebar: () => void
}

export default function PageView({ pageId, me, store, canWrite, sidebarCollapsed, onExpandSidebar }: Props) {
  const user = useMemo<OadUser>(() => ({ id: me.user.id, name: me.user.name, color: colorFor(me.user.id), kind: 'user' }), [me.user.id, me.user.name])
  const [conn, setConn] = useState<Conn | null>(null)
  // ready：内容到了（本机缓存或服务器）；编辑器先装好藏着，内容一到就露出来
  const [ready, setReady] = useState(false)
  // 编辑器装好之前先画上次的首屏快照；有快照时等它画出来再装编辑器，不然装编辑器的那几十毫秒会把快照那一帧挤掉
  const [snapshot] = useState(() => readSnapshot(pageId))
  const [canMount, setCanMount] = useState(!snapshot)
  const [editor, setEditor] = useState<Editor | null>(null)
  const [notFound, setNotFound] = useState(false)
  const editorRef = useRef<Editor | null>(null)
  const titleRef = useRef<OadTitleHandle>(null)

  const saveRef = useRef<() => void>(() => {})

  useEffect(() => {
    const c = takeConn(pageId)
    c.provider.awareness.setLocalStateField('user', user)
    setConn(c)
    setReady(false)
    let alive = true
    c.ready.then(() => {
      performance.mark('oad:ready')
      if (alive) setReady(true)
    })
    api.page(pageId).then(
      (p) => alive && store.upsert(p),
      (e) => alive && e instanceof ApiError && e.status === 404 && setNotFound(true),
    )
    return () => {
      alive = false
      saveRef.current()
      closePage(c)
      delete (window as unknown as { __oad?: unknown }).__oad
    }
  }, [pageId]) // eslint-disable-line react-hooks/exhaustive-deps

  // 存首屏快照：改动停下 2 秒后、离开页面时、关窗口前
  const saveSnap = () => {
    const e = editorRef.current
    if (!e || e.isDestroyed || !conn) return
    saveSnapshot(pageId, takeSnapshot(e.view.dom, conn.ydoc.getText('title').toString()))
  }
  saveRef.current = saveSnap
  // 内容露出来了
  useEffect(() => {
    if (!ready || !editor || !conn) return
    performance.mark('oad:editor')
    markFirstContent()
    // 调试和端到端测试用
    ;(window as unknown as { __oad?: unknown }).__oad = { editor, ydoc: conn.ydoc, provider: conn.provider }
    // 新建的空页面：光标放在标题里
    if (editable && conn.ydoc.getText('title').length === 0 && editor.isEmpty) titleRef.current?.focus()
  }, [ready, editor]) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const e = editor
    if (!ready || !e) return
    let timer = 0
    const later = () => {
      clearTimeout(timer)
      timer = window.setTimeout(() => saveRef.current(), 2000)
    }
    const now = () => saveRef.current()
    e.on('update', later)
    window.addEventListener('pagehide', now)
    later()
    return () => {
      clearTimeout(timer)
      e.off('update', later)
      window.removeEventListener('pagehide', now)
    }
  }, [ready, editor])

  const status = useSyncExternalStore(
    (l) => conn?.onStatus(l) ?? (() => {}),
    () => conn?.status() ?? 'connecting',
  )

  if (notFound)
    return (
      <div className="page">
        <div className="page-missing">
          <p>{t('pageNotFound')}</p>
          <button className="btn" onClick={() => navigate('/')}>
            {t('backHome')}
          </button>
        </div>
      </div>
    )

  const editable = canWrite && status !== 'denied'

  return (
    <div className="page">
      <header className="topbar">
        {sidebarCollapsed && (
          <button className="icon-btn" onClick={onExpandSidebar} aria-label={t('toggleSidebar')}>
            <SidebarIcon />
          </button>
        )}
        <Breadcrumb store={store} pageId={pageId} />
        <div className="topbar-right">
          {conn && <Presence provider={conn.provider} />}
          <StatusPill status={status} readOnly={!editable} />
        </div>
      </header>
      <div className="page-scroll">
        <article className="doc">
          {conn && canMount && (
            <div className={'doc-live' + (ready ? '' : ' is-pending')}>
              <OadTitle
                ref={titleRef}
                ydoc={conn.ydoc}
                readOnly={!editable}
                placeholder={t('untitled')}
                onChange={(title) => {
                  store.setLocalTitle(pageId, title)
                  document.title = (title || t('untitled')) + ' · ' + t('appName')
                }}
                onEnter={() => editorRef.current && focusStart(editorRef.current)}
              />
              <OadEditor
                ydoc={conn.ydoc}
                provider={conn.provider}
                user={user}
                editable={editable}
                onReady={(e) => {
                  editorRef.current = e
                  setEditor(e)
                }}
              />
            </div>
          )}
          {!(ready && canMount) &&
            (snapshot ? <SnapshotView snapshot={snapshot} onPainted={() => setCanMount(true)} /> : <DocSkeleton />)}
        </article>
      </div>
    </div>
  )
}

// 给性能测试用：正文第一次画出来的时间（快照或编辑器，哪个先到算哪个），以及编辑器装好的时间
function markFirstContent(kind: 'snapshot' | 'editor' = 'editor') {
  requestAnimationFrame(() => {
    const w = window as unknown as { __oadFirstContent?: number; __oadEditorReady?: number; __oadFirstKind?: string }
    const now = performance.now()
    if (w.__oadFirstContent === undefined) {
      w.__oadFirstContent = now
      w.__oadFirstKind = kind
    }
    if (kind === 'editor' && w.__oadEditorReady === undefined) w.__oadEditorReady = now
    performance.mark('oad:' + kind)
  })
}

function Breadcrumb({ store, pageId }: { store: PageStore; pageId: string }) {
  const pages = usePages(store)
  const chain = useMemo(() => {
    const byId = new Map(pages.map((p) => [p.id, p]))
    const out = []
    let p = byId.get(pageId)
    let guard = 0
    while (p && guard++ < 50) {
      out.unshift(p)
      p = p.parent_id ? byId.get(p.parent_id) : undefined
    }
    return out
  }, [pages, pageId])
  return (
    <nav className="crumbs" aria-label="breadcrumb">
      {chain.map((p, i) => (
        <span key={p.id} className="crumb">
          {i > 0 && <span className="crumb-sep">/</span>}
          <button className={'crumb-btn' + (p.id === pageId ? ' is-current' : '')} onClick={() => navigate(pagePath(p.id))}>
            {p.icon && <span className="crumb-icon">{p.icon}</span>}
            {p.title || t('untitled')}
          </button>
        </span>
      ))}
    </nav>
  )
}

interface PresenceUser {
  key: string
  name: string
  color: string
  agent: boolean
  writing: boolean
}

function Presence({ provider }: { provider: WebsocketProvider }) {
  const [users, setUsers] = useState<PresenceUser[]>([])
  useEffect(() => {
    const aw = provider.awareness
    const update = () => {
      const seen = new Map<string, PresenceUser>()
      aw.getStates().forEach((s, client) => {
        if (client === aw.clientID || !s?.user) return
        const u = s.user
        const key = String(u.id ?? client)
        const prev = seen.get(key)
        seen.set(key, {
          key,
          name: String(u.name ?? '?'),
          color: typeof u.color === 'string' ? u.color : colorFor(key),
          agent: u.kind === 'agent',
          // 服务器替 HTTP 写入者放的状态带 block（整页替换时是 null）
          writing: 'block' in s || !!prev?.writing,
        })
      })
      setUsers([...seen.values()])
    }
    update()
    aw.on('change', update)
    return () => aw.off('change', update)
  }, [provider])

  if (!users.length) return null
  const shown = users.slice(0, 5)
  const writing = users.find((u) => u.agent && u.writing)
  return (
    <div className="presence" title={t('editingNow', { n: users.length + 1 })}>
      {writing && <span className="presence-writing">{t('agentWriting', { name: writing.name })}</span>}
      <div className="avatars">
        {shown.map((u) => (
          <span key={u.key} className={'avatar' + (u.agent ? ' is-agent' : '')} style={{ background: u.color }} title={u.name}>
            {u.agent ? '✦' : initials(u.name)}
          </span>
        ))}
        {users.length > shown.length && <span className="avatar is-more">+{users.length - shown.length}</span>}
      </div>
    </div>
  )
}

function StatusPill({ status, readOnly }: { status: Status; readOnly: boolean }) {
  const label =
    status === 'synced'
      ? t('statusSynced')
      : status === 'syncing'
        ? t('statusSyncing')
        : status === 'connecting'
          ? t('statusConnecting')
          : status === 'denied'
            ? t('statusDenied')
            : t('statusOffline')
  return (
    <span className={'status status-' + status} role="status" data-status={status}>
      <span className="status-dot" />
      {label}
      {readOnly && status !== 'denied' && <span className="status-ro">· {t('readOnly')}</span>}
    </span>
  )
}

function SnapshotView({ snapshot, onPainted }: { snapshot: { title: string; html: string }; onPainted: () => void }) {
  // 画出来以后的下一帧算“出内容”（和 docs/baseline.md 的量法一样：DOM 里有了、再过一帧）
  useLayoutEffect(() => {
    markFirstContent('snapshot')
    requestAnimationFrame(() => setTimeout(onPainted, 0))
  }, []) // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div className="doc-snapshot" aria-busy="true">
      <div className="oad-title">{snapshot.title || t('untitled')}</div>
      <div className="oad-editor">
        <div className="oad-prose ProseMirror" dangerouslySetInnerHTML={{ __html: snapshot.html }} />
      </div>
    </div>
  )
}

function DocSkeleton() {
  return (
    <div className="doc-skeleton" aria-busy="true">
      <div className="sk sk-title" />
      <div className="sk" style={{ width: '92%' }} />
      <div className="sk" style={{ width: '86%' }} />
      <div className="sk" style={{ width: '64%' }} />
    </div>
  )
}
