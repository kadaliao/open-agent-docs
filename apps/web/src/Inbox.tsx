// 右上角的铃铛：自己的通知（被 @、参与的那串有新回复、自己的评论被解决），未读数；点一条跳到那页那串评论。
import { useCallback, useEffect, useRef, useState } from 'react'
import { api, type InboxItem } from './api'
import { ago, colorFor, initials, navigate, pagePath, threadPath } from './lib'
import type { PageStore } from './store'
import { t } from './strings'

function label(n: InboxItem) {
  const k = n.type ?? ''
  if (k.includes('mention')) return t('notifMention')
  if (k.includes('reply')) return t('notifReply')
  if (k.includes('resolve')) return t('notifResolved')
  return t('notifComment')
}

export function Inbox({ store }: { store: PageStore }) {
  const [items, setItems] = useState<InboxItem[]>([])
  const [open, setOpen] = useState(false)
  const box = useRef<HTMLDivElement>(null)

  const load = useCallback(() => {
    api.inbox().then((r) => setItems(r.items), () => {})
  }, [])
  useEffect(() => {
    load()
    const offs = [store.on('notification', load), store.on('resync', load)]
    return () => offs.forEach((f) => f())
  }, [store, load])

  useEffect(() => {
    if (!open) return
    const close = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', close)
    return () => document.removeEventListener('mousedown', close)
  }, [open])

  const unread = items.filter((n) => !n.read_at)
  const go = async (n: InboxItem) => {
    setOpen(false)
    if (!n.read_at) {
      setItems((xs) => xs.map((x) => (x.id === n.id ? { ...x, read_at: Date.now() } : x)))
      api.markRead({ ids: [n.id] }).catch(() => {})
    }
    const page = n.page?.id
    if (page) navigate(n.thread_id ? threadPath(page, n.thread_id) : pagePath(page))
  }

  return (
    <div className="inbox" ref={box}>
      <button className="icon-btn inbox-btn" aria-label={t('inbox')} title={t('inbox')} aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <svg width="17" height="17" viewBox="0 0 18 18" fill="none" aria-hidden>
          <path d="M4.5 7.5a4.5 4.5 0 019 0c0 3 1.25 4.5 1.75 5H2.75c.5-.5 1.75-2 1.75-5z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
          <path d="M7.25 14.75a1.9 1.9 0 003.5 0" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </svg>
        {unread.length > 0 && <span className="badge" data-testid="inbox-count">{unread.length > 99 ? '99+' : unread.length}</span>}
      </button>
      {open && (
        <div className="inbox-pop" role="dialog" aria-label={t('inbox')}>
          <div className="inbox-head">
            <span>{t('inbox')}</span>
            {unread.length > 0 && (
              <button
                className="link-btn"
                onClick={() => {
                  setItems((xs) => xs.map((x) => ({ ...x, read_at: x.read_at ?? Date.now() })))
                  api.markRead({ all: true }).catch(() => {})
                }}
              >
                {t('markAllRead')}
              </button>
            )}
          </div>
          <div className="inbox-list">
            {items.length === 0 && <div className="inbox-empty">{t('inboxEmpty')}</div>}
            {items.map((n) => {
              const who = n.by ?? n.comment?.author
              return (
                <button key={n.id} className={'inbox-item' + (n.read_at ? '' : ' is-unread')} onClick={() => go(n)}>
                  <span className={'avatar avatar-sm' + (who?.kind === 'agent' ? ' is-agent' : '')} style={{ background: colorFor(who?.id ?? '?') }}>
                    {who?.kind === 'agent' ? '✦' : initials(who?.name ?? '?')}
                  </span>
                  <span className="inbox-text">
                    <span className="inbox-line">
                      <b>{who?.name ?? ''}</b> {label(n)}
                    </span>
                    {(n.comment?.body || n.quote) && (
                      <span className="inbox-snippet">{(n.comment?.body || '“' + n.quote + '”').replace(/@\[([^\]]+)\]\([^)]*\)/g, '@$1')}</span>
                    )}
                    <span className="inbox-meta">
                      {n.page?.title || t('untitled')} · {ago(n.created_at)}
                    </span>
                  </span>
                </button>
              )
            })}
          </div>
        </div>
      )}
    </div>
  )
}
