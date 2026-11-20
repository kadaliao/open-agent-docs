// 接口里的数据结构：从 docs/openapi.json 生成的 openapi.ts 里取（pnpm --filter @open-agent-docs/client gen:types）。
// 只有两种还是手写的：文档里只写了 `object`、没写里面是什么的（表结构、视图、几个 `{ name }` 之类，下面标着“文档没写”），
// 和只在客户端里有的（WASM 引擎的接口、收件箱拍平以后的样子）。
import type { components } from './openapi.ts'

type S = components['schemas']

export type { components, paths, operations, webhooks } from './openapi.ts'

export type Page = S['Page']
export type Me = S['Me']
export type Role = S['Role']
export type Scope = S['Scope']
export type SignupMode = S['SignupMode']
export type PrincipalKind = S['PrincipalKind']
/** 文档里 mode 只写了 string */
export type AuthMode = 'local' | 'header' | 'none' | 'jwt'
export type AuthConfig = Omit<S['AuthConfigBody'], 'mode'> & { mode: AuthMode }

/** Agent 列表里的一项多 pending / webhook_error；建出来、改完返回的没有 */
export type Agent = S['Agent'] & { pending?: number; webhook_error?: string | null }
export type CreatedAgent = S['CreatedAgent']
export type AgentNotification = S['AgentNotification']
export type AgentNotes = S['AgentNotes']

export type Person = S['AuthorRef']
export type Comment = S['CommentView']
export type AnchorIn = S['Anchor']
export type Thread = S['ThreadView']

export type SearchResult = S['SearchResult']
/** 搜索结果，客户端按页合并以后多一个 hits（这一页一共命中几处） */
export type SearchHit = SearchResult & { hits?: number }

export type Version = S['VersionMeta']
export type VersionList = S['VersionList']
export type DiffBlock = S['BlockDiff']
export type VersionDiff = S['VersionDiff']

/** by：文档写的 object，实际也可能是认不出的人的原样引用（字符串）；old / new 是任意 JSON（文档写的 object） */
export type DbHistoryItem = Omit<S['DbHistoryItem'], 'by' | 'old' | 'new'> & { by?: Person | string | null; old?: unknown; new?: unknown }
export type DbHistory = Omit<S['DbHistory'], 'items'> & { items: DbHistoryItem[] }

/** deleted_by：文档没写里面是什么 */
export type TrashItem = Omit<S['TrashItem'], 'deleted_by'> & { deleted_by?: Person | string | null }
export type TrashList = Omit<S['TrashList'], 'items'> & { items: TrashItem[] }

export type Share = S['ShareView']
export type Invite = S['InviteView']
/** workspace / invited_by：文档没写里面是什么 */
export type InviteInfo = Omit<S['InviteInfo'], 'workspace' | 'invited_by'> & {
  workspace: { id: string; name: string; icon?: string | null }
  invited_by?: { name: string } | null
}
export type Member = S['Member']
export type Workspace = S['Workspace']
export type Uploaded = S['Uploaded']
export type UnfurlCard = S['UnfurlCard']

export type InboxNotification = S['InboxNotification']
export type Inbox = S['Inbox']
/** 收件箱的一条，客户端把 data 里的东西拍平到外面（data 文档没写里面是什么） */
export interface InboxItem {
  id: number
  type: string
  workspace_id?: string
  page?: { id: string; title?: string } | null
  thread_id?: string | null
  comment?: { id?: string; author?: Person; body?: string } | null
  by?: Person | null
  quote?: string | null
  created_at: number
  read_at?: number | null
  data?: Record<string, unknown>
}

// ---------- 数据库 ----------
// 表结构、视图在文档里只写了 object，这里照 docs/phase4-contract.md 手写。

export type PropType =
  | 'title'
  | 'text'
  | 'number'
  | 'select'
  | 'multi_select'
  | 'status'
  | 'date'
  | 'person'
  | 'checkbox'
  | 'url'
  | 'email'
  | 'phone'
  | 'files'
  | 'created_time'
  | 'updated_time'
  | 'created_by'
  | 'updated_by'
  | 'relation'
  | 'rollup'
  | 'formula'
  | 'unique_id'
  | 'button'

export type Color = 'default' | 'gray' | 'brown' | 'orange' | 'yellow' | 'green' | 'blue' | 'purple' | 'pink' | 'red'

export interface SelectOption {
  id: string
  name: string
  color: Color
  group?: 'todo' | 'doing' | 'done'
}

export interface Prop {
  key: string
  name: string
  type: PropType
  options?: SelectOption[]
  // number：number | comma | percent | cny | usd；date：date | datetime
  format?: string
  // date：iso | zh | slash | relative
  date_format?: string
  formula?: string
  relation?: { db_id: string; mirror_of?: string; two_way?: boolean }
  rollup?: { relation: string; prop: string; fn: string }
  // unique_id 的前缀
  prefix?: string
  // button：按了改哪些属性
  button?: { label?: string; set?: Record<string, unknown> }
}

export interface Schema {
  props: Prop[]
  next_id?: number
}

export interface Cond {
  prop: string
  cond: string
  value?: unknown
}
export interface Filter {
  op: 'and' | 'or'
  items: (Cond | Filter)[]
}

export interface Sort {
  prop: string
  dir: 'asc' | 'desc'
}

export type ViewType = 'table' | 'board' | 'calendar' | 'gallery' | 'list'

export interface View {
  id: string
  name: string
  type: ViewType
  filter?: Filter | null
  sorts?: Sort[]
  group_by?: string | null
  sub_group_by?: string | null
  hidden?: string[]
  order?: string[]
  widths?: Record<string, number>
  calendar_by?: string | null
  cover?: string | null
  card_size?: 'small' | 'medium' | 'large'
  wrap?: boolean
  // 每列底部的汇总函数、折叠了的分组
  totals?: Record<string, string>
  collapsed?: string[]
}

/** 服务器存的一行（DbRow）。客户端乐观加的行还没有服务器给的那些字段，所以除了 id、props 都可以没有 */
export type DbRow = S['DbRow']
export type Row = Pick<DbRow, 'id'> & Partial<Omit<DbRow, 'id' | 'props'>> & { props: Record<string, unknown> }
export type RowsDelta = S['RowsDelta']
/** 服务器查询（POST /databases/{id}/query）的结果；和下面 WASM 引擎的 QueryResult 不是一个东西 */
export type ServerQueryResult = S['QueryResult']

export interface DateValue {
  start: string
  end?: string
  time?: boolean
}

export interface Group {
  key: string
  name: string
  color?: string
  ids: string[]
  count: number
}

export interface QueryResult {
  ids: string[]
  groups?: Group[]
  totals?: Record<string, unknown>
}

/** @open-agent-docs/db 的接口（WASM）。网页只通过它算筛选、排序、分组、公式、汇总。 */
export interface DbEngine {
  loadRows(rows: Row[] | string): void
  loadRelated(dbId: string, schema: Schema, rows: Row[]): void
  apply(changes: { row_id: string; props?: Record<string, unknown>; deleted?: boolean; row?: Row }[]): string[]
  setSchema(schema: Schema): void
  query(view: Partial<View> & { search?: string }): QueryResult
  computed(rowId: string): Record<string, unknown>
  display(key: string, value: unknown): string
  checkFormula(src: string, key?: string): { ok: true } | { error: string; at: number }
  setPeople?(people: { id: string; name: string }[]): void
  normalize(key: string, input: unknown): unknown
}

/** GET /databases/{id}：schema / views 文档里只写了 object */
export type DbMeta = Omit<S['DbInfo'], 'schema' | 'views'> & { schema: Schema; views: View[] }
