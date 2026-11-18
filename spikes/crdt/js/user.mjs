// 1) Rust 建的文档两个绑定能不能原样读出来；2) 模拟人在网页上改（走绑定的同步逻辑），导出增量 *.user。
import { writeFileSync, readFileSync } from 'node:fs'
import { EditorState } from 'prosemirror-state'
import * as Y from 'yjs'
import { updateLoroToPmState } from 'loro-prosemirror'
import { updateYFragment } from 'y-prosemirror'
import { schema } from './schema.mjs'
import { OUT, loroLoad, loroToPm, yLoad, yToPm, posOf, norm } from './common.mjs'

const base = norm(JSON.parse(readFileSync(OUT + 'base.json')))
const T = JSON.parse(readFileSync(OUT + 'targets.json'))
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)

// 人做的三件事
function userEdits(state) {
  const out = []
  let s = state
  const step = (f) => { const tr = f(s); s = s.apply(tr); out.push(s) }
  step((s) => { const p = posOf(s.doc, T.P); return s.tr.insertText('【用户在这里打字】', p.pos + 1 + Math.floor(p.node.content.size / 2)) })
  step((s) => { const p = posOf(s.doc, T.D); return s.tr.delete(p.pos, p.pos + p.node.nodeSize) })
  step((s) => { const p = posOf(s.doc, T.B); return s.tr.addMark(p.pos + 1, p.pos + 6, schema.marks.bold.create()) })
  return out
}

// ---- Loro ----
{
  const doc = loroLoad(['loro.base'], 3)
  const { node, mapping } = loroToPm(doc)
  node.check()
  console.log('loro: Rust 建的文档读成 ProseMirror', same(node.toJSON(), base) ? '一致 ✓' : '不一致 ✗')
  const vv = doc.oplogVersion()
  for (const st of userEdits(EditorState.create({ schema, doc: node }))) updateLoroToPmState(doc, mapping, st)
  writeFileSync(OUT + 'loro.user', doc.export({ mode: 'update', from: vv }))
}
// ---- Yjs ----
{
  const doc = yLoad(['yrs.base'], 3)
  const { node, meta } = yToPm(doc)
  node.check()
  console.log('yjs:  Rust 建的文档读成 ProseMirror', same(node.toJSON(), base) ? '一致 ✓' : '不一致 ✗')
  const sv = Y.encodeStateVector(doc)
  const frag = doc.getXmlFragment('default')
  for (const st of userEdits(EditorState.create({ schema, doc: node }))) doc.transact(() => updateYFragment(doc, frag, st.doc, meta))
  writeFileSync(OUT + 'yrs.user', Y.encodeStateAsUpdate(doc, sv))
}
console.log('user: 增量写好了')
