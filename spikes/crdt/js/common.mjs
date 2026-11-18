import { readFileSync } from 'node:fs'
import { LoroDoc } from 'loro-crdt'
import * as Y from 'yjs'
import { createNodeFromLoroObj } from 'loro-prosemirror'
import { initProseMirrorDoc } from 'y-prosemirror'
import { schema } from './schema.mjs'

export const OUT = '../out/'
export const read = (f) => readFileSync(OUT + f)
export const styles = JSON.parse(readFileSync(OUT + 'styles.json'))
export const norm = (json) => schema.nodeFromJSON(json).toJSON()

export function loroLoad(files, peer) {
  const doc = new LoroDoc()
  if (peer != null) doc.setPeerId(peer)
  doc.configTextStyle(Object.fromEntries(Object.entries(styles).map(([k, v]) => [k, { expand: v }])))
  for (const f of files) doc.import(read(f))
  return doc
}
export function loroToPm(doc) {
  const mapping = new Map()
  const node = createNodeFromLoroObj(schema, doc.getMap('doc'), mapping)
  return { node, mapping }
}
export function yLoad(files, client) {
  const doc = new Y.Doc({ gc: false })
  if (client != null) doc.clientID = client
  for (const f of files) Y.applyUpdate(doc, read(f))
  return doc
}
export function yToPm(doc) {
  const { doc: node, meta } = initProseMirrorDoc(doc.getXmlFragment('default'), schema)
  return { node, meta }
}
// 按编号找节点的位置
export function posOf(doc, id) {
  let hit = null
  doc.descendants((n, pos) => {
    if (hit != null) return false
    if (n.attrs?.id === id) { hit = { node: n, pos }; return false }
  })
  return hit
}
export function pct(arr, p) { const s = [...arr].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.floor(s.length * p))] }
