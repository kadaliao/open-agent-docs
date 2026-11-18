// 版本历史：右边列出每一版（同一个人 10 分钟内接着改的服务器已经并成一版，见 open_agent_docs/pagedoc.py），
// 点开一版在正文的位置显示和现在 / 和上一版的差别，或者只看这一版；能回到这一版，也能只恢复其中一段。
// 手机上列表是整屏的：点开一版时列表收起来（不然对比被它盖在底下），对比上面有“← 历史”回到列表；左边缘右滑也是返回。
import { useEffect, useMemo, useRef, useState } from 'react'
import { api } from '../../store'
import { notice } from '../Dialog'
import { locale, t } from '../../i18n'
import Markdown from '../Markdown'
import { diffBlocks, type Row } from './diff'
import { useBack } from '../../swipe'
import { useAuthors } from '../../authors'
import { dbApi, type DbProp } from '../db/db'
import { PropsDiff, PropsSnapshot, propChanges, type PropChange, type PropsSnap } from '../db/PropsDiff'
import { DbCtx } from '../db/cells'

export type Version = { id: number; version: number; title: string; by: string; ts_start: number; ts: number; label: string; size: number }
// 数据库的行多带着那一版的属性、所在的库和现在认不出的键（open_agent_docs/pagedoc.version）
type Snap = Version & { content: string; blocks?: { id: string; md: string }[]; props?: Record<string, any>; db_id?: number; stale?: string[] }
type Live = { schema: DbProp[]; now: PropsSnap; refs: Record<number, string> }

const when = (ts: number) => new Date(ts * 1000).toLocaleString(locale, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
const clock = (ts: number) => new Date(ts * 1000).toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })

function useName() {
  const nameOf = useAuthors()
  return (by: string) => nameOf(by, t('原来的样子'))
}

const labelOf = (v: Version) => v.label === 'base' ? t('改之前的样子') : v.label.startsWith('restore:') ? t('回到了版本 {n}', { n: v.label.slice(8) }) : ''

export function HistoryPanel({ pageId, picked, onPick, onClose, stamp }: {
  pageId: number; picked: Version | null; onPick: (v: Version, list: Version[]) => void; onClose: () => void; stamp: number
}) {
  const name = useName()
  const root = useRef<HTMLElement>(null)
  useBack(onClose, root)
  const [list, setList] = useState<Version[] | null>(null)
  const [more, setMore] = useState(true)
  useEffect(() => { api(`/api/pages/${pageId}/versions?limit=50`).then(v => { setList(v); setMore(v.length === 50) }) }, [pageId, stamp])
  const older = async () => {
    const v: Version[] = await api(`/api/pages/${pageId}/versions?limit=50&before=${list![list!.length - 1].id}`)
    setList([...list!, ...v])
    setMore(v.length === 50)
  }
  return (
    <aside className={'doc-comments doc-history' + (picked ? ' picked' : '')} ref={root}>
      <header className="dc-head"><b>{t('历史')}</b><span className="grow" /><button className="icon-btn" title={t('关闭')} onClick={onClose}>×</button></header>
      <div className="dc-list">
        {!list ? <p className="muted small">{t('加载中…')}</p> : list.map((v, k) => (
          <button key={v.id} className={'dh-row' + (picked?.id === v.id ? ' on' : '')} onClick={() => onPick(v, list)}>
            <b>{name(v.by)}</b>
            <span className="muted small">{v.ts - v.ts_start > 60 ? `${when(v.ts_start)}–${clock(v.ts)}` : when(v.ts)}</span>
            <span className="muted small">{t('版本 {n}', { n: v.version })}{k === 0 ? ' · ' + t('现在') : ''}{labelOf(v) ? ' · ' + labelOf(v) : ''}</span>
          </button>
        ))}
        {list && more && <button className="link-btn small" onClick={older}>{t('更早的')}</button>}
      </div>
    </aside>
  )
}

type Mode = 'now' | 'prev' | 'only'

export function HistoryView({ pageId, version, list, current, onRestorePage, onRestoreBlock, onRestored, onClose, onBack }: {
  pageId: number; version: Version; list: Version[]; current: { id: string; md: string }[]
  onRestorePage: (v: Version) => Promise<void>; onRestoreBlock: (row: Row) => Promise<void>; onClose: () => void
  onRestored: () => void   // 恢复了一个属性（数据库的行）：历史列表要重新取
  onBack: () => void   // 回到版本列表（手机上）
}) {
  const name = useName()
  const root = useRef<HTMLDivElement>(null)
  useBack(onBack, root)
  const [mode, setMode] = useState<Mode>('now')
  const [snap, setSnap] = useState<Snap | null>(null)
  const [prev, setPrev] = useState<Snap | null>(null)
  const [busy, setBusy] = useState(false)
  const [open, setOpen] = useState<Set<number>>(new Set())   // 展开了的“没变的段”
  const before = list[list.findIndex(v => v.id === version.id) + 1]
  const [tick, setTick] = useState(0)   // 数据库的表结构变了：重新取这两版（哪些旧值认不出由服务器按新的表结构算）
  useEffect(() => { setSnap(null); setPrev(null) }, [pageId, version.id])
  useEffect(() => {
    api(`/api/pages/${pageId}/versions/${version.id}`).then(setSnap)
    if (before) api(`/api/pages/${pageId}/versions/${before.id}`).then(setPrev)
  }, [pageId, version.id, tick])
  // 数据库的行：库现在的表结构和这一行现在的属性（“和现在比”用），别人改了这一行就重新取
  const [live, setLive] = useState<Live | null>(null)
  const dbId = snap?.db_id
  useEffect(() => {
    if (!dbId) return
    const load = () => api(`/api/dbs/${dbId}/rows/${pageId}`).then(d => setLive({ schema: d.db.schema, now: { title: d.row.title, props: d.row.props }, refs: d.refs || {} })).catch(() => {})
    load()
    const h = (e: Event) => {
      const x = (e as CustomEvent).detail
      if (x?.db_id !== dbId) return
      if (x.schema) setTick(n => n + 1)
      if (x.schema || !x.rows?.length || x.rows.includes(pageId)) load()
    }
    window.addEventListener('oad-db', h)
    return () => window.removeEventListener('oad-db', h)
  }, [dbId, pageId])
  const asProps = (s: Snap): PropsSnap => ({ title: s.title, props: s.props || {}, stale: s.stale })
  const props = useMemo(() => {
    if (!live || !snap?.props || mode === 'only') return null
    const [from, to] = mode === 'now' ? [asProps(snap), live.now] : prev?.props ? [asProps(prev), asProps(snap)] : [null, null]
    if (!from || !to) return null
    // 认不出的旧值不比；只提那些这次真有不同的（不然每一版都提一遍）
    const stale = new Set([...from.stale || [], ...to.stale || []])
    const names = live.schema.filter(p => stale.has(p.key) && JSON.stringify(from.props[p.key]) !== JSON.stringify(to.props[p.key])).map(p => p.name)
    return { changes: propChanges(live.schema, from, to), stale: names }
  }, [live, snap, prev, mode])
  const restoreProp = (c: PropChange) => run(async () => {
    try {
      await dbApi.update(dbId!, pageId, { [c.prop.key]: c.old })
      onRestored()
    } catch (e: any) { notice(e.message) }
  })
  const rows = useMemo(() => {
    if (!snap?.blocks) return null
    if (mode === 'now') return diffBlocks(snap.blocks, current)   // 从这一版到现在改了什么
    if (mode === 'prev') return diffBlocks(prev?.blocks || [], snap.blocks)   // 这一版改了什么
    return null
  }, [snap, prev, mode, current])
  const run = async (fn: () => Promise<void>) => { setBusy(true); try { await fn() } finally { setBusy(false) } }
  // 连续没变的段收起来，只露一行“几段没变”
  const groups: (Row | { kind: 'fold'; rows: Row[]; at: number })[] = []
  for (const [k, r] of (rows || []).entries()) {
    const last = groups[groups.length - 1]
    if (r.kind === 'same' && last && 'rows' in last) last.rows.push(r)
    else groups.push(r.kind === 'same' ? { kind: 'fold', rows: [r], at: k } : r)
  }
  // 两种对比的恢复都一样：改过的段换回旧的内容（r.old），删掉的段用原来的编号插回去；只是按钮上的话不同
  const backLabel = mode === 'now' ? t('恢复成这一版的样子') : t('改回改之前')
  return (
    <div className="doc-hview" ref={root}>
      <div className="dhv-head">
        <button className="btn dhv-back" onClick={onBack}>← {t('历史')}</button>
        <div>
          <b>{t('版本 {n}', { n: version.version })}</b> <span className="muted small">{name(version.by)} · {when(version.ts)}{labelOf(version) ? ' · ' + labelOf(version) : ''}</span>
        </div>
        <div className="panel-tabs">
          <button className={mode === 'now' ? 'on' : ''} onClick={() => setMode('now')}>{t('和现在比')}</button>
          <button className={mode === 'prev' ? 'on' : ''} disabled={!before} onClick={() => setMode('prev')}>{t('这一版改了什么')}</button>
          <button className={mode === 'only' ? 'on' : ''} onClick={() => setMode('only')}>{t('只看这一版')}</button>
        </div>
        <span className="grow" />
        {list[0]?.id !== version.id && <button className="btn" disabled={busy} onClick={() => run(() => onRestorePage(version))}>{t('回到这一版')}</button>}
        <button className="btn" onClick={onClose}>{t('回到编辑')}</button>
      </div>
      <DbCtx.Provider value={{ dbId: dbId || 0, refs: live?.refs || {}, addRefs: () => {}, onPressed: () => {} }}>
        {snap && mode === 'only' && live && snap.props && <PropsSnapshot schema={live.schema} snap={asProps(snap)} />}
        {props && mode !== 'only' && <PropsDiff changes={props.changes} mode={mode} staleNames={props.stale} busy={busy} restoreLabel={backLabel} onRestore={restoreProp} />}
      </DbCtx.Provider>
      {!snap ? <p className="muted">{t('加载中…')}</p> : mode === 'only' || !snap.blocks ? (
        <div className="md"><Markdown text={snap.content} /></div>
      ) : !rows!.some(r => r.kind !== 'same') ? (
        snap.props ? (props?.changes.length || props?.stale.length ? <p className="muted small">{t('正文没变。')}</p>
          : <p className="muted">{mode === 'now' ? t('这一版和现在一样。') : t('这一版和上一版看不出差别。')}</p>)
          : <p className="muted">{mode === 'now' ? t('这一版和现在一样。') : t('这一版只改了标题。')}</p>
      ) : (
        <div className="dhv-rows">
          {groups.map(g => {
            if ('rows' in g) return open.has(g.at) ? g.rows.map(r => <div key={r.id} className="dhv-row same"><Markdown text={r.kind === 'same' ? r.md : ''} /></div>)
              : <button key={'f' + g.at} className="dhv-fold muted small" onClick={() => setOpen(new Set([...open, g.at]))}>{t('… {n} 段没变 …', { n: g.rows.length })}</button>
            const r = g
            return (
              <div key={r.kind + r.id} className={'dhv-row ' + r.kind}>
                <div className="dhv-tag small">{{ add: mode === 'now' ? t('后来加的') : t('加了'), del: mode === 'now' ? t('后来删了') : t('删了'),
                  change: mode === 'now' ? t('后来改了') : t('改了'), move: t('挪了位置'), same: '' }[r.kind]}{r.kind === 'change' && r.moved ? ' · ' + t('挪了位置') : ''}</div>
                {r.kind === 'change' ? <pre className="dhv-inline">{r.segs.map((s, k) => s.op === '=' ? <span key={k}>{s.text}</span>
                  : s.op === '-' ? <del key={k}>{s.text}</del> : <ins key={k}>{s.text}</ins>)}</pre>
                  : <div className="md"><Markdown text={r.md} /></div>}
                {(r.kind === 'change' || r.kind === 'del') &&
                  <button className="link-btn small" disabled={busy} onClick={() => run(() => onRestoreBlock(r))}>{backLabel}</button>}
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
