import { memo, useEffect, useRef, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { t } from '../i18n'
import { AUDIO, VIDEO, openImage } from './Media'

function CodeBlock(props: any) {
  const { children, ...rest } = props
  const copy = (e: React.MouseEvent) => {
    const pre = (e.currentTarget as HTMLElement).parentElement?.querySelector('code')
    navigator.clipboard?.writeText(pre?.textContent || '')
    ;(e.currentTarget as HTMLElement).textContent = t('已复制')
    setTimeout(() => ((e.target as HTMLElement).textContent = t('复制')), 1200)
  }
  return <div className="code-wrap"><button className="code-copy" onClick={copy}>{t('复制')}</button><pre {...rest}>{children}</pre></div>
}

// 模型常在表格单元格里写 <br> 换行。网页里的 HTML 一律不当真（防注入），只把单独的 <br> 换成真的换行
function brTags() {
  const walk = (node: any) => {
    if (!node.children) return
    node.children = node.children.map((c: any) => (c.type === 'html' && /^<br\s*\/?>$/i.test(c.value.trim()) ? { type: 'break' } : c))
    node.children.forEach(walk)
  }
  return (tree: any) => walk(tree)
}

// GitHub 的提示框写法（> [!NOTE]、[!TIP]、[!WARNING]……）：显示成带颜色的框。编辑器里的提示框也是这样存的
const ALERT = /^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\][ \t]*\n?/i
function alerts() {
  const walk = (node: any) => {
    for (const c of node.children || []) {
      const para = c.type === 'blockquote' && c.children?.[0]?.type === 'paragraph' ? c.children[0] : null
      const text = para?.children?.[0]?.type === 'text' ? para.children[0] : null
      const m = text?.value.match(ALERT)
      if (m) {
        text.value = text.value.slice(m[0].length)
        if (!text.value) para.children.shift()
        if (para.children[0]?.type === 'break') para.children.shift()
        if (!para.children.length) c.children.shift()
        const kind = m[1].toLowerCase()
        c.data = { ...c.data, hName: 'div', hProperties: { className: ['callout', 'callout-' + kind] } }
      }
      walk(c)
    }
  }
  return (tree: any) => walk(tree)
}

// 折叠块 <details><summary>…</summary> … </details>：网页里的 HTML 一律不当真，只认这一种写法，换成真的折叠块。
// 中间隔着空行的内容照常按 Markdown 显示；嵌套的也认
const D_OPEN = /^<details(\s[^>]*)?>\s*(?:<summary>([\s\S]*?)<\/summary>)?\s*$/i
const D_ONE = /^<details(\s[^>]*)?>\s*<summary>([\s\S]*?)<\/summary>([\s\S]*?)<\/details\s*>$/i
const D_CLOSE = /^<\/details\s*>$/i
const unescape = (s: string) => s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&')
const box = (attrs: string | undefined, summary: string | undefined, rest: any[] = []) => ({
  type: 'blockquote', data: { hName: 'details', hProperties: attrs && /\bopen\b/i.test(attrs) ? { open: true } : {} },
  children: [...(summary ? [{ type: 'paragraph', data: { hName: 'summary' }, children: [{ type: 'text', value: unescape(summary.trim()) }] }] : []), ...rest],
})
function details() {
  const fold = (node: any) => {
    if (!node.children) return
    node.children.forEach(fold)
    const out: any[] = [], stack: any[] = []
    const put = (n: any) => (stack.length ? stack[stack.length - 1].children : out).push(n)
    for (const c of node.children) {
      const v = c.type === 'html' ? String(c.value).trim() : ''
      let m
      if (v && (m = v.match(D_ONE))) put(box(m[1], m[2], m[3].trim() ? [{ type: 'paragraph', children: [{ type: 'text', value: unescape(m[3].trim()) }] }] : []))
      else if (v && (m = v.match(D_OPEN))) { const b = box(m[1], m[2]); put(b); stack.push(b) }
      else if (v && D_CLOSE.test(v) && stack.length) stack.pop()
      else put(c)
    }
    node.children = out
  }
  return (tree: any) => fold(tree)
}

// 站内的页面链接（/pages/12）在应用里打开（按住 Cmd / Ctrl 点照常开新标签页）；其余链接开新标签页
function Link({ node, href, ...p }: any) {
  const page = href?.match(/^\/pages\/(\d+)$/)
  if (page) return <a {...p} href={href} onClick={e => {
    if (e.metaKey || e.ctrlKey) return
    e.preventDefault(); window.dispatchEvent(new CustomEvent('oad-open-page', { detail: Number(page[1]) }))
  }} />
  return <a {...p} href={href} target="_blank" rel="noreferrer" />
}

// 链接指向的文件路径（看扩展名认音视频）
const pathOf = (href: string) => { try { return new URL(href, location.href).pathname } catch { return href || '' } }

// 图片限宽限高，点开看大图；![](x.mp4) 这样写的视频直接播放
function Img({ node, src, alt, ...p }: any) {
  if (typeof src === 'string' && VIDEO.test(pathOf(src))) return <video className="md-video" controls playsInline preload="metadata" src={src} />
  return <img {...p} src={src} alt={alt || ''} loading="lazy" onClick={() => src && openImage({ src, name: alt || undefined })} />
}

// 段落里只有一个链接（一个裸网址，或者一个 [文字](网址)）时返回它，句子里的链接不算
export function soleLink(node: any): { href: string } | null {
  const kids = (node?.children || []).filter((c: any) => !(c.type === 'text' && !c.value.trim()))
  const a = kids.length === 1 && kids[0].type === 'element' && kids[0].tagName === 'a' ? kids[0] : null
  return a && typeof a.properties?.href === 'string' ? { href: a.properties.href } : null
}

// 单独一段的音频、视频链接换成播放器，链接留在下面
function Para({ node, children, ...p }: any) {
  const one = soleLink(node)
  const path = one ? pathOf(one.href) : ''
  if (one && AUDIO.test(path)) return <div className="md-media"><audio controls preload="metadata" src={one.href} /><div className="md-media-cap">{children}</div></div>
  if (one && VIDEO.test(path)) return <div className="md-media"><video className="md-video" controls playsInline preload="metadata" src={one.href} /><div className="md-media-cap">{children}</div></div>
  return <p {...p}>{children}</p>
}

// 表格比正文宽时横向滑动。还有没露出来的列，就把那一侧渐隐（styles.css 的 .md-table），不然看不出能滑
function Table({ node, ...p }: any) {
  const box = useRef<HTMLDivElement>(null)
  const [more, setMore] = useState('')
  const check = () => {
    const el = box.current
    if (!el) return
    const left = el.scrollLeft > 1, right = el.scrollLeft + el.clientWidth < el.scrollWidth - 1
    setMore((left ? ' more-l' : '') + (right ? ' more-r' : ''))
  }
  useEffect(check)   // 流式输出时表格一行行变长，每次重画都看一眼
  useEffect(() => {  // 右栏、讨论串开关时正文变宽变窄
    if (!box.current || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(check)
    ro.observe(box.current)
    return () => ro.disconnect()
  }, [])
  return <div className={'md-table' + more} ref={box} onScroll={check}><table {...p} /></div>
}

const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="md">
      <ReactMarkdown remarkPlugins={[[remarkGfm, { singleTilde: false }], brTags, alerts, details]} rehypePlugins={[[rehypeHighlight, { ignoreMissing: true }]]}
        components={{ pre: CodeBlock, a: Link, img: Img, p: Para, table: Table }}>
        {text}
      </ReactMarkdown>
    </div>
  )
})
export default Markdown
