// @open-agent-docs/elements：Open Agent Docs 的 Web Components，宿主不用 React。
//   <script type="module" src="https://cdn.jsdelivr.net/npm/@open-agent-docs/elements@0.2.0/dist/elements.js"></script>
//   <oad-editor base-url="https://docs.example.com" page-id="…" token="…"></oad-editor>
//   <oad-viewer base-url="…" page-id="…"></oad-viewer>        只读，最轻（不连 WebSocket）
//   <oad-database base-url="…" db-id="…"></oad-database>      数据库视图
// 引进来就注册好；名字已经被占了（引了两份）就跳过。
import { OadDatabaseElement } from './database.ts'
import { OadEditorElement } from './editor.ts'
import { OadViewerElement } from './viewer.ts'

export { OadDatabaseElement, OadEditorElement, OadViewerElement }
export type * from './types.ts'

const defs: [string, CustomElementConstructor][] = [
  ['oad-editor', OadEditorElement],
  ['oad-viewer', OadViewerElement],
  ['oad-database', OadDatabaseElement],
]
if (typeof customElements !== 'undefined') for (const [name, c] of defs) if (!customElements.get(name)) customElements.define(name, c)
