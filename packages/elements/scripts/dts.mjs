// 类型声明：dist/types.d.ts 从 src/types.ts 生成（只有元素的属性、事件，不引别的包），dist/elements.d.ts 是入口。
//   import '@open-agent-docs/elements' 以后 document.querySelector('oad-editor') 的类型就是 OadEditorElement
import { spawnSync } from 'node:child_process'
import { writeFileSync } from 'node:fs'

const here = new URL('..', import.meta.url).pathname
const tsc = new URL('../node_modules/.bin/tsc', import.meta.url).pathname
const r = spawnSync(
  tsc,
  ['-p', 'tsconfig.types.json'],
  { cwd: here, stdio: 'inherit' },
)
if (r.status) process.exit(r.status)

const names = ['OadEditorElement', 'OadViewerElement', 'OadDatabaseElement']
writeFileSync(
  new URL('../dist/elements.d.ts', import.meta.url),
  `// @open-agent-docs/elements：引进来就注册好 <oad-editor>、<oad-viewer>、<oad-database>
import type * as T from './types.js'
export type * from './types.js'
${names.map((n) => `export interface ${n} extends T.${n} {}\nexport declare const ${n}: { prototype: ${n}; new (): ${n} }`).join('\n')}
`,
)
console.log('dist/elements.d.ts、dist/types.d.ts 写好了')
