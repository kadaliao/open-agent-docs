// 右侧评论栏：未解决 / 已解决两组，按在正文里的位置排；点一串高亮正文里那段并滚过去；回复、编辑、删除、解决、重新打开。
import { useLocale } from '../i18n'
import type { Editor } from '@tiptap/core'
import { scrollToThread, threadRange, type CommentAnchor } from '../extensions/comments'
import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { threadAnchor, type Comment, type EventsLike, type Me, type OadClient, type Thread } from '@oad/client/core'
import { OadProvider, useOadClient } from '../react/context'
import { ago, colorFor, initials } from '../react/util'
import { t } from './strings'
import '../react/base.css'
import './comments.css'
import { renderBody } from './body'
import { Composer, type Candidate } from './Composer'

/** 一页的评论：先拉一次，之后跟着工作区 SSE 的评论事件重新拉 */
export function usePageComments(client: OadClient, pageId: string, events?: EventsLike | null) {
  const [threads, setThreads] = useState<Thread[]>([])
  const [loaded, setLoaded] = useState(false)
  const timer = useRef(0)
  const reload = useCallback(() => {
    client.threads(pageId, 'all').then(
      (ts) => {
        setThreads(ts)
        setLoaded(true)
      },
      () => setLoaded(true),
    )
  }, [client, pageId])
  useEffect(() => {
    reload()
    const later = (d: any) => {
      const pid = d?.page_id ?? d?.thread?.page_id ?? d?.comment?.page_id
      if (pid && pid !== pageId) return
      clearTimeout(timer.current)
      timer.current = window.setTimeout(reload, 60)
    }
    const offs = ['comment.created', 'comment.updated', 'comment.deleted', 'thread.updated', 'resync'].map((e) => events?.on(e, later) ?? (() => {}))
    return () => {
      offs.forEach((f) => f())
      clearTimeout(timer.current)
    }
  }, [pageId, events, reload])
  return { threads, setThreads, reload, loaded }
}

interface Props {
  /** 不给就用 <OadProvider> 的 */
  client?: OadClient
  pageId: string
  me: Me
  editor: Editor | null
  threads: Thread[]
  reload: () => void
  active: string | null
  setActive: (id: string | null) => void
  draft: CommentAnchor | null
  setDraft: (a: CommentAnchor | null) => void
  candidates: Candidate[]
  nameOf: (kind: string, id: string) => string
  canWrite: boolean
  onClose: () => void
  docVersion: number
}

export function CommentsPanel(p: Props) {
  const client = useOadClient(p.client)
  return (
    <OadProvider client={client}>
      <Panel {...p} />
    </OadProvider>
  )
}

function Panel(p: Props) {
  const client = useOadClient()
  const [tab, setTab] = useState<'open' | 'resolved'>('open')
  const open = useMemo(() => p.threads.filter((x) => !x.resolved_at), [p.threads])
  const done = useMemo(() => p.threads.filter((x) => x.resolved_at), [p.threads])

  // 打开的那串是已解决的，切到已解决
  useEffect(() => {
    const th = p.threads.find((x) => x.id === p.active)
    if (th) setTab(th.resolved_at ? 'resolved' : 'open')
  }, [p.active, p.threads])

  // 按在正文里的位置排；找不到位置的（原文删了、整页评论）放最后
  const ordered = useMemo(() => {
    const list = tab === 'open' ? open : done
    if (!p.editor) return list
    const pos = (x: Thread) => threadRange(p.editor!, x.id)?.from ?? Number.MAX_SAFE_INTEGER
    return [...list].sort((a, b) => pos(a) - pos(b) || a.created_at - b.created_at)
  }, [tab, open, done, p.editor, p.docVersion]) // eslint-disable-line react-hooks/exhaustive-deps

  const list = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!p.active) return
    list.current?.querySelector(`[data-thread-id="${p.active}"]`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  }, [p.active, tab])

  const create = async (body: string) => {
    const th = await client.createThread(p.pageId, { anchor: p.draft, body })
    p.setDraft(null)
    p.reload()
    if (th?.id) p.setActive(th.id)
  }

  return (
    <aside className="comments oad-root" aria-label={t('comments')}>
      <div className="comments-head">
        <div className="seg" role="tablist">
          <button role="tab" aria-selected={tab === 'open'} onClick={() => setTab('open')}>
            {t('commentsOpen')}
            {open.length > 0 && <span className="seg-count">{open.length}</span>}
          </button>
          <button role="tab" aria-selected={tab === 'resolved'} onClick={() => setTab('resolved')}>
            {t('commentsResolved')}
            {done.length > 0 && <span className="seg-count">{done.length}</span>}
          </button>
        </div>
        <button className="icon-btn" onClick={p.onClose} aria-label={t('closePanel')} title={t('closePanel')}>
          ✕
        </button>
      </div>
      <div className="comments-list" ref={list}>
        {p.draft && (
          <div className="thread is-active is-draft">
            <div className="thread-quote">{p.draft.quote}</div>
            <Composer candidates={p.candidates} autoFocus onSubmit={create} onCancel={() => p.setDraft(null)} />
          </div>
        )}
        {ordered.length === 0 && !p.draft && <div className="comments-empty">{tab === 'open' ? t('noComments') : t('noResolved')}</div>}
        {ordered.map((th) => (
          <ThreadCard
            key={th.id}
            thread={th}
            active={th.id === p.active}
            lost={!!threadAnchor(th).start && !!p.editor && !threadRange(p.editor, th.id)}
            me={p.me}
            candidates={p.candidates}
            nameOf={p.nameOf}
            canWrite={p.canWrite}
            reload={p.reload}
            onActivate={() => {
              p.setActive(th.id)
              if (p.editor) scrollToThread(p.editor, th.id)
            }}
          />
        ))}
      </div>
    </aside>
  )
}

interface CardProps {
  thread: Thread
  active: boolean
  lost: boolean
  me: Me
  candidates: Candidate[]
  nameOf: (kind: string, id: string) => string
  canWrite: boolean
  reload: () => void
  onActivate: () => void
}

const ThreadCard = memo(function ThreadCard({ thread, active, lost, me, candidates, nameOf, canWrite, reload, onActivate }: CardProps) {
  useLocale()
  const client = useOadClient()
  const a = threadAnchor(thread)
  const resolved = !!thread.resolved_at
  const comments = thread.comments ?? []
  const toggle = async (e: React.MouseEvent) => {
    e.stopPropagation()
    await client.resolveThread(thread.id, !resolved)
    reload()
  }
  return (
    <div className={'thread' + (active ? ' is-active' : '') + (resolved ? ' is-resolved' : '')} data-thread-id={thread.id} onClick={onActivate}>
      <div className="thread-top">
        {a.start ? (
          <div className={'thread-quote' + (lost ? ' is-lost' : '')} title={lost ? t('anchorLost') : undefined}>
            {a.quote || '…'}
          </div>
        ) : (
          <div className="thread-quote is-page">{t('pageComment')}</div>
        )}
        {canWrite && (
          <button className="thread-resolve" onClick={toggle} title={resolved ? t('reopen') : t('resolve')}>
            {resolved ? t('reopen') : '✓ ' + t('resolve')}
          </button>
        )}
      </div>
      {lost && <div className="thread-lost">{t('anchorLost')}</div>}
      {comments.map((c) => (
        <CommentItem key={c.id} c={c} me={me} candidates={candidates} nameOf={nameOf} reload={reload} />
      ))}
      {active && canWrite && !resolved && (
        <div onClick={(e) => e.stopPropagation()}>
          <Composer
            compact
            candidates={candidates}
            placeholder={t('replyPlaceholder')}
            submitLabel={t('reply')}
            onSubmit={async (body) => {
              await client.reply(thread.id, { body })
              reload()
            }}
          />
        </div>
      )}
    </div>
  )
})

function CommentItem({ c, me, candidates, nameOf, reload }: { c: Comment; me: Me; candidates: Candidate[]; nameOf: (k: string, id: string) => string; reload: () => void }) {
  const [editing, setEditing] = useState(false)
  const { kind, id } = c.author
  const name = c.author.name || nameOf(kind, id)
  const mine = kind === 'user' && id === me.user.id
  const client = useOadClient()
  if (c.deleted_at) return <div className="comment is-deleted">{t('commentDeleted')}</div>
  return (
    <div className="comment" data-comment-id={c.id}>
      <div className="comment-head">
        <span className={'avatar avatar-sm' + (kind === 'agent' ? ' is-agent' : '')} style={{ background: colorFor(id) }}>
          {kind === 'agent' ? '✦' : initials(name)}
        </span>
        <span className="comment-author">{name}</span>
        {kind === 'agent' && <span className="mention-tag">{t('agentTag')}</span>}
        <span className="comment-time" title={new Date(c.created_at).toLocaleString()}>
          {ago(c.created_at)}
          {c.edited_at ? ' · ' + t('edited') : ''}
        </span>
        {mine && !editing && (
          <span className="comment-actions" onClick={(e) => e.stopPropagation()}>
            <button onClick={() => setEditing(true)}>{t('edit')}</button>
            <button
              onClick={async () => {
                if (!window.confirm(t('deleteComment'))) return
                await client.deleteComment(c.id)
                reload()
              }}
            >
              {t('delete')}
            </button>
          </span>
        )}
      </div>
      {editing ? (
        <div onClick={(e) => e.stopPropagation()}>
          <Composer
            compact
            autoFocus
            initial={c.body}
            candidates={candidates}
            submitLabel={t('save')}
            onCancel={() => setEditing(false)}
            onSubmit={async (body) => {
              await client.editComment(c.id, body)
              setEditing(false)
              reload()
            }}
          />
        </div>
      ) : (
        <div className="comment-body" dangerouslySetInnerHTML={{ __html: renderBody(c.body) }} />
      )}
    </div>
  )
}
