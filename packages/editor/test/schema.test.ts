// schema 和服务器一致：全部节点的文档在 ProseMirror 里原样来回；按编辑器的方式写进 Y.Doc 后，
// 照服务器（crates/oad-doc/src/convert.rs）的读法读出来还是同一棵树。顺带写出给 Rust 测试用的两个文件。
import { Editor } from '@tiptap/core'
import { OadCollaboration } from '../src/extensions/collaboration'
import { prosemirrorJSONToYDoc, yXmlFragmentToProseMirrorRootNode } from 'y-prosemirror'
import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import * as Y from 'yjs'
import { getOadSchema, schemaExtensions, MARK_NAMES, NODE_NAMES } from '../src/schema'
import { allNodesDoc, allNodesTitle, expectedMarkAttrs, expectedNodeAttrs } from './all-nodes'

const here = dirname(fileURLToPath(import.meta.url))
const schema = getOadSchema()

const RANK: Record<string, number> = { link: 0, bold: 1, italic: 2, strike: 3, underline: 4, highlight: 5, code: 9 }
const sortKeys = (o: Record<string, unknown>) => Object.fromEntries(Object.entries(o).sort(([a], [b]) => (a < b ? -1 : 1)))

// 和 convert.rs 的 read_children 一样：XmlElement → 节点（null 属性不要），XmlText → 按格式切成 text
function readY(parent: Y.XmlFragment | Y.XmlElement): any[] {
  const out: any[] = []
  for (const c of parent.toArray()) {
    if (c instanceof Y.XmlElement) {
      const attrs = Object.fromEntries(Object.entries(c.getAttributes()).filter(([, v]) => v !== null && v !== undefined))
      const n: any = { type: c.nodeName }
      if (Object.keys(attrs).length) n.attrs = sortKeys(attrs)
      const content = readY(c)
      if (content.length) n.content = content
      out.push(n)
    } else if (c instanceof Y.XmlText) {
      for (const d of c.toDelta() as { insert: string; attributes?: Record<string, any> }[]) {
        if (typeof d.insert !== 'string' || !d.insert) continue
        const n: any = { type: 'text', text: d.insert }
        const marks = Object.entries(d.attributes ?? {})
          .filter(([, v]) => v !== null && v !== undefined)
          .map(([type, v]) => {
            const a = Object.fromEntries(Object.entries(v ?? {}).filter(([, x]) => x !== null))
            return Object.keys(a).length ? { type, attrs: sortKeys(a) } : { type }
          })
          .sort((a, b) => (RANK[a.type] ?? 8) - (RANK[b.type] ?? 8))
        if (marks.length) n.marks = marks
        out.push(n)
      }
    }
  }
  return out
}

// 去掉 PM JSON 里的 null 属性、按键排序，变成服务器那边的形状
function normalize(n: any): any {
  const o: any = { type: n.type }
  if (n.attrs) {
    const a = Object.fromEntries(Object.entries(n.attrs).filter(([, v]) => v !== null))
    if (Object.keys(a).length) o.attrs = sortKeys(a)
  }
  if (n.content?.length) o.content = n.content.map(normalize)
  if (n.text !== undefined) o.text = n.text
  if (n.marks?.length) o.marks = n.marks.map(normalize)
  return o
}

function liveEncode(): Y.Doc {
  const ydoc = new Y.Doc()
  ydoc.clientID = 1 // 固定，文件内容每次一样
  ydoc.getText('title').insert(0, allNodesTitle)
  const editor = new Editor({
    element: document.createElement('div'),
    extensions: [...schemaExtensions(), OadCollaboration.configure({ fragment: ydoc.getXmlFragment('default') })],
  })
  editor.commands.setContent(allNodesDoc)
  editor.destroy()
  return ydoc
}

describe('schema', () => {
  it('节点和 mark 一个不多一个不少', () => {
    expect(Object.keys(schema.nodes).sort()).toEqual([...NODE_NAMES].sort())
    expect(Object.keys(schema.marks).sort()).toEqual([...MARK_NAMES].sort())
  })

  it('每种节点、mark 的属性和 node.rs 一致', () => {
    for (const [name, type] of Object.entries(schema.nodes)) {
      expect([name, Object.keys(type.spec.attrs ?? {}).sort()]).toEqual([name, expectedNodeAttrs[name]])
    }
    for (const [name, type] of Object.entries(schema.marks)) {
      expect([name, Object.keys(type.spec.attrs ?? {}).sort()]).toEqual([name, expectedMarkAttrs[name]])
    }
  })

  it('全部节点的文档用到了每种节点、属性、mark', () => {
    const seenNodes = new Set<string>()
    const seenMarks = new Set<string>()
    const walk = (n: any) => {
      seenNodes.add(n.type)
      for (const k of Object.keys(n.attrs ?? {})) seenNodes.add(n.type + '.' + k)
      for (const m of n.marks ?? []) {
        seenMarks.add(m.type)
        for (const k of Object.keys(m.attrs ?? {})) seenMarks.add(m.type + '.' + k)
      }
      n.content?.forEach(walk)
    }
    walk(allNodesDoc)
    for (const [n, attrs] of Object.entries(expectedNodeAttrs)) {
      expect(seenNodes).toContain(n)
      for (const a of attrs) expect(seenNodes).toContain(n + '.' + a)
    }
    for (const [m, attrs] of Object.entries(expectedMarkAttrs)) {
      expect(seenMarks).toContain(m)
      for (const a of attrs) expect(seenMarks).toContain(m + '.' + a)
    }
  })

  it('ProseMirror 原样来回', () => {
    const node = schema.nodeFromJSON(allNodesDoc)
    node.check()
    expect(node.toJSON()).toEqual(allNodesDoc)
  })

  it('编辑器写进 Y.Doc，服务器读出来是同一棵树；写出测试文件', () => {
    const ydoc = liveEncode()
    const frag = ydoc.getXmlFragment('default')
    expect({ type: 'doc', content: readY(frag) }).toEqual(normalize(allNodesDoc))

    // 和 y-prosemirror 直接转换的结果一样
    const direct = prosemirrorJSONToYDoc(schema, allNodesDoc, 'default')
    expect(readY(direct.getXmlFragment('default'))).toEqual(readY(frag))

    // 从 Y 读回 ProseMirror 也一样
    expect(yXmlFragmentToProseMirrorRootNode(frag, schema).toJSON()).toEqual(allNodesDoc)
    expect(ydoc.getText('title').toString()).toBe(allNodesTitle)

    const dir = join(here, 'fixtures')
    mkdirSync(dir, { recursive: true })
    writeFileSync(join(dir, 'all-nodes.json'), JSON.stringify(allNodesDoc, null, 2) + '\n')
    writeFileSync(join(dir, 'all-nodes.ydoc.bin'), Y.encodeStateAsUpdate(ydoc))
  })
})
