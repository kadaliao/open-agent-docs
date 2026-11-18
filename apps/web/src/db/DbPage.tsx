// 数据库整页（PageView 按 kind 切过来，单独一个包）：标题 + 库。也带出行页面用的属性头、内嵌用的挂载函数。
import { useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react'
import { api, type Me } from '../api'
import { usePages, type PageStore } from '../store'
import { DatabaseView } from './DatabaseView'
import { readDbSnapshot, saveDbSnapshot, takeDbSnapshot } from './snapshot'
import { DbStore } from './store'
import { t } from './strings'
import { preloadEngine } from './engine'
import './db.css'

export { RowHeader } from './RowPanel'
export { mountDatabaseEmbed } from './embed'

export function DbPageBody({ pageId, me, store, canWrite }: { pageId: string; me: Me; store: PageStore; canWrite: boolean }) {
  const pages = usePages(store)
  const page = pages.find((p) => p.id === pageId)
  const db = DbStore.get(pageId)
  const dbs = useMemo(() => pages.filter((p) => p.kind === 'database').map((p) => ({ id: p.id, title: p.title })), [pages])
  const [title, setTitle] = useState(() => page?.title ?? db.meta?.page.title ?? '')
  const typing = useRef(0)
  const timer = useRef(0)
  // 别人改了标题：没在打字就跟上
  const remote = page?.title ?? db.meta?.page.title ?? ''
  useEffect(() => {
    if (Date.now() - typing.current > 1500) setTitle(remote)
  }, [remote])
  useEffect(() => {
    document.title = (title || t('untitled')) + ' · Open Agent Docs'
  }, [title])
  // 首屏快照：库的数据（IndexedDB + 引擎）还没好时先画上次的样子
  useSyncExternalStore(db.subscribe, db.getVersion)
  const [snap] = useState(() => (db.meta ? null : readDbSnapshot(pageId)))
  useEffect(() => {
    db.open(store)
  }, [db, store])
  const live = !!db.meta
  const box = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    if (snap && !live) {
      ;(window as unknown as { __oadDbFirst?: number }).__oadDbFirst ??= performance.now()
    }
  }, [snap, live])
  useEffect(() => {
    if (!live) return
    let timer = 0
    const save = () => box.current && saveDbSnapshot(pageId, takeDbSnapshot(box.current))
    const later = () => {
      clearTimeout(timer)
      timer = window.setTimeout(save, 1500)
    }
    const off = db.subscribe(later)
    later()
    window.addEventListener('pagehide', save)
    return () => {
      off()
      clearTimeout(timer)
      window.removeEventListener('pagehide', save)
      save()
    }
  }, [live, pageId, db])
  if (snap && !live) return <div className="db-page db-snapshot" aria-busy="true" dangerouslySetInnerHTML={{ __html: snap }} />
  return (
    <div className="db-page" ref={box}>
      <div className="db-page-head">
        <input
          className="db-page-title"
          value={title}
          placeholder={t('untitled')}
          readOnly={!canWrite}
          aria-label={t('dbLabel')}
          onChange={(e) => {
            const v = e.target.value
            setTitle(v)
            typing.current = Date.now()
            store.setLocalTitle(pageId, v)
            clearTimeout(timer.current)
            timer.current = window.setTimeout(() => api.patchPage(pageId, { title: v }).catch(() => {}), 400)
          }}
          onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
        />
      </div>
      <DatabaseView dbId={pageId} me={me} ws={store.ws} canWrite={canWrite} events={store} dbs={dbs} />
    </div>
  )
}

export { preloadEngine }

/** 提前开始打开一个库（App 在认出地址是库时调） */
export function prefetchDb(id: string, events?: PageStore) {
  DbStore.get(id).open(events)
}

// 调试和端到端测试用
;(window as unknown as { __oadDb?: typeof DbStore }).__oadDb = DbStore
