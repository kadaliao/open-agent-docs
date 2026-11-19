// 评论正文：Markdown 的一小部分（粗体、斜体、删除线、行内代码、链接、换行）+ 提到 @[名字](agent:<id>) / @[名字](user:<id>)。
// 先转义再替换，不吃任意 HTML。
const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

export const MENTION_RE = /@\[([^\]\n]{1,64})\]\((agent|user):([0-9A-Za-z_-]{1,64})\)/g

export function renderBody(md: string): string {
  const codes: string[] = []
  // 行内代码先拿出来，里面不再处理
  let s = md.replace(/`([^`\n]+)`/g, (_, c) => {
    codes.push(c)
    return `\u0000${codes.length - 1}\u0000`
  })
  s = esc(s)
  s = s.replace(/@\[([^\]\n]{1,64})\]\((agent|user):([0-9A-Za-z_-]{1,64})\)/g, (_, name, kind, id) => `<span class="mention is-${kind}" data-id="${id}">@${name}</span>`)
  s = s.replace(/\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g, (_, t, u) => `<a href="${u}" target="_blank" rel="noopener noreferrer nofollow">${t}</a>`)
  s = s.replace(/(^|[^"=])(https?:\/\/[^\s<]+)/g, (_, pre, u) => `${pre}<a href="${u}" target="_blank" rel="noopener noreferrer nofollow">${u}</a>`)
  s = s.replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>')
  s = s.replace(/(^|[^*\w])\*([^*\n]+)\*(?!\w)/g, '$1<em>$2</em>')
  s = s.replace(/~~([^~\n]+)~~/g, '<s>$1</s>')
  s = s.replace(/\n/g, '<br>')
  return s.replace(/\u0000(\d+)\u0000/g, (_, i) => `<code>${esc(codes[+i])}</code>`)
}

/** 存的格式 → 输入框里显示的 @名字，同时记下名字对应谁 */
export function bodyToDraft(md: string): { text: string; mentions: Map<string, string> } {
  const mentions = new Map<string, string>()
  const text = md.replace(MENTION_RE, (_, name, kind, id) => {
    mentions.set(name, `${kind}:${id}`)
    return '@' + name
  })
  return { text, mentions }
}

/** 输入框里的 @名字 → 存的格式（只换从候选里选过的） */
export function draftToBody(text: string, mentions: Map<string, string>): string {
  const names = [...mentions.keys()].sort((a, b) => b.length - a.length)
  if (!names.length) return text
  const re = new RegExp(`@(${names.map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`, 'g')
  return text.replace(re, (_, name) => `@[${name}](${mentions.get(name)})`)
}

/** 正文里 @ 到的 Agent */
export function mentionedAgents(md: string): string[] {
  return [...md.matchAll(MENTION_RE)].filter((m) => m[2] === 'agent').map((m) => m[3])
}
