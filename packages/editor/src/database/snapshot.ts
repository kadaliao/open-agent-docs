// 库的首屏快照：离开或改动停下时记下整个库页面（标题、标签、表格最前面几十行）的 HTML，下次打开先画出来，数据好了再换成真的。
// 和页面的快照一样存在 localStorage（同步读）。只在表格滚在最上面时记（不然记下的是中间的行）。
// 键名带版本：class 都换成了 oad- 前缀，旧快照里是旧 class，画出来没样式，不用了（第一次存的时候顺手删掉）
const PREFIX = 'oad:dbsnap2:'
const INDEX = 'oad:dbsnap2-index'
const OLD = ['oad:dbsnap:', 'oad:dbsnap-index'] as const
const MAX = 12
const MAX_BYTES = 160_000

export function readDbSnapshot(id: string): string | null {
  try {
    return localStorage.getItem(PREFIX + id)
  } catch {
    return null
  }
}

const DROP = '.oad-db-pop, .oad-db-peek, .oad-db-ime-sink, .oad-db-toast, .oad-db-cell-input, .oad-db-cell-err, .oad-db-open'

export function takeDbSnapshot(el: HTMLElement): string | null {
  const scroller = el.querySelector('.oad-db-table, .oad-db-board, .oad-db-gallery, .oad-db-list') as HTMLElement | null
  if (scroller && (scroller.scrollTop > 40 || scroller.scrollLeft > 40)) return null
  const c = el.cloneNode(true) as HTMLElement
  c.querySelectorAll(DROP).forEach((n) => n.remove())
  // 屏幕下面的行不要
  c.querySelectorAll<HTMLElement>('.oad-db-tr, .oad-db-group-row, .oad-db-gallery-row, .oad-db-list-row').forEach((r) => {
    if (parseFloat(r.style.top || '0') > 1400) r.remove()
  })
  c.querySelectorAll('.oad-is-active, .oad-is-range').forEach((n) => n.classList.remove('oad-is-active', 'oad-is-range'))
  c.querySelectorAll('input').forEach((i) => i.setAttribute('value', (i as HTMLInputElement).value))
  const html = c.innerHTML
  return html.length > MAX_BYTES ? null : html
}

function dropOld() {
  const raw = localStorage.getItem(OLD[1])
  if (raw === null) return
  for (const id of JSON.parse(raw) as string[]) localStorage.removeItem(OLD[0] + id)
  localStorage.removeItem(OLD[1])
}

export function saveDbSnapshot(id: string, html: string | null) {
  if (!html) return
  try {
    dropOld()
    localStorage.setItem(PREFIX + id, html)
    const idx = (JSON.parse(localStorage.getItem(INDEX) || '[]') as string[]).filter((x) => x !== id)
    idx.unshift(id)
    for (const old of idx.splice(MAX)) localStorage.removeItem(PREFIX + old)
    localStorage.setItem(INDEX, JSON.stringify(idx))
  } catch {
    /* 存不下就算了 */
  }
}
