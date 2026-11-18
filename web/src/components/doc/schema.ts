// 编辑器用到的扩展：只开 Markdown 存得下的格式（标题、列表、待办、表格、代码、引用、分割线、链接、图片、提示框、折叠块）。
// 不开下划线、文字颜色、高亮、分栏、合并单元格：存成 Markdown 会丢。
import type { AnyExtension } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'
import { TaskItem, TaskList } from '@tiptap/extension-list'
import { TableCell, TableHeader, TableKit } from '@tiptap/extension-table'
import Image from '@tiptap/extension-image'
import { DetailsContent, DetailsSummary } from '@tiptap/extension-details'
import { Placeholder } from '@tiptap/extensions'
import { Markdown } from '@tiptap/markdown'
import { t } from '../../i18n'
import { BlockIds } from './blockIds'
import { Gutter, type Who } from './gutter'
import { Callout, Details, RawBlock, SafeCodeBlock, SoftBreak } from './nodes'
import { SlashCommands } from './menus'

export function extensions(opts: { who: (by: string) => Who; when: (ts: number) => string }): AnyExtension[] {
  return [
    StarterKit.configure({
      underline: false,
      codeBlock: false,
      hardBreak: false,
      heading: { levels: [1, 2, 3, 4, 5, 6] },   // Agent 写的四到六级标题也认得；菜单里只给一到三级
      link: { openOnClick: false, autolink: true, linkOnPaste: true, protocols: ['mailto'] },
    }),
    SafeCodeBlock,
    SoftBreak,
    TaskList,
    TaskItem.configure({ nested: true }),
    // 表格：格子里只放一段文字（Markdown 的表格放不下列表、代码块），不能拖宽、不能合并单元格
    TableKit.configure({ table: { resizable: false }, tableCell: false, tableHeader: false }),
    TableCell.extend({ content: 'paragraph' }),
    TableHeader.extend({ content: 'paragraph' }),
    Image.configure({ inline: true }),
    Callout,
    Details,
    DetailsSummary,
    DetailsContent,
    RawBlock,
    Placeholder.configure({
      placeholder: ({ node, pos, editor }) => (node.type.name === 'paragraph' && editor.state.doc.resolve(pos).depth === 0 ? t('输入 / 插入标题、列表、表格……') : ''),
      showOnlyCurrent: true,
    }),
    Markdown,
    BlockIds,
    Gutter.configure(opts),
    SlashCommands,
  ]
}
