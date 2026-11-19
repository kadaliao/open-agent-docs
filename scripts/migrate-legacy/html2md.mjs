// 旧版 HTML 页面 → Markdown（新版只收 Markdown）。只认常见的标签：标题、段落、粗斜体、删除线、行内代码、代码块、链接、图片、
// 列表（可嵌套）、引用、分割线、换行、表格；别的标签去掉、留下文字。不依赖别的包。
const ENT = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ', '#39': "'" }
const decode = (s) => s.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+\d*);/gi, (m, e) => (e[0] === '#' ? String.fromCodePoint(e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : Number(e.slice(1))) : (ENT[e.toLowerCase()] ?? m)))

/** 很小的 HTML 解析：得到 { tag, attrs, children } / 字符串 组成的树 */
function parse(html) {
  const root = { tag: '#root', attrs: {}, children: [] }
  const stack = [root]
  const VOID = new Set(['br', 'hr', 'img', 'input', 'meta', 'link', 'col', 'source', 'wbr'])
  const re = /<!--[\s\S]*?-->|<(\/?)([a-zA-Z][\w-]*)((?:\s+[^\s=>\/]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?)*)\s*(\/?)>|([^<]+|<)/g
  let m
  while ((m = re.exec(html))) {
    const [all, close, tagRaw, attrStr, selfClose, text] = m
    if (all.startsWith('<!--')) continue
    if (text !== undefined) {
      stack.at(-1).children.push(decode(text))
      continue
    }
    const tag = tagRaw.toLowerCase()
    if (close) {
      const i = stack.findLastIndex((n) => n.tag === tag)
      if (i > 0) stack.length = i
      continue
    }
    const attrs = {}
    for (const a of attrStr.matchAll(/([^\s=]+)(?:\s*=\s*("([^"]*)"|'([^']*)'|([^\s>]+)))?/g)) attrs[a[1].toLowerCase()] = decode(a[3] ?? a[4] ?? a[5] ?? '')
    const node = { tag, attrs, children: [] }
    stack.at(-1).children.push(node)
    if (!VOID.has(tag) && !selfClose) stack.push(node)
  }
  return root
}

const BLOCK = new Set(['p', 'div', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'ul', 'ol', 'li', 'blockquote', 'pre', 'hr', 'table', 'section', 'article', 'header', 'footer', 'figure'])

function inline(n) {
  if (typeof n === 'string') return n.replace(/\s+/g, ' ').replace(/([*_`~\[\]])/g, '\\$1')
  const kids = () => n.children.map(inline).join('')
  switch (n.tag) {
    case 'b':
    case 'strong':
      return wrap('**', kids())
    case 'i':
    case 'em':
      return wrap('*', kids())
    case 's':
    case 'del':
    case 'strike':
      return wrap('~~', kids())
    case 'code':
      return '`' + text(n).replace(/`/g, '\\`') + '`'
    case 'a': {
      const t = kids().trim()
      const href = n.attrs.href ?? ''
      return href ? `[${t || href}](${href})` : t
    }
    case 'img':
      return `![${(n.attrs.alt ?? '').replace(/[\[\]]/g, '')}](${n.attrs.src ?? ''})`
    case 'br':
      return '\n'
    default:
      return kids()
  }
}
const wrap = (m, s) => (s.trim() ? m + s.trim() + m : s)
const text = (n) => (typeof n === 'string' ? n : n.children.map(text).join(''))

function blocks(n, out, depth = 0) {
  // 连着的行内内容合成一段
  let para = []
  const flush = () => {
    const s = para.map(inline).join('').replace(/[ \t]+\n/g, '\n').trim()
    if (s) out.push(s)
    para = []
  }
  for (const c of n.children) {
    if (typeof c === 'string' || !BLOCK.has(c.tag)) {
      para.push(c)
      continue
    }
    flush()
    const t = c.tag
    if (/^h[1-6]$/.test(t)) out.push('#'.repeat(Number(t[1])) + ' ' + c.children.map(inline).join('').trim())
    else if (t === 'p' || t === 'div' || t === 'section' || t === 'article' || t === 'header' || t === 'footer' || t === 'figure') blocks(c, out, depth)
    else if (t === 'hr') out.push('---')
    else if (t === 'pre') out.push('```\n' + text(c).replace(/\n$/, '') + '\n```')
    else if (t === 'blockquote') {
      const inner = []
      blocks(c, inner, depth)
      out.push(inner.join('\n\n').split('\n').map((l) => '> ' + l).join('\n'))
    } else if (t === 'ul' || t === 'ol') out.push(list(c, depth))
    else if (t === 'table') out.push(table(c))
    else if (t === 'li') out.push('- ' + c.children.map(inline).join('').trim())
  }
  flush()
  return out
}

function list(n, depth) {
  const lines = []
  let i = 1
  for (const li of n.children) {
    if (typeof li === 'string' || li.tag !== 'li') continue
    const mark = n.tag === 'ol' ? `${i++}.` : '-'
    const own = li.children.filter((c) => typeof c === 'string' || (c.tag !== 'ul' && c.tag !== 'ol'))
    const nested = li.children.filter((c) => typeof c !== 'string' && (c.tag === 'ul' || c.tag === 'ol'))
    const first = own.map((c) => (typeof c !== 'string' && c.tag === 'p' ? c.children.map(inline).join('') : inline(c))).join('').trim()
    lines.push('  '.repeat(depth) + mark + ' ' + first)
    for (const sub of nested) lines.push(list(sub, depth + 1))
  }
  return lines.join('\n')
}

function table(n) {
  const rows = []
  const walk = (x) => {
    for (const c of x.children) {
      if (typeof c === 'string') continue
      if (c.tag === 'tr') rows.push(c.children.filter((d) => typeof d !== 'string' && (d.tag === 'td' || d.tag === 'th')).map((d) => d.children.map(inline).join('').trim().replace(/\|/g, '\\|')))
      else walk(c)
    }
  }
  walk(n)
  if (!rows.length) return ''
  const w = Math.max(...rows.map((r) => r.length))
  const line = (r) => '| ' + Array.from({ length: w }, (_, i) => r[i] ?? '').join(' | ') + ' |'
  return [line(rows[0]), '| ' + Array(w).fill('---').join(' | ') + ' |', ...rows.slice(1).map(line)].join('\n')
}

export function html2md(html) {
  return blocks(parse(String(html ?? '')), []).join('\n\n') + '\n'
}
