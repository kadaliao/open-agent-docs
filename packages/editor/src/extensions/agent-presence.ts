// Agent 正在写哪一段：服务器在 awareness 里替 Agent 放 { user: { kind: "agent", ... }, block: "<块编号>" }，
// 这里给那一块加上颜色和名字标签。
import { Extension } from '@tiptap/core'
import { isChangeOrigin } from './collaboration'
import type { Node as PMNode } from '@tiptap/pm/model'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'
import type { Awareness } from 'y-protocols/awareness'

export const agentPresenceKey = new PluginKey<DecorationSet>('agentPresence')

interface Writer {
  block: string
  name: string
  color: string
}

function writers(awareness: Awareness): Writer[] {
  const out: Writer[] = []
  awareness.getStates().forEach((s, clientId) => {
    if (clientId === awareness.clientID) return
    const u = s?.user
    if (u?.kind === 'agent' && typeof s.block === 'string') {
      out.push({ block: s.block, name: String(u.name ?? 'Agent'), color: /^#[0-9a-f]{3,8}$/i.test(u.color) ? u.color : '#8b5cf6' })
    }
  })
  return out
}

function build(doc: PMNode, ws: Writer[]): DecorationSet {
  if (!ws.length) return DecorationSet.empty
  const want = new Map(ws.map((w) => [w.block, w]))
  const decos: Decoration[] = []
  doc.descendants((node, pos) => {
    if (!want.size) return false
    const w = node.attrs.id ? want.get(node.attrs.id) : undefined
    if (w) {
      want.delete(node.attrs.id)
      decos.push(
        Decoration.node(pos, pos + node.nodeSize, {
          class: 'oad-agent-writing',
          style: `--oad-agent-color: ${w.color}`,
          'data-agent': w.name,
        }),
      )
    }
    return !node.isTextblock
  })
  return DecorationSet.create(doc, decos)
}

export const AgentPresence = Extension.create<{ awareness: Awareness | null }>({
  name: 'agentPresence',
  addOptions() {
    return { awareness: null }
  },
  addProseMirrorPlugins() {
    const awareness = this.options.awareness
    if (!awareness) return []
    return [
      new Plugin<DecorationSet>({
        key: agentPresenceKey,
        state: {
          init: (_, state) => build(state.doc, writers(awareness)),
          apply(tr, set, _old, state) {
            // 远端改动会整段换掉节点，装饰映射不过去，重新找一遍
            if (tr.getMeta(agentPresenceKey) || (tr.docChanged && set !== DecorationSet.empty && isChangeOrigin(tr)))
              return build(state.doc, writers(awareness))
            return tr.docChanged ? set.map(tr.mapping, tr.doc) : set
          },
        },
        props: {
          decorations: (state) => agentPresenceKey.getState(state),
        },
        view: (view) => {
          let last = ''
          const onChange = () => {
            const key = JSON.stringify(writers(awareness))
            if (key === last) return
            last = key
            view.dispatch(view.state.tr.setMeta(agentPresenceKey, true))
          }
          awareness.on('change', onChange)
          return { destroy: () => awareness.off('change', onChange) }
        },
      }),
    ]
  },
})
