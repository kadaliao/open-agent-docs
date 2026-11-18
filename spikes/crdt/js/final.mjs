// 合并后的检查：JS 端（两种导入顺序）和 Rust 端结果一致；人和 Agent 的改动都在；schema 校验通过。
import { readFileSync } from 'node:fs'
import { OUT, loroLoad, loroToPm, yLoad, yToPm, posOf, norm } from './common.mjs'

const T = JSON.parse(readFileSync(OUT + 'targets.json'))
const text = (n) => n.textContent
let failed = 0
const ok = (cond, msg) => { console.log((cond ? '  ✓ ' : '  ✗ ') + msg); if (!cond) failed++ }

function check(name, load, toPm, files) {
  console.log(name)
  const a = toPm(load(files)).node
  const b = toPm(load([files[0], files[2], files[1]])).node
  a.check()
  ok(JSON.stringify(a.toJSON()) === JSON.stringify(b.toJSON()), 'JS 两种导入顺序结果一样')
  const rust = norm(JSON.parse(readFileSync(OUT + name + '.final.json')))
  ok(JSON.stringify(a.toJSON()) === JSON.stringify(rust), 'JS 和 Rust 合并结果一样')
  const P = posOf(a, T.P)?.node
  ok(P && text(P).includes('【用户在这里打字】'), '同一段：人打的字还在')
  ok(P && text(P).startsWith('AGENT') && text(P).endsWith('（Agent 补充一句）'), '同一段：Agent 改的开头和结尾也在')
  console.log('     P =', P && text(P).slice(0, 80) + '…' + text(P).slice(-30))
  ok(posOf(a, T.Q)?.node.type.name === 'heading', 'Agent 把段落改成标题')
  const R = posOf(a, T.R)
  ok(a.resolve(R.pos + R.node.nodeSize).nodeAfter?.attrs.id === 'agentnew1' && posOf(a, 'agentnew2'), 'Agent 在指定段落后面插了两段')
  ok(text(posOf(a, T.L).node).startsWith('列表里这一项被 Agent 改写了'), 'Agent 改了列表里嵌着的段落')
  ok(!posOf(a, T.S), 'Agent 删掉的段落没了')
  ok(!posOf(a, T.D), '人删掉的段落没了')
  const B = posOf(a, T.B).node
  ok(B.firstChild.marks.some((m) => m.type.name === 'bold') && B.firstChild.text.length === 5, '人加的加粗在')
}
check('loro', (f) => loroLoad(f), loroToPm, ['loro.base', 'loro.user', 'loro.agent'])
check('yrs', (f) => yLoad(f), yToPm, ['yrs.base', 'yrs.user', 'yrs.agent'])
process.exit(failed ? 1 : 0)
