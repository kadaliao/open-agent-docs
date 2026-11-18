// 工作区里认识的人和 Agent：@ 候选、评论作者名字。约定里没有“工作区成员列表”接口，
// 人从自己、页面作者（/pages/{id}/authors）、评论作者里攒；Agent 从 /workspaces/{ws}/agents 拉。
import { useEffect, useMemo, useState, useSyncExternalStore } from 'react'
import { api, type Agent, type Me, type Person, type Thread } from './api'
import type { Candidate } from './comments/Composer'

const agentCache = new Map<string, { list: Agent[]; at: number }>()
const listeners = new Set<() => void>()
let version = 0

export async function loadAgents(ws: string, force = false) {
  const c = agentCache.get(ws)
  if (c && !force && Date.now() - c.at < 30_000) return c.list
  try {
    const list = await api.agents(ws)
    agentCache.set(ws, { list, at: Date.now() })
    version++
    listeners.forEach((l) => l())
    return list
  } catch {
    return c?.list ?? []
  }
}

export function useAgents(ws: string) {
  useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => version,
  )
  useEffect(() => {
    loadAgents(ws)
  }, [ws])
  return agentCache.get(ws)?.list ?? []
}

export function usePeople(ws: string, me: Me, authors: Record<string, Person>, threads: Thread[]) {
  const agents = useAgents(ws)
  const [extra] = useState(() => new Map<string, Person>())
  return useMemo(() => {
    const users = new Map<string, Person>()
    users.set(me.user.id, { kind: 'user', id: me.user.id, name: me.user.name })
    for (const p of Object.values(authors)) if (p.kind === 'user') users.set(p.id, p)
    for (const th of threads)
      for (const c of th.comments ?? []) if (c.author?.kind === 'user') users.set(c.author.id, c.author)
    extra.forEach((p, k) => users.set(k, p))
    const agentMap = new Map(agents.map((a) => [a.id, a]))
    const candidates: Candidate[] = [
      ...agents.filter((a) => !a.revoked_at).map((a) => ({ kind: 'agent', id: a.id, name: a.name, about: a.about })),
      ...[...users.values()].filter((u) => u.id !== me.user.id),
    ]
    const nameOf = (kind: string, id: string) => {
      if (kind === 'agent') return agentMap.get(id)?.name ?? 'Agent'
      if (id === me.user.id) return me.user.name
      return users.get(id)?.name ?? id
    }
    return { candidates, nameOf }
  }, [agents, authors, threads, me, extra])
}
