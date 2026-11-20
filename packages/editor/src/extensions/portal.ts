// 节点视图（不是 React）弹出来的东西（图片的大图预览）挂在哪。
// OadEditor 从 <OadPortalRoot> 拿到后放进这个扩展的选项（取的时候才读，宿主后来换了也能拿到最新的）；
// 没给的时候：编辑器在 Shadow DOM 里就挂到那个 shadow root 上（样式在那里），否则 document.body。
import { Extension, type Editor } from '@tiptap/core'

export const PortalRootExt = Extension.create<{ get: () => HTMLElement | null }>({
  name: 'oadPortalRoot',
  addOptions() {
    return { get: () => null }
  },
})

export function portalRoot(editor: Editor): HTMLElement | ShadowRoot {
  const ext = editor.extensionManager.extensions.find((e) => e.name === 'oadPortalRoot')
  const given = (ext?.options as { get?: () => HTMLElement | null } | undefined)?.get?.()
  if (given) return given
  try {
    const root = editor.view.dom.getRootNode()
    if (typeof ShadowRoot !== 'undefined' && root instanceof ShadowRoot) return root
  } catch {
    /* 还没挂到页面上 */
  }
  return document.body
}
