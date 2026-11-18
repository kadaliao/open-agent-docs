// 斜杠菜单：段首或空格后打 /（中文输入法下段首打 、）弹出，选了插入对应的块。界面在 ui/SlashMenu.tsx。
import { Extension, type Editor, type Range } from '@tiptap/core'
import { PluginKey, TextSelection } from '@tiptap/pm/state'
import Suggestion from '@tiptap/suggestion'
import { t, type EditorStrings } from '../strings'
import { openMathAt } from '../views/node-views'

export interface SlashItem {
  key: string
  icon: string
  label: keyof EditorStrings
  hint: keyof EditorStrings
  // 搜索词：英文、拼音全拼和首字母
  kw: string
  run: (editor: Editor, ui: SlashUi) => void
}

export interface SlashUi {
  askUrl: (kind: 'image' | 'embed') => Promise<string | null>
}

export interface SlashState {
  items: SlashItem[]
  rect: DOMRect | null
  pick: (it: SlashItem) => void
}

export interface SlashStorage {
  show: (s: SlashState | null) => void
  key: (e: KeyboardEvent) => boolean
  ui: SlashUi
}

// 插入图片、卡片这种整块的东西以后，光标放到它后面的段落里（没有就补一个）
function insertAtom(e: Editor, node: { type: string; attrs: Record<string, unknown> }) {
  e.chain().focus().insertContent(node).run()
  const { state } = e
  const sel = state.selection
  let after = -1
  state.doc.nodesBetween(Math.max(0, sel.from - 2), Math.min(state.doc.content.size, sel.to + 2), (n, pos) => {
    if (after < 0 && n.type.name === node.type) after = pos + n.nodeSize
    return after < 0
  })
  if (after < 0) return
  const tr = state.tr
  const next = tr.doc.resolve(after).nodeAfter
  if (!next || !next.isTextblock) tr.insert(after, state.schema.nodes.paragraph.create())
  e.view.dispatch(tr.setSelection(TextSelection.create(tr.doc, after + 1)).scrollIntoView())
}

export const SLASH_ITEMS: SlashItem[] = [
  { key: 'paragraph', icon: 'T', label: 'block.paragraph', hint: 'blockHint.paragraph', kw: 'text paragraph zhengwen zw', run: (e) => e.chain().focus().setParagraph().run() },
  { key: 'h1', icon: 'H1', label: 'block.h1', hint: 'blockHint.h1', kw: 'h1 heading title biaoti bt', run: (e) => e.chain().focus().setHeading({ level: 1 }).run() },
  { key: 'h2', icon: 'H2', label: 'block.h2', hint: 'blockHint.h2', kw: 'h2 heading title biaoti bt', run: (e) => e.chain().focus().setHeading({ level: 2 }).run() },
  { key: 'h3', icon: 'H3', label: 'block.h3', hint: 'blockHint.h3', kw: 'h3 heading title biaoti bt', run: (e) => e.chain().focus().setHeading({ level: 3 }).run() },
  { key: 'bulletList', icon: '•', label: 'block.bulletList', hint: 'blockHint.bulletList', kw: 'bullet list ul wuxu liebiao wxlb', run: (e) => e.chain().focus().toggleBulletList().run() },
  { key: 'orderedList', icon: '1.', label: 'block.orderedList', hint: 'blockHint.orderedList', kw: 'ordered numbered list ol youxu liebiao yxlb', run: (e) => e.chain().focus().toggleOrderedList().run() },
  { key: 'taskList', icon: '☐', label: 'block.taskList', hint: 'blockHint.taskList', kw: 'todo task check daiban db', run: (e) => e.chain().focus().toggleTaskList().run() },
  { key: 'blockquote', icon: '❝', label: 'block.blockquote', hint: 'blockHint.blockquote', kw: 'quote blockquote yinyong yy', run: (e) => e.chain().focus().toggleBlockquote().run() },
  { key: 'callout', icon: 'i', label: 'block.callout', hint: 'blockHint.callout', kw: 'callout note tip alert tishi tsk', run: (e) => e.chain().focus().wrapIn('callout', { kind: 'note' }).run() },
  { key: 'details', icon: '▸', label: 'block.details', hint: 'blockHint.details', kw: 'toggle details fold collapse zhedie zdk', run: (e) => e.chain().focus().setDetails().updateAttributes('details', { open: true }).run() },
  { key: 'codeBlock', icon: '</>', label: 'block.codeBlock', hint: 'blockHint.codeBlock', kw: 'code pre daima dmk', run: (e) => e.chain().focus().setCodeBlock().run() },
  {
    key: 'mathBlock',
    icon: '∑',
    label: 'block.mathBlock',
    hint: 'blockHint.mathBlock',
    kw: 'math latex equation formula gongshi gs',
    run: (e) => {
      const from = e.state.selection.from
      e.chain().focus().insertContent({ type: 'mathBlock', attrs: { latex: '' } }).run()
      // 找到刚插进去的公式块，直接打开编辑
      let at = -1
      const doc = e.state.doc
      doc.nodesBetween(Math.max(0, from - 2), Math.min(doc.content.size, from + 2), (n, pos) => {
        if (at < 0 && n.type.name === 'mathBlock') at = pos
        return at < 0
      })
      if (at >= 0) openMathAt(e.view, at)
    },
  },
  { key: 'table', icon: '▦', label: 'block.table', hint: 'blockHint.table', kw: 'table grid biaoge bg', run: (e) => e.chain().focus().insertTable({ rows: 3, cols: 3 }).run() },
  { key: 'horizontalRule', icon: '—', label: 'block.horizontalRule', hint: 'blockHint.horizontalRule', kw: 'divider hr line rule fengexian fgx', run: (e) => e.chain().focus().setHorizontalRule().run() },
  {
    key: 'image',
    icon: '◰',
    label: 'block.image',
    hint: 'blockHint.image',
    kw: 'image picture photo img tupian tp',
    run: async (e, ui) => {
      const src = await ui.askUrl('image')
      if (src) insertAtom(e, { type: 'image', attrs: { src } })
      else e.commands.focus()
    },
  },
  {
    key: 'embed',
    icon: '↗',
    label: 'block.embed',
    hint: 'blockHint.embed',
    kw: 'embed link bookmark card url lianjie kapian ljkp',
    run: async (e, ui) => {
      const url = await ui.askUrl('embed')
      if (url) insertAtom(e, { type: 'embed', attrs: { url } })
      else e.commands.focus()
    },
  },
]

export function filterSlash(query: string): SlashItem[] {
  const q = query.trim().toLowerCase()
  if (!q) return SLASH_ITEMS
  return SLASH_ITEMS.filter((it) => t(it.label).toLowerCase().includes(q) || it.kw.split(' ').some((w) => w.startsWith(q)) || it.key.toLowerCase().startsWith(q))
}

export const SlashCommands = Extension.create<object, SlashStorage>({
  name: 'slashCommands',
  addStorage() {
    return {
      show: () => {},
      key: () => false,
      ui: { askUrl: async () => null },
    }
  },
  addProseMirrorPlugins() {
    const storage = this.storage
    const make = (char: string, name: string, startOfLine: boolean) =>
      Suggestion<SlashItem, SlashItem>({
        editor: this.editor,
        char,
        pluginKey: new PluginKey(name),
        allowSpaces: false,
        startOfLine,
        allow: ({ state, range }) => {
          const $p = state.doc.resolve(range.from)
          return $p.parent.type.name === 'paragraph'
        },
        items: ({ query }) => filterSlash(query),
        command: ({ editor, range, props }: { editor: Editor; range: Range; props: SlashItem }) => {
          editor.chain().focus().deleteRange(range).run()
          props.run(editor, storage.ui)
        },
        render: () => {
          let current: SlashState | null = null
          const show = (p: { items: SlashItem[]; clientRect?: (() => DOMRect | null) | null; command: (it: SlashItem) => void }) => {
            current = { items: p.items, rect: p.clientRect?.() ?? null, pick: p.command }
            storage.show(current)
          }
          return {
            onStart: show,
            onUpdate: show,
            onExit: () => {
              current = null
              storage.show(null)
            },
            onKeyDown: ({ event }) => {
              if (!current) return false
              if (event.key === 'Escape') {
                storage.show(null)
                current = null
                return true
              }
              return storage.key(event)
            },
          }
        },
      })
    return [make('/', 'slash', false), make('、', 'slashZh', true)]
  },
})
