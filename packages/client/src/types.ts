// 接口里的数据结构。现在是照着 docs/phase*-contract.md 手写的；服务器出 OpenAPI 以后（utoipa），
// 用 openapi-typescript 生成到 openapi.ts，这里改成从那里取（名字不变，用的地方不用改）。

export interface Page {
  id: string
  // 第 4 阶段：database = 数据库，row = 库里的一行（侧栏不列）
  kind?: 'page' | 'database' | 'row' | string
  workspace_id?: string
  parent_id: string | null
  position: string
  title: string
  icon: string | null
  updated_at: number
}

export interface Me {
  user: { id: string; name: string; email?: string | null; kind?: string; avatar?: string | null; locale?: string | null }
  workspaces: { id: string; name: string; icon?: string | null; role: 'viewer' | 'editor' | 'admin' | 'owner' }[]
}

export type AuthMode = 'local' | 'header' | 'none'

export type Scope = 'read' | 'write' | 'admin'

export interface Agent {
  id: string
  name: string
  about?: string | null
  scope: Scope
  webhook?: string | null
  created_by?: string | null
  created_at: number
  last_used_at?: number | null
  revoked_at?: number | null
  // 下面两项约定里没写，服务器给了就显示
  pending?: number
  pending_notifications?: number
  webhook_error?: string | null
}

export interface Person {
  kind: 'user' | 'agent' | string
  id: string
  name: string
}

export interface Comment {
  id: string
  thread_id: string
  author_kind: string
  author_id: string
  author?: Person
  body: string
  created_at: number
  edited_at?: number | null
  deleted_at?: number | null
}

export interface AnchorIn {
  block_id: string | null
  start: string
  end: string
  quote: string
}

export interface Thread {
  id: string
  page_id: string
  block_id?: string | null
  // 约定的表字段是 anchor_start / anchor_end，也接受 anchor 对象
  anchor?: { block_id?: string | null; start?: string | null; end?: string | null; quote?: string | null } | null
  anchor_start?: string | null
  anchor_end?: string | null
  quote?: string | null
  created_by?: string | null
  created_at: number
  resolved_at?: number | null
  resolved_by?: string | null
  comments: Comment[]
}

export interface InboxItem {
  id: number | string
  kind?: string
  type?: string
  page?: { id: string; title?: string } | null
  page_id?: string
  thread_id?: string | null
  comment?: { id?: string; author?: Person; body?: string } | null
  actor?: Person | null
  by?: Person | null
  quote?: string | null
  created_at: number
  read_at?: number | null
  // 服务器把内容放在 data 里（{ id, type, data: { page, thread_id, comment, quote, by }, created_at, read_at }）
  data?: Partial<InboxItem>
}


export interface SearchHit {
  page_id: string
  block_id?: string | null
  title: string
  /** 命中的片段，命中的词用 <mark> 包着（别的都转义过） */
  snippet?: string
  kind: 'page' | 'row' | 'database' | string
  score?: number
  updated_at?: number
  icon?: string | null
  /** 这一页一共命中几处（服务器按页合并以后给） */
  hits?: number
}

export interface Version {
  id: string
  created_at: number
  kind: 'auto' | 'agent' | 'restore' | 'manual' | string
  label?: string | null
  authors?: Person[]
}

export interface DiffBlock {
  op: 'same' | 'added' | 'removed' | 'changed'
  id?: string | null
  type?: string
  /** 那一版 / 现在的 Markdown */
  before?: string | null
  after?: string | null
  by?: Person[]
}
export interface VersionDiff {
  blocks: DiffBlock[]
}

export interface DbHistoryItem {
  seq: number
  kind: 'cell' | 'schema' | 'row' | string
  row_id?: string | null
  row_title?: string | null
  prop?: string | null
  prop_name?: string | null
  old?: unknown
  new?: unknown
  /** 给人看的说明（表结构改动） */
  summary?: string | null
  by?: Person | string | null
  at: number
  undone?: boolean
}

export interface TrashItem {
  id: string
  title: string
  icon?: string | null
  kind?: 'page' | 'database' | 'row' | string
  deleted_at: number
  deleted_by?: Person | string | null
  parent_title?: string | null
}

export interface Share {
  url: string
  token?: string
  include_children?: boolean
  indexable?: boolean
  created_at?: number
}

export interface Invite {
  token: string
  url: string
  role: string
  expires_at?: number | null
  created_at?: number
  uses?: number
}

export interface InviteInfo {
  workspace: { id: string; name: string; icon?: string | null }
  role: string
  invited_by?: { name: string } | null
  expires_at?: number | null
  signup?: boolean
}


// ---------- 数据库（docs/phase4-contract.md） ----------
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
  // 下面几项约定里没列，网页存进视图：每列底部的汇总函数、折叠了的分组
  totals?: Record<string, string>
  collapsed?: string[]
}

export interface Row {
  id: string
  seq?: number
  // 手动排序的位置（服务器给的分数索引，按字符串比大小）
  position?: string
  props: Record<string, unknown>
  created_at?: number
  created_by?: string
  updated_at?: number
  updated_by?: string
  unique_id?: number
}

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

/** @oad/db 的接口（WASM）。网页只通过它算筛选、排序、分组、公式、汇总。 */
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

export interface DbMeta {
  page: { id: string; title: string; icon?: string | null; parent_id?: string | null; workspace_id?: string }
  schema: Schema
  views: View[]
  seq: number
}
