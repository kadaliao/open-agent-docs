// 样式：打包时并进 dist/style.css（宿主 import '@open-agent-docs/editor/style.css'）
import './style.css'
export { OadEditor, focusStart, type OadEditorProps } from './OadEditor'
export { OadTitle, applyTitle, minimalDiff, type OadTitleHandle, type OadTitleProps } from './OadTitle'
export { editorExtensions, type EditorExtensionOptions, type OadUser } from './extensions'
export { SLASH_ITEMS, SlashCommands, filterSlash, type SlashItem } from './extensions/slash'
export { AgentPresence } from './extensions/agent-presence'
export { BlockAuthor, mainClient, type AuthorInfo } from './extensions/block-author'
export {
  Comments,
  scrollToThread,
  selectionToAnchor,
  setActiveThread,
  setCommentThreads,
  threadRange,
  type CommentAnchor,
  type CommentMark,
} from './extensions/comments'
export { OadCaret, OadCollaboration, isChangeOrigin } from './extensions/collaboration'
export * from './schema'
export { strings, setEditorStrings, setEditorLocale, zh as editorStringsZh, en as editorStringsEn, type EditorStrings } from './strings'
export { loadKatex } from './views/katex'
export type { Editor } from '@tiptap/core'
export type { MediaServices } from './extensions/media'
export type { Unfurl, Uploaded } from './media/types'
export { createUrlResolver, clientUrlResolver, isServerAsset, type UrlResolver, type UrlResolverOptions } from './media/resolve'
export { looksLikeMarkdown, isUrl, uploadFiles, convertEmbed } from './extensions/paste'
export { openLightbox } from './views/image'
export { flashBlock } from './extensions/live-block'
// 编辑器自己的文字跟着界面语言（@open-agent-docs/editor/i18n 的 setLocale；默认跟浏览器）
import { getLocale, onLocale } from './i18n'
import { setEditorLocale as setLoc } from './strings'
setLoc(getLocale())
onLocale(() => setLoc(getLocale()))

// 第 6 阶段：给别的宿主用的组件（要 @open-agent-docs/client）
export { OadProvider, OadPortalRoot, useOadClient } from './react/context'
export { useOadPage, type OadPage } from './react/useOadPage'
export { CommentsPanel, CommentsPanel as OadComments, usePageComments } from './comments/CommentsPanel'
export type { Candidate } from './comments/Composer'
export { bodyToDraft, draftToBody, mentionedAgents, renderBody } from './comments/body'
export { loadAgents, useAgents, usePeople } from './react/people'
export { OadDatabaseView, type OadDatabaseViewProps } from './react/OadDatabaseView'
export { colorFor, initials } from './react/util'
export { getLocale, setLocale, useLocale, onLocale, type Locale } from './i18n'
