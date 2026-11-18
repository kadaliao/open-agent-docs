// Markdown 式的行内格式输入：**粗体**、*斜体*、~~删除线~~、`代码`、==高亮==。
// Tiptap 自带的规则要求前面是空格，中文里“，**重点**”这种不认；这里只要求前面不是字母数字或同一个符号。
import { InputRule } from '@tiptap/core'
import type { MarkType } from '@tiptap/pm/model'

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

export function markRule(delim: string, type: () => MarkType): InputRule {
  const d = esc(delim)
  const c = esc(delim[0])
  // 第 1 组：整段带符号的文字，第 2 组：里面的字（两头不能是空格）
  const body = delim.length === 1 ? `[^${c}]*` : `(?:[^${c}]|${c}(?!${c}))*`
  const find = new RegExp(`(?:^|[^\\w${c}])(${d}([^\\s${c}](?:${body}[^\\s${c}])?)${d})$`)
  return new InputRule({
    find,
    handler: ({ state, range, match }) => {
      const full = match[1]
      const inner = match[2]
      const start = range.from + match[0].lastIndexOf(full)
      const { tr } = state
      const textStart = start + delim.length
      const textEnd = textStart + inner.length
      // 最后打的那个符号还没进文档，range.to 是它前面的位置。先删后面的符号，再删前面的
      if (textEnd < range.to) tr.delete(textEnd, range.to)
      tr.delete(start, textStart)
      tr.addMark(start, start + inner.length, type().create())
      tr.removeStoredMark(type())
    },
  })
}
