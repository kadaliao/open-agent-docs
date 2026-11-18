// 分享：生成只读的公开链接（可带子页面、可让搜索引擎收录），复制、打开、停止分享。挂在顶栏“分享”按钮下面。
import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { api, type Share } from '../api'
import { useLocale } from '../i18n'
import { copyText } from '../lib'
import { t } from '../strings'

const abs = (u: string) => (u.startsWith('http') ? u : location.origin + u)

export function ShareDialog({ pageId, canWrite, anchor, onClose }: { pageId: string; canWrite: boolean; anchor: DOMRect | null; onClose: () => void }) {
  useLocale()
  const [share, setShare] = useState<Share | null | undefined>(undefined)
  const [opts, setOpts] = useState({ include_children: false, indexable: false })
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const box = useRef<HTMLDivElement>(null)
  const [pos, setPos] = useState<{ top: number; right: number }>({ top: 52, right: 16 })

  useEffect(() => {
    api.getShare(pageId).then(
      (s) => {
        setShare(s)
        if (s) setOpts({ include_children: !!s.include_children, indexable: !!s.indexable })
      },
      (e) => {
        setShare(null)
        setError(e instanceof Error ? e.message : String(e))
      },
    )
  }, [pageId])
  useLayoutEffect(() => {
    if (anchor) setPos({ top: anchor.bottom + 6, right: Math.max(8, window.innerWidth - anchor.right) })
  }, [anchor])
  useEffect(() => {
    const down = (e: MouseEvent) => box.current && !box.current.contains(e.target as Node) && onClose()
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    const id = setTimeout(() => document.addEventListener('mousedown', down))
    document.addEventListener('keydown', key)
    return () => {
      clearTimeout(id)
      document.removeEventListener('mousedown', down)
      document.removeEventListener('keydown', key)
    }
  }, [onClose])

  const publish = async (o = opts) => {
    setBusy(true)
    setError(null)
    try {
      const s = await api.createShare(pageId, o)
      setShare(s)
      setOpts({ include_children: s.include_children ?? o.include_children, indexable: s.indexable ?? o.indexable })
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }
  const revoke = async () => {
    if (!window.confirm(t('shareRevokeConfirm'))) return
    setBusy(true)
    try {
      await api.revokeShare(pageId)
      setShare(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }
  const toggle = (k: keyof typeof opts) => {
    const o = { ...opts, [k]: !opts[k] }
    setOpts(o)
    if (share) publish(o)
  }
  const copy = async () => {
    if (!share) return
    await copyText(abs(share.url))
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }

  return createPortal(
    <div ref={box} className="share-pop" role="dialog" aria-label={t('shareTitle')} style={{ top: pos.top, right: pos.right }}>
      <div className="share-head">
        <span className={'share-globe' + (share ? ' is-on' : '')} aria-hidden>
          <svg viewBox="0 0 24 24" width="18" height="18">
            <path fill="currentColor" d="M12 2a10 10 0 1 1 0 20 10 10 0 0 1 0-20zm6.9 9a7 7 0 0 0-3.4-5.3c.6 1.4 1 3.2 1.1 5.3zM12 4.1c-.8.9-1.8 3.2-1.9 6.9h3.8c-.1-3.7-1.1-6-1.9-6.9zM8.5 5.7A7 7 0 0 0 5.1 11h2.3c.1-2.1.5-3.9 1.1-5.3zM5.1 13a7 7 0 0 0 3.4 5.3c-.6-1.4-1-3.2-1.1-5.3zm5 0c.1 3.7 1.1 6 1.9 6.9.8-.9 1.8-3.2 1.9-6.9zm5.4 5.3a7 7 0 0 0 3.4-5.3h-2.3c-.1 2.1-.5 3.9-1.1 5.3z" />
          </svg>
        </span>
        <div className="share-text">
          <b>{share ? t('sharePublic') : t('shareTitle')}</b>
          <span className="muted">{t('shareHint')}</span>
        </div>
      </div>
      {share === undefined ? (
        <div className="sk" style={{ margin: '8px 0' }} />
      ) : share ? (
        <div className="share-link">
          <input readOnly value={abs(share.url)} onFocus={(e) => e.target.select()} aria-label={t('shareCopy')} />
          <button className="btn btn-sm btn-primary" onClick={copy}>
            {copied ? t('copied') : t('shareCopy')}
          </button>
        </div>
      ) : (
        canWrite && (
          <button className="btn btn-primary btn-block" disabled={busy} onClick={() => publish()}>
            {t('sharePublish')}
          </button>
        )
      )}
      <label className="share-opt">
        <span>{t('shareIncludeChildren')}</span>
        <input type="checkbox" className="share-switch" checked={opts.include_children} disabled={!canWrite || busy} onChange={() => toggle('include_children')} />
      </label>
      <label className="share-opt">
        <span>{t('shareIndexable')}</span>
        <input type="checkbox" className="share-switch" checked={opts.indexable} disabled={!canWrite || busy} onChange={() => toggle('indexable')} />
      </label>
      {share && (
        <div className="share-foot">
          <a className="link-btn" href={abs(share.url)} target="_blank" rel="noreferrer">
            {t('shareOpen')} ↗
          </a>
          {canWrite && (
            <button className="link-btn is-danger" disabled={busy} onClick={revoke}>
              {t('shareRevoke')}
            </button>
          )}
        </div>
      )}
      {error && (
        <div className="form-error" role="alert">
          {error}
        </div>
      )}
    </div>,
    document.body,
  )
}
