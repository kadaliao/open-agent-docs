// 检查打包出来的 dist/style.css：选择器里的每个 class、每个 @keyframes 名字、每个自定义属性（--x）都要带 oad- 前缀，
// 不然宿主页面里同名的 class / 动画 / 变量会被这份样式改掉（或者反过来）。
// 例外只有别人的：ProseMirror 自己加的 class（ProseMirror-focused 之类）、shiki 给的 --shiki-*，和组件内部用的 --_*。
// 用法：node scripts/check-css-prefix.mjs [文件，默认 dist/style.css]；有不合格的就列出来并以 1 退出。
import { readFileSync } from 'node:fs'

const file = process.argv[2] ?? new URL('../dist/style.css', import.meta.url).pathname
const ALLOW_CLASS = [/^ProseMirror(-|$)/]
const ALLOW_VAR = [/^--oad-/, /^--_/, /^--shiki-/]

const css = readFileSync(file, 'utf8')
  .replace(/\/\*[\s\S]*?\*\//g, '')
  .replace(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'/g, '""')

const bad = { class: new Map(), keyframes: new Set(), var: new Set() }
const stack = []
let buf = ''
for (const ch of css) {
  if (ch === '{') {
    const head = buf.trim()
    if (head.startsWith('@')) {
      const kf = /^@(?:-webkit-)?keyframes\s+([\w-]+)/.exec(head)
      if (kf && !kf[1].startsWith('oad-')) bad.keyframes.add(kf[1])
      stack.push(kf ? 'keyframes' : 'at')
    } else if (stack.at(-1) === 'keyframes') {
      stack.push('frame')
    } else {
      for (const m of head.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)) {
        const c = m[1]
        if (!c.startsWith('oad-') && !ALLOW_CLASS.some((r) => r.test(c))) {
          if (!bad.class.has(c)) bad.class.set(c, head.replace(/\s+/g, ' '))
        }
      }
      stack.push('rule')
    }
    buf = ''
  } else if (ch === '}') {
    if (stack.at(-1) === 'rule' || stack.at(-1) === 'frame') checkDecls(buf)
    stack.pop()
    buf = ''
  } else if (ch === ';' && (stack.at(-1) === 'rule' || stack.at(-1) === 'frame')) {
    checkDecls(buf)
    buf = ''
  } else buf += ch
}

function checkDecls(s) {
  const m = /^\s*(--[\w-]+)\s*:/.exec(s)
  if (m && !ALLOW_VAR.some((r) => r.test(m[1]))) bad.var.add(m[1])
}

let n = 0
for (const [c, sel] of bad.class) console.log(`class     .${c}    (${sel})`), n++
for (const k of bad.keyframes) console.log(`keyframes ${k}`), n++
for (const v of bad.var) console.log(`variable  ${v}`), n++
if (n) {
  console.error(`\n${file}: ${n} 个没有 oad- 前缀的名字`)
  process.exit(1)
}
console.log(`${file}: class、@keyframes、自定义属性都带 oad- 前缀（例外：ProseMirror-*、--shiki-*、--_*）`)
