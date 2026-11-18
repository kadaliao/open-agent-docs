// 整份文档 schema 用到的扩展（不含协作、菜单这些界面部分），别的应用和测试可以直接拿去建 schema。
import { getSchema, type AnyExtension } from '@tiptap/core'
import Blockquote from '@tiptap/extension-blockquote'
import Bold from '@tiptap/extension-bold'
import Code from '@tiptap/extension-code'
import Document from '@tiptap/extension-document'
import HardBreak from '@tiptap/extension-hard-break'
import Heading from '@tiptap/extension-heading'
import Highlight from '@tiptap/extension-highlight'
import HorizontalRule from '@tiptap/extension-horizontal-rule'
import Italic from '@tiptap/extension-italic'
import { BulletList, ListItem, TaskItem, TaskList } from '@tiptap/extension-list'
import Paragraph from '@tiptap/extension-paragraph'
import Strike from '@tiptap/extension-strike'
import Text from '@tiptap/extension-text'
import Underline from '@tiptap/extension-underline'
import { BlockId } from './block-id'
import {
  Callout,
  CodeBlock,
  Details,
  DetailsContent,
  DetailsSummary,
  Embed,
  Image,
  Link,
  MathBlock,
  MathInline,
  OrderedList,
  Table,
  TableCell,
  TableHeader,
  TableRow,
} from './nodes'
import { TableCommands } from './tables'
import { markRule } from './mark-rules'

export * from './ids'
export { BlockId, blockIdKey } from './block-id'
export { CALLOUT_KINDS, type CalloutKind } from './nodes'
export { findCell } from './tables'

export const NODE_NAMES = [
  'doc',
  'paragraph',
  'text',
  'heading',
  'bulletList',
  'orderedList',
  'listItem',
  'taskList',
  'taskItem',
  'blockquote',
  'callout',
  'codeBlock',
  'mathBlock',
  'horizontalRule',
  'image',
  'embed',
  'table',
  'tableRow',
  'tableHeader',
  'tableCell',
  'details',
  'detailsSummary',
  'detailsContent',
  'hardBreak',
  'mathInline',
] as const

export const MARK_NAMES = ['bold', 'italic', 'strike', 'code', 'link', 'underline', 'highlight'] as const

export function schemaExtensions(): AnyExtension[] {
  return [
    Document,
    Paragraph,
    Text,
    Heading.configure({ levels: [1, 2, 3, 4, 5, 6] }),
    Blockquote,
    BulletList,
    OrderedList,
    ListItem,
    TaskList,
    TaskItem.configure({ nested: true }),
    CodeBlock,
    HorizontalRule,
    HardBreak,
    Image,
    Embed,
    Callout,
    MathBlock,
    MathInline,
    Table,
    TableRow,
    TableHeader,
    TableCell,
    TableCommands,
    Details,
    DetailsSummary,
    DetailsContent,
    // mark 的顺序 = 嵌套顺序，和 oad-markdown 的 mark_rank 一致：链接最外，行内代码最里
    Link,
    Bold.extend({
      addInputRules() {
        return [markRule('**', () => this.type)]
      },
    }),
    Italic.extend({
      addInputRules() {
        return [markRule('*', () => this.type)]
      },
    }),
    Strike.extend({
      addInputRules() {
        return [markRule('~~', () => this.type)]
      },
    }),
    Underline,
    Highlight.extend({
      addInputRules() {
        return [markRule('==', () => this.type)]
      },
    }),
    // Tiptap 的行内代码排斥所有别的 mark，但 oad-markdown 允许“链接 + 代码”；改成只排斥自己（空串会让 y-prosemirror 当成可重叠 mark，名字后面加哈希），免得服务器写进来的内容校验不过
    Code.extend({
      excludes: 'code',
      addInputRules() {
        return [markRule('`', () => this.type)]
      },
    }),
    BlockId,
  ]
}

export const getOadSchema = () => getSchema(schemaExtensions())
