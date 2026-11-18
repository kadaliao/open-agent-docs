// 工作区设置 · Agent：列表、新建（token 和 webhook 密钥只显示一次，附接入命令）、重发 token / 密钥、撤销、改说明 / 权限 / webhook。
import { useEffect, useState } from 'react'
import { api, ApiError, type Agent, type Scope } from './api'
import { SidebarIcon } from './icons'
import { Inbox } from './Inbox'
import { ago, colorFor, copyText } from './lib'
import { loadAgents, useAgents } from './people'
import type { PageStore } from './store'
import { t, type AppStrings } from './strings'

const SCOPES: { v: Scope; label: keyof AppStrings; hint: keyof AppStrings }[] = [
  { v: 'read', label: 'scopeRead', hint: 'scopeReadHint' },
  { v: 'write', label: 'scopeWrite', hint: 'scopeWriteHint' },
  { v: 'admin', label: 'scopeAdmin', hint: 'scopeAdminHint' },
]
const scopeLabel = (s: Scope) => t(SCOPES.find((x) => x.v === s)?.label ?? 'scopeWrite')

interface Secrets {
  name: string
  token?: string
  webhook_secret?: string
}

export default function AgentsPage({ store, canManage, sidebarCollapsed, onExpandSidebar }: { store: PageStore; canManage: boolean; sidebarCollapsed: boolean; onExpandSidebar: () => void }) {
  const agents = useAgents(store.ws)
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<Agent | null>(null)
  const [secrets, setSecrets] = useState<Secrets | null>(null)
  const [error, setError] = useState<string | null>(null)
  const refresh = () => loadAgents(store.ws, true)
  useEffect(() => {
    refresh()
    // 通知数、webhook 状态会变：开着页面时隔一会儿刷新一次
    const id = window.setInterval(refresh, 15_000)
    return () => clearInterval(id)
  }, [store.ws]) // eslint-disable-line react-hooks/exhaustive-deps

  const act = async (f: () => Promise<unknown>) => {
    setError(null)
    try {
      await f()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
    refresh()
  }

  const list = [...agents].sort((a, b) => Number(!!a.revoked_at) - Number(!!b.revoked_at) || a.created_at - b.created_at)

  return (
    <div className="page">
      <header className="topbar">
        {sidebarCollapsed && (
          <button className="icon-btn" onClick={onExpandSidebar} aria-label={t('toggleSidebar')}>
            <SidebarIcon />
          </button>
        )}
        <nav className="crumbs">
          <span className="crumb-btn is-current">
            {t('settings')} / {t('agents')}
          </span>
        </nav>
        <div className="topbar-right">
          <Inbox store={store} />
        </div>
      </header>
      <div className="page-scroll">
        <div className="settings">
          <div className="settings-head">
            <div>
              <h1>{t('agentsTitle')}</h1>
              <p className="muted">{t('agentsHint')}</p>
            </div>
            {canManage && (
              <button className="btn btn-primary" onClick={() => setCreating(true)}>
                {t('newAgent')}
              </button>
            )}
          </div>
          {!canManage && <p className="muted">{t('readOnlyMember')}</p>}
          {error && (
            <div className="form-error" role="alert">
              {error}
            </div>
          )}
          {list.length === 0 ? (
            <div className="settings-empty">{t('noAgents')}</div>
          ) : (
            <div className="agent-list">
              {list.map((a) => {
                const pending = a.pending ?? a.pending_notifications
                return (
                  <div key={a.id} className={'agent-row' + (a.revoked_at ? ' is-revoked' : '')} data-agent={a.name}>
                    <span className="avatar is-agent" style={{ background: colorFor(a.id) }}>
                      ✦
                    </span>
                    <div className="agent-main">
                      <div className="agent-title">
                        <span className="agent-name">{a.name}</span>
                        <span className={'chip chip-' + a.scope}>{scopeLabel(a.scope)}</span>
                        {a.revoked_at && <span className="chip chip-revoked">{t('revoked')}</span>}
                      </div>
                      {a.about && <div className="agent-about">{a.about}</div>}
                      <div className="agent-meta">
                        <span>
                          {t('lastUsed')}：{a.last_used_at ? ago(a.last_used_at) : t('never')}
                        </span>
                        {pending !== undefined && (
                          <span className={pending > 0 ? 'is-warn' : ''}>
                            {t('pending')}：{pending}
                          </span>
                        )}
                        <span className={a.webhook_error ? 'is-bad' : ''} title={a.webhook_error ?? a.webhook ?? undefined}>
                          {t('webhook')}：{!a.webhook ? t('webhookNone') : a.webhook_error ? `${t('webhookError')}（${a.webhook_error}）` : t('webhookOk')}
                        </span>
                      </div>
                    </div>
                    {canManage && !a.revoked_at && (
                      <div className="agent-actions">
                        <button className="btn btn-sm" onClick={() => setEditing(a)}>
                          {t('edit')}
                        </button>
                        <button
                          className="btn btn-sm"
                          onClick={() =>
                            window.confirm(t('reissueTokenConfirm')) &&
                            act(async () => setSecrets({ name: a.name, token: (await api.reissueToken(a.id)).token }))
                          }
                        >
                          {t('reissueToken')}
                        </button>
                        <button
                          className="btn btn-sm"
                          onClick={() => act(async () => setSecrets({ name: a.name, webhook_secret: (await api.reissueWebhookSecret(a.id)).webhook_secret }))}
                        >
                          {t('reissueSecret')}
                        </button>
                        <button
                          className="btn btn-sm btn-danger"
                          onClick={() => window.confirm(t('revokeConfirm', { name: a.name })) && act(() => api.revokeAgent(a.id))}
                        >
                          {t('revoke')}
                        </button>
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          )}
        </div>
      </div>
      {creating && (
        <AgentDialog
          title={t('newAgent')}
          submitLabel={t('create')}
          onClose={() => setCreating(false)}
          onSubmit={async (v) => {
            const r = await api.createAgent(store.ws, { name: v.name, about: v.about || undefined, scope: v.scope, webhook: v.webhook || null })
            setCreating(false)
            setSecrets({ name: r.agent.name, token: r.token, webhook_secret: r.webhook_secret })
            refresh()
          }}
        />
      )}
      {editing && (
        <AgentDialog
          title={t('editAgent')}
          submitLabel={t('save')}
          initial={editing}
          onClose={() => setEditing(null)}
          onSubmit={async (v) => {
            await api.patchAgent(editing.id, { name: v.name, about: v.about, scope: v.scope, webhook: v.webhook || null })
            setEditing(null)
            refresh()
          }}
        />
      )}
      {secrets && <SecretsDialog s={secrets} onClose={() => setSecrets(null)} />}
    </div>
  )
}

function Modal({ title, children, onClose }: { title: string; children: React.ReactNode; onClose: () => void }) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', k)
    return () => window.removeEventListener('keydown', k)
  }, [onClose])
  return (
    <div className="modal-mask" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-modal="true" aria-label={title}>
        <div className="modal-head">
          <h2>{title}</h2>
          <button className="icon-btn" onClick={onClose} aria-label={t('cancel')}>
            ✕
          </button>
        </div>
        {children}
      </div>
    </div>
  )
}

interface FormValue {
  name: string
  about: string
  scope: Scope
  webhook: string
}

function AgentDialog({ title, submitLabel, initial, onClose, onSubmit }: { title: string; submitLabel: string; initial?: Agent; onClose: () => void; onSubmit: (v: FormValue) => Promise<void> }) {
  const [v, setV] = useState<FormValue>({ name: initial?.name ?? '', about: initial?.about ?? '', scope: initial?.scope ?? 'write', webhook: initial?.webhook ?? '' })
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      await onSubmit({ ...v, name: v.name.trim(), about: v.about.trim(), webhook: v.webhook.trim() })
    } catch (err) {
      setError(err instanceof ApiError ? err.message : String(err))
      setBusy(false)
    }
  }
  return (
    <Modal title={title} onClose={onClose}>
      <form className="modal-body" onSubmit={submit}>
        <label className="field">
          <span>{t('agentName')}</span>
          <input name="name" required autoFocus maxLength={64} value={v.name} onChange={(e) => setV({ ...v, name: e.target.value })} />
          <small>{t('agentNameHint')}</small>
        </label>
        <label className="field">
          <span>{t('agentAbout')}</span>
          <input name="about" maxLength={200} value={v.about} placeholder={t('agentAboutHint')} onChange={(e) => setV({ ...v, about: e.target.value })} />
        </label>
        <div className="field">
          <span>{t('agentScope')}</span>
          <div className="scope-pick" role="radiogroup">
            {SCOPES.map((s) => (
              <label key={s.v} className={'scope-opt' + (v.scope === s.v ? ' is-on' : '')}>
                <input type="radio" name="scope" value={s.v} checked={v.scope === s.v} onChange={() => setV({ ...v, scope: s.v })} />
                <b>{t(s.label)}</b>
                <small>{t(s.hint)}</small>
              </label>
            ))}
          </div>
        </div>
        <label className="field">
          <span>{t('webhook')}</span>
          <input name="webhook" type="url" placeholder="https://" value={v.webhook} onChange={(e) => setV({ ...v, webhook: e.target.value })} />
          <small>{t('webhookHint')}</small>
        </label>
        {error && (
          <div className="form-error" role="alert">
            {error}
          </div>
        )}
        <div className="modal-foot">
          <button type="button" className="btn" onClick={onClose}>
            {t('cancel')}
          </button>
          <button className="btn btn-primary" disabled={busy || !v.name.trim()}>
            {busy ? t('creating') : submitLabel}
          </button>
        </div>
      </form>
    </Modal>
  )
}

function CopyField({ label, value, multiline, testid }: { label: string; value: string; multiline?: boolean; testid?: string }) {
  const [ok, setOk] = useState(false)
  return (
    <div className="copy-field">
      <div className="copy-label">
        <span>{label}</span>
        <button
          type="button"
          className="link-btn"
          onClick={async () => {
            if (await copyText(value)) {
              setOk(true)
              setTimeout(() => setOk(false), 1500)
            }
          }}
        >
          {ok ? t('copied') : t('copy')}
        </button>
      </div>
      {multiline ? (
        <pre className="copy-value" data-testid={testid}>
          {value}
        </pre>
      ) : (
        <code className="copy-value" data-testid={testid}>
          {value}
        </code>
      )}
    </div>
  )
}

function SecretsDialog({ s, onClose }: { s: Secrets; onClose: () => void }) {
  const url = `${location.origin}/mcp`
  const json = s.token
    ? JSON.stringify({ mcpServers: { oad: { type: 'http', url, headers: { Authorization: `Bearer ${s.token}` } } } }, null, 2)
    : ''
  return (
    <Modal title={`${t('secretTitle')} · ${s.name}`} onClose={onClose}>
      <div className="modal-body">
        <div className="secret-warn">{t('secretWarn')}</div>
        {s.token && <CopyField label={t('token')} value={s.token} testid="agent-token" />}
        {s.webhook_secret && <CopyField label={t('webhookSecret')} value={s.webhook_secret} testid="agent-webhook-secret" />}
        {s.token && (
          <>
            <CopyField label={t('claudeCode')} value={`claude mcp add --transport http oad ${url} --header "Authorization: Bearer ${s.token}"`} testid="agent-claude-cmd" />
            <CopyField label={t('mcpJson')} value={json} multiline />
          </>
        )}
        <div className="modal-foot">
          <button className="btn btn-primary" onClick={onClose}>
            {t('done')}
          </button>
        </div>
      </div>
    </Modal>
  )
}
