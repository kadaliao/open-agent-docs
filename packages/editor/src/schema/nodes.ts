// 自己写的节点，以及改掉了默认属性的 Tiptap 节点 / mark。属性名和取值跟 crates/oad-markdown/src/node.rs 一字不差，
// Tiptap 默认多出来的属性（链接的 target/rel/class、图片的宽高、有序列表的 type、表格格子的 colspan 等）都去掉：
// 每个属性都会进共享的 Yjs 文档，服务器照着读。
import { Node, mergeAttributes, nodeInputRule, InputRule } from '@tiptap/core'
import { Details as TiptapDetails, DetailsContent as TiptapDetailsContent, DetailsSummary } from '@tiptap/extension-details'
import TiptapImage from '@tiptap/extension-image'
import TiptapLink from '@tiptap/extension-link'
import { OrderedList as TiptapOrderedList } from '@tiptap/extension-list'
import { calloutView, detailsView, mathBlockView, mathInlineView } from '../views/node-views'
import { codeBlockView } from '../views/code'
import { embedView } from '../views/embed'
import { imageView } from '../views/image'
import { linkMentionView } from '../views/link-mention'
import TiptapCodeBlock from '@tiptap/extension-code-block'

export const CALLOUT_KINDS = ['note', 'tip', 'important', 'warning', 'caution'] as const
export type CalloutKind = (typeof CALLOUT_KINDS)[number]

const str = (v: string | null) => (v === '' ? null : v)

export const Link = TiptapLink.extend({
  addAttributes() {
    return {
      href: { default: null, parseHTML: (el) => el.getAttribute('href') },
      title: { default: null, parseHTML: (el) => str(el.getAttribute('title')) },
    }
  },
}).configure({
  openOnClick: false,
  autolink: true,
  linkOnPaste: true,
  defaultProtocol: 'https',
  HTMLAttributes: { target: '_blank', rel: 'noopener noreferrer nofollow' },
})

export const Image = TiptapImage.extend({
  addAttributes() {
    return {
      src: { default: null },
      alt: { default: null, parseHTML: (el) => str(el.getAttribute('alt')) },
      title: { default: null, parseHTML: (el) => str(el.getAttribute('title')) },
      // 展示属性（Markdown 里没有）：占正文宽度的百分比 10–100，不写 = 原始大小但不超过正文宽；图注
      width: {
        default: null,
        parseHTML: (el) => {
          const w = parseInt(el.getAttribute('data-width') || '', 10)
          return w >= 10 && w <= 100 ? w : null
        },
        renderHTML: (a) => (a.width ? { 'data-width': a.width } : {}),
      },
      caption: {
        default: null,
        parseHTML: (el) => str(el.getAttribute('data-caption')),
        renderHTML: (a) => (a.caption ? { 'data-caption': a.caption } : {}),
      },
    }
  },
  renderHTML({ HTMLAttributes }) {
    return ['img', mergeAttributes(HTMLAttributes, { loading: 'lazy', decoding: 'async', draggable: 'false' })]
  },
  addNodeView() {
    return imageView
  },
}).configure({ inline: false, allowBase64: false })

export const OrderedList = TiptapOrderedList.extend({
  addAttributes() {
    return {
      start: {
        default: 1,
        parseHTML: (el) => (el.hasAttribute('start') ? parseInt(el.getAttribute('start') || '1', 10) : 1),
        renderHTML: (attrs) => (attrs.start !== 1 ? { start: attrs.start } : {}),
      },
    }
  },
})

export const CodeBlock = TiptapCodeBlock.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      // 展示属性：自动换行
      wrap: {
        default: false,
        parseHTML: (el) => el.hasAttribute('data-wrap'),
        renderHTML: (a) => (a.wrap ? { 'data-wrap': '' } : {}),
      },
    }
  },
  addNodeView() {
    return codeBlockView
  },
}).configure({ defaultLanguage: null, enableTabIndentation: true, tabSize: 2 })

// 折叠块：open 存进文档（Tiptap 的 persist 模式）。换掉自带的节点视图，它切换时会把 id 冲掉
export const Details = TiptapDetails.extend({
  addAttributes() {
    return {
      open: {
        default: false,
        parseHTML: (el) => el.hasAttribute('open'),
        renderHTML: ({ open }) => (open ? { open: '' } : {}),
      },
    }
  },
  addNodeView() {
    return detailsView
  },
}).configure({ persist: true, HTMLAttributes: { class: 'oad-details' } })

// 自带的内容区节点视图靠 hidden 属性和事件切换，换成纯 CSS（见上）
export const DetailsContent = TiptapDetailsContent.extend({
  addNodeView() {
    return null
  },
})

export { DetailsSummary }

export const Callout = Node.create({
  name: 'callout',
  group: 'block',
  content: 'block+',
  defining: true,

  addAttributes() {
    return {
      kind: {
        default: 'note',
        parseHTML: (el) => el.getAttribute('data-kind') || 'note',
        renderHTML: (a) => ({ 'data-kind': a.kind }),
      },
      title: {
        default: null,
        parseHTML: (el) => str(el.getAttribute('data-title')),
        renderHTML: (a) => (a.title ? { 'data-title': a.title } : {}),
      },
    }
  },
  parseHTML() {
    return [{ tag: 'div[data-type="callout"]' }]
  },
  renderHTML({ HTMLAttributes }) {
    return ['div', mergeAttributes(HTMLAttributes, { 'data-type': 'callout', class: 'oad-callout' }), 0]
  },
  addNodeView() {
    return calloutView
  },
  addInputRules() {
    // 在引用开头打 [!tip] 加空格，引用变成提示框
    return [
      new InputRule({
        find: /^\[!(note|tip|important|warning|caution)\]\s$/i,
        handler: ({ state, range, match }) => {
          const $from = state.doc.resolve(range.from)
          if ($from.depth < 2) return null
          const quote = $from.node(-1)
          if (quote.type.name !== 'blockquote' || $from.index(-1) !== 0) return null
          const pos = $from.before(-1)
          state.tr.delete(range.from, range.to).setNodeMarkup(pos, this.type, {
            id: quote.attrs.id,
            kind: match[1].toLowerCase(),
            title: null,
          })
        },
      }),
    ]
  },
})

export const Embed = Node.create({
  name: 'embed',
  group: 'block',
  atom: true,
  draggable: true,
  selectable: true,

  addAttributes() {
    return {
      url: { default: null, parseHTML: (el) => el.getAttribute('data-url') },
      // 展示属性：embed = 按网站画富卡片 / 播放器，bookmark = 书签卡片
      view: {
        default: 'embed',
        parseHTML: (el) => (el.getAttribute('data-view') === 'bookmark' ? 'bookmark' : 'embed'),
        renderHTML: (a) => ({ 'data-view': a.view }),
      },
    }
  },
  parseHTML() {
    return [{ tag: 'div[data-type="embed"]' }]
  },
  renderHTML({ node, HTMLAttributes }) {
    const url = node.attrs.url || ''
    return [
      'div',
      mergeAttributes(HTMLAttributes, { 'data-type': 'embed', 'data-url': url, class: 'oad-embed' }),
      ['a', { href: url, target: '_blank', rel: 'noopener noreferrer nofollow' }, url],
    ]
  },
  addNodeView() {
    return embedView
  },
})

// 行内提及：网站图标 + 标题的小块。Markdown 写成 <url>
export const LinkMention = Node.create({
  name: 'linkMention',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  draggable: true,

  addAttributes() {
    return {
      url: { default: null, parseHTML: (el) => el.getAttribute('data-url') ?? el.getAttribute('href') },
    }
  },
  parseHTML() {
    return [{ tag: 'a[data-type="linkMention"]' }]
  },
  renderHTML({ node, HTMLAttributes }) {
    const url = node.attrs.url || ''
    return [
      'a',
      mergeAttributes(HTMLAttributes, { 'data-type': 'linkMention', 'data-url': url, href: url, class: 'oad-link-mention', target: '_blank', rel: 'noopener noreferrer nofollow' }),
      url,
    ]
  },
  addNodeView() {
    return linkMentionView
  },
})

export const MathBlock = Node.create({
  name: 'mathBlock',
  group: 'block',
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      latex: { default: '', parseHTML: (el) => el.getAttribute('data-latex') ?? el.textContent ?? '' },
    }
  },
  parseHTML() {
    return [{ tag: 'div[data-type="mathBlock"]' }]
  },
  renderHTML({ node, HTMLAttributes }) {
    return [
      'div',
      mergeAttributes(HTMLAttributes, { 'data-type': 'mathBlock', 'data-latex': node.attrs.latex, class: 'oad-math-block' }),
      node.attrs.latex,
    ]
  },
  addNodeView() {
    return mathBlockView
  },
  addInputRules() {
    // 空段落里打 $$ 加空格
    return [nodeInputRule({ find: /^\$\$\s$/, type: this.type, getAttributes: () => ({ latex: '' }) })]
  },
})

export const MathInline = Node.create({
  name: 'mathInline',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      latex: { default: '', parseHTML: (el) => el.getAttribute('data-latex') ?? el.textContent ?? '' },
    }
  },
  parseHTML() {
    return [{ tag: 'span[data-type="mathInline"]' }]
  },
  renderHTML({ node, HTMLAttributes }) {
    return [
      'span',
      mergeAttributes(HTMLAttributes, { 'data-type': 'mathInline', 'data-latex': node.attrs.latex, class: 'oad-math-inline' }),
      node.attrs.latex,
    ]
  },
  addNodeView() {
    return mathInlineView
  },
  addInputRules() {
    // $x^2$：两边的 $ 紧贴公式，前面不是 $（免得和 $$ 冲突）
    return [
      nodeInputRule({
        find: /(?:^|[^$\\])(\$([^\s$](?:[^$]*[^\s$])?)\$)$/,
        type: this.type,
        getAttributes: (m) => ({ latex: m[2] }),
      }),
    ]
  },
})

// 表格：不用 prosemirror-tables（它要求格子带 colspan/rowspan，Markdown 存不下）。增删行列见 tables.ts
export const Table = Node.create({
  name: 'table',
  group: 'block',
  content: 'tableRow+',
  isolating: true,
  parseHTML() {
    return [{ tag: 'table' }]
  },
  renderHTML({ HTMLAttributes }) {
    return ['div', { class: 'oad-table' }, ['table', HTMLAttributes, ['tbody', 0]]]
  },
})

export const TableRow = Node.create({
  name: 'tableRow',
  content: '(tableCell | tableHeader)+',
  parseHTML() {
    return [{ tag: 'tr' }]
  },
  renderHTML({ HTMLAttributes }) {
    return ['tr', HTMLAttributes, 0]
  },
})

export const TableHeader = Node.create({
  name: 'tableHeader',
  content: 'block+',
  isolating: true,
  parseHTML() {
    return [{ tag: 'th' }]
  },
  renderHTML({ HTMLAttributes }) {
    return ['th', HTMLAttributes, 0]
  },
})

export const TableCell = Node.create({
  name: 'tableCell',
  content: 'block+',
  isolating: true,
  parseHTML() {
    return [{ tag: 'td' }]
  },
  renderHTML({ HTMLAttributes }) {
    return ['td', HTMLAttributes, 0]
  },
})
