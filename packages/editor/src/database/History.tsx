// 库的改动记录：谁、什么时候改了哪一格（从什么改成什么）、改了表结构；能撤销（撤销也记一条）。整个库或某一行。
import { useEffect, useState } from 'react'
import { ApiError, type DbHistoryItem, type Person } from '@open-agent-docs/client'
import { fmtAgo, fmtDate, useLocale } from '../i18n'
import { colorFor, initials } from '../react/util'
import { useDb } from './cells'
import { t } from './strings'

function personOf(by: DbHistoryItem['by'], people: Map<string, Person>): Person | null {
  if (!by) return null
  if (typeof by !== 'string') return by
  const [kind, id] = by.includes(':') ? by.split(':') : ['user', by]
  return people.get(id) ?? { kind: kind === 'agent' ? 'agent' : 'user', id, name: id }
}

export function DbHistory({ rowId, onClose }: { rowId?: string; onClose?: () => void }) {
  useLocale()
  const { store, people, canWrite } = useDb()
  const [items, setItems] = useState<DbHistoryItem[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [only, setOnly] = useState(false)
  const [busy, setBusy] = useState<number | null>(null)
  const load = () =>
    store.client.dbHistory(store.id, rowId ? { row: rowId } : {}).then(
      (r) => {
        setItems([...r.items].sort((a, b) => b.seq - a.seq))
        setError(null)
      },
      (e) => {
        setItems([])
        setError(e instanceof ApiError && e.status === 404 ? t('historyUnavailable') : e instanceof Error ? e.message : String(e))
      },
    )
  useEffect(() => {
    load()
  }, [store.id, rowId]) // eslint-disable-line react-hooks/exhaustive-deps
  // 库有新改动：刷新
  useEffect(() => {
    let last = store.seq
    const off = store.subscribe(() => {
      if (store.seq !== last) {
        last = store.seq
        load()
      }
    })
    return () => {
      off()
    }
  }, [store, rowId]) // eslint-disable-line react-hooks/exhaustive-deps

  const show = (key: string | null | undefined, v: unknown) => {
    if (v === null || v === undefined || v === '') return '—'
    try {
      if (key && store.prop(key) && store.engine) return store.engine.display(key, v) || '—'
    } catch {
      /* 属性删了：原样显示 */
    }
    return typeof v === 'string' ? v : JSON.stringify(v)
  }
  const describe = (x: DbHistoryItem) => {
    if (x.kind === 'schema') return x.summary || t('historySchema')
    if (x.kind === 'row') return x.summary || (x.new === null || x.prop === '@deleted' ? t('historyDeleteRow') : t('historyAddRow'))
    if (x.prop === '@position') return t('historyMoveRow')
    const prop = x.prop_name || (x.prop ? store.prop(x.prop)?.name : '') || x.prop || ''
    const o = show(x.prop, x.old)
    const n = show(x.prop, x.new)
    if (o === '—') return t('historyCellSet', { prop, new: n })
    if (n === '—') return t('historyCellClear', { prop, old: o })
    return t('historyCell', { prop, old: o, new: n })
  }
  const undo = async (x: DbHistoryItem) => {
    if (!window.confirm(t('historyUndoConfirm'))) return
    setBusy(x.seq)
    try {
      await store.client.dbUndo(store.id, x.seq)
      await Promise.all([store.sync(), store.refreshMeta()])
      load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(null)
    }
  }

  const shown = (items ?? []).filter((x) => !only || x.kind === 'schema')
  return (
    <div className="dbh" aria-label={rowId ? t('historyRowTitle') : t('historyTitle')}>
      <div className="dbh-head">
        <b>{rowId ? t('historyRowTitle') : t('historyTitle')}</b>
        {!rowId && (
          <div className="db-menu-seg dbh-seg">
            <button type="button" aria-pressed={!only} onClick={() => setOnly(false)}>
              {t('historyAll')}
            </button>
            <button type="button" aria-pressed={only} onClick={() => setOnly(true)}>
              {t('historyOnlySchema')}
            </button>
          </div>
        )}
        {onClose && (
          <button type="button" className="icon-btn" onClick={onClose} aria-label={t('closePanel')}>
            ×
          </button>
        )}
      </div>
      {error && <div className="db-empty-note">{error}</div>}
      {items && !shown.length && !error && <div className="db-empty-note">{t('historyEmpty')}</div>}
      <div className="dbh-list">
        {shown.map((x) => {
          const who = personOf(x.by, people)
          const row = x.row_id ? x.row_title ?? (store.rowTitle(store.rows.get(x.row_id)) || undefined) : null
          return (
            <div key={x.seq} className={'dbh-item is-' + x.kind + (x.undone ? ' is-undone' : '')} data-seq={x.seq}>
              {who ? (
                <span className={'avatar avatar-xs' + (who.kind === 'agent' ? ' is-agent' : '')} style={{ background: colorFor(who.id) }} title={who.name}>
                  {who.kind === 'agent' ? '✦' : initials(who.name)}
                </span>
              ) : (
                <span className="avatar avatar-xs" />
              )}
              <div className="dbh-main">
                <div className="dbh-text">
                  {who && <b>{who.name} </b>}
                  {describe(x)}
                </div>
                <div className="dbh-meta">
                  {!rowId && row !== null && row !== undefined && <span className="dbh-row">{row || t('untitled')}</span>}
                  <span title={fmtDate(x.at, true)}>{fmtAgo(x.at)}</span>
                  {x.undone && <span>· {t('historyUndone')}</span>}
                </div>
              </div>
              {canWrite && !x.undone && x.prop !== '@position' && (
                <button type="button" className="db-link-btn" disabled={busy === x.seq} onClick={() => undo(x)}>
                  {t('historyUndo')}
                </button>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}
