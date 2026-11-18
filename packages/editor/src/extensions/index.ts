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
import { OadCaret, OadCollaboration } from './collaboration'
import { LiveBlock } from './live-block'
import { OadPlaceholder } from './placeholder'
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
}

function caret(user: { name: string; color: string; kind?: string }) {
  const el = document.createElement('span')
  el.className = 'oad-caret'
  el.style.setProperty('--caret-color', user.color)
  const label = document.createElement('span')
  label.className = 'oad-caret-label'
  label.textContent = user.name
  if (user.kind === 'agent') el.classList.add('is-agent')
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
    LiveBlock,
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
