// 两版之间的差别：先按段的编号对（编号跨版本不变），分出新加、删掉、改过、挪了位置的段；改过的段里再按字对
// （中文一个字一个字、英文按词），标出删掉和加上的字。不用别的库。
export type Seg = { op: '=' | '-' | '+'; text: string }
export type Row =
  | { kind: 'same'; id: string; md: string }
  | { kind: 'add'; id: string; md: string }
  | { kind: 'del'; id: string; md: string; after: string | null }   // after：旧版里它前面那段（恢复时插回那里）
  | { kind: 'change'; id: string; old: string; md: string; segs: Seg[]; moved: boolean }
  | { kind: 'move'; id: string; md: string }

type B = { id: string; md: string }

function lcs<T>(a: T[], b: T[], eq: (x: T, y: T) => boolean = (x, y) => x === y): [number, number][] {
  const n = a.length, m = b.length
  const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1))
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) dp[i][j] = eq(a[i], b[j]) ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1])
  const out: [number, number][] = []
  for (let i = 0, j = 0; i < n && j < m;) {
    if (eq(a[i], b[j])) { out.push([i, j]); i++; j++ } else if (dp[i + 1][j] >= dp[i][j + 1]) i++; else j++
  }
  return out
}

const TOKEN = /[㐀-鿿豈-﫿　-〿＀-￯]|[A-Za-z0-9_]+|\s+|[^\sA-Za-z0-9_㐀-鿿]/g   // i18n-ignore：切词用的字符范围

/** 一段里改了哪些字。太长（两边的词数相乘超过一百万）就整段算换掉。 */
export function inline(a: string, b: string): Seg[] {
  if (a === b) return [{ op: '=', text: a }]
  const x = a.match(TOKEN) || [], y = b.match(TOKEN) || []
  let pre = 0
  while (pre < x.length && pre < y.length && x[pre] === y[pre]) pre++
  let suf = 0
  while (suf < x.length - pre && suf < y.length - pre && x[x.length - 1 - suf] === y[y.length - 1 - suf]) suf++
  const xs = x.slice(pre, x.length - suf), ys = y.slice(pre, y.length - suf)
  const out: Seg[] = []
  const push = (op: Seg['op'], text: string) => {
    if (!text) return
    const last = out[out.length - 1]
    if (last?.op === op) last.text += text
    else out.push({ op, text })
  }
  push('=', x.slice(0, pre).join(''))
  if (xs.length * ys.length > 1_000_000) {
    push('-', xs.join(''))
    push('+', ys.join(''))
  } else {
    let i = 0, j = 0
    for (const [p, q] of lcs(xs, ys)) {
      push('-', xs.slice(i, p).join(''))
      push('+', ys.slice(j, q).join(''))
      push('=', xs[p])
      i = p + 1
      j = q + 1
    }
    push('-', xs.slice(i).join(''))
    push('+', ys.slice(j).join(''))
  }
  push('=', x.slice(x.length - suf).join(''))
  return out
}

/** 旧版 → 新版：按新版的顺序排，删掉的段插在旧版里它前面那段的后面。 */
export function diffBlocks(oldB: B[], newB: B[]): Row[] {
  const oldIds = oldB.map(b => b.id), newIds = newB.map(b => b.id)
  const inNew = new Set(newIds), inOld = new Map(oldB.map(b => [b.id, b.md]))
  const fromOld = oldIds.filter(id => inNew.has(id)), both = new Set(fromOld), fromNew = newIds.filter(id => both.has(id))
  const kept = new Set(lcs(fromOld, fromNew).map(([i]) => fromOld[i]))   // 两版里都有、相对顺序没变的段；其余算挪了位置
  // 删掉的段挂在旧版里它前面最近的、新版里还在的那段后面
  const dels = new Map<string | null, Row[]>()
  let anchor: string | null = null
  oldB.forEach((b, k) => {
    if (inNew.has(b.id)) { anchor = b.id; return }
    dels.set(anchor, [...(dels.get(anchor) || []), { kind: 'del', id: b.id, md: b.md, after: k ? oldB[k - 1].id : null }])
  })
  const out: Row[] = [...(dels.get(null) || [])]
  for (const b of newB) {
    const old = inOld.get(b.id)
    if (old === undefined) out.push({ kind: 'add', id: b.id, md: b.md })
    else if (old !== b.md) out.push({ kind: 'change', id: b.id, old, md: b.md, segs: inline(old, b.md), moved: !kept.has(b.id) })
    else out.push(kept.has(b.id) ? { kind: 'same', id: b.id, md: b.md } : { kind: 'move', id: b.id, md: b.md })
    out.push(...(dels.get(b.id) || []))
  }
  return out
}
