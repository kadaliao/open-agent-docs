// 评论面板：按段的顺序列出评论串，点一下跳到那一段；写评论时选通知哪个 Agent（默认由服务器选：@ 到的、这串里最后说话的、
// 写这一段的、建页面的），通知了的显示“等它回复”直到它回复。
// 评论存在服务器上（open_agent_docs/pagecomments.py），这里只是界面。
import { useEffect, useMemo, useRef, useState } from 'react'
import { api } from '../../store'
import { locale, t } from '../../i18n'
import { ask } from '../Dialog'
import Markdown from '../Markdown'
import { useBack } from '../../swipe'
import { useAuthors, useLiveAgents } from '../../authors'

export type PageComment = {
  id: number; page_id: number; block_id: string | null; parent_id: number | null; by: string; text: string; quote: string
  ts: number; resolved: boolean; edited: number | null; woke: number[]
}
export type Draft = { block: string | null; quote: string }

const when = (ts: number) => new Date(ts * 1000).toLocaleString(locale, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })

/** 每段有几串没解决的评论（作者条旁边显示个数）。 */
export function countOpen(list: PageComment[]): Map<string, number> {
  const out = new Map<string, number>()
  for (const c of list) if (!c.parent_id && !c.resolved && c.block_id) out.set(c.block_id, (out.get(c.block_id) || 0) + 1)
  return out
}

function Composer({ placeholder, agents, auto, onSend, onCancel, autoFocus }: {
  placeholder: string; agents?: { id: number; name: string }[]; auto?: string; autoFocus?: boolean
  onSend: (text: string, wake: number | false | undefined) => Promise<void>; onCancel?: () => void
}) {
  const [text, setText] = useState('')
  const [wake, setWake] = useState<string>('')
  const [busy, setBusy] = useState(false)
  // 打 @ 弹出 Agent 的名字（评论里 @ 谁就通知谁，见 open_agent_docs/pagecomments.targets），上下键选，回车或 Tab 填进去
  const everyone = useLiveAgents()
  const [pick, setPick] = useState(0)
  const [shut, setShut] = useState(false)
  const box = useRef<HTMLTextAreaElement>(null)
  const at = text.match(/(?:^|\s)@([^\s@]*)$/)
  const matches = at && !shut ? everyone.filter(b => b.name.toLowerCase().startsWith(at[1].toLowerCase())).slice(0, 8) : []
  const choose = (name: string) => { setText(text.replace(/@([^\s@]*)$/, `@${name} `)); setPick(0); box.current?.focus() }
  const send = async () => {
    if (!text.trim() || busy) return
    setBusy(true)
    try {
      await onSend(text, wake === '' ? undefined : wake === 'none' ? false : Number(wake))
      setText('')
    } finally { setBusy(false) }
  }
  return (
    <div className="dc-composer">
      {matches.length > 0 && <div className="mention-list dc-mentions">
        {matches.map((b, i) => <button key={b.id} className={i === pick % matches.length ? 'on' : ''} onMouseDown={e => { e.preventDefault(); choose(b.name) }}>
          <b>@{b.name}</b><span className="muted small">{t('通知它来处理')}</span></button>)}
      </div>}
      <textarea ref={box} value={text} autoFocus={autoFocus} rows={2} placeholder={placeholder} onChange={e => { setText(e.target.value); setShut(false) }}
        onKeyDown={e => {
          if (matches.length && !e.nativeEvent.isComposing) {
            if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); setPick((pick + (e.key === 'ArrowDown' ? 1 : matches.length - 1)) % matches.length); return }
            if ((e.key === 'Enter' && !e.metaKey && !e.ctrlKey) || e.key === 'Tab') { e.preventDefault(); choose(matches[pick % matches.length].name); return }
            if (e.key === 'Escape') { e.preventDefault(); setShut(true); return }
          }
          if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') send()
          if (e.key === 'Escape') onCancel?.()
        }} />
      <div className="dc-row">
        {agents && <select value={wake} onChange={e => setWake(e.target.value)} title={t('谁来处理这条评论')}>
          <option value="">{auto}</option>
          {agents.map(a => <option key={a.id} value={a.id}>{t('通知 {name}', { name: a.name })}</option>)}
          <option value="none">{t('不通知 Agent')}</option>
        </select>}
        <span className="grow" />
        {onCancel && <button className="link-btn" onClick={onCancel}>{t('取消')}</button>}
        <button className="btn primary small" disabled={!text.trim() || busy} onClick={send}>{t('发送')}</button>
      </div>
    </div>
  )
}

export function CommentsPanel({ pageId, comments, order, blockMd, authorOf, owner, draft, setDraft, focus, onJump, onClose, reload }: {
  pageId: number; comments: PageComment[]; order: string[]; blockMd: (id: string) => string | null
  authorOf: (id: string) => string; owner: number | null; draft: Draft | null; setDraft: (d: Draft | null) => void
  focus: string | null; onJump: (block: string) => void; onClose: () => void; reload: () => void
}) {
  const agents = useLiveAgents()
  const root = useRef<HTMLElement>(null)
  useBack(onClose, root)   // 手机上整屏盖着页面：左边缘右滑关掉
  const [tab, setTab] = useState<'open' | 'done'>('open')
  const list = useRef<HTMLDivElement>(null)
  const name = useAuthors()
  // 按段在页面里的顺序排；整页的在最前面，原段落已经删掉的在最后
  const threads = useMemo(() => {
    const pos = new Map(order.map((id, k) => [id, k]))
    // 刚解决的（10 分钟内有人说过话）还留在“未解决”里，标着已解决：Agent 处理完顺手解决的，你还没看到它的回复
    const latest = (r: PageComment) => Math.max(...comments.filter(c => c.id === r.id || c.parent_id === r.id).map(c => c.ts))
    const fresh = (r: PageComment) => r.resolved && Date.now() / 1000 - latest(r) < 600
    const roots = comments.filter(c => !c.parent_id && (tab === 'done' ? c.resolved : !c.resolved || fresh(c)))
    const rank = (c: PageComment) => (c.block_id ? pos.get(c.block_id) ?? 1e9 : -1)
    return roots.sort((a, b) => rank(a) - rank(b) || a.id - b.id).map(r => ({ root: r, items: [r, ...comments.filter(c => c.parent_id === r.id)] }))
  }, [comments, order, tab])
  const counts = { open: comments.filter(c => !c.parent_id && !c.resolved).length, done: comments.filter(c => !c.parent_id && c.resolved).length }
  useEffect(() => { list.current?.querySelector('.dc-thread.on')?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }) }, [focus])

  const post = async (body: any) => { await api(`/api/pages/${pageId}/comments`, { method: 'POST', body }); reload() }
  const patch = async (id: number, body: any) => { await api(`/api/pages/${pageId}/comments/${id}`, { method: 'PATCH', body }); reload() }
  const del = async (c: PageComment) => {
    if (!(await ask(c.parent_id ? t('删除这条评论？') : t('删除这一串评论？'), { ok: t('删除'), danger: true }))) return
    await api(`/api/pages/${pageId}/comments/${c.id}`, { method: 'DELETE' })
    reload()
  }
  // 不指定时服务器怎么选（pagecomments.targets）：评论里 @ 了谁就通知谁，否则是写这一段的 Agent、再否则是建页面的 Agent
  const autoName = (block: string | null) => {
    const by = block ? authorOf(block) : ''
    const m = by.match(/^agent:(\d+)$/)
    const a = (m && agents.find(x => x.id === Number(m[1]))) || agents.find(x => x.id === owner)
    return a ? t('自动：通知 {name}（或者 @ 到的）', { name: a.name }) : t('自动：通知 @ 到的 Agent')
  }
  const quoteOf = (c: PageComment) => {
    if (!c.block_id) return t('整页')
    const md = blockMd(c.block_id)
    return c.quote || (md === null ? '' : md.slice(0, 140))
  }
  return (
    <aside className="doc-comments" ref={root}>
      <header className="dc-head">
        <b>{t('评论')}</b>
        <div className="panel-tabs">
          <button className={tab === 'open' ? 'on' : ''} onClick={() => setTab('open')}>{t('未解决')} {counts.open || ''}</button>
          <button className={tab === 'done' ? 'on' : ''} onClick={() => setTab('done')}>{t('已解决')} {counts.done || ''}</button>
        </div>
        <span className="grow" />
        <button className="icon-btn" title={t('关闭')} onClick={onClose}>×</button>
      </header>
      {draft && (
        <div className="dc-thread on dc-new">
          <div className="dc-quote">{draft.quote || (draft.block ? (blockMd(draft.block) || '').slice(0, 140) : t('整页'))}</div>
          <Composer autoFocus placeholder={t('写评论；@名字 通知某个 Agent')} agents={agents} auto={autoName(draft.block)}
            onCancel={() => setDraft(null)}
            onSend={async (text, wake) => { await post({ text, block: draft.block, quote: draft.quote, ...(wake !== undefined ? { wake } : {}) }); setDraft(null) }} />
        </div>
      )}
      <div className="dc-list" ref={list}>
        {threads.map(({ root, items }) => {
          const gone = root.block_id !== null && blockMd(root.block_id) === null
          const lastUser = [...items].reverse().find(c => c.by === 'user' && c.woke.length)
          const answered = lastUser && items.some(c => c.ts > lastUser.ts && c.by.startsWith('agent:'))
          return (
            <div key={root.id} className={'dc-thread' + (focus && root.block_id === focus ? ' on' : '') + (root.resolved ? ' done' : '')}
              onClick={() => root.block_id && !gone && onJump(root.block_id)}>
              <div className={'dc-quote' + (gone ? ' gone' : '')}>{gone ? t('原段落已删除') : quoteOf(root)}</div>
              {root.resolved && tab === 'open' && <div className="dc-done small">✓ {t('已解决')}</div>}
              {items.map(c => (
                <div key={c.id} className="dc-item">
                  <div className="dc-meta"><b>{name(c.by)}</b> <span className="muted">{when(c.ts)}</span>
                    {c.by === 'user' && <button className="dc-del" title={t('删除')} onClick={e => { e.stopPropagation(); del(c) }}>×</button>}</div>
                  <div className="dc-text"><Markdown text={c.text} /></div>
                  {c.woke.length > 0 && <div className="dc-woke muted small">→ {t('通知了 {names}', { names: c.woke.map(id => name(`agent:${id}`)).join(t('、')) })}</div>}
                </div>
              ))}
              {lastUser && !answered && !root.resolved && <div className="dc-pending muted small">
                {t('等 {names} 回复…', { names: lastUser.woke.map(id => name(`agent:${id}`)).join(t('、')) })}</div>}
              <div className="dc-foot" onClick={e => e.stopPropagation()}>
                <Composer placeholder={t('回复…')} onSend={text => post({ text, reply_to: root.id })} />
                <button className="link-btn small" onClick={() => patch(root.id, { resolved: !root.resolved })}>
                  {root.resolved ? t('重新打开') : t('标成已解决')}</button>
              </div>
            </div>
          )
        })}
        {!threads.length && !draft && <p className="muted small dc-empty">
          {tab === 'open' ? t('没有没解决的评论。选中一段文字点“评论”，Agent 会收到通知来处理；也可以对整页提意见。') : t('还没有解决过的评论。')}</p>}
      </div>
      {!draft && tab === 'open' && <button className="btn dc-page" onClick={() => setDraft({ block: null, quote: '' })}>{t('评论整页')}</button>}
    </aside>
  )
}
