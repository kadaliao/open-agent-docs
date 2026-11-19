// 第一次打开一页（本机没有首屏快照）：正文的 JSON 在启动时就开始要（和 WebSocket 同时，一般 20–30ms 到），
// 前几十块直接拼成静态 HTML 先画出来——不等页面那个包下载、不等 Y.Doc 同步完、不等编辑器把整页装好（1000 段时这几步加起来 150ms 以上）。
// 编辑器好了以后换上，快照就扔了（和本机快照 snapshot.ts 一样，只是来源是服务器）。
// 在主包里，所以不用 ProseMirror：自己按编辑器 schema 的 toDOM 拼标签和类名（样式就对得上），认不出的块跳过。
import { client } from './api'
import { readSnapshot } from './snapshot'

const MAX_BLOCKS = 60
const MAX_BYTES = 120_000

export interface FirstScreen {
  title: string
  html: string
}

type Node = { type: string; attrs?: Record<string, unknown>; content?: Node[]; text?: string; marks?: { type: string; attrs?: Record<string, unknown> }[] }

const esc = (s: unknown) => String(s ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
const safeUrl = (u: unknown) => (/^(https?:|mailto:|\/|#)/i.test(String(u ?? '')) ? esc(u) : '')

function marks(n: Node): string {
  let s = esc(n.text)
  // mark 从里往外包：数组里排前面的在外层
  for (const m of [...(n.marks ?? [])].reverse()) {
    if (m.type === 'bold') s = `<strong>${s}</strong>`
    else if (m.type === 'italic') s = `<em>${s}</em>`
    else if (m.type === 'strike') s = `<s>${s}</s>`
    else if (m.type === 'code') s = `<code>${s}</code>`
    else if (m.type === 'underline') s = `<u>${s}</u>`
    else if (m.type === 'highlight') s = `<mark>${s}</mark>`
    else if (m.type === 'link') s = `<a href="${safeUrl(m.attrs?.href)}" target="_blank" rel="noopener noreferrer nofollow">${s}</a>`
  }
  return s
}

const kids = (n: Node) => (n.content ?? []).map(node).join('')

function node(n: Node): string {
  const a = n.attrs ?? {}
  switch (n.type) {
    case 'text':
      return marks(n)
    case 'hardBreak':
      return '<br>'
    case 'mathInline':
      return `<span class="oad-math-inline" data-type="mathInline">${esc(a.latex)}</span>`
    case 'linkMention':
      return `<a href="${safeUrl(a.url)}" class="oad-link-mention">${esc(a.url)}</a>`
    case 'paragraph':
      return `<p>${kids(n)}</p>`
    case 'heading': {
      const l = Math.min(6, Math.max(1, Number(a.level) || 1))
      return `<h${l}>${kids(n)}</h${l}>`
    }
    case 'bulletList':
      return `<ul>${kids(n)}</ul>`
    case 'orderedList':
      return `<ol${a.start && a.start !== 1 ? ` start="${Number(a.start)}"` : ''}>${kids(n)}</ol>`
    case 'listItem':
      return `<li>${kids(n)}</li>`
    case 'taskList':
      return `<ul data-type="taskList">${kids(n)}</ul>`
    case 'taskItem':
      return `<li data-type="taskItem" data-checked="${a.checked ? 'true' : 'false'}"><label><input type="checkbox"${a.checked ? ' checked' : ''} disabled><span></span></label><div>${kids(n)}</div></li>`
    case 'blockquote':
      return `<blockquote>${kids(n)}</blockquote>`
    case 'callout':
      return `<div class="oad-callout" data-type="callout" data-kind="${esc(a.kind ?? 'note')}"${a.title ? ` data-title="${esc(a.title)}"` : ''}>${kids(n)}</div>`
    case 'codeBlock':
      return `<pre><code${a.language ? ` class="language-${esc(a.language)}"` : ''}>${esc((n.content ?? []).map((t) => t.text ?? '').join(''))}</code></pre>`
    case 'mathBlock':
      return `<div class="oad-math-block" data-type="mathBlock">${esc(a.latex)}</div>`
    case 'horizontalRule':
      return '<hr>'
    case 'image':
      return `<img src="${safeUrl(a.src)}" alt="${esc(a.alt)}" loading="lazy" decoding="async">`
    case 'embed':
      return `<div class="oad-embed" data-type="embed"><a href="${safeUrl(a.url)}" target="_blank" rel="noopener noreferrer nofollow">${esc(a.url)}</a></div>`
    case 'table':
      return `<div class="oad-table"><table><tbody>${kids(n)}</tbody></table></div>`
    case 'tableRow':
      return `<tr>${kids(n)}</tr>`
    case 'tableHeader':
      return `<th>${kids(n)}</th>`
    case 'tableCell':
      return `<td>${kids(n)}</td>`
    case 'details':
      return `<details class="oad-details"${a.open ? ' open' : ''}>${kids(n)}</details>`
    case 'detailsSummary':
      return `<summary>${kids(n)}</summary>`
    case 'detailsContent':
      return `<div data-type="detailsContent">${kids(n)}</div>`
    default:
      return ''
  }
}

/** 文档最前面的几十块拼成 HTML（块数到 MAX_BLOCKS、或 HTML 到 MAX_BYTES 停） */
export function firstScreenHtml(doc: { content?: unknown[] } | null | undefined): string {
  let out = ''
  for (const b of (doc?.content ?? []).slice(0, MAX_BLOCKS) as Node[]) {
    const h = node(b)
    if (out.length + h.length > MAX_BYTES) break
    out += h
  }
  return out
}

// ---------- 启动时要、页面装好时拿 ----------
let pending: { id: string; promise: Promise<string>; html: string | null } | null = null
const listeners = new Set<() => void>()

const load = (id: string) =>
  client.content(id, 'json').then(
    (r) => firstScreenHtml(r.doc as { content?: unknown[] } | null),
    () => '',
  )

/** 启动时调：这页本机有快照就不要了（快照同步就能读出来，更快） */
export function prefetchFirstScreen(id: string) {
  if (pending || readSnapshot(id)) return
  const p: { id: string; promise: Promise<string>; html: string | null } = { id, promise: load(id), html: null }
  p.promise.then((h) => {
    p.html = h
    listeners.forEach((l) => l())
  })
  pending = p
}

/** 已经到了的首屏（没有或者还没到：null） */
export function peekFirstScreen(id: string): string | null {
  return pending && pending.id === id ? pending.html : null
}

/** 首屏 HTML：启动时要过的用那一份，没有就现在要 */
export function firstScreen(id: string): Promise<string> {
  return pending && pending.id === id ? pending.promise : load(id)
}

export function onFirstScreen(f: () => void) {
  listeners.add(f)
  return () => void listeners.delete(f)
}

/** 给性能测试用：正文第一次画出来的时间（快照、服务器首屏或编辑器，哪个先到算哪个），以及编辑器装好的时间 */
export function markFirstContent(kind: 'snapshot' | 'server' | 'editor' = 'editor') {
  requestAnimationFrame(() => {
    const w = window as unknown as { __oadFirstContent?: number; __oadEditorReady?: number; __oadFirstKind?: string }
    const now = performance.now()
    if (w.__oadFirstContent === undefined) {
      w.__oadFirstContent = now
      w.__oadFirstKind = kind
    }
    if (kind === 'editor' && w.__oadEditorReady === undefined) w.__oadEditorReady = now
    performance.mark('oad:' + kind)
  })
}
