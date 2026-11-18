// 接口返回的数据（open_agent_docs/server.py）

export type Page = {
  id: number; bot_id: number | null; title: string; format: 'markdown' | 'html'; share_token: string | null; pinned: boolean
  created: number; updated: number; size?: number; content?: string
  version: number; locked: boolean   // version：每次修改 +1（pagedoc.py）；locked：Agent 只能读
  parent_id: number | null; pos: number   // 子页面：挂在哪个页面下面、同一层里的顺序
  agent_id: number | null   // 哪个 Agent 建的（经 /mcp）；bot_id 是从 OSA 导进来的页面原来的 Bot
  kind: 'page' | 'db' | 'row'   // db：数据库（dbs.py）；row：数据库的一行（页面列表里没有）
  ancestors?: { id: number; title: string }[]; children?: { id: number; title: string; format: string; kind?: string }[]   // 只有打开单个页面时给
}

export type SearchHit = { ref: number; title: string; snippet: string }

// 页面上显示 agent:<id> 写的段、评论用。撤销了的也在
export type AgentInfo = { id: number; name: string; revoked: boolean }

export type Agent = {
  id: number; name: string; scope: 'read' | 'write' | 'admin'; created: number; last_used: number | null; revoked: number | null
  webhook: string; about: string; pending: number
}

export type Bootstrap = { version: string; authed: boolean; lang: string; language_set: boolean; agents?: AgentInfo[] }
