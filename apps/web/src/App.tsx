import { lazy, Suspense, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { api, ApiError, type AuthMode, type Me } from './api'
import { Logo, SidebarIcon } from './icons'
import { Login } from './Login'
import { Sidebar } from './Sidebar'
import { adoptAccountLocale, useLocale } from './i18n'
import { clearLocalData, navigate, onCommand, pageIdOf, pagePath, pushRecent, storageGet, storageSet, usePath } from './lib'
import { Overlays } from './Overlays'
import { EMBED, post } from './embed'
import { onToken } from './token'
import { PageStore, usePages } from './store'
import { markFirstContent, onFirstScreen, peekFirstScreen, prefetchFirstScreen } from './firstscreen'
import { PageTitle } from './PageTitle'
import { t } from './strings'

// 编辑器那一块单独打包；一启动就开始下载，和外壳并行。
// 不用 React.lazy + Suspense：React 19 揭开 Suspense 时会故意等最多 300ms 凑一批，打开页面平白慢一截
type PageViewModule = typeof import('./PageView')
let pageViewModule: PageViewModule | null = null
const pageViewPromise = import('./PageView').then((m) => (pageViewModule = m))
// 打开的是某一页：不等登录信息和编辑器，先连上这一页（读本机缓存、连 WebSocket）
const firstPage = pageIdOf(location.pathname)
// 侧栏缓存里看得出它是数据库：不连文档，先下载库的代码（App 渲染时再开始拿数据）
const firstIsDb = (() => {
  try {
    const me = JSON.parse(localStorage.getItem('oad:me') ?? 'null') as Me | null
    const pages = JSON.parse(localStorage.getItem(`oad:pages:${me?.workspaces[0]?.id}`) ?? '[]') as { id: string; kind?: string }[]
    return pages.some((p) => p.id === firstPage && p.kind === 'database')
  } catch {
    return false
  }
})()
// 库的本机缓存（IndexedDB）和引擎（WASM）在界面启动之前就开始读
if (firstPage && firstIsDb) import('./db/DbPage').then((m) => m.prefetchDb(firstPage))
else if (firstPage) {
  // 正文的 JSON 和 WebSocket 一起要：第一次打开时先画首屏（见 firstscreen.ts）
  prefetchFirstScreen(firstPage)
  import('./conn').then((m) => m.preopenPage(firstPage))
}

/** 页面那个包还在下载时：服务器的首屏到了就先画（第一次打开、本机没有快照时，见 firstscreen.ts），不然是骨架 */
function PageLoading({ pageId, store }: { pageId: string; store: PageStore }) {
  const [html, setHtml] = useState(() => peekFirstScreen(pageId))
  useEffect(() => (html ? undefined : onFirstScreen(() => setHtml(peekFirstScreen(pageId)))), [pageId, html])
  useLayoutEffect(() => {
    if (html) markFirstContent('server')
  }, [html])
  if (!html) return <div className="page-skeleton" />
  return (
    <div className="page">
      <header className="topbar" />
      <div className="page-body">
        <div className="page-scroll">
          <article className="doc">
            <div className="doc-snapshot" aria-busy="true">
              <div className="oad-title">
                <PageTitle store={store} pageId={pageId} />
              </div>
              <div className="oad-editor">
                <div className="oad-prose ProseMirror" dangerouslySetInnerHTML={{ __html: html }} />
              </div>
            </div>
          </article>
        </div>
      </div>
    </div>
  )
}

function usePageView() {
  const [m, setM] = useState(pageViewModule)
  useEffect(() => {
    if (!m) pageViewPromise.then(setM)
  }, [m])
  return m?.default ?? null
}

// 设置页不在打开页面的关键路径上，用 lazy 就行
const SettingsPage = lazy(() => import('./settings/Settings'))
const TrashPage = lazy(() => import('./settings/Trash'))
const InvitePage = lazy(() => import('./settings/Invite'))

const ME_KEY = 'oad:me'
const LAST_KEY = 'oad:last-page'

type Session = { mode: AuthMode; me: Me } | { mode: AuthMode; me: null } | null

export function App() {
  // 换语言时整个界面重画（不刷新）
  useLocale()
  // 上次的登录信息先拿来用（本地优先），后台再向服务器确认
  const [session, setSession] = useState<Session>(() => {
    const me = storageGet<Me>(ME_KEY)
    return me ? { mode: 'local', me } : null
  })
  const [bootError, setBootError] = useState<string | null>(null)

  const boot = useCallback(async () => {
    setBootError(null)
    try {
      // 两个请求一起发
      const config = api.authConfig()
      const mePromise = api.me()
      mePromise.catch(() => {})
      const { mode } = await config
      try {
        const me = await mePromise
        storageSet(ME_KEY, me)
        adoptAccountLocale(me.user.locale)
        setSession({ mode, me })
      } catch (e) {
        if (e instanceof ApiError && e.status === 401) {
          if (storageGet(ME_KEY)) await clearLocalData()
          setSession({ mode, me: null })
        } else throw e
      }
    } catch (e) {
      // 断网：有缓存就照常用
      if (!storageGet(ME_KEY)) setBootError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  useEffect(() => {
    boot()
  }, [boot])

  // 跨域嵌入：宿主后来才给 token（或者换了一个），还没登录上的话用它再登录一次
  const loggedIn = !!session?.me
  useEffect(() => onToken(() => !loggedIn && boot()), [boot, loggedIn])

  if (bootError && !session)
    return (
      <div className="splash">
        <div className="splash-card">
          <div className="splash-title">{t('error')}</div>
          <div className="muted">{bootError}</div>
          <button className="btn" onClick={boot}>
            {t('retry')}
          </button>
        </div>
      </div>
    )
  if (!session) return <div className="splash" aria-busy="true" />
  // 邀请链接：没登录也能打开（登录或注册以后加入）
  const invite = /^\/invite\/([^/?#]+)/.exec(location.pathname)?.[1]
  if (invite)
    return (
      <Suspense fallback={<div className="splash" aria-busy="true" />}>
        <InvitePage
          token={invite}
          me={session.me}
          onJoined={(me) => {
            storageSet(ME_KEY, me)
            setSession({ mode: session.mode, me })
            navigate('/', true)
          }}
        />
      </Suspense>
    )
  // 嵌入且宿主签 JWT：登录是宿主的事，等它发 token 来（已经发了 oad:auth-required）
  if (!session.me && EMBED && session.mode === 'jwt') return <div className="splash" aria-busy="true" />
  if (!session.me)
    return (
      <Login
        onDone={(me) => {
          // 换了个人登录：别用上一个人留下的缓存
          const prev = storageGet<Me>(ME_KEY)
          if (prev && prev.user.id !== me.user.id) clearLocalData()
          storageSet(ME_KEY, me)
          setSession({ mode: session.mode, me })
        }}
      />
    )
  return (
    <Workspace
      me={session.me}
      mode={session.mode}
      onMe={(me) => {
        storageSet(ME_KEY, me)
        setSession({ mode: session.mode, me })
      }}
      onLogout={async () => {
        await api.logout().catch(() => {})
        setSession({ mode: session.mode, me: null })
        navigate('/', true)
        // 等页面卸载完（它卸载时会存一次快照）再清
        await new Promise((r) => setTimeout(r, 100))
        await clearLocalData()
      }}
    />
  )
}

function Workspace({ me, mode, onLogout, onMe }: { me: Me; mode: AuthMode; onLogout: () => void; onMe: (me: Me) => void }) {
  const ws = me.workspaces[0]
  const store = useMemo(() => (ws ? new PageStore(ws.id) : null), [ws?.id]) // eslint-disable-line react-hooks/exhaustive-deps
  const path = usePath()
  const pageId = pageIdOf(path)
  // 打开的是库（侧栏缓存里看得出来）：库的代码、引擎、数据马上开始拿，不等 PageView
  const prefetched = useRef<string | null>(null)
  if (store && pageId && prefetched.current !== pageId && store.pages.get(pageId)?.kind === 'database') {
    prefetched.current = pageId
    import('./db/DbPage').then((m) => m.prefetchDb(pageId, store))
  }
  const narrow = () => window.matchMedia('(max-width: 760px)').matches
  // 窄屏上侧栏盖在内容上面，默认收起；选了页面也收起
  const [collapsed, setCollapsed] = useState(() => narrow() || (storageGet<boolean>('oad:sidebar-collapsed') ?? false))
  useEffect(() => {
    if (narrow()) setCollapsed(true)
  }, [pageId])
  const PageView = usePageView()

  useEffect(() => {
    if (!store) return
    store.load().catch(() => {})
    store.connect()
    return () => store.close()
  }, [store])

  useEffect(() => {
    if (!pageId) return
    if (EMBED) {
      post({ type: 'oad:navigate', pageId, title: store?.pages.get(pageId)?.title ?? '' })
      return
    }
    storageSet(LAST_KEY, pageId)
    pushRecent(pageId)
  }, [pageId]) // eslint-disable-line react-hooks/exhaustive-deps
  // ⌘\ 收起 / 展开侧栏
  useEffect(
    () =>
      onCommand((c) => {
        if (c !== 'sidebar') return
        setCollapsed((v) => {
          if (!narrow()) storageSet('oad:sidebar-collapsed', !v)
          return !v
        })
      }),
    [],
  )

  // 首页：回到上次打开的那页
  useEffect(() => {
    if (path === '/') {
      const last = storageGet<string>(LAST_KEY)
      if (last) navigate(pagePath(last), true)
    }
  }, [path])

  if (!ws || !store) return <div className="splash">{t('error')}</div>
  const canWrite = ws.role !== 'viewer'
  const expand = () => {
    setCollapsed(false)
    if (!narrow()) storageSet('oad:sidebar-collapsed', false)
  }

  return (
    <div className={'app' + (collapsed || EMBED ? ' is-collapsed' : '') + (EMBED ? ' is-embed' : '')}>
      {!EMBED && <Sidebar
        store={store}
        me={me}
        workspaceName={ws.name}
        currentId={pageId}
        canWrite={canWrite}
        canLogout={mode === 'local'}
        onLogout={onLogout}
        onCollapse={() => {
          setCollapsed(true)
          storageSet('oad:sidebar-collapsed', true)
        }}
      />}
      <main className="main">
        {pageId ? (
          PageView ? (
            <PageView
              key={pageId}
              pageId={pageId}
              me={me}
              store={store}
              canWrite={canWrite}
              sidebarCollapsed={collapsed}
              onExpandSidebar={() => {
                setCollapsed(false)
                storageSet('oad:sidebar-collapsed', false)
              }}
            />
          ) : (
            <PageLoading pageId={pageId} store={store} />
          )
        ) : path.startsWith('/settings') ? (
          <Suspense fallback={<div className="page-skeleton" />}>
            <SettingsPage store={store} me={me} onMe={onMe} sidebarCollapsed={collapsed} onExpandSidebar={expand} />
          </Suspense>
        ) : path.startsWith('/trash') ? (
          <Suspense fallback={<div className="page-skeleton" />}>
            <TrashPage store={store} canWrite={canWrite} sidebarCollapsed={collapsed} onExpandSidebar={expand} />
          </Suspense>
        ) : (
          <Home store={store} canWrite={canWrite} collapsed={collapsed} onExpand={() => setCollapsed(false)} />
        )}
      </main>
      {/* 窄屏：侧栏是抽屉，点外面收起 */}
      {!collapsed && !EMBED && <div className="drawer-mask" onClick={() => setCollapsed(true)} aria-hidden />}
      {!EMBED && <Overlays store={store} me={me} pageId={pageId} />}
    </div>
  )
}

function Home({ store, canWrite, collapsed, onExpand }: { store: PageStore; canWrite: boolean; collapsed: boolean; onExpand: () => void }) {
  const pages = usePages(store)
  const create = async () => {
    const p = await api.createPage(store.ws, {})
    store.upsert(p)
    navigate(pagePath(p.id))
  }
  return (
    <div className="home">
      {collapsed && (
        <button className="icon-btn floating-toggle" onClick={onExpand} aria-label={t('toggleSidebar')}>
          <SidebarIcon />
        </button>
      )}
      <div className="home-inner">
        <Logo size={56} />
        <h1>{t('emptyTitle')}</h1>
        <p className="muted">{t('emptyHint')}</p>
        {canWrite && (
          <button className="btn btn-primary" onClick={create}>
            {pages.length ? t('newPage') : t('createFirst')}
          </button>
        )}
      </div>
    </div>
  )
}
