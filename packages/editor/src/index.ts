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
export { strings, setEditorStrings, zh as editorStringsZh, type EditorStrings } from './strings'
export { loadKatex } from './views/katex'
export type { Editor } from '@tiptap/core'
