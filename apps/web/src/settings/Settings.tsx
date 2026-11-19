// 设置：个人（名字、头像、语言、主题、密码）、工作区（名字、图标、成员和角色、邀请链接）、Agent。
import { useEffect, useRef, useState } from 'react'
import { api, ApiError, type Invite, type Me, type Member, type Role } from '../api'
import AgentsPage from '../AgentsPage'
import { getLocalePref, setLocale, useLocale, type LocalePref } from '../i18n'
import { applyTheme, colorFor, copyText, initials, navigate, usePath, useTheme, type Theme } from '../lib'
import { uploader } from '../media'
import type { PageStore } from '../store'
import { t, type AppStrings } from '../strings'
import { Shell } from './Shell'
import './settings.css'

type Tab = 'profile' | 'workspace' | 'agents'
const TABS: { id: Tab; label: keyof AppStrings }[] = [
  { id: 'profile', label: 'settingsProfile' },
  { id: 'workspace', label: 'settingsWorkspace' },
  { id: 'agents', label: 'agents' },
]

const errText = (e: unknown) => (e instanceof ApiError && e.status === 404 ? t('error') + '（404）' : e instanceof Error ? e.message : String(e))

export default function SettingsPage({ store, me, onMe, sidebarCollapsed, onExpandSidebar }: { store: PageStore; me: Me; onMe: (me: Me) => void; sidebarCollapsed: boolean; onExpandSidebar: () => void }) {
  useLocale()
  const path = usePath()
  const tab = (TABS.find((x) => path.startsWith('/settings/' + x.id))?.id ?? 'profile') as Tab
  const ws = me.workspaces[0]
  const isAdmin = ws?.role === 'owner' || ws?.role === 'admin'
  return (
    <Shell store={store} title={`${t('settings')} / ${t(TABS.find((x) => x.id === tab)!.label)}`} sidebarCollapsed={sidebarCollapsed} onExpandSidebar={onExpandSidebar}>
      <div className={'settings' + (tab === 'agents' ? ' set-tabs-only' : '')}>
        <nav className="set-tabs" role="tablist">
          {TABS.map((x) => (
            <button key={x.id} role="tab" aria-selected={tab === x.id} className={'set-tab' + (tab === x.id ? ' is-on' : '')} onClick={() => navigate('/settings/' + x.id, true)}>
              {t(x.label)}
            </button>
          ))}
        </nav>
        {tab === 'profile' && <Profile store={store} me={me} onMe={onMe} />}
        {tab === 'workspace' && <WorkspaceSettings store={store} me={me} onMe={onMe} isAdmin={isAdmin} />}
      </div>
      {tab === 'agents' && <AgentsPage store={store} canManage={ws?.role !== 'viewer'} />}
    </Shell>
  )
}

function useFlash() {
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const timer = useRef(0)
  const flash = (ok: boolean, text: string) => {
    setMsg({ ok, text })
    clearTimeout(timer.current)
    timer.current = window.setTimeout(() => setMsg(null), ok ? 2000 : 6000)
  }
  return [msg, flash] as const
}

function Flash({ msg }: { msg: { ok: boolean; text: string } | null }) {
  if (!msg) return null
  return (
    <span className={'set-flash' + (msg.ok ? ' is-ok' : ' is-bad')} role={msg.ok ? 'status' : 'alert'}>
      {msg.text}
    </span>
  )
}

const meFrom = (me: Me, r: Me | Me['user']): Me => ('user' in r ? r : { ...me, user: { ...me.user, ...r } })

function Profile({ store, me, onMe }: { store: PageStore; me: Me; onMe: (me: Me) => void }) {
  const [name, setName] = useState(me.user.name)
  const [msg, flash] = useFlash()
  const [pw, setPw] = useState({ cur: '', next: '' })
  const [pwMsg, pwFlash] = useFlash()
  const [uploading, setUploading] = useState(false)
  const file = useRef<HTMLInputElement>(null)
  const theme = useTheme()
  const pref = getLocalePref()

  const patch = async (b: Parameters<typeof api.patchMe>[0]) => {
    try {
      const r = await api.patchMe(b)
      onMe(meFrom(me, r))
      flash(true, t('saved'))
    } catch (e) {
      // 服务器还没有 PATCH /me 时本机先改着
      if (b.name || b.avatar !== undefined) onMe({ ...me, user: { ...me.user, ...(b.name ? { name: b.name } : {}), ...(b.avatar !== undefined ? { avatar: b.avatar } : {}) } })
      flash(false, errText(e))
    }
  }
  const saveName = () => {
    const n = name.trim()
    if (n && n !== me.user.name) patch({ name: n })
  }
  const pickLocale = (p: LocalePref) => {
    const l = setLocale(p)
    api.patchMe({ locale: p === 'auto' ? null : l }).catch(() => {})
  }
  const upload = async (f: File) => {
    setUploading(true)
    try {
      const up = await uploader(store.ws)(f, () => {})
      await patch({ avatar: up.url })
    } catch (e) {
      flash(false, errText(e))
    } finally {
      setUploading(false)
    }
  }
  const changePw = async (e: React.FormEvent) => {
    e.preventDefault()
    if (pw.next.length < 8) return pwFlash(false, t('passwordTooShort'))
    try {
      await api.changePassword(pw.cur, pw.next)
      setPw({ cur: '', next: '' })
      pwFlash(true, t('passwordChanged'))
    } catch (err) {
      pwFlash(false, errText(err))
    }
  }

  return (
    <div className="set-sections">
      <section className="set-card">
        <div className="set-row">
          <div className="set-label">{t('profileAvatar')}</div>
          <div className="set-avatar-row">
            {me.user.avatar ? (
              <img className="set-avatar" src={me.user.avatar} alt="" />
            ) : (
              <span className="set-avatar" style={{ background: colorFor(me.user.id) }}>
                {initials(me.user.name)}
              </span>
            )}
            <button className="btn btn-sm" onClick={() => file.current?.click()} disabled={uploading}>
              {uploading ? t('saving') : t('uploadAvatar')}
            </button>
            {me.user.avatar && (
              <button className="btn btn-sm" onClick={() => patch({ avatar: null })}>
                {t('removeAvatar')}
              </button>
            )}
            <input ref={file} type="file" accept="image/*" hidden onChange={(e) => e.target.files?.[0] && upload(e.target.files[0])} />
          </div>
        </div>
        <label className="set-row">
          <span className="set-label">{t('profileName')}</span>
          <input className="set-input" value={name} onChange={(e) => setName(e.target.value)} onBlur={saveName} onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()} maxLength={60} />
        </label>
        {me.user.email && (
          <div className="set-row">
            <span className="set-label">{t('profileEmail')}</span>
            <span className="set-value">{me.user.email}</span>
          </div>
        )}
        <div className="set-row">
          <span className="set-label">{t('language')}</span>
          <div className="seg" role="radiogroup" aria-label={t('language')}>
            {(
              [
                ['auto', t('languageAuto')],
                ['zh', '中文'],
                ['en', 'English'],
              ] as [LocalePref, string][]
            ).map(([v, label]) => (
              <button key={v} role="radio" aria-checked={pref === v} aria-selected={pref === v} onClick={() => pickLocale(v)}>
                {label}
              </button>
            ))}
          </div>
        </div>
        <div className="set-row">
          <span className="set-label">{t('theme')}</span>
          <div className="seg" role="radiogroup" aria-label={t('theme')}>
            {(
              [
                ['system', t('themeSystem')],
                ['light', t('themeLight')],
                ['dark', t('themeDark')],
              ] as [Theme, string][]
            ).map(([v, label]) => (
              <button key={v} role="radio" aria-checked={theme === v} aria-selected={theme === v} onClick={() => applyTheme(v)}>
                {label}
              </button>
            ))}
          </div>
        </div>
        <div className="set-foot">
          <Flash msg={msg} />
        </div>
      </section>

      <section className="set-card">
        <h2 className="set-h2">{t('password')}</h2>
        <form className="set-pw" onSubmit={changePw}>
          <input type="text" name="username" autoComplete="username" value={me.user.email ?? ''} hidden readOnly />
          <label className="field">
            <span>{t('currentPassword')}</span>
            <input type="password" autoComplete="current-password" value={pw.cur} onChange={(e) => setPw({ ...pw, cur: e.target.value })} />
          </label>
          <label className="field">
            <span>{t('newPassword')}</span>
            <input type="password" autoComplete="new-password" value={pw.next} onChange={(e) => setPw({ ...pw, next: e.target.value })} />
          </label>
          <div className="set-foot">
            <button className="btn" disabled={!pw.cur || !pw.next}>
              {t('changePassword')}
            </button>
            <Flash msg={pwMsg} />
          </div>
        </form>
      </section>
    </div>
  )
}

const ROLES = ['owner', 'admin', 'editor', 'viewer'] as const
const roleLabel = (r: string) => t(({ owner: 'roleOwner', admin: 'roleAdmin', editor: 'roleEditor', viewer: 'roleViewer' } as Record<string, keyof AppStrings>)[r] ?? 'roleEditor')
const EXPIRES: [number | null, keyof AppStrings][] = [
  [86400, 'expires1d'],
  [7 * 86400, 'expires7d'],
  [30 * 86400, 'expires30d'],
  [null, 'expiresNever'],
]


function WorkspaceSettings({ store, me, onMe, isAdmin }: { store: PageStore; me: Me; onMe: (me: Me) => void; isAdmin: boolean }) {
  const ws = me.workspaces[0]
  const [name, setName] = useState(ws.name)
  const [icon, setIcon] = useState(ws.icon ?? '')
  const [msg, flash] = useFlash()
  const [members, setMembers] = useState<Member[] | null>(null)
  const [invites, setInvites] = useState<Invite[] | null>(null)
  const [invMsg, invFlash] = useFlash()
  const [newRole, setNewRole] = useState('editor')
  const [newExp, setNewExp] = useState<number | null>(7 * 86400)
  const [fresh, setFresh] = useState<string | null>(null)

  const loadMembers = () => api.members(store.ws).then(setMembers, () => setMembers([]))
  const loadInvites = () => (isAdmin ? api.invites(store.ws).then(setInvites, () => setInvites([])) : Promise.resolve())
  useEffect(() => {
    loadMembers()
    loadInvites()
  }, [store.ws]) // eslint-disable-line react-hooks/exhaustive-deps

  const saveWs = async () => {
    const b = { name: name.trim() || ws.name, icon: icon.trim() || null }
    if (b.name === ws.name && b.icon === (ws.icon ?? null)) return
    try {
      const r = await api.patchWorkspace(store.ws, b)
      onMe({ ...me, workspaces: me.workspaces.map((x) => (x.id === ws.id ? { ...x, name: r.name ?? b.name, icon: r.icon ?? b.icon } : x)) })
      flash(true, t('saved'))
    } catch (e) {
      flash(false, errText(e))
    }
  }
  const setRole = async (m: Member, role: Role) => {
    setMembers((l) => l?.map((x) => (x.user.id === m.user.id ? { ...x, role } : x)) ?? l)
    try {
      await api.patchMember(store.ws, m.user.id, role)
    } catch (e) {
      flash(false, errText(e))
      loadMembers()
    }
  }
  const remove = async (m: Member) => {
    if (!window.confirm(t('removeMemberConfirm', { name: m.user.name }))) return
    try {
      await api.removeMember(store.ws, m.user.id)
      loadMembers()
    } catch (e) {
      flash(false, errText(e))
    }
  }
  const createInvite = async () => {
    try {
      const inv = await api.createInvite(store.ws, { role: newRole, expires_in: newExp })
      const url = inv.url.startsWith('http') ? inv.url : location.origin + inv.url
      setFresh(url)
      copyText(url)
      invFlash(true, t('copied'))
      loadInvites()
    } catch (e) {
      invFlash(false, errText(e))
    }
  }
  const revoke = async (inv: Invite) => {
    try {
      await api.deleteInvite(store.ws, inv.token)
      loadInvites()
    } catch (e) {
      invFlash(false, errText(e))
    }
  }

  return (
    <div className="set-sections">
      {!isAdmin && <p className="muted">{t('onlyAdmins')}</p>}
      <section className="set-card">
        <label className="set-row">
          <span className="set-label">{t('workspaceIcon')}</span>
          <input className="set-input set-icon-input" value={icon} placeholder="✦" maxLength={4} disabled={!isAdmin} onChange={(e) => setIcon(e.target.value)} onBlur={saveWs} title={t('workspaceIconHint')} />
        </label>
        <label className="set-row">
          <span className="set-label">{t('workspaceName')}</span>
          <input className="set-input" value={name} disabled={!isAdmin} onChange={(e) => setName(e.target.value)} onBlur={saveWs} onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()} maxLength={80} />
        </label>
        <div className="set-foot">
          <Flash msg={msg} />
        </div>
      </section>

      <section className="set-card">
        <h2 className="set-h2">
          {t('members')}
          {members && <span className="set-count">{members.length}</span>}
        </h2>
        <div className="set-members">
          {(members ?? []).map((m) => (
            <div key={m.user.id} className="set-member" data-user={m.user.id}>
              {m.user.avatar ? (
                <img className="avatar" src={m.user.avatar} alt="" />
              ) : (
                <span className="avatar" style={{ background: colorFor(m.user.id) }}>
                  {initials(m.user.name)}
                </span>
              )}
              <div className="set-member-name">
                <b>
                  {m.user.name}
                  {m.user.id === me.user.id && <span className="muted"> · {t('you')}</span>}
                </b>
                {m.user.email && <span className="muted">{m.user.email}</span>}
              </div>
              {isAdmin && m.user.id !== me.user.id ? (
                <select className="set-select" value={m.role} aria-label={t('role')} onChange={(e) => setRole(m, e.target.value as Role)}>
                  {ROLES.map((r) => (
                    <option key={r} value={r}>
                      {roleLabel(r)}
                    </option>
                  ))}
                </select>
              ) : (
                <span className="set-role">{roleLabel(m.role)}</span>
              )}
              {isAdmin && m.user.id !== me.user.id && m.role !== 'owner' && (
                <button className="icon-btn" title={t('removeMember')} aria-label={t('removeMember')} onClick={() => remove(m)}>
                  ×
                </button>
              )}
            </div>
          ))}
        </div>
      </section>

      {isAdmin && (
        <section className="set-card">
          <h2 className="set-h2">{t('invites')}</h2>
          <p className="muted set-p">{t('invitesHint')}</p>
          <div className="set-invite-new">
            <label>
              <span>{t('inviteRole')}</span>
              <select className="set-select" value={newRole} onChange={(e) => setNewRole(e.target.value)}>
                {ROLES.filter((r) => r !== 'owner').map((r) => (
                  <option key={r} value={r}>
                    {roleLabel(r)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>{t('inviteExpires')}</span>
              <select className="set-select" value={String(newExp)} onChange={(e) => setNewExp(e.target.value === 'null' ? null : Number(e.target.value))}>
                {EXPIRES.map(([v, l]) => (
                  <option key={String(v)} value={String(v)}>
                    {t(l)}
                  </option>
                ))}
              </select>
            </label>
            <button className="btn btn-primary" onClick={createInvite}>
              {t('newInvite')}
            </button>
            <Flash msg={invMsg} />
          </div>
          {fresh && (
            <div className="set-fresh">
              <input readOnly value={fresh} onFocus={(e) => e.target.select()} aria-label={t('invites')} />
              <button className="btn btn-sm" onClick={() => copyText(fresh).then(() => invFlash(true, t('copied')))}>
                {t('copy')}
              </button>
            </div>
          )}
          <div className="set-invites">
            {invites && !invites.length && <div className="muted set-p">{t('noInvites')}</div>}
            {(invites ?? []).map((inv) => {
              const url = inv.url.startsWith('http') ? inv.url : location.origin + inv.url
              return (
                <div key={inv.token} className="set-invite">
                  <span className="set-role">{roleLabel(inv.role)}</span>
                  <code className="set-invite-url">{url.replace(/^https?:\/\//, '')}</code>
                  <span className="muted">
                    {inv.expires_at ? t('expiresAt', { when: new Date(inv.expires_at).toLocaleDateString() }) : t('expiresNever')}
                    {inv.uses ? ' · ' + t('inviteUses', { n: inv.uses }) : ''}
                  </span>
                  <button className="btn btn-sm" onClick={() => copyText(url).then(() => invFlash(true, t('copied')))}>
                    {t('copy')}
                  </button>
                  <button className="btn btn-sm btn-danger" onClick={() => revoke(inv)}>
                    {t('inviteRevoke')}
                  </button>
                </div>
              )
            })}
          </div>
        </section>
      )}
    </div>
  )
}
