// 页面里内嵌的库：编辑器的 embed 节点视图调 renderDatabase，这里挂一个 React 根。
import { createRoot } from 'react-dom/client'
import { DatabaseView } from '@oad/editor/database'
import { client, type Me } from '../api'
import { navigate, pagePath } from '../lib'
import { mediaServices } from '../media'
import type { PageStore } from '../store'

export interface EmbedCtx {
  me: Me
  store: PageStore
  canWrite: boolean
}

export function mountDatabaseEmbed(box: HTMLElement, dbId: string, viewId: string | null, ctx: EmbedCtx) {
  const host = document.createElement('div')
  host.className = 'db-embed'
  box.replaceChildren(host)
  const root = createRoot(host)
  const dbs = [...ctx.store.pages.values()].filter((p) => p.kind === 'database').map((p) => ({ id: p.id, title: p.title }))
  root.render(<DatabaseView client={client} navigate={navigate} pageHref={pagePath} media={mediaServices(ctx.store.ws)} dbId={dbId} viewId={viewId} me={ctx.me} ws={ctx.store.ws} canWrite={ctx.canWrite} events={ctx.store} embedded dbs={dbs} />)
  return () => {
    // 节点视图销毁可能发生在别的组件渲染中，晚一点再卸
    setTimeout(() => root.unmount(), 0)
  }
}
