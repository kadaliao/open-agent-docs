// 设置：语言、外观、Agent、改密码、退出登录。
import { useEffect, useState } from 'react'
import { api, logout, useStore } from '../store'
import { lang, t } from '../i18n'
import { themePref, setThemePref, type ThemePref } from '../theme'
import Agents from './Agents'

function Language() {
  const [busy, setBusy] = useState(false)
  const pick = async (l: 'zh' | 'en') => {
    if (l === lang || busy) return
    setBusy(true)
    try {
      await api('/api/settings', { method: 'PUT', body: { language: l } })
      try { localStorage.setItem('oad.lang', l) } catch { /* 记不住：下次打开按服务器的 */ }
      location.reload()
    } finally { setBusy(false) }
  }
  return <div className="seg">
    <button className={lang === 'zh' ? 'on' : ''} onClick={() => pick('zh')}>中文</button>{/* i18n-ignore */}
    <button className={lang === 'en' ? 'on' : ''} onClick={() => pick('en')}>English</button>
  </div>
}

function Theme() {
  const [pref, setPref] = useState<ThemePref>(themePref)
  const pick = (p: ThemePref) => { setThemePref(p); setPref(p) }
  const items: [ThemePref, string][] = [['system', t('跟随系统')], ['light', t('明亮')], ['dark', t('夜间')]]
  return <div className="seg">{items.map(([k, label]) => <button key={k} className={pref === k ? 'on' : ''} onClick={() => pick(k)}>{label}</button>)}</div>
}

function Password() {
  const [old, setOld] = useState('')
  const [pw, setPw] = useState('')
  const [pw2, setPw2] = useState('')
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const [busy, setBusy] = useState(false)
  const save = async () => {
    if (pw !== pw2) { setMsg({ ok: false, text: t('两次输入的新密码不一样') }); return }
    setBusy(true); setMsg(null)
    try {
      await api('/api/password', { method: 'POST', body: { old, new: pw } })
      setOld(''); setPw(''); setPw2('')
      setMsg({ ok: true, text: t('密码改好了。别的设备上的登录都失效了，要用新密码重新登录。') })
    } catch (e: any) { setMsg({ ok: false, text: e.message }) }
    finally { setBusy(false) }
  }
  return (
    <form className="pw-form" onSubmit={e => { e.preventDefault(); save() }}>
      <input type="password" autoComplete="current-password" value={old} placeholder={t('原来的密码')} onChange={e => setOld(e.target.value)} />
      <input type="password" autoComplete="new-password" value={pw} placeholder={t('新密码（至少 8 个字符）')} onChange={e => setPw(e.target.value)} />
      <input type="password" autoComplete="new-password" value={pw2} placeholder={t('再输一遍新密码')} onChange={e => setPw2(e.target.value)} />
      <div className="row-actions"><button className="btn" type="submit" disabled={busy || !old || pw.length < 8 || !pw2}>{t('改密码')}</button></div>
      {msg && <div className={msg.ok ? 'ok-line' : 'error-line'}>{msg.text}</div>}
    </form>
  )
}

export default function Settings() {
  const version = useStore(s => s.version)
  useEffect(() => { document.title = `${t('设置')} · Open Agent Docs` }, [])
  return (
    <>
      <header className="chat-head page-head"><span className="head-name">{t('设置')}</span></header>
      <div className="scroller"><div className="lib-body settings">
        <section className="set-sec">
          <div className="set-sec-head"><div><h3>{t('语言')}</h3><p>{t('界面文字和给 Agent 的工具说明用哪种语言。')}</p></div></div>
          <Language />
        </section>
        <section className="set-sec">
          <div className="set-sec-head"><div><h3>{t('外观')}</h3><p>{t('只在这台设备上生效。')}</p></div></div>
          <Theme />
        </section>
        <section className="set-sec" id="agents">
          <div className="set-sec-head"><div><h3>{t('Agent')}</h3></div></div>
          <Agents />
        </section>
        <section className="set-sec">
          <div className="set-sec-head"><div><h3>{t('登录密码')}</h3><p>{t('改了以后，所有设备上的登录都要重新登录。')}</p></div></div>
          <Password />
        </section>
        <section className="set-sec">
          <div className="set-sec-head"><div><h3>{t('退出登录')}</h3><p>{t('这台设备上退出。')}</p></div>
            <button className="btn" onClick={() => logout()}>{t('退出登录')}</button></div>
        </section>
        <p className="muted small">Open Agent Docs {version}</p>
      </div></div>
    </>
  )
}
