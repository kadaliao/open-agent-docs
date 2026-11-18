// 资料库的数据库（open_agent_docs/dbs.py）：类型、显示值、颜色、筛选条件。显示的样子和服务器给模型看的一样（dbs.display）。
import { api, openPage } from '../../store'
import { nameOf } from '../../authors'
import { locale, t } from '../../i18n'

export type PropType = 'title' | 'text' | 'number' | 'select' | 'multi_select' | 'status' | 'date' | 'person' | 'checkbox'
  | 'url' | 'email' | 'phone' | 'created_time' | 'updated_time' | 'created_by' | 'updated_by'
  | 'relation' | 'rollup' | 'formula' | 'files' | 'unique_id' | 'button'
export type DbOption = { id: string; name: string; color: string; group?: 'todo' | 'doing' | 'done' }
export type DbFile = { name: string; url: string }
export type DbProp = { key: string; name: string; type: PropType; options?: DbOption[]; format?: string
  formula?: string                                       // 公式
  db?: number; mirror_of?: string; pair?: string          // 关联：到哪个库；双向时对方的那一半
  relation?: string; target?: string; fn?: string         // 汇总：沿哪个关联、取对方哪个属性、怎么算
  prefix?: string; next?: number                          // 自动编号
  label?: string; set?: Record<string, any>; comment?: string; agent?: number | null   // 按钮
}
export type Filter = { prop: string; op: string; value?: any }
export type Sort = { prop: string; desc: boolean }
export type DbView = { id: string; name: string; type: 'table' | 'board'; sort?: Sort[]; filter?: Filter[]; group_by?: string; hidden?: string[]; widths?: Record<string, number> }
export type DbRow = { id: number; title: string; version: number; pos: number; created: number; updated: number
  created_by: string; updated_by: string; props: Record<string, any>; locked: boolean
  calc?: Record<string, any> }   // 公式、汇总、反向关联：服务器算好的
export type DbData = { id: number; title: string; locked: boolean; version: number; schema: DbProp[]; views: DbView[]; view: string
  total: number; count: number; rows: DbRow[]; refs: Record<number, string>; groups?: Record<string, number> }

export const TYPES: [PropType, string, string][] = [
  ['text', t('文本'), '≡'], ['number', t('数字'), '#'], ['select', t('单选'), '◉'], ['multi_select', t('多选'), '☰'],
  ['status', t('状态'), '◔'], ['date', t('日期'), '◷'], ['person', t('人员'), '◯'], ['checkbox', t('勾选'), '☑'],
  ['url', t('网址'), '↗'], ['email', t('邮箱'), '@'], ['phone', t('电话'), '☏'],
  ['relation', t('关联'), '⇄'], ['rollup', t('汇总'), 'Σ'], ['formula', t('公式'), 'ƒ'], ['files', t('文件'), '⎘'],
  ['unique_id', t('自动编号'), '№'], ['button', t('按钮'), '▸'],
  ['created_time', t('创建时间'), '◷'], ['updated_time', t('最后编辑时间'), '◷'], ['created_by', t('创建人'), '◯'], ['updated_by', t('最后编辑人'), '◯'],
]
export const typeName = (type: PropType) => type === 'title' ? t('名称') : TYPES.find(x => x[0] === type)?.[1] || type
export const typeIcon = (type: PropType) => type === 'title' ? 'Aa' : TYPES.find(x => x[0] === type)?.[2] || '·'
export const COMPUTED = new Set<PropType>(['created_time', 'updated_time', 'created_by', 'updated_by'])
export const CALCULATED = new Set<PropType>(['formula', 'rollup'])
/** 不能直接改值的：自动算的、公式、汇总、自动编号、按钮。 */
export const readOnly = (p: DbProp) => COMPUTED.has(p.type) || CALCULATED.has(p.type) || p.type === 'unique_id' || p.type === 'button'
export const isMirror = (p: DbProp) => p.type === 'relation' && !!p.mirror_of
export const ROLLUPS: [string, string][] = [['count', t('条数')], ['count_values', t('有值的个数')], ['count_unique', t('不同值的个数')],
  ['count_empty', t('空的个数')], ['percent_empty', t('空的占比')], ['sum', t('求和')], ['average', t('平均')], ['median', t('中位数')],
  ['min', t('最小')], ['max', t('最大')], ['range', t('最大减最小')], ['earliest', t('最早')], ['latest', t('最晚')],
  ['checked', t('勾了几个')], ['percent_checked', t('勾了的占比')], ['show', t('显示原值')], ['show_unique', t('去重显示')]]
export const OPTIONED = new Set<PropType>(['select', 'multi_select', 'status'])
export const COLORS = ['default', 'gray', 'brown', 'orange', 'yellow', 'green', 'blue', 'purple', 'pink', 'red']
export const COLOR_NAMES: Record<string, string> = {
  default: t('默认'), gray: t('灰'), brown: t('棕'), orange: t('橙'), yellow: t('黄'), green: t('绿'), blue: t('蓝'), purple: t('紫'), pink: t('粉'), red: t('红'),
}
export const GROUPS: ['todo' | 'doing' | 'done', string][] = [['todo', t('未开始')], ['doing', t('进行中')], ['done', t('已完成')]]
export const FORMATS: [string, string][] = [['number', t('数字')], ['comma', t('千分位')], ['percent', t('百分比')], ['cny', t('人民币')], ['usd', t('美元')]]

export function valueOf(row: DbRow, p: DbProp): any {
  if (CALCULATED.has(p.type) || isMirror(p)) return row.calc?.[p.key] ?? null
  switch (p.type) {
    case 'button': return null
    case 'title': return row.title
    case 'created_time': return row.created
    case 'updated_time': return row.updated
    case 'created_by': return row.created_by
    case 'updated_by': return row.updated_by
    default: return row.props[p.key]
  }
}

export const isEmpty = (v: any) => v === null || v === undefined || v === '' || (Array.isArray(v) && !v.length) || v === false

export function fmtNumber(p: DbProp, n: number) {
  const s = (x: number) => x.toLocaleString(locale)
  switch (p.format) {
    case 'comma': return s(n)
    case 'percent': return `${n}%`
    case 'cny': return `¥${s(n)}`
    case 'usd': return `$${s(n)}`
    default: return String(n)
  }
}

export const optionOf = (p: DbProp, id: string) => p.options?.find(o => o.id === id)

/** 公式、汇总算出来的值给人看的样子（数按格式，日期、列表、真假）。 */
export function calcText(p: DbProp, v: any): string {
  if (v === null || v === undefined || v === '' || (Array.isArray(v) && !v.length)) return ''
  if (typeof v === 'boolean') return v ? '✓' : '✗'
  if (typeof v === 'number') return fmtNumber(p, v)
  if (Array.isArray(v)) return v.join(', ')
  if (typeof v === 'object') return v.start + (v.end ? ` → ${v.end}` : '')
  return String(v)
}

/** 给人看的值（筛选、看板卡片、分享的样子都用它）。关联的标题从 refs 取。 */
export function display(p: DbProp, v: any, refs: Record<number, string> = {}): string {
  if (CALCULATED.has(p.type)) return calcText(p, v)
  if (isEmpty(v)) return ''
  switch (p.type) {
    case 'relation': return (v as number[]).filter(id => id in refs).map(id => refs[id] || t('无标题')).join(', ')
    case 'files': return (v as DbFile[]).map(f => f.name).join(', ')
    case 'unique_id': return `${p.prefix || ''}${v}`
    case 'number': return fmtNumber(p, v)
    case 'select': case 'status': return optionOf(p, v)?.name || '?'
    case 'multi_select': return (v as string[]).map(id => optionOf(p, id)?.name || '?').join(', ')
    case 'date': return v.start + (v.end ? ` → ${v.end}` : '')
    case 'person': return (v as string[]).map(x => nameOf(x)).join(', ')
    case 'created_by': case 'updated_by': return nameOf(v)
    case 'checkbox': return v ? '✓' : ''
    case 'created_time': case 'updated_time': return new Date(v * 1000).toLocaleString(locale, { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
    default: return String(v)
  }
}

// 筛选的条件：每种类型能用哪些
const TEXT_OPS = ['contains', 'not_contains', 'eq', 'neq', 'empty', 'not_empty']
const OPS_BY: Partial<Record<PropType, string[]>> = {
  number: ['eq', 'neq', 'gt', 'gte', 'lt', 'lte', 'empty', 'not_empty'],
  select: ['eq', 'neq', 'empty', 'not_empty'], status: ['eq', 'neq', 'empty', 'not_empty'],
  multi_select: ['contains', 'not_contains', 'empty', 'not_empty'], person: ['contains', 'not_contains', 'empty', 'not_empty'],
  created_by: ['eq', 'neq'], updated_by: ['eq', 'neq'],
  date: ['eq', 'lt', 'gt', 'lte', 'gte', 'empty', 'not_empty'], created_time: ['eq', 'lt', 'gt', 'lte', 'gte'], updated_time: ['eq', 'lt', 'gt', 'lte', 'gte'],
  checkbox: ['eq'],
  relation: ['contains', 'not_contains', 'empty', 'not_empty'], files: ['contains', 'not_contains', 'empty', 'not_empty'],
  unique_id: ['eq', 'neq', 'gt', 'gte', 'lt', 'lte'],
  formula: ['eq', 'neq', 'contains', 'not_contains', 'gt', 'gte', 'lt', 'lte', 'empty', 'not_empty'],
  rollup: ['eq', 'neq', 'contains', 'not_contains', 'gt', 'gte', 'lt', 'lte', 'empty', 'not_empty'],
}
export const opsOf = (type: PropType) => OPS_BY[type] || TEXT_OPS
export function opName(op: string, type: PropType) {
  const dated = type === 'date' || type === 'created_time' || type === 'updated_time'
  return ({
    eq: t('是'), neq: t('不是'), contains: t('包含'), not_contains: t('不包含'),
    gt: dated ? t('晚于') : t('大于'), gte: dated ? t('不早于') : t('大于等于'), lt: dated ? t('早于') : t('小于'), lte: dated ? t('不晚于') : t('小于等于'),
    empty: t('是空的'), not_empty: t('不是空的'),
  } as Record<string, string>)[op] || op
}

// ---------- 接口 ----------
export const PAGE = 200   // 一次取多少行，滚到底再取（open_agent_docs/dbs.PAGE）
const q = (o: Record<string, any>) => { const s = new URLSearchParams(Object.entries(o).filter(([, v]) => v !== undefined && v !== null && v !== '').map(([k, v]) => [k, String(v)])).toString(); return s ? '?' + s : '' }
export const dbApi = {
  get: (id: number, view?: string | null, limit?: number, include?: number[]) =>
    api<DbData>(`/api/dbs/${id}${q({ view, limit, include: include?.join(',') })}`),
  schema: (id: number, ops: any[], view?: string, limit?: number) => api<DbData>(`/api/dbs/${id}/schema`, { method: 'PATCH', body: { ops, view, limit } }),
  views: (id: number, views: DbView[], view?: string, limit?: number) => api<DbData>(`/api/dbs/${id}/views`, { method: 'PUT', body: { views, view, limit } }),
  find: (id: number, text: string) => api<{ id: number; title: string }[]>(`/api/dbs/${id}/find${q({ q: text })}`),
  press: (id: number, row: number, key: string) => api<{ row: DbRow; changed: string[]; comment: any }>(`/api/dbs/${id}/rows/${row}/press/${key}`, { method: 'POST' }),
  add: (id: number, body: { props?: Record<string, any>; title?: string; after?: number | null }) => api<DbRow>(`/api/dbs/${id}/rows`, { method: 'POST', body }),
  update: (id: number, row: number, props: Record<string, any>, version?: number) =>
    api<DbRow>(`/api/dbs/${id}/rows/${row}`, { method: 'PATCH', body: { props, version } }),
  move: (id: number, row: number, after: number | null) => api(`/api/dbs/${id}/rows/${row}/move`, { method: 'POST', body: { after } }),
  remove: (id: number, row: number) => api(`/api/dbs/${id}/rows/${row}`, { method: 'DELETE' }),
}

export async function newDb(parent: number | null = null) {
  const db = await api('/api/dbs', { method: 'POST', body: { title: t('无标题数据库'), ...(parent ? { parent_id: parent } : {}) } })
  openPage(db.id)
}
