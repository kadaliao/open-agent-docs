// 回收站：删掉的页面、数据库、行留 30 天（open_agent_docs/library.py）
import { useEffect, useState } from 'react'
import { api, openPage } from '../store'
import { t } from '../i18n'
import { ask, notice } from './Dialog'
import { pageIcon, when } from './PageView'

type TrashItem = { id: number; page_id: number; title: string; kind: string; format: string; deleted: number; count: number; expires: number }

export default function Trash() {
  const [list, setList] = useState<TrashItem[] | null>(null)
  const load = () => api<TrashItem[]>('/api/trash').then(setList).catch(() => setList([]))
  useEffect(() => {
    load()
    window.addEventListener('oad-pages', load)
    return () => window.removeEventListener('oad-pages', load)
  }, [])
  useEffect(() => { document.title = `${t('回收站')} · Open Agent Docs` }, [])
  const restore = async (x: TrashItem) => {
    try {
      const p = await api(`/api/trash/${x.id}/restore`, { method: 'POST' })
      load()
      if (await ask(t('「{title}」放回去了。', { title: p.title || t('无标题') }), { ok: t('打开'), cancel: t('好的') })) openPage(p.id)
    } catch (e: any) { notice(e.message) }
  }
  const drop = async (x?: TrashItem) => {
    const msg = x ? t('彻底删除「{title}」？删了就找不回来了。', { title: x.title || t('无标题') }) : t('清空回收站？里面的都会彻底删掉，找不回来。')
    if (!(await ask(msg, { ok: t('彻底删除'), danger: true }))) return
    await api(x ? `/api/trash/${x.id}` : '/api/trash', { method: 'DELETE' })
    load()
  }
  const days = (ts: number) => Math.max(0, Math.ceil((ts - Date.now() / 1000) / 86400))
  return (
    <>
      <header className="chat-head page-head">
        <span className="head-name">{t('回收站')}</span>
        <span className="grow" />
        {!!list?.length && <button className="btn danger-text" onClick={() => drop()}>{t('清空回收站')}</button>}
      </header>
      <div className="scroller"><div className="lib-body">
        <p className="muted small">{t('删掉的页面、数据库和数据库的行在这里放 30 天，可以放回原来的地方。')}</p>
        {!list ? <div className="muted">{t('加载中…')}</div> : !list.length ? <div className="muted small">{t('回收站是空的。')}</div> : list.map(x => (
          <div key={x.id} className="lib-row trash-row">
            <span className="lib-icon">{pageIcon(x)}</span>
            <span className="lib-main"><b>{x.title || t('无标题')}</b>
              <span className="lib-meta">{t('{time} 删的', { time: when(x.deleted) })}{x.count > 1 ? ' · ' + t('连同 {n} 个子项', { n: x.count - 1 }) : ''} · {t('{n} 天后清掉', { n: days(x.expires) })}</span></span>
            <button className="btn" onClick={() => restore(x)}>{t('放回去')}</button>
            <button className="ghost-btn danger-text" onClick={() => drop(x)}>{t('彻底删除')}</button>
          </div>))}
      </div></div>
    </>
  )
}
