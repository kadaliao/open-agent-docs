// 用到全部节点、属性、mark 的一份文档。所有属性都给了非 null 的值（null 的属性不进 Yjs，服务器那边读不到），
// 块编号固定，方便服务器那边的测试对照。
const id = (n: number) => 'blk' + String(n).padStart(5, '0')
let k = 0
const next = () => id(++k)

const text = (t: string, marks?: any[]) => (marks ? { type: 'text', text: t, marks } : { type: 'text', text: t })
const p = (...content: any[]) => ({ type: 'paragraph', attrs: { id: next() }, content })

export const allNodesTitle = '全部节点 All nodes'

export const allNodesDoc = {
  type: 'doc',
  content: [
    { type: 'heading', attrs: { id: next(), level: 1 }, content: [text('标题 Heading 1')] },
    { type: 'heading', attrs: { id: next(), level: 3 }, content: [text('三级标题')] },
    p(
      text('普通文字，'),
      text('粗体', [{ type: 'bold' }]),
      text('斜体', [{ type: 'italic' }]),
      text('删除线', [{ type: 'strike' }]),
      text('下划线', [{ type: 'underline' }]),
      text('高亮', [{ type: 'highlight' }]),
      text('code', [{ type: 'code' }]),
      text('链接', [{ type: 'link', attrs: { href: 'https://example.com/a?b=1', title: '示例' } }]),
      text('粗斜', [{ type: 'bold' }, { type: 'italic' }]),
      text('链接里的代码', [{ type: 'link', attrs: { href: 'https://example.com/code', title: 'c' } }, { type: 'code' }]),
      { type: 'hardBreak' },
      text('换行后，行内公式 '),
      { type: 'mathInline', attrs: { latex: 'e^{i\\pi}+1=0' } },
      text(' 结束 😀'),
    ),
    {
      type: 'bulletList',
      attrs: { id: next() },
      content: [
        { type: 'listItem', attrs: { id: next() }, content: [p(text('无序一'))] },
        {
          type: 'listItem',
          attrs: { id: next() },
          content: [
            p(text('无序二')),
            {
              type: 'bulletList',
              attrs: { id: next() },
              content: [{ type: 'listItem', attrs: { id: next() }, content: [p(text('嵌套'))] }],
            },
          ],
        },
      ],
    },
    {
      type: 'orderedList',
      attrs: { id: next(), start: 3 },
      content: [
        { type: 'listItem', attrs: { id: next() }, content: [p(text('第三'))] },
        { type: 'listItem', attrs: { id: next() }, content: [p(text('第四'))] },
      ],
    },
    {
      type: 'taskList',
      attrs: { id: next() },
      content: [
        { type: 'taskItem', attrs: { id: next(), checked: true }, content: [p(text('做完了'))] },
        {
          type: 'taskItem',
          attrs: { id: next(), checked: false },
          content: [
            p(text('还没做')),
            {
              type: 'taskList',
              attrs: { id: next() },
              content: [{ type: 'taskItem', attrs: { id: next(), checked: false }, content: [p(text('子任务'))] }],
            },
          ],
        },
      ],
    },
    { type: 'blockquote', attrs: { id: next() }, content: [p(text('引用一段话'))] },
    {
      type: 'callout',
      attrs: { id: next(), kind: 'warning', title: '小心' },
      content: [p(text('提示框里的内容')), { type: 'bulletList', attrs: { id: next() }, content: [{ type: 'listItem', attrs: { id: next() }, content: [p(text('列表'))] }] }],
    },
    { type: 'codeBlock', attrs: { id: next(), language: 'rust' }, content: [text('fn main() {\n    println!("你好");\n}')] },
    { type: 'mathBlock', attrs: { id: next(), latex: '\\int_0^1 x^2\\,dx = \\frac{1}{3}' } },
    { type: 'horizontalRule', attrs: { id: next() } },
    { type: 'image', attrs: { id: next(), src: 'https://example.com/cat.png', alt: '一只猫', title: '猫' } },
    { type: 'embed', attrs: { id: next(), url: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ' } },
    {
      type: 'table',
      attrs: { id: next() },
      content: [
        {
          type: 'tableRow',
          attrs: { id: next() },
          content: [
            { type: 'tableHeader', attrs: { id: next() }, content: [p(text('名称'))] },
            { type: 'tableHeader', attrs: { id: next() }, content: [p(text('数量'))] },
          ],
        },
        {
          type: 'tableRow',
          attrs: { id: next() },
          content: [
            { type: 'tableCell', attrs: { id: next() }, content: [p(text('苹果', [{ type: 'bold' }]))] },
            { type: 'tableCell', attrs: { id: next() }, content: [p(text('3'))] },
          ],
        },
      ],
    },
    {
      type: 'details',
      attrs: { id: next(), open: true },
      content: [
        { type: 'detailsSummary', attrs: { id: next() }, content: [text('点开看')] },
        { type: 'detailsContent', attrs: { id: next() }, content: [p(text('折叠的内容'))] },
      ],
    },
    p(text('最后一段')),
  ],
}

// 每种节点、mark 应有的属性（不多不少）
export const expectedNodeAttrs: Record<string, string[]> = {
  doc: [],
  text: [],
  hardBreak: [],
  mathInline: ['latex'],
  paragraph: ['id'],
  heading: ['id', 'level'],
  bulletList: ['id'],
  orderedList: ['id', 'start'],
  listItem: ['id'],
  taskList: ['id'],
  taskItem: ['checked', 'id'],
  blockquote: ['id'],
  callout: ['id', 'kind', 'title'],
  codeBlock: ['id', 'language'],
  mathBlock: ['id', 'latex'],
  horizontalRule: ['id'],
  image: ['alt', 'id', 'src', 'title'],
  embed: ['id', 'url'],
  table: ['id'],
  tableRow: ['id'],
  tableHeader: ['id'],
  tableCell: ['id'],
  details: ['id', 'open'],
  detailsSummary: ['id'],
  detailsContent: ['id'],
}

export const expectedMarkAttrs: Record<string, string[]> = {
  bold: [],
  italic: [],
  strike: [],
  code: [],
  link: ['href', 'title'],
  underline: [],
  highlight: [],
}
