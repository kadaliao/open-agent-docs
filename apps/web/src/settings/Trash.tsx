// 回收站：删掉的页面、库、行，恢复或彻底删除；30 天后服务器自动清。
import { useEffect, useMemo, useState } from 'react'
import { api, ApiError, type Page, type TrashItem } from '../api'
import { fmtAgo, useLocale } from '../i18n'
import { navigate, pagePath } from '../lib'
import type { PageStore } from '../store'
import { t } from '../strings'
import { Shell } from './Shell'
import './settings.css'

const who = (x: TrashItem['deleted_by']) => (!x ? null : typeof x === 'string' ? null : x.name)

export default function TrashPage({ store, canWrite, sidebarCollapsed, onExpandSidebar }: { store: PageStore; canWrite: boolean; sidebarCollapsed: boolean; onExpandSidebar: () => void }) {
  useLocale()
  const [items, setItems] = useState<TrashItem[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [q, setQ] = useState('')
  const [busy, setBusy] = useState<string | null>(null)
  const load = () =>
    api.trash(store.ws).then(
      (r) => {
        setItems(r.sort((a, b) => b.deleted_at - a.deleted_at))
        setError(null)
      },
      (e) => {
        setItems([])
        setError(e instanceof ApiError && e.status === 404 ? null : e instanceof Error ? e.message : String(e))
      },
    )
  useEffect(() => {
    load()
  }, [store.ws]) // eslint-disable-line react-hooks/exhaustive-deps
  // 别处删了东西（page.deleted）：刷新
  useEffect(() => store.on('page.deleted', () => load()), [store]) // eslint-disable-line react-hooks/exhaustive-deps

  const shown = useMemo(() => {
    const w = q.trim().toLowerCase()
    return (items ?? []).filter((x) => !w || (x.title || '').toLowerCase().includes(w))
  }, [items, q])

  const restore = async (x: TrashItem) => {
    setBusy(x.id)
    try {
      const r = await api.restoreTrash(x.id)
      const page = (r && 'page' in r ? r.page : r) as Page | undefined
      if (page?.id && page.kind !== 'row') store.upsert(page)
      else store.load().catch(() => {})
      setItems((l) => l?.filter((y) => y.id !== x.id) ?? l)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(null)
    }
  }
  const purge = async (x: TrashItem) => {
    if (!window.confirm(t('deleteForeverConfirm', { title: x.title || t('untitled') }))) return
    setBusy(x.id)
    try {
      await api.purgeTrash(x.id)
      setItems((l) => l?.filter((y) => y.id !== x.id) ?? l)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(null)
    }
  }

  return (
    <Shell store={store} title={t('trash')} sidebarCollapsed={sidebarCollapsed} onExpandSidebar={onExpandSidebar}>
      <div className="settings trash">
        <div className="settings-head">
          <div>
            <h1>{t('trash')}</h1>
            <p className="muted">{t('trashHint')}</p>
          </div>
        </div>
        <input className="set-input trash-search" type="search" placeholder={t('trashSearch')} value={q} onChange={(e) => setQ(e.target.value)} />
        {error && (
          <div className="form-error" role="alert">
            {error}
          </div>
        )}
        {items && !shown.length && <div className="settings-empty">{t('trashEmpty')}</div>}
        <div className="trash-list">
          {shown.map((x) => (
            <div key={x.id} className="trash-item" data-id={x.id}>
              <span className="trash-ico" aria-hidden>
                {x.icon || (x.kind === 'database' ? '▦' : x.kind === 'row' ? '▤' : (
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                    <path d="M4 1.75h5.2L12.5 5v8.25c0 .55-.45 1-1 1H4c-.55 0-1-.45-1-1v-10.5c0-.55.45-1 1-1z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
                    <path d="M9 1.9V5.2h3.3M5.5 8h5M5.5 10.5h3.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
                  </svg>
                ))}
              </span>
              <div className="trash-main">
                <button className="trash-title" onClick={() => navigate(pagePath(x.id))} disabled>
                  {x.title || t('untitled')}
                </button>
                <span className="muted trash-meta">
                  {x.parent_title ? x.parent_title + ' · ' : ''}
                  {t('deletedAgo', { when: fmtAgo(x.deleted_at) })}
                  {who(x.deleted_by) ? ' · ' + t('deletedBy', { name: who(x.deleted_by)! }) : ''}
                </span>
              </div>
              {canWrite && (
                <>
                  <button className="btn btn-sm" disabled={busy === x.id} onClick={() => restore(x)}>
                    {t('restore')}
                  </button>
                  <button className="btn btn-sm btn-danger" disabled={busy === x.id} onClick={() => purge(x)}>
                    {t('deleteForever')}
                  </button>
                </>
              )}
            </div>
          ))}
        </div>
      </div>
    </Shell>
  )
}
