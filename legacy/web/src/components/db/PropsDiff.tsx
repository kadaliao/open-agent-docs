// 数据库的行在“历史”里的属性：和正文的对比放在一起（doc/history.tsx），按库现在的表结构画。
// 改过的属性显示 旧 → 新，能只把这一个属性恢复回去；“只看这一版”时列出那一版所有有值的属性。
// 表结构后来改过（删了列、改了类型、删了选项）的值服务器标成 stale，不比、不恢复，只提一句。
import { t } from '../../i18n'
import { Value } from './cells'
import { CALCULATED, COMPUTED, isEmpty, isMirror, typeIcon, type DbProp } from './db'

export type PropsSnap = { title: string; props: Record<string, any>; stale?: string[] }
export type PropChange = { prop: DbProp; old: any; now: any }

const valOf = (s: PropsSnap, p: DbProp) => p.type === 'title' ? s.title : s.props[p.key]
const same = (a: any, b: any) => (isEmpty(a) && isEmpty(b)) || JSON.stringify(a) === JSON.stringify(b)
const shown = (schema: DbProp[], stale: Set<string>) => schema.filter(p => !COMPUTED.has(p.type) && !CALCULATED.has(p.type) && p.type !== 'button'
  && !isMirror(p) && !stale.has(p.key))   // 算出来的、按钮、反向关联不存在行里，历史里没有

/** from → to 改了哪些属性（名称也算）。 */
export function propChanges(schema: DbProp[], from: PropsSnap, to: PropsSnap): PropChange[] {
  const stale = new Set([...from.stale || [], ...to.stale || []])
  return shown(schema, stale).filter(p => !same(valOf(from, p), valOf(to, p))).map(p => ({ prop: p, old: valOf(from, p), now: valOf(to, p) }))
}

function Name({ p }: { p: DbProp }) {
  return <span className="dhv-pname"><span className="db-type-icon">{typeIcon(p.type)}</span>{p.name}</span>
}

const V = ({ p, v }: { p: DbProp; v: any }) => isEmpty(v) && p.type !== 'checkbox' ? <span className="muted">{t('空')}</span> : <Value prop={p} v={v} />

export function PropsDiff({ changes, mode, staleNames, busy, restoreLabel, onRestore }: {
  changes: PropChange[]; mode: 'now' | 'prev'; staleNames: string[]; busy: boolean; restoreLabel: string; onRestore: (c: PropChange) => void
}) {
  if (!changes.length && !staleNames.length) return null
  return (
    <div className="dhv-props">
      {changes.map(c => (
        <div key={c.prop.key} className="dhv-prop">
          <Name p={c.prop} />
          <span className="dhv-pvals">
            <span className="dhv-old"><V p={c.prop} v={c.old} /></span>
            <span className="muted">→</span>
            <span className="dhv-new"><V p={c.prop} v={c.now} /></span>
            <span className="dhv-tag small">{mode === 'now' ? t('后来改了') : t('改了')}</span>
          </span>
          <button className="link-btn small" disabled={busy} onClick={() => onRestore(c)}>{restoreLabel}</button>
        </div>
      ))}
      {!!staleNames.length && <p className="muted small">{t('表结构后来改过，{names}当时的值不显示。', { names: staleNames.map(n => `「${n}」`).join(t('、')) })}</p>}
    </div>
  )
}

/** “只看这一版”：那一版所有有值的属性。 */
export function PropsSnapshot({ schema, snap }: { schema: DbProp[]; snap: PropsSnap }) {
  const list = shown(schema, new Set(snap.stale)).filter(p => p.type !== 'title' && !isEmpty(valOf(snap, p)))
  if (!list.length) return null
  return (
    <div className="dhv-props">
      {list.map(p => <div key={p.key} className="dhv-prop"><Name p={p} /><span className="dhv-pvals"><V p={p} v={valOf(snap, p)} /></span></div>)}
    </div>
  )
}
