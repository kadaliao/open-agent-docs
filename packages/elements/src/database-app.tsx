// <oad-database> 的界面（React）：数据库视图（表格、看板、日历、画廊、列表）。按需加载，元素本身在 database.ts。
import { registerPlugin, type Me, type OadClient, type Page } from '@open-agent-docs/client/core'
import { databasePlugin } from '@open-agent-docs/client/db'
import { OadDatabaseView, OadPortalRoot, OadProvider } from '@open-agent-docs/editor'
import { useEffect, useMemo, useState } from 'react'
import { createRoot } from 'react-dom/client'
import type { ElementHost } from './base.ts'
import type { UrlResolver } from '@open-agent-docs/editor/resolve'

// 元素用的是 client/core 的客户端：本地数据库（WASM 引擎）在这里登记
registerPlugin('database', databasePlugin)

/** 画数据库视图（<oad-database> 的 start() 按需加载这个文件后调用） */
export function mountDatabase(h: ElementHost, o: { dbId: string; opts: () => { viewId: string | null; readonly: boolean } }) {
  const root = createRoot(h.mount)
  const portal = document.createElement('div')
  portal.className = 'oad-el-portal'
  h.shadow.append(portal)
  const host: Host = {
    ready: (title) => h.emit('oad-ready', { id: o.dbId, title }),
    fail: (e) => h.fail(e),
    navigate: (href) => h.navigate(href, h.pageIdOf(href) ?? undefined),
  }
  const render = () => {
    const { viewId, readonly } = o.opts()
    root.render(
      <OadPortalRoot.Provider value={portal}>
        <OadProvider client={h.client}>
          <DbApp host={host} client={h.client} dbId={o.dbId} viewId={viewId} readonly={readonly} base={h.baseUrl} resolveUrl={h.resolveUrl} />
        </OadProvider>
      </OadPortalRoot.Provider>,
    )
  }
  render()
  return {
    rerender: render,
    stop: () => {
      root.unmount()
      const s = h.client.database(o.dbId)
      s.flush()
      s.close()
      portal.remove()
    },
  }
}

interface Host {
  ready: (title: string) => void
  fail: (e: unknown) => void
  navigate: (href: string) => void
}

function DbApp({ host, client, dbId, viewId, readonly, base, resolveUrl }: { host: Host; client: OadClient; dbId: string; viewId: string | null; readonly: boolean; base: string; resolveUrl: UrlResolver | null }) {
  // 文件属性的缩略图、画廊封面、行里的正文：token 登录时换成签名地址
  const media = useMemo(() => ({ unfurl: (url: string) => client.unfurl(url) as never, resolveUrl }), [client, resolveUrl])
  const [got, setGot] = useState<{ me: Me; page: Page } | null>(null)
  useEffect(() => {
    let alive = true
    Promise.all([client.me(), client.page(dbId)]).then(
      ([me, page]) => alive && setGot({ me, page }),
      (e) => alive && host.fail(e),
    )
    return () => {
      alive = false
    }
  }, [client, dbId]) // eslint-disable-line react-hooks/exhaustive-deps

  const ws = got?.page.workspace_id
  const events = useMemo(() => (ws ? client.events(ws) : null), [client, ws])
  useEffect(() => () => events?.close(), [events])

  // 数据到了（缓存或服务器）：oad-ready；出错：oad-error
  useEffect(() => {
    if (!got) return
    const store = client.database(dbId)
    let done = false
    const check = () => {
      if (done) return
      if (store.status === 'ready' || store.status === 'cache') {
        done = true
        host.ready(got.page.title ?? '')
      } else if (store.status === 'error') {
        done = true
        host.fail(new Error(store.error ?? '数据库打不开'))
      }
    }
    const off = store.subscribe(check)
    check()
    return () => {
      off()
    }
  }, [got, client, dbId]) // eslint-disable-line react-hooks/exhaustive-deps

  if (!got || !events) return <div className="oad-el-loading" aria-busy="true" />
  const role = got.me.workspaces.find((w) => w.id === got.page.workspace_id)?.role
  return (
    <OadDatabaseView
      client={client}
      dbId={dbId}
      me={got.me}
      ws={got.page.workspace_id}
      canWrite={!readonly && role !== 'viewer'}
      events={events}
      viewId={viewId}
      embedded
      navigate={host.navigate}
      pageHref={(id) => (base || '') + '/p/' + encodeURIComponent(id)}
      media={media}
    />
  )
}
