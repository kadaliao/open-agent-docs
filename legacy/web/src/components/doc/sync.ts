// 文档和服务器之间的同步：自动保存只发改过的段；Agent 或别的窗口改了，合进正在编辑的文档。
// - 基线：每段服务器上的原文（md）和当时编辑器里对应的节点（node）、节点写回成 Markdown 的样子（canon）。
//   节点没变（同一个对象，或者内容相等）就不算改过；变了再写回比一下 canon：Markdown 写不出的差别不算。
//   没改过的段从来不写回，Agent 写的原文一个字符都不会被改样。
// - 同时只有一个保存请求；保存期间又改了就接着存。冲突（409）：先把别人的改动合进来，自己也改了的那几段留给你选，其余照常存。
// - 合进来的时候按服务器的顺序重建文档，只替换真正不一样的那一截（findDiffStart / findDiffEnd），光标不跳、撤销记录尽量保住。
import type { Editor, JSONContent } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { TextSelection } from '@tiptap/pm/state'
import { api } from '../../store'
import { blocksOf, buildDoc, restOf, serializeBlock, type Block, type Loc } from './markdown'
import { gutterKey } from './gutter'
import { newId } from './blockIds'

type Base = { md: string; gap: string; v: number; by: string; ts: number; node: PMNode | null; canon: string }
type Op = Record<string, unknown>
export type PageBlocks = { version: number; order: string[]; blocks: Block[]; locked?: boolean; title?: string }
export type Status = 'saved' | 'saving' | 'offline' | 'conflict' | 'error'
export type Conflict = { id: string; by: string; md: string }

export const REMOTE = 'oadRemote'   // 合进来的改动：不触发保存
const DEBOUNCE = 700, MAX_WAIT = 4000, RETRY = [1000, 2000, 5000, 10000, 30000], KEEPALIVE_MAX = 60_000
const draftKey = (id: number) => `oad.doc.${id}`

function lcs(a: string[], b: string[]): Set<string> {
  const n = a.length, m = b.length
  if (n * m > 4_000_000) return new Set(b.filter((x, k) => a[k] === x))   // 太大了：只认位置没变的
  const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1))
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1])
  const out = new Set<string>()
  for (let i = 0, j = 0; i < n && j < m;) {
    if (a[i] === b[j]) { out.add(a[i]); i++; j++ } else if (dp[i + 1][j] >= dp[i][j + 1]) i++; else j++
  }
  return out
}

export class DocSync {
  version = 0
  order: string[] = []
  base = new Map<string, Base>()
  conflicts = new Map<string, Conflict>()
  counts = new Map<string, number>()   // 每段有几串没解决的评论（评论面板给的）
  status: Status = 'saved'
  error = ''
  onChange: () => void = () => {}
  onNotice: (text: string) => void = () => {}
  private timer: ReturnType<typeof setTimeout> | undefined
  private firstDirty = 0
  private inflight: Promise<void> | null = null
  private again = false
  private tries = 0
  private clashes = 0
  private remoteWant = 0
  private dead = false

  constructor(readonly pageId: number, readonly editor: Editor) {}

  // ---------- 载入 ----------
  load(page: PageBlocks) {
    this.version = page.version
    this.order = page.order
    this.base = new Map(page.blocks.map(b => [b.id, { md: b.md, gap: b.gap, v: b.v, by: b.by, ts: b.ts, node: null, canon: b.md }]))
    this.replaceDoc(buildDoc(this.editor, this.order.map(id => ({ id, ...this.base.get(id)! }))), true)
    this.bind()
  }

  /** 基线对上编辑器里现在的节点（skip 里的是还没存的本地修改，保持“改过”）。 */
  private bind(skip: Set<string> = new Set()) {
    for (const l of blocksOf(this.editor.state.doc)) {
      const b = l.id && !skip.has(l.id) ? this.base.get(l.id) : undefined
      if (!b) continue
      b.node = l.node
      b.canon = serializeBlock(this.editor, l)
      for (const r of restOf(l.node)) { const rb = this.base.get(r); if (rb) { rb.node = null; rb.canon = rb.md } }
    }
    this.paint()
  }

  paint(flash: string[] = []) {
    if (this.editor.isDestroyed) return
    const by = new Map([...this.base].map(([id, b]) => [id, { by: b.by, ts: b.ts }]))
    const tr = this.editor.state.tr.setMeta(gutterKey, { by, conflicts: new Set(this.conflicts.keys()), flash: new Set(flash), counts: this.counts })
    this.editor.view.dispatch(tr.setMeta('addToHistory', false).setMeta('preventUpdate', true))
    if (flash.length) setTimeout(() => !this.editor.isDestroyed && this.editor.view.dispatch(
      this.editor.state.tr.setMeta(gutterKey, { flash: new Set() }).setMeta('addToHistory', false).setMeta('preventUpdate', true)), 1200)
  }

  /** 整篇换成 json：只替换不一样的那一截，光标留在原来那段的同一个位置。 */
  private replaceDoc(json: JSONContent, initial = false) {
    const { state, view } = this.editor
    const doc = state.schema.nodeFromJSON(json)
    const start = state.doc.content.findDiffStart(doc.content)
    if (start == null) return
    const end = state.doc.content.findDiffEnd(doc.content)!
    let endA = end.a, endB = end.b
    const overlap = start - Math.min(endA, endB)
    if (overlap > 0) { endA += overlap; endB += overlap }
    const head = state.selection.head
    const at = blocksOf(state.doc).find(l => head >= l.pos && head <= l.pos + l.node.nodeSize)
    const tr = state.tr.replace(start, endA, doc.slice(start, endB)).setMeta('addToHistory', false).setMeta('preventUpdate', true).setMeta(REMOTE, true)
    if (!initial && at?.id && head >= start && head <= endA) {
      const again = blocksOf(tr.doc).find(l => l.id === at.id)
      if (again) {
        const pos = Math.min(again.pos + (head - at.pos), again.pos + again.node.nodeSize - 1)
        try { tr.setSelection(TextSelection.near(tr.doc.resolve(pos))) } catch { /* 光标放不进去就算了 */ }
      }
    }
    view.dispatch(tr)
  }

  // ---------- 找出改了什么 ----------
  /** 这段改过就返回它现在的 Markdown，没改过返回 null。 */
  private changed(l: Loc): string | null {
    const b = l.id ? this.base.get(l.id) : undefined
    if (!b) return null
    if (l.node === b.node) return null
    if (b.node && l.node.eq(b.node)) { b.node = l.node; return null }
    const md = serializeBlock(this.editor, l)
    if (md === b.canon) { b.node = l.node; return null }
    return md
  }

  computeOps(): { ops: Op[]; sent: Map<string, { node: PMNode; md: string }> } {
    const locs = blocksOf(this.editor.state.doc).filter(l => l.id)
    const here = new Set<string>()
    for (const l of locs) { here.add(l.id!); restOf(l.node).forEach(r => here.add(r)) }
    const ops: Op[] = []
    const sent = new Map<string, { node: PMNode; md: string }>()
    for (const id of this.order) if (!here.has(id)) ops.push({ op: 'delete', block: id })
    const now = locs.filter(l => this.base.has(l.id!)).map(l => l.id!)
    const nowSet = new Set(now)
    const stay = lcs(this.order.filter(id => nowSet.has(id)), now)
    let prev: string | null = null
    for (const l of locs) {
      const id = l.id!
      if (!this.base.has(id)) {
        const md = serializeBlock(this.editor, l)
        if (!md.trim()) continue   // 空的新段（刚按了回车）等写了字再存
        ops.push({ op: 'insert', after: prev, new_id: id, markdown: md })
        sent.set(id, { node: l.node, md })
        prev = id
        continue
      }
      if (!stay.has(id)) ops.push({ op: 'move', block: id, after: prev })
      prev = restOf(l.node).at(-1) ?? id
    }
    for (const l of locs) {
      const id = l.id!
      if (sent.has(id) || this.conflicts.has(id)) continue
      const md = this.changed(l)
      if (md === null) continue
      const rest = restOf(l.node)
      // 原文块整组改了：整组交给服务器，按内容和原来的几段对齐（没改的行留着编号和评论）
      ops.push(rest.length ? { op: 'replace', block: id, markdown: md, group: rest } : { op: 'replace', block: id, markdown: md })
      sent.set(id, { node: l.node, md })
    }
    return { ops, sent }
  }

  // ---------- 保存 ----------
  schedule(delay = DEBOUNCE) {
    if (this.dead) return
    const now = Date.now()
    if (!this.firstDirty) this.firstDirty = now
    clearTimeout(this.timer)
    this.timer = setTimeout(() => this.save(), Math.min(delay, Math.max(0, this.firstDirty + MAX_WAIT - now)))
    if (this.status === 'saved') this.setStatus('saving')
  }

  async save(): Promise<void> {
    clearTimeout(this.timer)
    this.timer = undefined
    if (this.dead) return
    if (this.inflight) { this.again = true; return this.inflight }
    if (this.editor.view.composing) { this.schedule(300); return }
    this.firstDirty = 0
    const { ops, sent } = this.computeOps()
    if (!ops.length) { this.setStatus(this.conflicts.size ? 'conflict' : 'saved'); return }
    this.setStatus('saving')
    this.inflight = (async () => {
      try {
        const res = await api(`/api/pages/${this.pageId}/blocks`, { method: 'POST', body: { version: this.version, ops } })
        this.tries = 0
        this.clashes = 0
        this.saved(res, sent)
        try { localStorage.removeItem(draftKey(this.pageId)) } catch { /* 存不了就算了 */ }
      } catch (e: any) {
        if (e?.status === 409 && ++this.clashes <= 3) {
          await this.refetch()
          this.again = true
        } else if (!e?.status || e.status >= 500) {
          this.writeDraft(ops)
          this.setStatus('offline')
          this.timer = setTimeout(() => this.save(), RETRY[Math.min(this.tries++, RETRY.length - 1)])
        } else {
          this.error = e.message
          this.setStatus('error')
        }
      }
    })()
    await this.inflight
    this.inflight = null
    if (this.remoteWant > this.version) await this.refetch()
    if (this.again) { this.again = false; this.schedule(0) }
  }

  /** 保存成功：基线换成发出去的那一版（发出去之后又改的，下次接着存）。服务器切出来的和本地不一样时按服务器的校正。 */
  private saved(res: { version: number; order: string[]; blocks: Block[]; removed: string[] }, sent: Map<string, { node: PMNode; md: string }>) {
    this.version = res.version
    for (const id of res.removed) this.base.delete(id)
    let mismatch = false
    for (const b of res.blocks) {
      const s = sent.get(b.id), cur = this.base.get(b.id)
      if (s) {
        this.base.set(b.id, { md: b.md, gap: b.gap, v: b.v, by: b.by, ts: b.ts, node: s.node, canon: s.md })
        if (b.md !== s.md) mismatch = true
      } else if (cur) {
        if (cur.md !== b.md) mismatch = true
        Object.assign(cur, { md: b.md, gap: b.gap, v: b.v, by: b.by, ts: b.ts })
      } else {
        this.base.set(b.id, { md: b.md, gap: b.gap, v: b.v, by: b.by, ts: b.ts, node: null, canon: b.md })
        mismatch = true
      }
    }
    // 发出去的段服务器原样留着（不在 blocks 里，比如原文块整组改了、第一行没变）：也算存上了，不然下次还当它没存、再发一遍
    const kept = new Set(res.order), back = new Set(res.blocks.map(b => b.id))
    for (const [id, s] of sent) {
      const cur = this.base.get(id)
      if (back.has(id) || !cur || !kept.has(id)) continue
      cur.node = s.node
      cur.canon = s.md
      if (s.md !== cur.md) mismatch = true
    }
    this.order = res.order
    const local: string[] = []
    for (const l of blocksOf(this.editor.state.doc)) if (l.id && this.base.has(l.id)) local.push(l.id, ...restOf(l.node))
    if (mismatch || local.join() !== res.order.join()) this.rebuild()
    else this.paint()
    this.setStatus(this.conflicts.size ? 'conflict' : 'saved')
  }

  // ---------- 合进别人的改动 ----------
  /** 收到页面改动的通知：比自己新才去取。正在保存时等存完再取。 */
  remote(version: number) {
    if (this.dead || version <= this.version) return
    this.remoteWant = Math.max(this.remoteWant, version)
    if (!this.inflight) this.refetch()
  }

  async refetch() {
    if (this.dead) return
    try {
      const data: PageBlocks = await api(`/api/pages/${this.pageId}/blocks?since=${this.version}`)
      this.merge(data)
    } catch { /* 下次通知或者重连时再取 */ }
  }

  private merge(data: PageBlocks) {
    if (this.editor.view.composing) { setTimeout(() => this.merge(data), 300); return }
    if (data.version <= this.version && !data.blocks.length) return
    const { edits: dirty, order: localOrder } = this.localEdits()
    const changed: string[] = []
    for (const b of data.blocks) {
      const old = this.base.get(b.id)
      if (old && old.md === b.md) { Object.assign(old, { gap: b.gap, v: b.v, by: b.by, ts: b.ts }); continue }
      if (dirty.has(b.id)) this.conflicts.set(b.id, { id: b.id, by: b.by, md: b.md })
      this.base.set(b.id, { md: b.md, gap: b.gap, v: b.v, by: b.by, ts: b.ts, node: null, canon: b.md })
      changed.push(b.id)
    }
    const live = new Set(data.order)
    for (const id of [...this.base.keys()]) {
      if (live.has(id)) continue
      this.base.delete(id)
      this.conflicts.delete(id)
      const mine = dirty.get(id)
      if (mine) {   // 别人删了你正在改的段：你的修改留下来，当成新的一段
        mine.id = newId()
        mine.fresh = true
        this.onNotice('deleted')
      }
    }
    this.order = data.order
    this.version = Math.max(this.version, data.version)
    this.remoteWant = 0
    this.rebuild(dirty, localOrder)
    this.paint(changed)
    this.setStatus(this.conflicts.size ? 'conflict' : this.status === 'saving' ? 'saving' : 'saved')
    if ([...dirty.values()].some(d => d.md.trim())) this.schedule()   // 末尾自动补的空段不算
  }

  /** 本地还没存的：改过的段（编号 → 现在的 Markdown）、新加的段；order 是本地所有段的顺序。 */
  private localEdits(): { edits: Map<string, { id: string; md: string; fresh: boolean }>; order: string[] } {
    const edits = new Map<string, { id: string; md: string; fresh: boolean }>()
    const order: string[] = []
    for (const l of blocksOf(this.editor.state.doc)) {
      if (!l.id) continue
      order.push(l.id)
      if (!this.base.has(l.id)) { edits.set(l.id, { id: l.id, md: serializeBlock(this.editor, l), fresh: true }); continue }
      const md = this.conflicts.has(l.id) ? serializeBlock(this.editor, l) : this.changed(l)
      if (md !== null) edits.set(l.id, { id: l.id, md, fresh: false })
    }
    return { edits, order }
  }

  /** 按服务器现在的段重建文档：本地改过的段用本地的内容；新加的段（和被别人删掉、但你改过的段）
   *  放在本地顺序里它前面最近的、服务器上还在的那段后面。 */
  private rebuild(edits?: Map<string, { id: string; md: string; fresh: boolean }>, localOrder?: string[]) {
    if (!edits || !localOrder) ({ edits, order: localOrder } = this.localEdits())
    const live = new Set(this.order)
    const after = new Map<string | null, { id: string; md: string }[]>()
    const lastLive = localOrder.reduce((k, id, i) => (live.has(id) ? i : k), -1)
    const tail: { id: string; md: string }[] = []
    let anchor: string | null = null
    localOrder.forEach((id, i) => {
      if (live.has(id)) { anchor = id; return }
      const d = edits.get(id)
      // 末尾自动补的空段（还没打字）留在最后：不然别人在页面最后加的段会排到它后面，列表被它隔成两个
      if (d && i > lastLive && !d.md.trim()) tail.push(d)
      else if (d) after.set(anchor, [...(after.get(anchor) || []), d])
    })
    const seq: { id: string; md: string; gap: string }[] = []
    const put = (id: string | null) => (after.get(id) || []).forEach(d => seq.push({ id: d.id, md: d.md, gap: '\n\n' }))
    put(null)
    for (const id of this.order) {
      const b = this.base.get(id)!
      const mine = edits.get(id)
      seq.push({ id, md: mine && !mine.fresh ? mine.md : b.md, gap: b.gap })
      put(id)
    }
    tail.forEach(d => seq.push({ id: d.id, md: d.md, gap: '\n\n' }))
    this.replaceDoc(buildDoc(this.editor, seq))
    this.bind(new Set([...edits.values()].map(d => d.id)))
  }

  // ---------- 版本历史 ----------
  /** 直接改服务器上的段（恢复某一段时用）：先把手头没存的存掉，再发，再把结果合进来。冲突照样抛出去（409）。 */
  async push(ops: Op[]) {
    await this.save()
    while (this.inflight) await this.inflight
    await api(`/api/pages/${this.pageId}/blocks`, { method: 'POST', body: { version: this.version, ops } })
    await this.refetch()
  }

  /** 回到某一版（服务器把它作为新的一版提交，段的编号回来，评论重新挂上）。 */
  async restore(versionId: number) {
    await this.save()
    while (this.inflight) await this.inflight
    await api(`/api/pages/${this.pageId}/versions/${versionId}/restore`, { method: 'POST' })
    await this.refetch()
  }

  /** 服务器上现在的段（版本历史拿来和旧版比）。 */
  current(): { id: string; md: string }[] {
    return this.order.map(id => ({ id, md: this.base.get(id)?.md ?? '' }))
  }

  // ---------- 冲突 ----------
  /** 用我的：下次保存照样发出去（盖掉别人的）。 */
  keepMine(id: string) {
    const b = this.base.get(id)
    if (b) { b.node = null; b.canon = '\u0000' }
    this.conflicts.delete(id)
    this.schedule(0)
    this.onChange()
  }

  /** 用别人的：这段换成服务器上的。 */
  takeTheirs(id: string) {
    this.conflicts.delete(id)
    const { edits, order } = this.localEdits()
    edits.delete(id)
    this.rebuild(edits, order)
    this.setStatus(this.conflicts.size ? 'conflict' : 'saved')
  }

  // ---------- 离开页面 ----------
  private writeDraft(ops: Op[]) {
    try { localStorage.setItem(draftKey(this.pageId), JSON.stringify({ version: this.version, ops, ts: Date.now() })) } catch { /* 存不了就算了 */ }
  }

  /** 页面要关了：还没存的留一份草稿（下次打开时补存），没有正在发的保存就再用 keepalive 发出去。
   *  正在发的时候不重发：新加的段会被存两遍。 */
  flush = () => {
    if (this.dead || this.editor.isDestroyed) return
    const { ops } = this.computeOps()
    if (!ops.length) return
    this.writeDraft(ops)
    const body = JSON.stringify({ version: this.version, ops })
    if (!this.inflight && body.length < KEEPALIVE_MAX) {
      fetch(`/api/pages/${this.pageId}/blocks`, { method: 'POST', keepalive: true, headers: { 'content-type': 'application/json' }, body }).catch(() => {})
    }
  }

  /** 上次没存上的草稿（离开页面时留的）：服务器上已经都有了（keepalive 那次存上了）就丢掉；
   *  没有就补存，和别人的改动冲突了返回 lost，由界面告诉用户。 */
  static async recover(pageId: number): Promise<'none' | 'saved' | 'lost'> {
    let draft: { version: number; ops: Op[] } | null = null
    try { draft = JSON.parse(localStorage.getItem(draftKey(pageId)) || 'null') } catch { /* 坏了就当没有 */ }
    try { localStorage.removeItem(draftKey(pageId)) } catch { /* 存不了就算了 */ }
    if (!draft?.ops?.length) return 'none'
    try {
      const now: PageBlocks = await api(`/api/pages/${pageId}/blocks`)
      const md = new Map(now.blocks.map(b => [b.id, b.md]))
      // 已经存上的去掉（keepalive 那次、或者离开前正在发的那次）；新段已经在了就改成替换它，免得存两遍
      const ops = draft.ops.flatMap((o): Op[] => {
        if (o.op === 'delete') return md.has(String(o.block)) ? [o] : []
        if (o.op === 'insert') {
          const have = md.get(String(o.new_id))
          return have === undefined ? [o] : have === o.markdown ? [] : [{ op: 'replace', block: o.new_id, markdown: o.markdown }]
        }
        if (o.op === 'replace') return (o.markdown ? md.get(String(o.block)) === o.markdown : !md.has(String(o.block))) ? [] : [o]
        return [o]
      })
      if (!ops.length) return 'none'
      try {
        await api(`/api/pages/${pageId}/blocks`, { method: 'POST', body: { version: draft.version, ops } })
      } catch (e: any) {
        // 冲突的段都是你自己（或者你的另一个窗口）后来改的：按现在的版本补存
        const clash = e?.data?.detail?.conflicts
        if (e?.status !== 409 || !clash?.length || !clash.every((c: any) => c.by === 'user')) throw e
        await api(`/api/pages/${pageId}/blocks`, { method: 'POST', body: { version: e.data.detail.version, ops } })
      }
      return 'saved'
    } catch (e: any) {
      return e?.status === 409 ? 'lost' : 'none'
    }
  }

  private setStatus(s: Status) {
    if (this.status === s) return
    this.status = s
    this.onChange()
  }

  retryNow() {
    if (this.status === 'offline') { this.tries = 0; this.save() }
  }

  destroy() {
    this.flush()
    this.dead = true
    clearTimeout(this.timer)
  }
}
