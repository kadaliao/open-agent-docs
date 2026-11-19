// 页面历史：右侧的版本列表（谁、什么时候、自动 / Agent / 恢复 / 手动存的、名字），点一版在正文位置看它和现在的按块对比，能恢复、能给现在存一版。
// 还带分享弹窗（同一个按需加载的包）。
import { useEffect, useMemo, useState } from 'react'
import { api, ApiError, type DiffBlock, type Person, type Version } from '../api'
import { fmtAgo, fmtDate, useLocale } from '../i18n'
import { colorFor, initials } from '../lib'
import { t } from '../strings'
import { inlineMd } from './md'
import './history.css'

export { ShareDialog } from './Share'

const KIND: Record<string, Parameters<typeof t>[0]> = { auto: 'versionAuto', agent: 'versionAgent', restore: 'versionRestore', manual: 'versionManual' }

function Avatars({ people }: { people?: Person[] }) {
  if (!people?.length) return null
  return (
    <span className="hist-people">
      {people.slice(0, 4).map((p) => (
        <span key={p.kind + p.id} className={'avatar avatar-xs' + (p.kind === 'agent' ? ' is-agent' : '')} style={{ background: colorFor(p.id) }} title={p.name}>
          {p.kind === 'agent' ? '✦' : initials(p.name)}
        </span>
      ))}
      <span className="hist-names">{people.map((p) => p.name).join('、')}</span>
    </span>
  )
}

export function HistoryPanel({
  pageId,
  canWrite,
  selected,
  onSelect,
  onClose,
}: {
  pageId: string
  canWrite: boolean
  selected: number | null
  onSelect: (v: Version | null) => void
  onClose: () => void
}) {
  useLocale()
  const [versions, setVersions] = useState<Version[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [naming, setNaming] = useState(false)
  const [label, setLabel] = useState('')
  const load = () =>
    api.versions(pageId).then(
      (v) => {
        setVersions([...v.versions].sort((a, b) => b.created_at - a.created_at))
        setError(null)
      },
      (e) => {
        setVersions([])
        setError(e instanceof ApiError && e.status === 404 ? t('historyUnavailable') : e instanceof Error ? e.message : String(e))
      },
    )
  useEffect(() => {
    load()
  }, [pageId]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || (e.target as HTMLElement).closest?.('input')) return
      if (selected) onSelect(null)
      else onClose()
    }
    window.addEventListener('keydown', k)
    return () => window.removeEventListener('keydown', k)
  }, [selected, onSelect, onClose])

  const save = async () => {
    const l = label.trim()
    if (!l) return
    try {
      await api.saveVersion(pageId, l)
      setNaming(false)
      setLabel('')
      load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  return (
    <aside className="hist" aria-label={t('historyTitle')}>
      <div className="hist-head">
        <h2>{t('historyTitle')}</h2>
        <button className="icon-btn" onClick={onClose} aria-label={t('cancel')}>
          ×
        </button>
      </div>
      {canWrite && (
        <div className="hist-name">
          {naming ? (
            <form
              className="hist-name-form"
              onSubmit={(e) => {
                e.preventDefault()
                save()
              }}
            >
              <input autoFocus value={label} placeholder={t('versionNamePlaceholder')} onChange={(e) => setLabel(e.target.value)} maxLength={80} onKeyDown={(e) => e.key === 'Escape' && setNaming(false)} />
              <button className="btn btn-sm btn-primary" disabled={!label.trim()}>
                {t('saveVersion')}
              </button>
            </form>
          ) : (
            <button className="btn btn-sm hist-name-btn" onClick={() => setNaming(true)}>
              ＋ {t('nameVersion')}
            </button>
          )}
        </div>
      )}
      <div className="hist-list" role="listbox" aria-label={t('historyTitle')}>
        <button className={'hist-item is-current' + (!selected ? ' is-on' : '')} role="option" aria-selected={!selected} onClick={() => onSelect(null)}>
          <span className="hist-dot" />
          <span className="hist-main">
            <span className="hist-when">{t('versionCurrent')}</span>
          </span>
        </button>
        {error && <div className="hist-note">{error}</div>}
        {versions && !versions.length && !error && <div className="hist-note">{t('versionsEmpty')}</div>}
        {(versions ?? []).map((v) => (
          <button key={v.id} className={'hist-item' + (selected === v.id ? ' is-on' : '') + ' is-' + v.kind} role="option" aria-selected={selected === v.id} data-version={v.id} onClick={() => onSelect(v)}>
            <span className="hist-dot" />
            <span className="hist-main">
              <span className="hist-when" title={fmtDate(v.created_at, true)}>
                {v.label ? <b>{v.label}</b> : fmtDate(v.created_at, true)}
              </span>
              <span className="hist-meta">
                <span className={'hist-kind is-' + v.kind}>{t(KIND[v.kind] ?? 'versionAuto')}</span>
                {v.label && <span>{fmtAgo(v.created_at)}</span>}
              </span>
              <Avatars people={v.authors} />
            </span>
          </button>
        ))}
      </div>
    </aside>
  )
}

// ---------- 对比 ----------

/** 按词（英文单词、中文单字）比两段文字，标出删掉的和加上的 */
export function wordDiff(a: string, b: string): { op: 'same' | 'del' | 'ins'; s: string }[] {
  const tok = (s: string) => s.match(/[A-Za-z0-9_]+|\s+|[^\sA-Za-z0-9_]/g) ?? []
  const x = tok(a)
  const y = tok(b)
  // 太长就整段换（LCS 是平方的）
  if (x.length * y.length > 400_000) return [...(a ? [{ op: 'del' as const, s: a }] : []), ...(b ? [{ op: 'ins' as const, s: b }] : [])]
  const n = x.length
  const m = y.length
  const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1))
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) dp[i][j] = x[i] === y[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1])
  const out: { op: 'same' | 'del' | 'ins'; s: string }[] = []
  const push = (op: 'same' | 'del' | 'ins', s: string) => {
    const last = out[out.length - 1]
    if (last && last.op === op) last.s += s
    else out.push({ op, s })
  }
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (x[i] === y[j]) {
      push('same', x[i])
      i++
      j++
    } else if (dp[i + 1][j] >= dp[i][j + 1]) push('del', x[i++])
    else push('ins', y[j++])
  }
  while (i < n) push('del', x[i++])
  while (j < m) push('ins', y[j++])
  return out
}

/** 一块 Markdown 画成大致的样子（标题、列表、待办、引用、代码），不进编辑器 */
function MdBlock({ md, parts }: { md: string; parts?: { op: 'same' | 'del' | 'ins'; s: string }[] }) {
  // 多行的列表、待办：一行一行画
  if (!parts && md.includes('\n') && /^\s*([-*+]|\d+\.)\s/.test(md))
    return (
      <>
        {md.split('\n').map((l, i) => (
          <MdBlock key={i} md={l} />
        ))}
      </>
    )
  const m = /^(#{1,6})\s|^(\s*)([-*+]|\d+\.)\s(\[[ xX]\]\s)?|^>\s?|^```/.exec(md)
  let cls = 'hb-p'
  let body = md
  if (m?.[1]) {
    cls = 'hb-h' + m[1].length
    body = md.slice(m[0].length)
  } else if (m?.[3]) {
    cls = m[4] ? 'hb-todo' + (/x/i.test(m[4]) ? ' is-done' : '') : /\d/.test(m[3]) ? 'hb-ol' : 'hb-ul'
    body = md.slice(m[0].length)
  } else if (md.startsWith('>')) {
    cls = 'hb-quote'
    body = md.replace(/^>\s?/gm, '')
  } else if (md.startsWith('```')) {
    cls = 'hb-code'
    body = md.replace(/^```\w*\n?/, '').replace(/\n?```$/, '')
  }
  if (parts) {
    // 对比：前缀（# 、- ）不进对比
    const skip = md.length - body.length
    let used = 0
    const shown = parts
      .map((p) => {
        if (p.op === 'ins') return p
        const start = used
        used += p.s.length
        if (used <= skip) return null
        return start < skip ? { ...p, s: p.s.slice(skip - start) } : p
      })
      .filter(Boolean) as typeof parts
    return (
      <div className={'hb ' + cls}>
        {shown.map((p, i) => (p.op === 'same' ? <span key={i}>{p.s}</span> : p.op === 'del' ? <del key={i}>{p.s}</del> : <ins key={i}>{p.s}</ins>))}
      </div>
    )
  }
  return <div className={'hb ' + cls} dangerouslySetInnerHTML={{ __html: cls === 'hb-code' ? body.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c]!) : inlineMd(body) }} />
}

type Part = { op: 'same' | 'del' | 'ins'; s: string }
type Line = { op: 'same' | 'del' | 'ins' | 'mod'; md: string; parts?: Part[] }

/** 多行的块（列表、引用）：先按行对，成对的增删再按词对 */
function lineDiff(a: string, b: string): Line[] {
  const x = a.split('\n')
  const y = b.split('\n')
  const ops = wordDiffLines(x, y)
  const out: Line[] = []
  for (let k = 0; k < ops.length; k++) {
    const o = ops[k]
    const n = ops[k + 1]
    if (o.op === 'del' && n?.op === 'ins') {
      out.push({ op: 'mod', md: n.s, parts: wordDiff(o.s, n.s) })
      k++
    } else out.push({ op: o.op, md: o.s })
  }
  return out
}
function wordDiffLines(x: string[], y: string[]): { op: 'same' | 'del' | 'ins'; s: string }[] {
  const n = x.length
  const m = y.length
  const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1))
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) dp[i][j] = x[i] === y[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1])
  const out: { op: 'same' | 'del' | 'ins'; s: string }[] = []
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (x[i] === y[j]) out.push({ op: 'same', s: x[i++] }), j++
    else if (dp[i + 1][j] >= dp[i][j + 1]) out.push({ op: 'del', s: x[i++] })
    else out.push({ op: 'ins', s: y[j++] })
  }
  while (i < n) out.push({ op: 'del', s: x[i++] })
  while (j < m) out.push({ op: 'ins', s: y[j++] })
  return out
}
// 对比里显示的文字：去掉行内格式的记号
const plain = (p: Part[]) => p.map((x) => ({ ...x, s: x.s.replace(/\*\*|~~|`/g, '') }))

function DiffRow({ b }: { b: DiffBlock }) {
  const lines = useMemo(() => (b.op === 'changed' && !(b.type ?? '').startsWith('code') ? lineDiff(b.before ?? '', b.after ?? '') : null), [b])
  const md = (b.op === 'removed' ? b.before : b.after ?? b.before) ?? ''
  return (
    <div className={'hd is-' + b.op} data-op={b.op}>
      {b.op !== 'same' && (
        <span className="hd-tag">
          {b.op === 'added' ? t('diffAdded') : b.op === 'removed' ? t('diffRemoved') : t('diffChanged')}
          {b.by?.length ? ' · ' + t('diffBy', { name: b.by.map((p) => p.name).join('、') }) : ''}
        </span>
      )}
      {lines ? (
        lines.map((l, i) => (
          <div key={i} className={'hd-line is-' + l.op}>
            <MdBlock md={l.md} parts={l.parts ? plain(l.parts) : undefined} />
          </div>
        ))
      ) : b.op === 'changed' ? (
        <MdBlock md={md} parts={plain(wordDiff(b.before ?? '', b.after ?? ''))} />
      ) : (
        <MdBlock md={md} />
      )}
    </div>
  )
}

export function VersionView({ pageId, version, canWrite, onClose, onRestored }: { pageId: string; version: Version; canWrite: boolean; onClose: () => void; onRestored: () => void }) {
  useLocale()
  const [diff, setDiff] = useState<DiffBlock[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [only, setOnly] = useState(false)
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    setDiff(null)
    setError(null)
    api.versionDiff(pageId, version.id).then(
      (d) => setDiff(d.blocks ?? []),
      (e) => setError(e instanceof Error ? e.message : String(e)),
    )
  }, [pageId, version.id])
  const changed = diff?.filter((b) => b.op !== 'same').length ?? 0
  const shown = only ? diff?.filter((b) => b.op !== 'same') : diff
  const restore = async () => {
    if (!window.confirm(t('restoreConfirm', { when: version.label || fmtDate(version.created_at, true) }))) return
    setBusy(true)
    try {
      await api.restoreVersion(pageId, version.id)
      onRestored()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
      setBusy(false)
    }
  }
  return (
    <div className="hv">
      <div className="hv-bar">
        <div className="hv-title">
          <b>{version.label || fmtDate(version.created_at, true)}</b>
          <span className="muted">
            {t('diffTitle')}
            {diff ? ` · ${changed ? `${changed} ${t('diffChanged')}` : t('noChanges')}` : ''}
          </span>
        </div>
        <label className="hv-only">
          <input type="checkbox" checked={only} onChange={(e) => setOnly(e.target.checked)} />
          {t('showChangesOnly')}
        </label>
        {canWrite && (
          <button className="btn btn-sm btn-primary" disabled={busy} onClick={restore}>
            {t('restoreVersion')}
          </button>
        )}
        <button className="icon-btn" onClick={onClose} aria-label={t('cancel')}>
          ×
        </button>
      </div>
      <article className="doc hv-doc">
        {error && <div className="form-error">{error}</div>}
        {!diff && !error && (
          <div className="doc-skeleton" aria-busy="true">
            <div className="sk sk-title" />
            <div className="sk" style={{ width: '90%' }} />
            <div className="sk" style={{ width: '70%' }} />
          </div>
        )}
        {shown?.map((b, i) => <DiffRow key={(b.id ?? '') + i} b={b} />)}
        {diff && !changed && <div className="hist-note">{t('noChanges')}</div>}
      </article>
    </div>
  )
}
