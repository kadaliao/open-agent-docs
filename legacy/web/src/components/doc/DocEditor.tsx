// markdown 页面的编辑器（懒加载，见 lazy.tsx）：直接改、自动保存，Agent 改了实时合进来，每段左边标着是谁写的。
import { useEffect, useMemo, useRef, useState } from 'react'
import { EditorContent, useEditor } from '@tiptap/react'
import { api, useStore } from '../../store'
import { locale, t } from '../../i18n'
import { ask, notice } from '../Dialog'
import Markdown from '../Markdown'
import { extensions } from './schema'
import { DocSync, REMOTE, type Conflict, type PageBlocks, type Status } from './sync'
import { BlockHandle, ImageBox, SlashMenu, Toolbar, insertImages, moveBlock, safeHref, slashItems } from './menus'
import { CommentsPanel, countOpen, type Draft, type PageComment } from './comments'
import { HistoryPanel, HistoryView, type Version } from './history'
import type { Row } from './diff'
import { blocksOf } from './markdown'
import { isPhone } from '../../swipe'
import { useAuthors, who } from '../../authors'

const when = (ts: number) => new Date(ts * 1000).toLocaleString(locale, { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })

export default function DocEditor({ id, fallback }: { id: number; fallback: string }) {
  const connected = useStore(s => s.connected)
  const [status, setStatus] = useState<Status>('saved')
  const [conflicts, setConflicts] = useState<Conflict[]>([])
  const [editing, setEditing] = useState<string | null>(null)
  const [ready, setReady] = useState(false)
  const [err, setErr] = useState('')
  const [imageBox, setImageBox] = useState(false)
  const [comments, setComments] = useState<PageComment[]>([])
  const [side, setSide] = useState<'comments' | 'history' | null>(null)   // 右边开着评论还是历史
  const [viewing, setViewing] = useState<{ v: Version; list: Version[] } | null>(null)   // 正在看的那一版（正文的位置换成对比）
  const [stamp, setStamp] = useState(0)   // 恢复以后让历史列表重新取
  const [draft, setDraft] = useState<Draft | null>(null)
  const [focus, setFocus] = useState<string | null>(null)
  const [owner, setOwner] = useState<number | null>(null)
  const sync = useRef<DocSync | null>(null)
  const loadComments = () => api(`/api/pages/${id}/comments`).then(setComments).catch(() => {})

  const nameOf = useAuthors()
  const exts = useMemo(() => extensions({ who, when }), [])   // 作者条按现在的 store 取名字（Agent，见 authors.ts）

  // 挂上以后再建编辑器：渲染时就建的那个会在 effect 跑之前被 useEditor 销毁重建（Tiptap 的 1 毫秒延迟销毁）
  const editor = useEditor({
    extensions: exts,
    immediatelyRender: false,
    shouldRerenderOnTransaction: false,
    editorProps: {
      attributes: { class: 'md doc-body' },
      scrollThreshold: 80,
      scrollMargin: 80,
      // 链接：按住 Cmd / Ctrl 点才打开（直接点是在里面改字）；站内的页面就地打开
      handleClick: (view, _pos, event) => {
        const a = (event.target as HTMLElement).closest('a')
        if (!a || (view.editable && !(event.metaKey || event.ctrlKey))) return false
        event.preventDefault()
        const href = a.getAttribute('href') || ''
        const page = href.match(/^\/pages\/(\d+)$/)
        if (page) window.dispatchEvent(new CustomEvent('oad-open-page', { detail: Number(page[1]) }))
        else if (safeHref(href)) window.open(href, '_blank', 'noopener,noreferrer')
        return true
      },
      // 粘贴、拖进来的图片：传上去插进页面（拖的插在松手的位置）
      handlePaste: (_view, e) => {
        const files = Array.from(e.clipboardData?.files || []).filter(f => f.type.startsWith('image/'))
        if (!files.length || !edRef.current) return false
        e.preventDefault()
        insertImages(edRef.current, files)
        return true
      },
      handleDrop: (view, e, _slice, moved) => {
        const files = Array.from((e as DragEvent).dataTransfer?.files || []).filter(f => f.type.startsWith('image/'))
        if (moved || !files.length || !edRef.current) return false
        e.preventDefault()
        insertImages(edRef.current, files, view.posAtCoords({ left: (e as DragEvent).clientX, top: (e as DragEvent).clientY })?.pos)
        return true
      },
      handleKeyDown: (_view, e) => {
        if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); sync.current?.save(); return true }   // 顺手按了保存：马上存，也不弹浏览器的另存为
        if (e.altKey && e.shiftKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown') && edRef.current) {   // 整段上移、下移
          e.preventDefault()
          moveBlock(edRef.current, e.key === 'ArrowUp' ? -1 : 1)
          return true
        }
        return false
      },
    },
  }, [id])
  const edRef = useRef(editor)
  edRef.current = editor

  useEffect(() => {
    if (!editor || editor.isDestroyed) return
    const s = new DocSync(id, editor)
    sync.current = s
    ;(editor.storage as any).slashCommands.items = () => slashItems(() => setImageBox(true))
    s.onChange = () => { setStatus(s.status); setConflicts([...s.conflicts.values()]) }
    s.onNotice = () => notice(t('你正在改的一段被别人删掉了，你的修改留成了新的一段。'))
    let alive = true
    let editTimer: ReturnType<typeof setTimeout> | undefined
    ;(async () => {
      if (await DocSync.recover(id) === 'lost') notice(t('上次离开时没存上的修改和别人的修改冲突了，没能补存。'))
      const data: PageBlocks & { agent_id?: number | null } = await api(`/api/pages/${id}/blocks`)
      if (!alive) return
      s.load(data)
      setOwner(data.agent_id ?? null)
      setReady(true)
      loadComments()
    })().catch(e => alive && setErr(e.message))
    const onUpdate = ({ transaction }: any) => { if (!transaction.getMeta(REMOTE)) s.schedule() }
    const onPage = (e: Event) => {
      const d = (e as CustomEvent).detail
      if (d.page_id !== id) return
      if (d.by && d.by !== 'user') {
        setEditing(d.by)
        clearTimeout(editTimer)
        editTimer = setTimeout(() => setEditing(null), 15000)
      }
      s.remote(d.version)
    }
    const onResync = () => { s.remote(Number.MAX_SAFE_INTEGER); loadComments() }   // 断线重连：漏掉的改动补上
    const onComments = (e: Event) => { if ((e as CustomEvent).detail?.page_id === id) loadComments() }
    // 光标停在有评论的段上：面板里那串评论亮起来
    const onSelect = () => {
      const at = editor.state.selection.from
      const l = blocksOf(editor.state.doc).find(b => at >= b.pos && at <= b.pos + b.node.nodeSize)
      setFocus(l?.id && s.counts.has(l.id) ? l.id : null)
    }
    editor.on('selectionUpdate', onSelect)
    window.addEventListener('oad-page-comments', onComments)
    const onHide = () => { if (document.visibilityState === 'hidden') s.save() }
    editor.on('update', onUpdate)
    window.addEventListener('oad-page', onPage)
    window.addEventListener('oad-page-resync', onResync)
    document.addEventListener('visibilitychange', onHide)
    window.addEventListener('pagehide', s.flush)
    return () => {
      alive = false
      clearTimeout(editTimer)
      editor.off('update', onUpdate)
      window.removeEventListener('oad-page', onPage)
      window.removeEventListener('oad-page-resync', onResync)
      window.removeEventListener('oad-page-comments', onComments)
      editor.off('selectionUpdate', onSelect)
      document.removeEventListener('visibilitychange', onHide)
      window.removeEventListener('pagehide', s.flush)
      s.destroy()
    }
  }, [editor, id])

  useEffect(() => { if (connected) sync.current?.retryNow() }, [connected])
  useEffect(() => {   // 每段有几串没解决的评论：画在作者条旁边
    const s = sync.current
    if (!s || !ready) return
    s.counts = countOpen(comments)
    s.paint()
  }, [comments, ready])

  // 选中文字点“评论”：挂在选区所在的那一段，选中的字作为引文
  const startComment = () => {
    if (!editor) return
    const { from, to } = editor.state.selection
    const l = blocksOf(editor.state.doc).find(b => from >= b.pos && from <= b.pos + b.node.nodeSize)
    const known = l?.id && sync.current?.base.has(l.id) ? l.id : null   // 还没存上的新段先挂整页
    setDraft({ block: known, quote: editor.state.doc.textBetween(from, to, ' ').slice(0, 200) })
    setSide('comments')
  }
  const jumpTo = (bid: string) => {
    document.querySelector(`.doc-body [data-bid="${bid}"]`)?.scrollIntoView({ block: 'center', behavior: 'smooth' })
    sync.current?.paint([bid])
    setFocus(bid)
  }
  const open = comments.filter(c => !c.parent_id && !c.resolved).length

  // 版本历史：恢复一段（改过的换回旧内容，删掉的用原来的编号插回去）、回到某一版
  const restoreBlock = async (r: Row) => {
    const s = sync.current
    if (!s || (r.kind !== 'change' && r.kind !== 'del')) return
    const ops = r.kind === 'change' ? [{ op: 'replace', block: r.id, markdown: r.old }]
      : [{ op: 'insert', ...(r.after && s.base.has(r.after) ? { after: r.after } : { end: true }), new_id: r.id, markdown: r.md }]
    try {
      await s.push(ops)
      setStamp(x => x + 1)
    } catch (e: any) {
      notice(e?.status === 409 ? t('这一段刚又被改过，先看看现在的内容再决定。') : e.message)
    }
  }
  const restorePage = async (v: Version) => {
    if (!(await ask(t('回到版本 {n}？现在的内容会作为新的一版留在历史里，随时能再回来。', { n: v.version }), { ok: t('回到这一版') }))) return
    try {
      await sync.current?.restore(v.id)
      setStamp(x => x + 1)
      setViewing(null)
    } catch (e: any) { notice(e.message) }
  }

  const STATUS: Record<Status, string> = {
    saved: t('已保存'), saving: t('保存中…'), offline: t('离线，连上以后自动保存'), conflict: t('有冲突'), error: t('保存失败'),
  }
  const jump = (bid: string) => document.querySelector(`.doc-body [data-bid="${bid}"]`)?.scrollIntoView({ block: 'center', behavior: 'smooth' })
  return (
    <div className={'doc' + (side ? ' with-comments' : '')}>
      <div className="doc-status muted small">
        <span className={'doc-dot ' + status} />{status === 'error' && sync.current?.error ? `${STATUS.error}：${sync.current.error}` : STATUS[status]}
        {editing && <span className="doc-editing"> · {t('{name} 正在编辑', { name: nameOf(editing) })}</span>}
        <span className="grow" />
        {ready && <button className={'doc-cbtn' + (side === 'history' ? ' on' : '')} title={t('历史')}
          onClick={() => { sync.current?.save(); setSide(side === 'history' ? null : 'history'); if (side === 'history') setViewing(null) }}>🕘 {t('历史')}</button>}
        {ready && <button className={'doc-cbtn' + (side === 'comments' ? ' on' : '')} onClick={() => { setSide(side === 'comments' ? null : 'comments'); setViewing(null) }} title={t('评论')}>
          💬 {open ? t('{n} 条评论', { n: open }) : t('评论')}</button>}
      </div>
      {conflicts.map(c => (
        <div className="doc-banner" key={c.id}>
          <span>{t('「{name}」也改了这一段，你的修改还没存：', { name: nameOf(c.by) })}</span>
          <button className="link-btn" onClick={() => jump(c.id)}>{t('看看是哪段')}</button>
          <button className="btn" onClick={() => sync.current?.keepMine(c.id)}>{t('用我的')}</button>
          <button className="btn" onClick={() => sync.current?.takeTheirs(c.id)}>{t('用 TA 的')}</button>
          <details className="doc-theirs"><summary>{t('TA 改成了')}</summary><Markdown text={c.md} /></details>
        </div>
      ))}
      {err && <div className="error-line">{err}</div>}
      {!ready && <div className="page-md"><Markdown text={fallback} /></div>}
      {/* 看历史时编辑器藏起来但不卸掉：没存的修改和撤销记录都还在 */}
      <div className="page-md doc-wrap" style={ready && !viewing ? undefined : { display: 'none' }}>
        <EditorContent editor={editor} />
      </div>
      {viewing && sync.current && <div className="page-md">
        <HistoryView key={viewing.v.id} pageId={id} version={viewing.v} list={viewing.list} current={sync.current.current()}
          onRestorePage={restorePage} onRestoreBlock={restoreBlock} onRestored={() => setStamp(x => x + 1)} onBack={() => setViewing(null)}
          onClose={() => { setViewing(null); if (isPhone()) setSide(null) }} />   {/* 手机上列表是整屏的：回到编辑就一起关掉 */}
      </div>}
      {side === 'history' && <HistoryPanel pageId={id} picked={viewing?.v || null} stamp={stamp} onPick={(v, list) => setViewing({ v, list })}
        onClose={() => { setSide(null); setViewing(null) }} />}
      {side === 'comments' && <CommentsPanel pageId={id} comments={comments} order={sync.current?.order || []} owner={owner}
        blockMd={b => sync.current?.base.get(b)?.md ?? null} authorOf={b => sync.current?.base.get(b)?.by || ''}
        draft={draft} setDraft={setDraft} focus={focus} onJump={jumpTo} onClose={() => { setSide(null); setDraft(null) }} reload={loadComments} />}
      {editor && !editor.isDestroyed && <>
        <SlashMenu editor={editor} />
        <Toolbar editor={editor} onComment={startComment} />
        {!viewing && <BlockHandle editor={editor} />}
        {imageBox && <ImageBox editor={editor} onClose={() => setImageBox(false)} />}
      </>}
    </div>
  )
}
