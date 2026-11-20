// 组件共用的客户端：<OadProvider client={client}> 包一层，里面的组件（评论栏、数据库视图……）就不用一个个传 client。
import type { OadClient } from '@open-agent-docs/client/core'
import { createContext, useContext, type ReactNode } from 'react'

const Ctx = createContext<OadClient | null>(null)

export function OadProvider({ client, children }: { client: OadClient; children: ReactNode }) {
  return <Ctx.Provider value={client}>{children}</Ctx.Provider>
}

/** 拿到 <OadProvider> 给的客户端；没有就报错（说清楚要怎么包） */
export function useOadClient(explicit?: OadClient | null): OadClient {
  const c = useContext(Ctx)
  const client = explicit ?? c
  if (!client) throw new Error('@open-agent-docs/editor：组件外面要包一层 <OadProvider client={createClient(...)}>，或者直接传 client')
  return client
}
