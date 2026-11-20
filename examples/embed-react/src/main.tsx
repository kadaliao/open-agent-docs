// 宿主应用：自己的页面框架里放 Open Agent Docs 的编辑器、评论栏、数据库视图。
// 登录由宿主的 server.mjs 管（它转发 /api 时带上用户），这里只管用同源的客户端。
import { createClient, type Me, type Page } from '@open-agent-docs/client'
import { OadComments, OadDatabaseView, OadEditor, OadProvider, OadTitle, colorFor, focusStart, useOadPage, usePageComments, usePeople, type CommentAnchor } from '@open-agent-docs/editor'
import '@open-agent-docs/editor/style.css'
import { StrictMode, useEffect, useMemo, useState } from 'react'
import { createRoot } from 'react-dom/client'
import './host.css'

// 同源：浏览器 → 宿主的 server.mjs → Open Agent Docs
const client = createClient({ baseUrl: '' })

function Doc({ page, me, ws, onTitle }: { page: Page; me: Me; ws: string; onTitle: (id: string, title: string) => void }) {
  const user = useMemo(() => ({ id: me.user.id, name: me.user.name, color: colorFor(me.user.id) }), [me])
  const doc = useOadPage(client, page.id, { user })
  const events = useMemo(() => client.events(ws), [ws])
  useEffect(() => () => events.close(), [events])
  const { threads, reload } = usePageComments(client, page.id, events)
  const people = usePeople(client, ws, me, {}, threads)
  const [draft, setDraft] = useState<CommentAnchor | null>(null)
  const [active, setActive] = useState<string | null>(null)
  const [editor, setEditor] = useState<import('@open-agent-docs/editor').Editor | null>(null)

  if (page.kind === 'database')
    return (
      <div className="host-doc">
        <OadDatabaseView dbId={page.id} me={me} ws={ws} canWrite events={events} embedded navigate={() => {}} />
      </div>
    )
  if (!doc.ydoc) return <div className="host-doc">连接中…</div>
  return (
    <div className="host-split">
      <article className="host-doc" data-status={doc.status}>
        <OadTitle ydoc={doc.ydoc} placeholder="无标题" onEnter={() => editor && focusStart(editor)} onChange={(title) => onTitle(page.id, title)} />
        <OadEditor
          ydoc={doc.ydoc}
          provider={doc.provider}
          user={user}
          onReady={setEditor}
          onComment={(a) => {
            setDraft(a)
            setActive(null)
          }}
        />
      </article>
      <OadComments
        pageId={page.id}
        me={me}
        editor={doc.ready ? editor : null}
        threads={threads}
        reload={reload}
        active={active}
        setActive={setActive}
        draft={draft}
        setDraft={setDraft}
        candidates={people.candidates}
        nameOf={people.nameOf}
        canWrite
        docVersion={0}
        onClose={() => setDraft(null)}
      />
    </div>
  )
}

function App() {
  const [me, setMe] = useState<Me | null>(null)
  const [pages, setPages] = useState<Page[]>([])
  const [current, setCurrent] = useState<string | null>(null)
  useEffect(() => {
    client.me().then(async (m) => {
      setMe(m)
      const list = await client.pages(m.workspaces[0].id)
      setPages(list)
      setCurrent((c) => c ?? list[0]?.id ?? null)
      // 别人新建、改名的页面（实时事件）
      const ev = client.events(m.workspaces[0].id)
      const upsert = (p: Page) => p.kind !== 'row' && setPages((l) => (l.some((x) => x.id === p.id) ? l.map((x) => (x.id === p.id ? { ...x, ...p } : x)) : [...l, p]))
      ev.on('page.created', upsert)
      ev.on('page.updated', upsert)
    })
  }, [])
  if (!me) return <div className="host-loading">登录中…</div>
  const ws = me.workspaces[0].id
  const page = pages.find((p) => p.id === current)
  const create = async () => {
    const p = await client.createPage(ws, { title: '' })
    setPages((l) => (l.some((x) => x.id === p.id) ? l : [...l, p]))
    setCurrent(p.id)
  }
  return (
    <OadProvider client={client}>
      <div className="host">
        <header className="host-bar">
          <b>我的产品</b>
          <span className="host-me" data-user={me.user.id}>
            {me.user.name} · <a href="/login?as=alice">Alice</a> / <a href="/login?as=bob">Bob</a>
          </span>
        </header>
        <nav className="host-nav">
          {pages.map((p) => (
            <button key={p.id} className={p.id === current ? 'is-on' : ''} onClick={() => setCurrent(p.id)} data-page={p.id}>
              {p.title || '无标题'}
            </button>
          ))}
          <button className="host-new" onClick={create}>
            + 新建页面
          </button>
        </nav>
        <main className="host-main">{page ? <Doc key={page.id} page={page} me={me} ws={ws} onTitle={(id, title) => setPages((l) => l.map((p) => (p.id === id ? { ...p, title } : p)))} /> : <p>还没有页面</p>}</main>
      </div>
    </OadProvider>
  )
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
