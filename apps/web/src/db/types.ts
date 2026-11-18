// 数据库的数据结构（docs/phase4-contract.md「模型」）和计算引擎的接口（@oad/db，WASM）。
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
export const isGroup = (x: Cond | Filter): x is Filter => (x as Filter).items !== undefined

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
  loadRows(rows: Row[]): void
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
