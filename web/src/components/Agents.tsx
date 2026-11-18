// 设置 → Agent：给 Claude Code、Codex 这些 Agent 建 token，让它们经 /mcp 读写页面（open_agent_docs/agents.py、mcpserver.py）。
// token 只在建好时显示一次（服务器只存 sha256），这里顺手给出各家的接法，复制就能用。撤销后马上连不上，它写过的内容和署名都留着。
// 用户在页面上写的评论要某个 Agent 处理时，服务器给它发通知（notify.py）：设了 webhook 就 POST 过去（用密钥签名），没设也存着，宿主经 SSE 或轮询来取。
import { useEffect, useState } from 'react'
import { api, openPage } from '../store'
import { locale, t, tn } from '../i18n'
import { ask, notice } from './Dialog'
import type { Agent } from '../types'

const when = (ts: number) => new Date(ts * 1000).toLocaleString(locale, { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
const SCOPES = ['write', 'read', 'admin'] as const
const scopeName = (s: string) => (s === 'write' ? t('能读能写') : s === 'admin' ? t('管理（能读写，还能管别的 Agent）') : t('只读'))
const scopeShort = (s: string) => (s === 'write' ? t('能写') : s === 'admin' ? t('管理') : t('只读'))

function Copy({ text }: { text: string }) {
  const [ok, setOk] = useState(false)
  return <button className="ghost-btn" onClick={() => { navigator.clipboard?.writeText(text).catch(() => {}); setOk(true); setTimeout(() => setOk(false), 1500) }}>
    {ok ? t('已复制') : t('复制')}</button>
}

function Snippet({ text }: { text: string }) {
  return <div className="agent-snippet"><pre>{text}</pre><Copy text={text} /></div>
}

type Made = Agent & { token: string; secret: string }

// 刚建好的 token、webhook 密钥和各家的接法：关掉就再也看不到 token 了
function MadePanel({ made, url, onClose }: { made: Made; url: string; onClose: () => void }) {
  const claude = `claude mcp add --transport http docs ${url} --header "Authorization: Bearer ${made.token}"`
  const codex = `[mcp_servers.docs]\nurl = "${url}"\nbearer_token_env_var = "OAD_TOKEN"`
  const json = JSON.stringify({ mcpServers: { docs: { type: 'http', url, headers: { Authorization: `Bearer ${made.token}` } } } }, null, 2)
  return (
    <div className="agent-made">
      <p><b>{t('「{name}」的 token 建好了。只显示这一次，关掉就看不到了，先复制下来。', { name: made.name })}</b></p>
      <Snippet text={made.token} />
      <h4>Claude Code</h4>
      <p className="muted small">{t('在终端里运行（加上 -s user 就是每个项目都能用）：')}</p>
      <Snippet text={claude} />
      <h4>Codex</h4>
      <p className="muted small">{tn('加到 {file}，再在启动 Codex 的环境里设好 {env}：', { file: <code>~/.codex/config.toml</code>, env: <code>OAD_TOKEN</code> })}</p>
      <Snippet text={codex} />
      <Snippet text={`export OAD_TOKEN=${made.token}`} />
      <h4>{t('其他 MCP 客户端')}</h4>
      <p className="muted small">{tn('支持 Streamable HTTP 的 MCP 客户端都能接：地址填 {url}，请求头带 {header}。常见的 JSON 配置：', {
        url: <code>{url}</code>, header: <code>Authorization: Bearer &lt;token&gt;</code> })}</p>
      <Snippet text={json} />
      <h4>{t('Webhook 密钥')}</h4>
      <p className="muted small">{tn('有评论要它处理时，服务器往 webhook 发 POST，请求头 {sig} 是用这个密钥对“时间戳.请求体”算的 HMAC-SHA256。以后在列表里也能看。', {
        sig: <code>X-OAD-Signature</code> })}</p>
      <Snippet text={made.secret} />
      <div className="row-actions"><button className="btn primary" onClick={onClose}>{t('复制好了，关掉')}</button></div>
    </div>
  )
}

type Note = { id: number; page: { id: number; title: string }; comment: { text: string; by_name: string }; created: number }

// 一个 Agent 的详情：改名字、webhook、介绍，看 / 换 webhook 密钥，等它处理的评论
function AgentEdit({ x, onSaved }: { x: Agent; onSaved: () => void }) {
  const [name, setName] = useState(x.name)
  const [webhook, setWebhook] = useState(x.webhook)
  const [about, setAbout] = useState(x.about)
  const [secret, setSecret] = useState<string | null>(null)
  const [notes, setNotes] = useState<Note[] | null>(null)
  const [err, setErr] = useState('')
  const [saved, setSaved] = useState(false)
  useEffect(() => {
    if (x.pending) api<Note[]>(`/api/agents/${x.id}/notifications?pending=true`).then(setNotes).catch(() => setNotes([]))
    else setNotes([])
  }, [x.id, x.pending])
  const dirty = name !== x.name || webhook !== x.webhook || about !== x.about
  const save = async () => {
    setErr('')
    try {
      await api(`/api/agents/${x.id}`, { method: 'PATCH', body: { name, webhook, about } })
      setSaved(true); setTimeout(() => setSaved(false), 1500)
      onSaved()
    } catch (e: any) { setErr(e.message) }
  }
  const showSecret = async () => { try { setSecret((await api(`/api/agents/${x.id}/secret`)).secret) } catch (e: any) { setErr(e.message) } }
  const rotate = async () => {
    if (!await ask(t('换一个新的 webhook 密钥？旧的马上失效，收 webhook 的那边要换成新的才验得过签名。'), { ok: t('换新的') })) return
    try { setSecret((await api(`/api/agents/${x.id}/secret`, { method: 'POST' })).secret) } catch (e: any) { setErr(e.message) }
  }
  return (
    <div className="agent-edit">
      <label className="field">{t('名字（评论里 @ 它用这个）')}
        <input value={name} maxLength={40} onChange={e => setName(e.target.value)} /></label>
      <label className="field">{t('Webhook（可选）：有评论要它处理时 POST 到这里')}
        <input value={webhook} placeholder="https://…" onChange={e => setWebhook(e.target.value)} /></label>
      <label className="field">{t('介绍（可选）：它是做什么的，别的 Agent 和你都看得到')}
        <textarea rows={2} maxLength={300} value={about} onChange={e => setAbout(e.target.value)} /></label>
      <div className="row-actions">
        <button className="btn primary" disabled={!dirty || !name.trim()} onClick={save}>{saved ? t('已保存') : t('保存')}</button>
        {secret === null ? <button className="btn" onClick={showSecret}>{t('看 webhook 密钥')}</button>
          : <button className="btn" onClick={rotate}>{t('换一个密钥')}</button>}
      </div>
      {secret !== null && <Snippet text={secret} />}
      {err && <div className="error-line">{err}</div>}
      {!!notes?.length && <>
        <h4>{t('等它处理的评论')}</h4>
        {notes.map(n => (
          <button key={n.id} className="agent-note" onClick={() => openPage(n.page.id)}>
            <b>{n.page.title || t('无标题')}</b> <span className="muted small">{when(n.created)}</span>
            <span className="muted small agent-note-text">{n.comment.text}</span>
          </button>
        ))}
      </>}
    </div>
  )
}

function Row({ x, open, onToggle, onRevoke, onHide, onSaved }: {
  x: Agent; open?: boolean; onToggle?: () => void; onRevoke?: () => void; onHide?: () => void; onSaved: () => void
}) {
  return (
    <div className={'agent-item' + (open ? ' open' : '')}>
      <div className={'kv agent-row' + (x.revoked ? ' revoked' : '')}>
        <span className="agent-row-main">
          <span><b>{x.name}</b> <span className="chip">{scopeShort(x.scope)}</span>
            {x.pending > 0 && <span className="chip warn" title={t('还没处理的评论通知')}>{t('{n} 条待处理', { n: x.pending })}</span>}</span>
          <span className="muted small">{x.revoked ? t('{time} 撤销了', { time: when(x.revoked) })
            : x.last_used ? t('最后用过：{time}', { time: when(x.last_used) }) : t('还没用过')}
            {x.webhook && <> · {t('webhook：{url}', { url: x.webhook })}</>}</span>
          {x.about && <span className="muted small">{x.about}</span>}
        </span>
        <span className="agent-row-side">
          {onToggle && <button className="ghost-btn" onClick={onToggle}>{open ? t('收起') : t('设置')}</button>}
          {onRevoke && <button className="ghost-btn danger-text" onClick={onRevoke}>{t('撤销')}</button>}
          {onHide && <button className="ghost-btn" title={t('它写过的内容照样署它的名字')} onClick={onHide}>{t('从列表里删掉')}</button>}
        </span>
      </div>
      {open && <AgentEdit x={x} onSaved={onSaved} />}
    </div>
  )
}

export default function Agents() {
  const [list, setList] = useState<Agent[] | null>(null)
  const [name, setName] = useState('')
  const [scope, setScope] = useState<Agent['scope']>('write')
  const [webhook, setWebhook] = useState('')
  const [made, setMade] = useState<Made | null>(null)
  const [open, setOpen] = useState<number | null>(null)
  const [err, setErr] = useState('')
  const load = () => api<Agent[]>('/api/agents').then(setList).catch((e: any) => setErr(e.message))
  useEffect(() => {
    load()
    // 名字改了、新建撤销了（agents_changed），有了新评论（待处理的数变了）：重新取
    window.addEventListener('oad-agents', load)
    window.addEventListener('oad-page-comments', load)
    return () => { window.removeEventListener('oad-agents', load); window.removeEventListener('oad-page-comments', load) }
  }, [])
  const url = location.origin + '/mcp'
  const add = async () => {
    setErr('')
    try {
      setMade(await api('/api/agents', { method: 'POST', body: { name, scope, ...(webhook.trim() ? { webhook: webhook.trim() } : {}) } }))
      setName(''); setWebhook('')
      load()
    } catch (e: any) { setErr(e.message) }
  }
  const hide = async (x: Agent) => {
    try { await api(`/api/agents/${x.id}/hide`, { method: 'POST' }); load() } catch (e: any) { notice(e.message) }
  }
  const revoke = async (x: Agent) => {
    if (!await ask(t('撤销「{name}」？撤销后它马上就连不上了；它写过的内容和署名都留着。', { name: x.name }), { ok: t('撤销'), danger: true })) return
    try { await api(`/api/agents/${x.id}`, { method: 'DELETE' }); load() } catch (e: any) { notice(e.message) }
  }
  const live = (list || []).filter(x => !x.revoked), gone = (list || []).filter(x => x.revoked)
  return (
    <div>
      <p className="muted small">{tn('Agent 经 MCP 读写这里的页面：它们写的段署它们的名字，你在网页上实时看到；你在段落上写评论、@ 它们，它们会收到通知；你锁定的页面它们只能读。每个 Agent 建一个 token，不用了就撤销。MCP 地址：{url}', { url: <code>{url}</code> })}</p>
      {made ? <MadePanel made={made} url={url} onClose={() => setMade(null)} /> : <>
        <h4>{t('新建')}</h4>
        <div className="agent-new">
          <input value={name} maxLength={40} onChange={e => setName(e.target.value)} onKeyDown={e => { if (e.key === 'Enter' && name.trim()) add() }}
            placeholder={t('名字，比如 Claude Code')} />
          <select value={scope} onChange={e => setScope(e.target.value as Agent['scope'])}>
            {SCOPES.map(s => <option key={s} value={s}>{scopeName(s)}</option>)}
          </select>
          <input value={webhook} onChange={e => setWebhook(e.target.value)} placeholder={t('webhook（可选）')} />
          <button className="btn primary" disabled={!name.trim()} onClick={add}>{t('新建')}</button>
        </div>
      </>}
      {err && <div className="error-line">{err}</div>}
      <h4>{t('已建的')}</h4>
      {live.map(x => <Row key={x.id} x={x} open={open === x.id} onToggle={() => setOpen(open === x.id ? null : x.id)} onRevoke={() => revoke(x)} onSaved={load} />)}
      {/* 撤销了的收起来；删掉只是不在这里列了，它写过的东西照样署它的名字 */}
      {!!gone.length && <details className="agent-gone">
        <summary className="muted small">{t('已撤销的（{n}）', { n: gone.length })}</summary>
        {gone.map(x => <Row key={x.id} x={x} onHide={() => hide(x)} onSaved={load} />)}
      </details>}
      {list && !live.length && <div className="muted small">{t('还没有 Agent')}</div>}
    </div>
  )
}
