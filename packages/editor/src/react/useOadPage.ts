// useOadPage(client, id)：打开一页的协作文档，组件卸载时关掉。配 <OadTitle> / <OadEditor> 用：
//   const page = useOadPage(client, id, { user })
//   if (!page.conn) return null
//   <OadTitle ydoc={page.ydoc} /> <OadEditor ydoc={page.ydoc} provider={page.provider} user={user} />
import type { OadClient, OpenPageOptions, PageConn, PageStatus } from '@oad/client/core'
import { registerPlugin } from '@oad/client/core'
import { openPage } from '@oad/client/page'
import { useEffect, useState, useSyncExternalStore } from 'react'
import type { OadUser } from '../extensions'

// 宿主只引了 '@oad/client/core' 也能用
registerPlugin('openPage', openPage)

export interface OadPage {
  conn: PageConn | null
  ydoc: PageConn['ydoc'] | null
  provider: PageConn['provider'] | null
  /** 内容到了（本机缓存或服务器） */
  ready: boolean
  status: PageStatus
}

export function useOadPage(client: OadClient, id: string | null | undefined, o: OpenPageOptions & { user?: OadUser } = {}): OadPage {
  const [conn, setConn] = useState<PageConn | null>(null)
  const [ready, setReady] = useState(false)
  useEffect(() => {
    if (!id) return
    const c = client.openPage(id, o)
    if (o.user) c.awareness.setLocalStateField('user', { kind: 'user', ...o.user })
    setConn(c)
    setReady(false)
    let alive = true
    c.ready.then(() => alive && setReady(true))
    return () => {
      alive = false
      c.destroy()
      setConn(null)
    }
  }, [client, id]) // eslint-disable-line react-hooks/exhaustive-deps
  const status = useSyncExternalStore(
    (l) => conn?.onStatus(l) ?? (() => {}),
    () => conn?.status() ?? 'connecting',
    () => 'connecting' as PageStatus,
  )
  return { conn, ydoc: conn?.ydoc ?? null, provider: conn?.provider ?? null, ready, status }
}
