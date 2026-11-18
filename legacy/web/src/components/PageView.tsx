// 打开着的一页：顶上的标题（点了改名）、分享、⋯ 菜单（置顶、锁定、挪到、删除），下面是路径、作者、正文（文档编辑器或数据库），
// 数据库的行多一条属性栏，普通页面底下列出子页面。分享链接打开的只读页面（SharedPage）也在这里。
import { useEffect, useState, type RefObject } from 'react'
import { api, openPage, useStore } from '../store'
import type { Page } from '../types'
import Markdown from './Markdown'
import { locale, t } from '../i18n'
import { isEnter } from '../keys'
import { useBack } from '../swipe'
import { DocPage } from './doc/lazy'
import { MoveDialog, deletePage, newPage } from './PageTree'
import DbView from './db/DbView'
import PropsBar from './db/PropsBar'
import { newDb } from './db/db'

export const pageIcon = (p: { kind?: string; format?: string }) => (p.kind === 'db' ? '▦' : p.kind === 'row' ? '▭' : p.format === 'html' ? '◧' : '▤')
export const when = (ts: number) => new Date(ts * 1000).toLocaleString(locale, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
const shareUrl = (token: string) => `${location.origin}/s/${token}`

/** 谁建的：你、某个 Agent，或者从 OSA 导进来的页面原来的 Bot。 */
export function AuthorTag({ bot, agent }: { bot: number | null; agent?: number | null }) {
  const a = useStore(s => s.agents.find(x => x.id === agent))
  if (agent) return <span className="muted small">{a?.name || t('已删除的 Agent')}</span>
  if (bot) return <span className="muted small">{t('Bot #{id}', { id: bot })}</span>
  return <span className="muted small">{t('你建的')}</span>
}

function HtmlNote() {
  return <div className="page-md"><p className="muted">{t('这是一个 HTML 页面（从 Open Super Agent 导进来的），这里显示不了。')}</p></div>
}

export default function PageView({ id, frame }: { id: number; frame?: RefObject<HTMLElement | null> }) {
  const [p, setP] = useState<Page | null>(null)
  const [err, setErr] = useState('')
  const [rename, setRename] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [more, setMore] = useState(false)
  const [moving, setMoving] = useState<Page[] | null>(null)   // 挪到哪里：打开时取一次页面列表
  useEffect(() => {   // “⋯”菜单：点别处就收起
    if (!more) return
    const close = () => setMore(false)
    const timer = setTimeout(() => document.addEventListener('click', close), 0)
    return () => { clearTimeout(timer); document.removeEventListener('click', close) }
  }, [more])
  const load = () => api(`/api/pages/${id}`).then(x => { setP(x); setErr('') }).catch((e: any) => setErr(e.message))
  useEffect(() => {
    setErr('')
    load()
    window.addEventListener('oad-pages', load)
    window.addEventListener('oad-page-resync', load)
    return () => { window.removeEventListener('oad-pages', load); window.removeEventListener('oad-page-resync', load) }
  }, [id])
  useEffect(() => { if (p?.id === id) document.title = `${p.title || t('无标题')} · Open Agent Docs` }, [p?.title, p?.id, id])
  const patch = async (body: any) => { setP(await api(`/api/pages/${id}`, { method: 'PATCH', body }).then(x => ({ ...p!, ...x }))) }
  const share = async (on: boolean) => {
    const x = await api(`/api/pages/${id}/share`, { method: 'POST', body: { on } })
    setP({ ...p!, ...x })
    if (on) copy(x.share_token)
  }
  const copy = (token: string) => { navigator.clipboard?.writeText(shareUrl(token)).catch(() => {}); setCopied(true); setTimeout(() => setCopied(false), 1500) }
  const del = async () => {
    if (await deletePage(p!, p!.children?.length || 0)) openPage(p!.parent_id)
  }
  const parent = p?.ancestors?.at(-1)
  useBack(() => openPage(parent?.id ?? null), frame)   // 手机上左边缘右滑：回上一级（没有就回首页）
  return (
    <>
      <header className="chat-head page-head">
        <button className="ghost-btn back" onClick={() => openPage(parent?.id ?? null)}>← {parent ? parent.title || t('无标题') : t('首页')}</button>
        {p && (rename !== null
          ? <input className="page-title-input" autoFocus value={rename} onChange={e => setRename(e.target.value)}
              onBlur={() => { if (rename !== p.title) patch({ title: rename }); setRename(null) }}
              onKeyDown={e => { if (isEnter(e)) (e.target as HTMLInputElement).blur(); if (e.key === 'Escape') setRename(null) }} />
          : <span className="head-name page-title" title={t('点击改名')} onClick={() => setRename(p.title)}>{p.title || t('无标题')}</span>)}
        <span className="grow" />
        {p && <div className="page-actions">
          {p.share_token
            ? <><button className="btn" onClick={() => copy(p.share_token!)}>{copied ? t('已复制链接') : t('复制链接')}</button>
                <button className="btn" onClick={() => share(false)}>{t('取消分享')}</button></>
            : <button className="btn" onClick={() => share(true)}>{t('分享')}</button>}
          <div className="page-more">
            <button className="btn" title={t('更多')} onClick={() => setMore(!more)}>⋯</button>
            {more && <div className="menu">
              <button onClick={() => patch({ pinned: !p.pinned })}>{p.pinned ? t('取消置顶') : t('置顶')}</button>
              {p.format === 'markdown' && p.kind !== 'row' && <button title={t('锁定后 Agent 只能读，你照样能改')} onClick={() => patch({ locked: !p.locked })}>
                {p.locked ? t('解锁') : t('锁定（Agent 只能读）')}</button>}
              {p.kind !== 'row' && <button onClick={() => api('/api/pages').then(setMoving)}>{t('挪到…')}</button>}
              <button className="danger-text" onClick={del}>{t('删除')}</button>
            </div>}
          </div>
        </div>}
      </header>
      {moving && p && <MoveDialog page={p} list={moving} onClose={() => { setMoving(null); load() }} />}
      {err ? <div className="lib-body"><div className="error-line">{err}</div></div> : !p || p.id !== id ? <div className="lib-body muted">{t('加载中…')}</div> : (
        <div className={'scroller' + (p.kind === 'db' ? ' db-page' : '')}>
          {!!p.ancestors?.length && <div className="page-path small">
            <button className="link-btn" onClick={() => openPage()}>{t('首页')}</button>
            {p.ancestors.map(a => <span key={a.id}> / <button className="link-btn" onClick={() => openPage(a.id)}>{a.title || t('无标题')}</button></span>)}
          </div>}
          {p.format !== 'html' && <div className="page-info muted small"><AuthorTag bot={p.bot_id} agent={p.agent_id} /> · {t('更新于 {time}', { time: when(p.updated) })}
            {p.share_token && <> · {t('任何拿到链接的人都能看')}</>}{p.locked && <> · {t('已锁定：Agent 只能读')}</>}</div>}
          {p.kind === 'db' ? <DbView id={id} />
            : p.format === 'markdown' ? <>
              {p.kind === 'row' && p.parent_id != null && <PropsBar dbId={p.parent_id} rowId={id} />}
              <DocPage id={id} content={p.content || ''} />
            </>
            : <HtmlNote />}
          {p.format === 'markdown' && p.kind === 'page' && <div className="page-kids">
            {!!p.children?.length && <div className="muted small">{t('子页面')}</div>}
            {p.children?.map(c => <button key={c.id} className="page-kid" onClick={() => openPage(c.id)}>{pageIcon(c)} {c.title || t('无标题')}</button>)}
            <span><button className="link-btn small" onClick={() => newPage(id)}>＋ {t('子页面')}</button>{/* i18n-ignore */}
              <button className="link-btn small" onClick={() => newDb(id)}>＋ {t('数据库')}</button></span>{/* i18n-ignore */}
          </div>}
        </div>
      )}
    </>
  )
}

// 分享链接打开的页面：只读，不用登录，不加载应用的其他部分
export function SharedPage({ token }: { token: string }) {
  const [p, setP] = useState<{ title: string; format: string; content: string; updated: number } | null>(null)
  const [err, setErr] = useState('')
  useEffect(() => { api(`/api/shared/${token}`).then(x => { setP(x); document.title = x.title || 'Open Agent Docs' }).catch((e: any) => setErr(e.message)) }, [token])
  if (err) return <div className="shared"><div className="shared-empty">{err}</div></div>
  if (!p) return <div className="loading">◆</div>
  return (
    <div className="shared">
      <header className="shared-head">
        <b>{p.title || t('无标题')}</b>
        <span className="muted small">{t('更新于 {time}', { time: when(p.updated) })}</span>
      </header>
      {/* 页面里的图片（/api/media/编号）要登录才能看：分享页从这个分享链接下面取 */}
      {p.format === 'markdown'
        ? <div className="page-md"><Markdown text={p.content.replaceAll('/api/media/', `/api/shared/${token}/media/`)} /></div>
        : <HtmlNote />}
      <footer className="shared-foot muted small">Open Agent Docs</footer>
    </div>
  )
}
