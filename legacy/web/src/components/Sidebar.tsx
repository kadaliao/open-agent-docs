// 左边的侧栏：找页面、置顶的页面、页面树、新建页面 / 数据库、回收站、设置。手机上是抽屉（App.tsx 管开关）。
import { useEffect, useState } from 'react'
import { api, navigate, openPage, useStore } from '../store'
import type { Page, SearchHit } from '../types'
import { t, tc } from '../i18n'
import { MoveDialog, PageTree, newPage } from './PageTree'
import { newDb } from './db/db'
import { pageIcon } from './PageView'

// 找页面：按标题和正文（open_agent_docs/search.py）
function useSearch() {
  const [q, setQ] = useState('')
  const [hits, setHits] = useState<SearchHit[] | null>(null)
  useEffect(() => {
    if (!q.trim()) { setHits(null); return }
    const timer = setTimeout(() => api<SearchHit[]>(`/api/search?q=${encodeURIComponent(q)}`).then(setHits).catch(() => setHits([])), 250)
    return () => clearTimeout(timer)
  }, [q])
  return { q, setQ, hits }
}

export default function Sidebar({ onClose }: { onClose: () => void }) {
  const list = useStore(s => s.pages)
  const route = useStore(s => s.route)
  const connected = useStore(s => s.connected)
  const [moving, setMoving] = useState<Page | null>(null)
  const { q, setQ, hits } = useSearch()
  const active = route.name === 'page' ? route.id : null
  const go = (id: number) => { openPage(id); onClose() }
  const pins = (list || []).filter(p => p.pinned)
  return (
    <aside className="sidebar">
      <div className="side-top">
        <button className="logo-btn" onClick={() => { navigate({ name: 'home' }); onClose() }} title={t('首页')}>
          <span className="logo">◆</span> Open Agent Docs
        </button>
      </div>
      <div className="side-search">
        <input value={q} placeholder={t('找页面')} onChange={e => setQ(e.target.value)}
          onKeyDown={e => { if (e.key === 'Escape') setQ('') }} />
      </div>
      <nav className="side-pages">
        {hits ? (hits.length ? hits.map(h => (
          <button key={h.ref} className={'side-hit' + (h.ref === active ? ' active' : '')} onClick={() => go(h.ref)}>
            <b>{h.title || t('无标题')}</b><span className="muted small">{h.snippet}</span>
          </button>
        )) : <div className="muted small side-empty">{t('没找到相关的页面')}</div>) : <>
          {pins.length > 0 && <>
            <div className="side-group">{tc('group', '置顶')}</div>
            {pins.map(p => (
              <button key={p.id} className={'side-pin' + (p.id === active ? ' active' : '')} onClick={() => go(p.id)}>
                <span className="lib-icon">{pageIcon(p)}</span><span className="side-title">{p.title || t('无标题')}</span>
              </button>
            ))}
            <div className="side-group">{tc('group', '页面')}</div>
          </>}
          {!list ? <div className="muted small side-empty">{t('加载中…')}</div>
            : !list.length ? <div className="muted small side-empty">{t('还没有页面。')}</div>
            : <PageTree list={list} active={active} onMove={setMoving} onOpen={onClose} row={p => <>
                <span className="lib-icon">{pageIcon(p)}</span>
                <span className="side-title">{p.title || t('无标题')}</span>
                {p.share_token && <span className="side-shared" title={t('已分享')}>↗</span>}
              </>} />}
        </>}
      </nav>
      <div className="side-new">
        <button className="btn" onClick={() => { newPage(); onClose() }}>＋ {t('页面')}</button>{/* i18n-ignore */}
        <button className="btn" onClick={() => { newDb(); onClose() }}>＋ {t('数据库')}</button>{/* i18n-ignore */}
      </div>
      <div className="side-bottom">
        <button className={'side-link' + (route.name === 'trash' ? ' on' : '')} onClick={() => { navigate({ name: 'trash' }); onClose() }}>🗑 {t('回收站')}</button>
        <button className={'side-link' + (route.name === 'settings' ? ' on' : '')} onClick={() => { navigate({ name: 'settings' }); onClose() }}>⚙ {t('设置')}</button>
        <span className="grow" />
        <span className={'conn' + (connected ? ' on' : '')} title={connected ? t('已连接') : t('连接中断，正在重连')} />
      </div>
      {moving && list && <MoveDialog page={moving} list={list} onClose={() => setMoving(null)} />}
    </aside>
  )
}
