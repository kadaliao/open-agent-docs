// ⌘K 面板：边打边搜（服务器的全文搜索，结果带高亮片段；服务器还没好时只按标题找），最近看过的页面，命令。
import { useEffect, useMemo, useRef, useState } from 'react'
import { api, ApiError, type Me, type SearchHit } from '../api'
import { getLocale, setLocale } from '../i18n'
import { navigate, onCommand, pagePath, recentPages, runCommand, toggleTheme, type Command } from '../lib'
import { usePages, type PageStore } from '../store'
import { t, type AppStrings } from '../strings'
import './palette.css'

export { ShortcutHelp } from './Shortcuts'

interface Item {
  key: string
  section: 'recent' | 'results' | 'commands'
  title: string
  icon?: string | null
  kind?: string
  snippet?: string
  hint?: string
  run: () => void
}

interface Cmd {
  id: string
  label: keyof AppStrings
  kw: string
  icon: string
  keys?: string
  when?: () => boolean
  run: () => void
}

/** 只认 <mark>，别的都当文字（服务器说过别的已经转义了，这里再保险一次） */
export function Snippet({ html }: { html: string }) {
  const parts = html.split(/(<\/?mark>)/i)
  const out: React.ReactNode[] = []
  let on = false
  const decode = (s: string) => s.replace(/<[^>]*>/g, '').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;|&#x27;/g, "'").replace(/&amp;/g, '&')
  parts.forEach((p, i) => {
    if (/^<mark>$/i.test(p)) on = true
    else if (/^<\/mark>$/i.test(p)) on = false
    else if (p) out.push(on ? <mark key={i}>{decode(p)}</mark> : decode(p))
  })
  return <>{out}</>
}

/** 本机按标题找（服务器搜索不能用时、或者结果还没回来时先出） */
function localHits(pages: { id: string; title: string; icon?: string | null; kind?: string }[], q: string): SearchHit[] {
  const words = q.toLowerCase().split(/\s+/).filter(Boolean)
  if (!words.length) return []
  const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  return pages
    .filter((p) => words.every((w) => (p.title || '').toLowerCase().includes(w)))
    .slice(0, 12)
    .map((p) => {
      let title = esc(p.title || '')
      for (const w of words) title = title.replace(new RegExp(esc(w).replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'ig'), (m) => `<mark>${m}</mark>`)
      return { page_id: p.id, title: p.title, snippet: title, kind: p.kind ?? 'page', icon: p.icon }
    })
}

export function Palette({ store, me, pageId, onClose }: { store: PageStore; me: Me; pageId: string | null; onClose: () => void }) {
  void me
  const pages = usePages(store)
  const byId = useMemo(() => new Map(pages.map((p) => [p.id, p])), [pages])
  const [q, setQ] = useState('')
  const [hits, setHits] = useState<SearchHit[] | null>(null)
  const [busy, setBusy] = useState(false)
  const [offline, setOffline] = useState(false)
  const [active, setActive] = useState(0)
  const input = useRef<HTMLInputElement>(null)
  const list = useRef<HTMLDivElement>(null)

  useEffect(() => input.current?.focus(), [])
  // 面板开着时又按了一次 ⌘K：关掉
  useEffect(() => onCommand((c) => c === 'palette' && onClose()), [onClose])

  // 边打边搜：每次按键都发，旧的作废；60ms 内连着打只发最后一次
  useEffect(() => {
    const query = q.trim()
    if (!query) {
      setHits(null)
      setBusy(false)
      return
    }
    setBusy(true)
    let alive = true
    const timer = window.setTimeout(() => {
      api.search(store.ws, query, 20).then(
        (r) => {
          if (!alive) return
          setOffline(false)
          setHits(r)
          setBusy(false)
        },
        (e) => {
          if (!alive) return
          // 服务器还没有搜索（404）或断网：按标题找
          setOffline(!(e instanceof ApiError) || e.status === 404 || e.status >= 500)
          setHits(localHits(pages, query))
          setBusy(false)
        },
      )
    }, 60)
    return () => {
      alive = false
      clearTimeout(timer)
    }
  }, [q, store.ws]) // eslint-disable-line react-hooks/exhaustive-deps

  const close = onClose
  const go = (id: string, block?: string | null) => {
    close()
    navigate(pagePath(id) + (block ? '#' + block : ''))
  }
  const cmd = (c: Command) => () => {
    close()
    setTimeout(() => runCommand(c), 0)
  }
  const onPage = !!pageId && byId.get(pageId)?.kind !== 'database'
  const cmds: Cmd[] = [
    { id: 'new', label: 'cmdNewPage', kw: 'new page create xinjian yemian', icon: '+', run: cmd('newPage') },
    { id: 'newdb', label: 'cmdNewDatabase', kw: 'new database table create shujuku', icon: '▦', run: cmd('newDatabase') },
    { id: 'history', label: 'cmdHistory', kw: 'history version restore lishi banben', icon: '↺', when: () => onPage, run: cmd('history') },
    { id: 'share', label: 'cmdShare', kw: 'share publish public link fenxiang', icon: '↗', when: () => !!pageId, run: cmd('share') },
    { id: 'theme', label: 'cmdToggleTheme', kw: 'theme dark light zhuti shense', icon: '◐', keys: '⌘⇧L', run: () => (close(), toggleTheme()) },
    {
      id: 'lang',
      label: 'cmdLanguage',
      kw: 'language english chinese yuyan zhongwen 中文 english',
      icon: 'A',
      run: () => {
        close()
        const next = setLocale(getLocale() === 'en' ? 'zh' : 'en')
        api.patchMe({ locale: next }).catch(() => {})
      },
    },
    { id: 'sidebar', label: 'cmdToggleSidebar', kw: 'sidebar celan', icon: '⇤', keys: '⌘\\', run: cmd('sidebar') },
    { id: 'shortcuts', label: 'cmdShortcuts', kw: 'shortcuts keyboard help kuaijiejian', icon: '⌘', keys: '⌘/', run: cmd('shortcuts') },
    { id: 'settings', label: 'cmdSettings', kw: 'settings profile preferences shezhi', icon: '⚙', run: () => (close(), navigate('/settings/profile')) },
    { id: 'agents', label: 'cmdAgents', kw: 'agents mcp token', icon: '✦', run: () => (close(), navigate('/settings/agents')) },
    { id: 'trash', label: 'cmdTrash', kw: 'trash deleted restore huishouzhan', icon: '🗑', run: () => (close(), navigate('/trash')) },
  ]

  const items: Item[] = useMemo(() => {
    const query = q.trim().toLowerCase()
    const out: Item[] = []
    if (!query) {
      for (const id of recentPages().filter((x) => x !== pageId && byId.has(x)).slice(0, 8)) {
        const p = byId.get(id)!
        out.push({ key: 'r' + id, section: 'recent', title: p.title || t('untitled'), icon: p.icon, kind: p.kind, run: () => go(id) })
      }
    } else {
      // 一页只列一次（服务器还没按页合并时，这里合并：留分数最高的片段，记下还有几处命中）
      const seen = new Map<string, SearchHit & { more: number }>()
      for (const h of hits ?? localHits(pages, query)) {
        const cur = seen.get(h.page_id)
        if (!cur) seen.set(h.page_id, { ...h, more: h.hits ? h.hits - 1 : 0 })
        else {
          cur.more++
          // 标题命中的那条没有正文片段：换成有片段的
          if (cur.snippet?.replace(/<\/?mark>/g, '') === cur.title && h.snippet && h.snippet.replace(/<\/?mark>/g, '') !== h.title) Object.assign(cur, { snippet: h.snippet, block_id: h.block_id })
        }
      }
      const shown = [...seen.values()]
      for (const h of shown)
        out.push({
          key: 'h' + h.page_id + (h.block_id ?? ''),
          section: 'results',
          title: h.title || t('untitled'),
          icon: h.icon ?? byId.get(h.page_id)?.icon,
          kind: h.kind,
          snippet: h.snippet,
          hint: [h.kind === 'row' ? t('kindRow') : h.kind === 'database' ? t('kindDatabase') : parentTitle(h.page_id), h.more > 0 ? t('moreHits', { n: h.more }) : ''].filter(Boolean).join(' · ') || undefined,
          run: () => go(h.page_id, h.block_id),
        })
    }
    for (const c of cmds) {
      if (c.when && !c.when()) continue
      const label = t(c.label)
      if (query && !(label.toLowerCase().includes(query) || c.kw.includes(query))) continue
      out.push({ key: 'c' + c.id, section: 'commands', title: label, icon: c.icon, hint: c.keys, run: c.run })
    }
    return out
  }, [q, hits, pages, byId, pageId]) // eslint-disable-line react-hooks/exhaustive-deps

  function parentTitle(id: string) {
    const p = byId.get(id)
    const parent = p?.parent_id ? byId.get(p.parent_id) : null
    return parent ? parent.title || t('untitled') : undefined
  }

  useEffect(() => setActive(0), [q])
  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' })
  }, [active])

  const onKey = (e: React.KeyboardEvent) => {
    if (e.nativeEvent.isComposing) return
    if (e.key === 'ArrowDown' || (e.key === 'n' && e.ctrlKey)) {
      e.preventDefault()
      setActive((a) => Math.min(items.length - 1, a + 1))
    } else if (e.key === 'ArrowUp' || (e.key === 'p' && e.ctrlKey)) {
      e.preventDefault()
      setActive((a) => Math.max(0, a - 1))
    } else if (e.key === 'Enter') {
      e.preventDefault()
      items[active]?.run()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      close()
    }
  }

  const sections: { id: Item['section']; label: string }[] = [
    { id: 'recent', label: t('recent') },
    { id: 'results', label: t('results') },
    { id: 'commands', label: t('commands') },
  ]
  let n = -1
  return (
    <div className="modal-mask palette-mask" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="palette" role="dialog" aria-label={t('searchPlaceholder')} onKeyDown={onKey}>
        <div className="palette-search">
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden>
            <path fill="currentColor" d="M10 3a7 7 0 0 1 5.6 11.2l4.6 4.6-1.4 1.4-4.6-4.6A7 7 0 1 1 10 3zm0 2a5 5 0 1 0 0 10 5 5 0 0 0 0-10z" />
          </svg>
          <input
            ref={input}
            value={q}
            placeholder={t('searchPlaceholder')}
            onChange={(e) => setQ(e.target.value)}
            role="combobox"
            aria-expanded
            aria-controls="palette-list"
            aria-activedescendant={items[active] ? 'pi-' + items[active].key : undefined}
            spellCheck={false}
            autoComplete="off"
          />
          {busy && <span className="palette-spin" aria-label={t('searching')} />}
        </div>
        <div className="palette-list" id="palette-list" role="listbox" ref={list}>
          {offline && q.trim() && <div className="palette-note">{t('searchOffline')}</div>}
          {sections.map((s) => {
            const its = items.filter((x) => x.section === s.id)
            if (!its.length) return null
            return (
              <div key={s.id} className="palette-section">
                <div className="palette-label">{s.label}</div>
                {its.map((it) => {
                  n++
                  const idx = n
                  return (
                    <div
                      key={it.key}
                      id={'pi-' + it.key}
                      role="option"
                      aria-selected={idx === active}
                      className={'palette-item' + (it.snippet && it.snippet.replace(/<\/?mark>/g, '') !== it.title ? ' has-snippet' : '')}
                      onMouseMove={() => idx !== active && setActive(idx)}
                      onClick={it.run}
                    >
                      <span className="palette-ico" aria-hidden>
                        {it.icon || (it.kind === 'database' ? '▦' : it.kind === 'row' ? '▤' : <DocIco />)}
                      </span>
                      <span className="palette-main">
                        <span className="palette-title">{it.section === 'results' && it.snippet && it.snippet.replace(/<\/?mark>/g, '') === it.title ? <Snippet html={it.snippet} /> : it.title}</span>
                        {it.snippet && it.snippet.replace(/<\/?mark>/g, '') !== it.title && (
                          <span className="palette-snippet">
                            <Snippet html={it.snippet} />
                          </span>
                        )}
                      </span>
                      {it.hint && <span className={'palette-hint' + (it.section === 'commands' ? ' is-keys' : '')}>{it.hint}</span>}
                    </div>
                  )
                })}
              </div>
            )
          })}
          {q.trim() && !busy && !items.some((x) => x.section === 'results') && <div className="palette-empty">{t('noSearchResults', { q: q.trim() })}</div>}
        </div>
        <div className="palette-foot">{t('paletteHint')}</div>
      </div>
    </div>
  )
}

function DocIco() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <path d="M4 1.75h5.2L12.5 5v8.25c0 .55-.45 1-1 1H4c-.55 0-1-.45-1-1v-10.5c0-.55.45-1 1-1z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
      <path d="M9 1.9V5.2h3.3M5.5 8h5M5.5 10.5h3.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  )
}
