// 工作区里认识的人和 Agent：用 @oad/editor 的（绑上网页的客户端）。
import { loadAgents as load, useAgents as use, usePeople as usePeopleOf, type Candidate } from '@oad/editor'
import { client, type Me, type Person, type Thread } from './api'

export type { Candidate }
export const loadAgents = (ws: string, force = false) => load(client, ws, force)
export const useAgents = (ws: string) => use(client, ws)
export const usePeople = (ws: string, me: Me, authors: Record<string, Person>, threads: Thread[]) => usePeopleOf(client, ws, me, authors, threads)
