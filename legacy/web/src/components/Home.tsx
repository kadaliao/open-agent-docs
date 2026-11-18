// 首页：最近改过的页面，和怎么接上一个 Agent。
import { useEffect } from 'react'
import { navigate, openPage, useStore } from '../store'
import { t, tn } from '../i18n'
import { newPage } from './PageTree'
import { newDb } from './db/db'
import { AuthorTag, pageIcon, when } from './PageView'

export default function Home() {
  const list = useStore(s => s.pages)
  const agents = useStore(s => s.agents.filter(a => !a.revoked).length)
  useEffect(() => { document.title = 'Open Agent Docs' }, [])
  const recent = [...(list || [])].sort((a, b) => b.updated - a.updated).slice(0, 12)
  return (
    <>
      <header className="chat-head page-head"><span className="head-name">{t('首页')}</span></header>
      <div className="scroller"><div className="lib-body home">
        <h1 className="home-title">Open Agent Docs</h1>
        <p className="muted">{t('你和 AI Agent 一起写的文档：一段一段地改，每段记着是谁写的；在段落上写评论，Agent 收到通知来处理。')}</p>
        <div className="home-new">
          <button className="btn primary" onClick={() => newPage()}>＋ {t('新建页面')}</button>{/* i18n-ignore */}
          <button className="btn" onClick={() => newDb()}>＋ {t('新建数据库')}</button>{/* i18n-ignore */}
        </div>
        {!agents && <div className="home-hint">
          <b>{t('接上一个 Agent')}</b>
          <p className="muted small">{tn('在 {settings} 里给 Claude Code、Codex 这些 Agent 建一个 token，它们就能经 MCP（{url}）读写这里的页面，回复你的评论。', {
            settings: <button className="link-btn" onClick={() => navigate({ name: 'settings' })}>{t('设置 → Agent')}</button>,
            url: <code>{location.origin}/mcp</code> })}</p>
        </div>}
        <h3 className="home-h">{t('最近的页面')}</h3>
        {!list ? <div className="muted">{t('加载中…')}</div> : !recent.length ? <div className="muted small">{t('还没有页面。')}</div> : (
          <div className="lib-list">
            {recent.map(p => (
              <button key={p.id} className="lib-row" onClick={() => openPage(p.id)}>
                <span className="lib-icon">{pageIcon(p)}</span>
                <span className="lib-main">
                  <b>{p.pinned && <span className="pin-mark">📌 </span>}{p.title || t('无标题')}</b>
                  <span className="lib-meta"><AuthorTag bot={p.bot_id} agent={p.agent_id} /> · {when(p.updated)}{p.share_token && <span className="tag">{t('已分享')}</span>}</span>
                </span>
              </button>
            ))}
          </div>
        )}
        {!!agents && <p className="muted small home-foot">{tn('Agent 的 MCP 地址：{url}。在 {settings} 里管理。', {
          url: <code>{location.origin}/mcp</code>,
          settings: <button className="link-btn" onClick={() => navigate({ name: 'settings' })}>{t('设置 → Agent')}</button> })}</p>}
      </div></div>
    </>
  )
}
