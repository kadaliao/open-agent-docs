// 设置页、回收站这些不是文档的页面：顶栏（侧栏开关、标题、通知）+ 能滚的内容。
import type { ReactNode } from 'react'
import { SidebarIcon } from '../icons'
import { Inbox } from '../Inbox'
import type { PageStore } from '../store'
import { t } from '../strings'

export function Shell({ store, title, sidebarCollapsed, onExpandSidebar, children }: { store: PageStore; title: ReactNode; sidebarCollapsed: boolean; onExpandSidebar: () => void; children: ReactNode }) {
  return (
    <div className="page">
      <header className="topbar">
        {sidebarCollapsed && (
          <button className="icon-btn" onClick={onExpandSidebar} aria-label={t('toggleSidebar')}>
            <SidebarIcon />
          </button>
        )}
        <nav className="crumbs">
          <span className="crumb-btn is-current">{title}</span>
        </nav>
        <div className="topbar-right">
          <Inbox store={store} />
        </div>
      </header>
      <div className="page-scroll">{children}</div>
    </div>
  )
}
