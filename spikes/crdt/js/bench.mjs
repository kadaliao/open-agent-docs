// 浏览器端会遇到的开销（Node 里跑，不含 DOM 渲染）：打开（导入快照 + 转成 ProseMirror 文档）、
// 每次按键绑定要做的同步、收到别人的一次改动后重建文档。
import { readFileSync } from 'node:fs'
import { EditorState } from 'prosemirror-state'
import { LoroDoc } from 'loro-crdt'
import * as Y from 'yjs'
import { updateLoroToPmState, createNodeFromLoroObj } from 'loro-prosemirror'
import { updateYFragment, initProseMirrorDoc, prosemirrorToYXmlFragment } from 'y-prosemirror'
import { schema } from './schema.mjs'
import { styles, pct } from './common.mjs'

const cfg = Object.fromEntries(Object.entries(styles).map(([k, v]) => [k, { expand: v }]))
const now = () => performance.now()
const f = (x) => x.toFixed(2)

function loroMake(pm) {
  const doc = new LoroDoc(); doc.setPeerId(1); doc.configTextStyle(cfg)
  updateLoroToPmState(doc, new Map(), EditorState.create({ schema, doc: pm }))
  return doc.export({ mode: 'snapshot' })
}
function yMake(pm) {
  const doc = new Y.Doc({ gc: false }); doc.clientID = 1
  prosemirrorToYXmlFragment(pm, doc.getXmlFragment('default'))
  return Y.encodeStateAsUpdate(doc)
}

function typing(name, open, sync) {
  const { node, ctx } = open()
  let state = EditorState.create({ schema, doc: node })
  // 找第 70 个顶层段落，在末尾一个字一个字地打 200 个字
  let idx = 0, target = null
  state.doc.forEach((n, off) => { if (n.type.name === 'paragraph' && idx++ === 70) target = off + n.nodeSize - 1 })
  const times = []
  for (let i = 0; i < 200; i++) {
    const t = now()
    state = state.apply(state.tr.insertText(i % 2 ? '字' : 'a', target + i))
    sync(ctx, state)
    times.push(now() - t)
  }
  console.log(`  ${name} 打字：每键 p50 ${f(pct(times, 0.5))}ms，p95 ${f(pct(times, 0.95))}ms，max ${f(Math.max(...times))}ms`)
}

for (const file of ['base.json', 'base10k.json']) {
  const pm = schema.nodeFromJSON(JSON.parse(readFileSync('../out/' + file)))
  console.log(`== ${file}（${pm.childCount} 个顶层块）==`)
  const ls = loroMake(pm), ys = yMake(pm)
  console.log(`  快照大小：loro ${(ls.length / 1024) | 0} KB，yjs ${(ys.length / 1024) | 0} KB`)

  // 打开
  for (let round = 0; round < 2; round++) {
    let t = now()
    const ld = new LoroDoc(); ld.configTextStyle(cfg); ld.import(ls)
    const t1 = now() - t
    const lnode = createNodeFromLoroObj(schema, ld.getMap('doc'), new Map())
    const lt = now() - t
    t = now()
    const yd = new Y.Doc({ gc: false }); Y.applyUpdate(yd, ys)
    const t2 = now() - t
    const { doc: ynode } = initProseMirrorDoc(yd.getXmlFragment('default'), schema)
    const yt = now() - t
    if (round === 1) console.log(`  打开：loro ${f(lt)}ms（导入 ${f(t1)}）| yjs ${f(yt)}ms（导入 ${f(t2)}）`)
    void lnode, ynode
  }

  // 打字
  typing('loro', () => {
    const doc = new LoroDoc(); doc.configTextStyle(cfg); doc.import(ls)
    const mapping = new Map()
    return { node: createNodeFromLoroObj(schema, doc.getMap('doc'), mapping), ctx: { doc, mapping } }
  }, (c, st) => updateLoroToPmState(c.doc, c.mapping, st))
  typing('yjs ', () => {
    const doc = new Y.Doc({ gc: false }); Y.applyUpdate(doc, ys)
    const { doc: node, meta } = initProseMirrorDoc(doc.getXmlFragment('default'), schema)
    return { node, ctx: { doc, meta, frag: doc.getXmlFragment('default') } }
  }, (c, st) => c.doc.transact(() => updateYFragment(c.doc, c.frag, st.doc, c.meta)))

  // 收到别人的一次改动（Agent 在第 10 段里加一句）：导入增量 + 重建文档（两个绑定都是整份重建，靠缓存复用没变的节点）
  {
    const a = new LoroDoc(); a.configTextStyle(cfg); a.import(ls); a.setPeerId(2)
    const vv = a.oplogVersion()
    const m = new Map(); const n0 = createNodeFromLoroObj(schema, a.getMap('doc'), m)
    const st = EditorState.create({ schema, doc: n0 })
    let p = 0; st.doc.forEach((n, off, i) => { if (i === 10) p = off + 1 })
    updateLoroToPmState(a, m, st.apply(st.tr.insertText('远端', p)))
    const upd = a.export({ mode: 'update', from: vv })
    const b = new LoroDoc(); b.configTextStyle(cfg); b.import(ls)
    const mb = new Map(); createNodeFromLoroObj(schema, b.getMap('doc'), mb)
    const times = []
    let changed = []
    b.subscribe((e) => { changed = e.events.map((x) => x.target) })
    for (let i = 0; i < 20; i++) {
      const t = now()
      if (i === 0) b.import(upd)
      for (const id of changed) mb.delete(id)  // 和 clearChangedNodes 一样：清掉变了的容器的缓存
      createNodeFromLoroObj(schema, b.getMap('doc'), mb)
      times.push(now() - t)
    }
    console.log(`  loro 收到远端改动后重建：${f(times[0])}ms（含导入），之后 p50 ${f(pct(times.slice(1), 0.5))}ms`)
  }
  {
    const a = new Y.Doc({ gc: false }); Y.applyUpdate(a, ys); a.clientID = 2
    const sv = Y.encodeStateVector(a)
    const { doc: n0, meta } = initProseMirrorDoc(a.getXmlFragment('default'), schema)
    const st = EditorState.create({ schema, doc: n0 })
    let p = 0; st.doc.forEach((n, off, i) => { if (i === 10) p = off + 1 })
    a.transact(() => updateYFragment(a, a.getXmlFragment('default'), st.apply(st.tr.insertText('远端', p)).doc, meta))
    const upd = Y.encodeStateAsUpdate(a, sv)
    const b = new Y.Doc({ gc: false }); Y.applyUpdate(b, ys)
    const frag = b.getXmlFragment('default')
    const { meta: mb } = initProseMirrorDoc(frag, schema)
    const t = now()
    Y.applyUpdate(b, upd)
    // y-prosemirror 的 _typeChanged：把变了的类型从缓存里删掉，再整份 createNodeFromYElement
    const { doc: _n } = initProseMirrorDoc(frag, schema)
    console.log(`  yjs  收到远端改动后重建：${f(now() - t)}ms（含导入；这里没复用缓存，是上限）`)
    void mb, _n
  }
}
