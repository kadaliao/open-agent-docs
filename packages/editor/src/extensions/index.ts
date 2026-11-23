// 编辑器完整的扩展：schema + 协作（Yjs 同步、别人的光标）+ 输入体验（斜杠菜单、提示文字、拖放光标）。
import type { AnyExtension } from '@tiptap/core'
import { ListKeymap } from '@tiptap/extension-list'
import { Dropcursor, Gapcursor } from '@tiptap/extensions'
import type * as Y from 'yjs'
import type { Awareness } from 'y-protocols/awareness'
import { schemaExtensions } from '../schema'
import { t } from '../strings'
import { AgentPresence } from './agent-presence'
import { BlockAuthor, type AuthorInfo } from './block-author'
import { Comments } from './comments'
import { LinkOpen } from './link-open'
import { OadCaret, OadCollaboration } from './collaboration'
import { BlockHandle } from './block-handle'
import { CodeHighlight } from './code-highlight'
import { FlashBlock, LiveBlock } from './live-block'
import { MediaServicesExt, type MediaServices } from './media'
import { Paste } from './paste'
import { PortalRootExt } from './portal'
import { OadPlaceholder } from './placeholder'
import { NotionKeys } from './keys'
import { SlashCommands } from './slash'

export interface OadUser {
  id: string
  name: string
  color: string
  kind?: 'user' | 'agent'
}

export interface EditorExtensionOptions {
  ydoc: Y.Doc
  field?: string
  awareness?: Awareness | null
  user?: OadUser
  // 评论：点了正文里有评论的文字
  onOpenThreads?: (ids: string[]) => void
  // 块作者：client id → 人 / Agent
  authors?: () => Record<string, AuthorInfo>
  onUnknownAuthor?: (clientId: string) => void
  media?: () => Partial<MediaServices>
  // 大图预览这些挂在哪（Shadow DOM 里给 shadow 里的一个元素）；不给就是 document.body，见 portal.ts
  portalRoot?: () => HTMLElement | null
  // 打开正文里的链接：返回 true 表示宿主自己处理了（比如本工作区的页面在应用里打开），否则在新标签页打开
  openLink?: (href: string) => boolean
}

function caret(user: { name: string; color: string; kind?: string }) {
  const el = document.createElement('span')
  el.className = 'oad-caret'
  el.style.setProperty('--oad-caret-color', user.color)
  const label = document.createElement('span')
  label.className = 'oad-caret-label'
  label.textContent = user.name
  if (user.kind === 'agent') el.classList.add('oad-is-agent')
  el.append(label)
  return el
}

export function editorExtensions(o: EditorExtensionOptions): AnyExtension[] {
  const ext: AnyExtension[] = [
    ...schemaExtensions(),
    ListKeymap,
    Dropcursor.configure({ width: 2, class: 'oad-dropcursor' }),
    Gapcursor,
    OadPlaceholder.configure({ empty: () => t('emptyPlaceholder'), line: () => t('placeholder') }),
    SlashCommands,
    NotionKeys,
    LiveBlock,
    FlashBlock,
    MediaServicesExt.configure({ get: () => o.media?.() ?? {} }),
    PortalRootExt.configure({ get: () => o.portalRoot?.() ?? null }),
    LinkOpen.configure({ open: (href) => o.openLink?.(href) ?? false }),
    CodeHighlight,
    Paste,
    BlockHandle,
    Comments.configure({ onOpen: (ids) => o.onOpenThreads?.(ids) }),
    BlockAuthor.configure({
      authors: () => o.authors?.() ?? {},
      onUnknown: (c) => o.onUnknownAuthor?.(c),
      label: (a) => (a.kind === 'agent' ? '✦ ' : '') + a.name,
    }),
    OadCollaboration.configure({ fragment: o.ydoc.getXmlFragment(o.field ?? 'default') }),
  ]
  if (o.awareness) {
    ext.push(
      OadCaret.configure({
        awareness: o.awareness,
        user: o.user ? { kind: 'user', ...o.user } : null,
        render: caret,
        selection: (u) => ({
          class: 'oad-remote-selection',
          style: `background-color: color-mix(in srgb, ${u.color} 22%, transparent)`,
        }),
      }),
      AgentPresence.configure({ awareness: o.awareness }),
    )
  }
  return ext
}
