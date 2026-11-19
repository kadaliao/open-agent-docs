import { useState } from 'react'
import { api, ApiError, type Me } from './api'
import { t } from './strings'
import { Logo } from './icons'

export function Login({ onDone }: { onDone: (me: Me) => void }) {
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (busy) return
    setBusy(true)
    setError(null)
    try {
      onDone(await api.login(email.trim(), password))
    } catch (err) {
      setError(err instanceof ApiError && (err.status === 401 || err.status === 400) ? t('loginFailed') : err instanceof Error ? err.message : String(err))
      setBusy(false)
    }
  }

  return (
    <div className="login">
      <form className="login-card" onSubmit={submit}>
        <Logo size={48} />
        <h1>{t('appName')}</h1>
        <p className="muted">{t('loginSubtitle')}</p>
        <label className="field">
          <span>{t('email')}</span>
          <input type="email" autoComplete="username" autoFocus required value={email} onChange={(e) => setEmail(e.target.value)} />
        </label>
        <label className="field">
          <span>{t('password')}</span>
          <input type="password" autoComplete="current-password" required value={password} onChange={(e) => setPassword(e.target.value)} />
        </label>
        {error && (
          <div className="form-error" role="alert">
            {error}
          </div>
        )}
        <button className="btn btn-primary btn-block" disabled={busy}>
          {busy ? t('loggingIn') : t('login')}
        </button>
      </form>
    </div>
  )
}
