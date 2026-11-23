// 首屏快照：每页记下编辑器最前面几十块的 HTML 和标题，存在 localStorage（同步读，不用等 IndexedDB）。
// 再打开这页时先把快照画出来，编辑器在后面装好后换上，看起来就是一打开就有内容。
// 键名带版本：编辑器的 class 换成了 oad- 前缀，旧快照里是旧 class，画出来没样式，不用了（第一次存的时候顺手删掉）
const PREFIX = 'oad:snap2:'
const INDEX = 'oad:snap2-index'
const OLD = ['oad:snap:', 'oad:snap-index'] as const
const MAX_PAGES = 40
const MAX_BLOCKS = 60
const MAX_BYTES = 120_000

export interface Snapshot {
  title: string
  html: string
}

export function readSnapshot(pageId: string): Snapshot | null {
  try {
    const raw = localStorage.getItem(PREFIX + pageId)
    return raw ? (JSON.parse(raw) as Snapshot) : null
  } catch {
    return null
  }
}

// 别人的光标、Agent 正在写的标记、空段落提示这些不进快照
const DROP = '.ProseMirror-yjs-cursor, .oad-caret, .ProseMirror-yjs-selection, .ProseMirror-gapcursor, .ProseMirror-separator, .ProseMirror-trailingBreak'

export function takeSnapshot(dom: HTMLElement, title: string): Snapshot {
  const box = document.createElement('div')
  let bytes = 0
  for (let i = 0; i < dom.children.length && i < MAX_BLOCKS; i++) {
    const c = dom.children[i].cloneNode(true) as HTMLElement
    c.querySelectorAll(DROP).forEach((n) => n.remove())
    c.querySelectorAll('.oad-remote-selection, .oad-agent-writing').forEach((n) => {
      n.removeAttribute('class')
      n.removeAttribute('style')
    })
    c.classList.remove('oad-agent-writing', 'oad-is-empty', 'oad-is-editor-empty', 'ProseMirror-selectednode')
    c.removeAttribute('style')
    c.removeAttribute('data-agent')
    c.removeAttribute('data-placeholder')
    bytes += c.outerHTML.length
    if (bytes > MAX_BYTES) break
    box.append(c)
  }
  return { title, html: box.innerHTML }
}

function dropOld() {
  const raw = localStorage.getItem(OLD[1])
  if (raw === null) return
  for (const id of JSON.parse(raw) as string[]) localStorage.removeItem(OLD[0] + id)
  localStorage.removeItem(OLD[1])
}

export function saveSnapshot(pageId: string, s: Snapshot) {
  try {
    dropOld()
    localStorage.setItem(PREFIX + pageId, JSON.stringify(s))
    const idx = (JSON.parse(localStorage.getItem(INDEX) || '[]') as string[]).filter((x) => x !== pageId)
    idx.unshift(pageId)
    for (const old of idx.splice(MAX_PAGES)) localStorage.removeItem(PREFIX + old)
    localStorage.setItem(INDEX, JSON.stringify(idx))
  } catch {
    // 存满了：清掉所有快照，下次再存
    try {
      for (const id of JSON.parse(localStorage.getItem(INDEX) || '[]') as string[]) localStorage.removeItem(PREFIX + id)
      localStorage.removeItem(INDEX)
    } catch {
      /* 忽略 */
    }
  }
}
