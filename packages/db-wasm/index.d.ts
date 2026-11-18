// @oad/db 的类型。数据结构见 docs/phase4-contract.md「模型」，引擎的取舍见「引擎实现说明」。
import type { InitInput, InitOutput, SyncInitInput } from './dist/oad_db'

export type PropType =
  | 'title' | 'text' | 'number' | 'select' | 'multi_select' | 'status' | 'date' | 'person' | 'checkbox' | 'url' | 'email'
  | 'phone' | 'files' | 'created_time' | 'updated_time' | 'created_by' | 'updated_by' | 'relation' | 'rollup' | 'formula'
  | 'unique_id' | 'button'

export interface SelectOption {
  id: string
  name: string
  color?: string
  /** status：todo | doing | done */
  group?: 'todo' | 'doing' | 'done'
}

export interface Prop {
  key: string
  name: string
  type: PropType
  options?: SelectOption[]
  /** 数字：number | comma | percent（乘 100）| cny | usd */
  format?: string
  /** 数字显示几位小数 */
  precision?: number
  /** 日期显示：iso（默认）| zh | slash | relative */
  date_format?: string
  formula?: string
  /** mirror_of 有值：双向关联在这边的反向那一半 */
  relation?: { db_id: string; mirror_of?: string; [k: string]: unknown }
  /** 沿 relation（这个库的关联属性 key）取对方库的 prop，按 fn 归总 */
  rollup?: { relation: string; prop: string; fn: RollupFn | string }
  prefix?: string
  [k: string]: unknown
}

export interface Schema {
  props: Prop[]
  next_id?: number
  [k: string]: unknown
}

export interface Row {
  id: string
  seq?: number
  props: Record<string, unknown>
  /** 毫秒时间戳 */
  created_at?: number
  created_by?: string
  updated_at?: number
  updated_by?: string
  unique_id?: number
}

/** row：整行换掉（同步来的）；props：只改给了的格子（null / "" / [] 是清空）；deleted：删行。没有这一行时是加。 */
export interface Change {
  row_id: string
  props?: Record<string, unknown>
  row?: Row
  deleted?: boolean
}

export type RollupFn =
  | 'show_original' | 'show_unique' | 'count' | 'count_values' | 'count_unique' | 'count_empty' | 'count_not_empty'
  | 'percent_empty' | 'percent_not_empty' | 'sum' | 'average' | 'median' | 'min' | 'max' | 'range' | 'earliest' | 'latest'
  | 'date_range' | 'checked' | 'unchecked' | 'percent_checked' | 'percent_unchecked'

export type Cond =
  | 'eq' | 'neq' | 'contains' | 'not_contains' | 'starts_with' | 'ends_with' | 'gt' | 'gte' | 'lt' | 'lte'
  | 'empty' | 'not_empty' | 'is_within' | 'checked' | 'unchecked' | 'in_group'

export interface Condition {
  /** 属性 key（也认名字） */
  prop: string
  cond: Cond | string
  value?: unknown
}

export interface Filter {
  op: 'and' | 'or'
  items: (Condition | Filter)[]
}

export interface Sort {
  prop: string
  dir?: 'asc' | 'desc'
}

export interface View {
  filter?: Filter | Condition[] | null
  sorts?: Sort[]
  group_by?: string | null
  sub_group_by?: string | null
  /** 按日期分组：day | week | month（默认）| year | relative */
  group_date?: string
  sub_group_date?: string
  hide_empty_groups?: boolean
  /** 每列底部的汇总：{ key: fn }，fn 同 RollupFn，也认 avg、unique、empty、not_empty */
  totals?: Record<string, string>
  /** 本人临时的搜索 */
  search?: string
  limit?: number
  offset?: number
  [k: string]: unknown
}

export interface Group {
  /** 选项 id、人的 id、日期分段（2026-11）……；空值的分组是 "" */
  key: string
  name: string
  color?: string
  ids: string[]
  count: number
  groups?: Group[]
  totals?: Record<string, unknown>
}

export interface QueryResult {
  ids: string[]
  /** 符合条件的总行数（给了 limit / offset 时 ids 只是一段） */
  total: number
  groups?: Group[]
  /** 占比（percent_*）是 0–1 */
  totals?: Record<string, unknown>
}

export interface Person {
  id: string
  name: string
}

export interface EngineOptions {
  /** 主库自己的 id（关联指向自己时用） */
  id?: string
  /** 固定“现在”（毫秒时间戳，测试用）；不给就每次调用前取浏览器的时间 */
  now?: number
  /** 当地时间比 UTC 快几分钟（北京时间 480）；不给用浏览器的 */
  tz?: number
  people?: Person[]
}

export declare class DbEngine {
  static create(schema: Schema | string, opts?: EngineOptions): Promise<DbEngine>
  /** 已经 init / initSync 过时同步建 */
  static createSync(schema: Schema | string, opts?: EngineOptions): DbEngine
  loadRows(rows: Row[] | string): void
  loadRelated(dbId: string, schema: Schema | string, rows: Row[] | string): void
  /** 返回受影响的行 id（改了的，和通过关联连着、算出来的值可能变了的） */
  apply(changes: Change[] | string): string[]
  setSchema(schema: Schema | string): void
  setPeople(people: Person[]): void
  /** 固定时钟（之后不再跟着浏览器走） */
  setClock(nowMs: number, tzOffsetMinutes: number): void
  /** 出错（属性、选项不存在，条件不对）会抛 Error，信息是中文 */
  query(view: View | string): QueryResult
  /** 公式、汇总、反向关联（对方行 id）、创建 / 编辑时间（{ start, time }）、创建 / 编辑人、自动编号 */
  computed(rowId: string): Record<string, unknown>
  display(key: string, value: unknown): string
  /** 一行每格给人看的文字（空的不列） */
  displayRow(rowId: string): Record<string, string>
  /** at 是出错处在 src 里的下标（UTF-16）；给了 key 还查用没用到自己、成没成环 */
  checkFormula(src: string, key?: string): { ok: true } | { error: string; at: number }
  /** 用户输入 / 粘贴的文字（或存的样子）→ 存的样子；空是 null；认不出抛 Error（中文，说清楚该怎么写） */
  normalize(key: string, input: unknown): unknown
  /** 表结构里有问题的属性（公式写错、成环、汇总的关联不在了） */
  schemaErrors(): { key: string; error: string; at?: number }[]
  free(): void
}

export declare function init(input?: InitInput | Promise<InitInput> | { module_or_path: InitInput | Promise<InitInput> }): Promise<InitOutput>
export declare function initSync(module: SyncInitInput | { module: SyncInitInput }): InitOutput
export default init
