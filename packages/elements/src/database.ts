// <oad-database db-id="…" base-url="…" token="…" view-id="…" readonly>：数据库视图（表格、看板、日历、画廊、列表）。
// 界面在 database-app.tsx，第一次用到时才加载。
import { OadBaseElement } from './base.ts'
import type { OadDatabaseElement as Api } from './types.ts'

export class OadDatabaseElement extends OadBaseElement implements Api {
  static get observedAttributes() {
    return [...super.observedAttributes, 'db-id', 'view-id', 'readonly']
  }
  private rerender: (() => void) | null = null

  get dbId(): string {
    return this.getAttribute('db-id') ?? ''
  }
  set dbId(v: string) {
    this.setAttribute('db-id', v)
  }
  get viewId(): string | null {
    return this.getAttribute('view-id')
  }
  set viewId(v: string | null) {
    if (v) this.setAttribute('view-id', v)
    else this.removeAttribute('view-id')
  }
  get readonly(): boolean {
    return this.hasAttribute('readonly')
  }
  set readonly(v: boolean) {
    this.toggleAttribute('readonly', !!v)
  }
  protected upgradeProps() {
    return [...super.upgradeProps(), 'dbId', 'viewId', 'readonly']
  }

  protected key() {
    return this.dbId || null
  }

  protected attributeUpdated(name: string) {
    if ((name === 'view-id' || name === 'readonly') && this.rerender) this.rerender()
    else this.schedule()
  }

  protected start() {
    const dbId = this.dbId
    const stop = this.lazyStart(async () => {
      const { mountDatabase } = await import('./database-app.tsx')
      return () => {
        const m = mountDatabase(this.host(), { dbId, opts: () => ({ viewId: this.viewId, readonly: this.readonly }) })
        this.rerender = m.rerender
        return m.stop
      }
    })
    return () => {
      this.rerender = null
      stop()
    }
  }
}
