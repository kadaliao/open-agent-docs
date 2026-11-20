// <oad-editor> 的界面（React）：标题 + 协作编辑器（+ 评论栏）。按需加载，元素本身在 editor.ts。
import type { Me, OadClient, Page, PageConn } from '@open-agent-docs/client/core'
import {
  OadComments,
  OadEditor,
  OadPortalRoot,
  OadProvider,
  OadTitle,
  colorFor,
  focusStart,
  useOadPage,
  usePageComments,
  usePeople,
  setEditorLocale,
  type CommentAnchor,
  type Editor,
  type Uploaded,
} from '@open-agent-docs/editor'
import { getLocale, onLocale } from '@open-agent-docs/editor/i18n'
// 编辑器的样式：@open-agent-docs/editor 的入口是“无副作用”的，它里面的 import './style.css' 打包时会被摇掉，这里显式引
// （打包时变成 addCss，进 Shadow DOM）
import '@open-agent-docs/editor/style.css'
import { useEffect, useMemo, useRef, useState } from 'react'
import { createRoot } from 'react-dom/client'
import type { ElementHost } from './base.ts'
import type { UrlResolver } from '@open-agent-docs/editor/resolve'

// 编辑器自己的文字（斜杠菜单、格式菜单）跟着界面语言：同样是编辑器入口里会被摇掉的那几行
setEditorLocale(getLocale())
onLocale(() => setEditorLocale(getLocale()))
import type { OadChangeDetail, OadReadyDetail } from './types.ts'

/** 画编辑器（<oad-editor> 的 start() 按需加载这个文件后调用） */
export function mountEditor(h: ElementHost, o: { pageId: string; opts: () => { comments: boolean; readonly: boolean }; setConn: (c: PageConn | null) => void }) {
  const root = createRoot(h.mount)
  const portal = document.createElement('div')
  portal.className = 'oad-el-portal'
  h.shadow.append(portal)
  const host: Host = {
    el: h.el,
    ready: (d) => h.emit('oad-ready', d),
    change: (d) => h.emit('oad-change', d),
    fail: (e) => h.fail(e),
    setConn: o.setConn,
  }
  const render = () => {
    const { comments, readonly } = o.opts()
    root.render(
      <OadPortalRoot.Provider value={portal}>
        <OadProvider client={h.client}>
          <EditorApp host={host} client={h.client} pageId={o.pageId} comments={comments} readonly={readonly} baseUrl={h.baseUrl} resolveUrl={h.resolveUrl} />
        </OadProvider>
      </OadPortalRoot.Provider>,
    )
  }
  render()
  return {
    rerender: render,
    stop: () => {
      root.unmount() // useOadPage 的清理里关掉 WebSocket、释放 Y.Doc
      portal.remove()
    },
  }
}

interface Host {
  el: HTMLElement
  ready: (d: OadReadyDetail) => void
  change: (d: OadChangeDetail) => void
  fail: (e: unknown) => void
  setConn: (c: PageConn | null) => void
}

function EditorApp(p: { host: Host; client: OadClient; pageId: string; comments: boolean; readonly: boolean; baseUrl: string; resolveUrl: UrlResolver | null }) {
  const [got, setGot] = useState<{ me: Me; page: Page } | null>(null)
  useEffect(() => {
    let alive = true
    Promise.all([p.client.me(), p.client.page(p.pageId)]).then(
      ([me, page]) => alive && setGot({ me, page }),
      (e) => alive && p.host.fail(e),
    )
    return () => {
      alive = false
    }
  }, [p.client, p.pageId]) // eslint-disable-line react-hooks/exhaustive-deps
  if (!got) return <div className="oad-el-loading" aria-busy="true" />
  return <Doc {...p} me={got.me} page={got.page} />
}

function Doc({ host, client, me, page, comments, readonly, baseUrl, resolveUrl }: { host: Host; client: OadClient; me: Me; page: Page; comments: boolean; readonly: boolean; baseUrl: string; resolveUrl: UrlResolver | null }) {
  const user = useMemo(() => ({ id: me.user.id, name: me.user.name, color: colorFor(me.user.id) }), [me])
  const doc = useOadPage(client, page.id, { user })
  const role = me.workspaces.find((w) => w.id === page.workspace_id)?.role
  const canWrite = !readonly && role !== 'viewer'
  const [editor, setEditor] = useState<Editor | null>(null)
  const title = useRef(page.title ?? '')
  // OadTitle 挂上时会先报一次现在的值（还没同步时是空的），那次不算“变了”
  const titleSeen = useRef(false)

  useEffect(() => {
    host.setConn(doc.conn)
    return () => host.setConn(null)
  }, [doc.conn]) // eslint-disable-line react-hooks/exhaustive-deps

  // 内容到了：oad-ready
  useEffect(() => {
    if (doc.ready) host.ready({ id: page.id, title: doc.ydoc?.getText('title').toString() || title.current })
  }, [doc.ready]) // eslint-disable-line react-hooks/exhaustive-deps

  // 服务器拒绝了（没有这一页的权限）
  useEffect(() => {
    if (doc.status === 'denied') host.fail(Object.assign(new Error('没有这一页的权限'), { name: 'ApiError', status: 403, code: 'forbidden' }))
  }, [doc.status]) // eslint-disable-line react-hooks/exhaustive-deps

  // 正文变了：合并 300 毫秒内的改动发一次 oad-change
  useEffect(() => {
    const ydoc = doc.ydoc
    if (!ydoc) return
    const frag = ydoc.getXmlFragment('default')
    let timer: ReturnType<typeof setTimeout> | null = null
    let local = false
    const obs = (_: unknown, tr: { local: boolean }) => {
      local ||= tr.local
      if (timer) return
      timer = setTimeout(() => {
        timer = null
        host.change({ id: page.id, kind: 'content', title: title.current, local })
        local = false
      }, 300)
    }
    frag.observeDeep(obs as never)
    return () => {
      frag.unobserveDeep(obs as never)
      if (timer) clearTimeout(timer)
    }
  }, [doc.ydoc]) // eslint-disable-line react-hooks/exhaustive-deps

  const media = useMemo(
    () => ({
      unfurl: (url: string) => client.unfurl(url) as never,
      upload: canWrite ? (file: File, onProgress: (f: number) => void) => client.upload(page.workspace_id, file, onProgress) as Promise<unknown> as Promise<Uploaded> : null,
      pageUrl: () => (baseUrl || location.origin) + '/p/' + encodeURIComponent(page.id),
      // 图片、文件：token 登录时换成签名地址、跨域时拼上服务器地址
      resolveUrl,
    }),
    [client, page.id, page.workspace_id, canWrite, baseUrl, resolveUrl],
  )

  if (!doc.ydoc) return <div className="oad-el-loading" aria-busy="true" />
  const titleEl = (
    <OadTitle
      ydoc={doc.ydoc}
      readOnly={!canWrite}
      placeholder="无标题"
      onEnter={() => editor && focusStart(editor)}
      onChange={(t) => {
        if (!titleSeen.current) {
          titleSeen.current = true
          if (t) title.current = t
          return
        }
        if (t === title.current) return
        const local = (host.el.shadowRoot?.activeElement as Element | null)?.classList?.contains('oad-title') ?? false
        title.current = t
        host.change({ id: page.id, kind: 'title', title: t, local })
      }}
    />
  )
  const editorProps = { ydoc: doc.ydoc, provider: doc.provider, user, editable: canWrite, onReady: setEditor, media }
  if (!comments)
    return (
      <article className="oad-el-doc" data-status={doc.status}>
        {titleEl}
        <OadEditor {...editorProps} />
      </article>
    )
  return <WithComments client={client} me={me} page={page} editor={doc.ready ? editor : null} canWrite={canWrite} titleEl={titleEl} editorProps={editorProps} />
}

// 带评论栏：格式菜单里有“评论”，右边是评论串
function WithComments(p: {
  client: OadClient
  me: Me
  page: Page
  editor: Editor | null
  canWrite: boolean
  titleEl: React.ReactElement
  editorProps: React.ComponentProps<typeof OadEditor>
}) {
  const ws = p.page.workspace_id
  const events = useMemo(() => p.client.events(ws), [p.client, ws])
  useEffect(() => () => events.close(), [events])
  const { threads, reload } = usePageComments(p.client, p.page.id, events)
  const people = usePeople(p.client, ws, p.me, {}, threads)
  const [draft, setDraft] = useState<CommentAnchor | null>(null)
  const [active, setActive] = useState<string | null>(null)
  return (
    <div className="oad-el-split">
      <article className="oad-el-doc">
        {p.titleEl}
        <OadEditor
          {...p.editorProps}
          onOpenThreads={(ids) => setActive(ids[0] ?? null)}
          onComment={
            p.canWrite
              ? (a) => {
                  setDraft(a)
                  setActive(null)
                }
              : undefined
          }
        />
      </article>
      <div className="oad-el-comments">
        <OadComments
          pageId={p.page.id}
          me={p.me}
          editor={p.editor}
          threads={threads}
          reload={reload}
          active={active}
          setActive={setActive}
          draft={draft}
          setDraft={setDraft}
          candidates={people.candidates}
          nameOf={people.nameOf}
          canWrite={p.canWrite}
          docVersion={0}
          onClose={() => setDraft(null)}
        />
      </div>
    </div>
  )
}
