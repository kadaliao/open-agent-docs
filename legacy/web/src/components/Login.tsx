// 登录：只有一个密码（第一次启动时打印在终端里，或者 OAD_PASSWORD；改密码用 open-agent-docs password 或设置页）。
import { useState } from 'react'
import { login } from '../store'
import { t } from '../i18n'

export default function Login() {
  const [pw, setPw] = useState('')
  const [err, setErr] = useState('')
  const [busy, setBusy] = useState(false)
  const go = async () => {
    if (!pw || busy) return
    setBusy(true); setErr('')
    try { await login(pw) } catch (e: any) { setErr(e.message); setBusy(false) }
  }
  return (
    <div className="login">
      <form className="login-box" onSubmit={e => { e.preventDefault(); go() }}>
        <div className="login-logo">◆</div>
        <h2>Open Agent Docs</h2>
        <p className="muted small">{t('输入登录密码。第一次启动时它打印在服务器的终端里。')}</p>
        <input type="password" autoFocus autoComplete="current-password" value={pw} placeholder={t('密码')} onChange={e => setPw(e.target.value)} />
        <button className="btn primary" type="submit" disabled={!pw || busy}>{busy ? t('登录中…') : t('登录')}</button>
        {err && <div className="error-line">{err}</div>}
        <p className="muted small">{t('忘了密码：在服务器上运行 open-agent-docs password 设一个新的。')}</p>
      </form>
    </div>
  )
}
