// 页面里“谁写的”（段、评论、历史版本的 by）：user（你）、agent:<id>（经 /mcp 连进来的 Agent）、
// bot:<id>（从 OSA 导进来的页面里原来的 Bot，这里没有它们的名字，显示成“Bot #编号”）。
// 编辑器、评论、历史、页面列表共用这一份，免得各认各的。按现在的 store 取名字：用到它的组件自己订阅 agents 才会跟着刷新。
import { getState, useStore } from './store'
import { t } from './i18n'

export type Who = { name: string; color: string; agent?: boolean }

/** Agent（和导进来的 Bot）写的：名字和颜色；你写的、不知道是谁的：null。 */
export function who(by: string): Who | null {
  const { agents, colors } = getState()
  let m = by.match(/^agent:(\d+)$/)
  if (m) {
    const id = Number(m[1])
    const a = agents.find(x => x.id === id)
    return { name: a?.name || t('已删除的 Agent'), color: colors[id % colors.length], agent: true }
  }
  m = by.match(/^bot:(\d+)$/)
  if (m) return { name: t('Bot #{id}', { id: m[1] }), color: 'var(--fg3)' }
  return null
}

/** 显示用的名字：你写的是“你”，不知道是谁的用 other。 */
export function nameOf(by: string, other = t('有人')): string {
  return by === 'user' ? t('你') : who(by)?.name ?? other
}

/** 组件里用：Agent 的名字变了跟着重新渲染。 */
export function useAuthors() {
  useStore(s => s.agents)
  return nameOf
}

/** 没撤销的 Agent（评论里 @、选通知谁、数据库的人员和按钮用）。 */
export function useLiveAgents() {
  return useStore(s => s.agents).filter(a => !a.revoked)
}
