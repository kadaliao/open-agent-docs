// 组件共用的客户端：<OadProvider client={client}> 包一层，里面的组件（评论栏、数据库视图……）就不用一个个传 client。
import type { OadClient } from '@open-agent-docs/client/core'
import { createContext, useContext, type ReactNode } from 'react'

const Ctx = createContext<OadClient | null>(null)

export function OadProvider({ client, children }: { client: OadClient; children: ReactNode }) {
  return <Ctx.Provider value={client}>{children}</Ctx.Provider>
}

/** 浮层（弹出菜单、公式编辑器）挂在哪：默认 document.body；放进 Shadow DOM 时（@open-agent-docs/elements）给 shadow 里的一个元素，样式才跟得过去 */
export const OadPortalRoot = createContext<HTMLElement | null>(null)
export const usePortalRoot = () => useContext(OadPortalRoot) ?? document.body

/** 拿到 <OadProvider> 给的客户端；没有就报错（说清楚要怎么包） */
export function useOadClient(explicit?: OadClient | null): OadClient {
  const c = useContext(Ctx)
  const client = explicit ?? c
  if (!client) throw new Error('@open-agent-docs/editor：组件外面要包一层 <OadProvider client={createClient(...)}>，或者直接传 client')
  return client
}
