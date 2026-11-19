import { usePages, type PageStore } from './store'
import { t } from './strings'

/** 首屏（快照）上的标题：侧栏的页面列表到了才知道是什么，之前先空着（不先写“无标题”再变） */
export function PageTitle({ store, pageId }: { store: PageStore; pageId: string }) {
  const p = usePages(store).find((x) => x.id === pageId)
  return <>{p ? p.title || t('untitled') : ''}</>
}
