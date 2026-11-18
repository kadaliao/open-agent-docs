// /invite/<token>：看邀请（不用登录），已登录直接加入；没登录可以登录或注册（注册同时加入）。
import { useEffect, useState } from 'react'
import { api, ApiError, type InviteInfo, type Me } from '../api'
import { useLocale } from '../i18n'
import { navigate } from '../lib'
import { t, type AppStrings } from '../strings'
import './settings.css'

const roleLabel = (r: string) => t(({ owner: 'roleOwner', admin: 'roleAdmin', editor: 'roleEditor', viewer: 'roleViewer' } as Record<string, keyof AppStrings>)[r] ?? 'roleEditor')

export default function InvitePage({ token, me, onJoined }: { token: string; me: Me | null; onJoined: (me: Me) => void }) {
  useLocale()
  const [info, setInfo] = useState<InviteInfo | null>(null)
  const [bad, setBad] = useState(false)
  const [mode, setMode] = useState<'signup' | 'login'>('signup')
  const [form, setForm] = useState({ name: '', email: '', password: '' })
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.inviteInfo(token).then(
      (i) => {
        setInfo(i)
        if (i.signup === false) setMode('login')
      },
      () => setBad(true),
    )
  }, [token])

  const join = async () => {
    setBusy(true)
    setError(null)
    try {
      onJoined(await api.acceptInvite(token))
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
      setBusy(false)
    }
  }

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (busy) return
    setBusy(true)
    setError(null)
    try {
      if (mode === 'signup') {
        onJoined(await api.signup({ name: form.name.trim(), email: form.email.trim(), password: form.password, invite: token }))
      } else {
        await api.login(form.email.trim(), form.password)
        onJoined(await api.acceptInvite(token))
      }
    } catch (err) {
      const msg = err instanceof ApiError && err.status === 401 ? t('loginFailed') : err instanceof Error ? err.message : String(err)
      setError(mode === 'signup' ? t('signupFailed', { msg }) : msg)
      setBusy(false)
    }
  }

  const already = me && info && me.workspaces.some((w) => w.id === info.workspace.id)
  return (
    <div className="login">
      <div className="login-card invite-card">
        <div className="login-mark" aria-hidden>
          {info?.workspace.icon || '✦'}
        </div>
        {bad ? (
          <>
            <h1>{t('inviteInvalid')}</h1>
            <button className="btn" onClick={() => navigate('/', true)}>
              {t('backHome')}
            </button>
          </>
        ) : !info ? (
          <div className="sk" style={{ width: 200, margin: '20px auto' }} />
        ) : (
          <>
            <h1>{t('inviteTitle', { ws: info.workspace.name })}</h1>
            <p className="muted">{info.invited_by ? t('inviteBy', { name: info.invited_by.name, role: roleLabel(info.role) }) : t('inviteRoleOnly', { role: roleLabel(info.role) })}</p>
            {me ? (
              <>
                <div className="invite-me muted">{me.user.name}{me.user.email ? ` · ${me.user.email}` : ''}</div>
                <button className="btn btn-primary btn-block" disabled={busy} onClick={already ? () => navigate('/', true) : join}>
                  {already ? t('inviteJoined') : t('inviteAccept')}
                </button>
              </>
            ) : (
              <form onSubmit={submit} className="invite-form">
                {mode === 'signup' && (
                  <label className="field">
                    <span>{t('signupName')}</span>
                    <input autoComplete="name" required value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} />
                  </label>
                )}
                <label className="field">
                  <span>{t('email')}</span>
                  <input type="email" autoComplete="username" required value={form.email} onChange={(e) => setForm({ ...form, email: e.target.value })} />
                </label>
                <label className="field">
                  <span>{t('password')}</span>
                  <input type="password" autoComplete={mode === 'signup' ? 'new-password' : 'current-password'} required minLength={mode === 'signup' ? 8 : undefined} value={form.password} onChange={(e) => setForm({ ...form, password: e.target.value })} />
                </label>
                <button className="btn btn-primary btn-block" disabled={busy}>
                  {mode === 'signup' ? t('signupTitle') : t('login')}
                </button>
                {info.signup !== false && (
                  <button type="button" className="link-btn" onClick={() => setMode(mode === 'signup' ? 'login' : 'signup')}>
                    {mode === 'signup' ? t('haveAccount') : t('noAccount')}
                  </button>
                )}
              </form>
            )}
            {error && (
              <div className="form-error" role="alert">
                {error}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  )
}
