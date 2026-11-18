import { useCallback, useEffect, useMemo, useState } from 'react'
import { api, ApiError, type AuthMode, type Me } from './api'
import { SidebarIcon } from './icons'
import { Login } from './Login'
import { Sidebar } from './Sidebar'
import { clearLocalData, navigate, pageIdOf, pagePath, storageGet, storageSet, usePath } from './lib'
import { PageStore, usePages } from './store'
import { t } from './strings'

// 编辑器那一块单独打包；一启动就开始下载，和外壳并行。
// 不用 React.lazy + Suspense：React 19 揭开 Suspense 时会故意等最多 300ms 凑一批，打开页面平白慢一截
type PageViewModule = typeof import('./PageView')
let pageViewModule: PageViewModule | null = null
const pageViewPromise = import('./PageView').then((m) => (pageViewModule = m))
// 打开的是某一页：不等登录信息和编辑器，先连上这一页（读本机缓存、连 WebSocket）
const firstPage = pageIdOf(location.pathname)
if (firstPage) import('./conn').then((m) => m.preopenPage(firstPage))

function usePageView() {
  const [m, setM] = useState(pageViewModule)
  useEffect(() => {
    if (!m) pageViewPromise.then(setM)
  }, [m])
  return m?.default ?? null
}

const ME_KEY = 'oad:me'
const LAST_KEY = 'oad:last-page'

type Session = { mode: AuthMode; me: Me } | { mode: AuthMode; me: null } | null

export function App() {
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

function Workspace({ me, mode, onLogout }: { me: Me; mode: AuthMode; onLogout: () => void }) {
  const ws = me.workspaces[0]
  const store = useMemo(() => (ws ? new PageStore(ws.id) : null), [ws?.id]) // eslint-disable-line react-hooks/exhaustive-deps
  const path = usePath()
  const pageId = pageIdOf(path)
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
    if (pageId) storageSet(LAST_KEY, pageId)
  }, [pageId])

  // 首页：回到上次打开的那页
  useEffect(() => {
    if (path === '/') {
      const last = storageGet<string>(LAST_KEY)
      if (last) navigate(pagePath(last), true)
    }
  }, [path])

  if (!ws || !store) return <div className="splash">{t('error')}</div>
  const canWrite = ws.role !== 'viewer'

  return (
    <div className={'app' + (collapsed ? ' is-collapsed' : '')}>
      <Sidebar
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
      />
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
            <div className="page-skeleton" />
          )
        ) : (
          <Home store={store} canWrite={canWrite} collapsed={collapsed} onExpand={() => setCollapsed(false)} />
        )}
      </main>
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
        <div className="home-mark" aria-hidden>
          ✦
        </div>
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
