// 类型声明里不留 `import './x.css'`（样式在 dist/style.css 里，宿主自己引；留着的话开了 noUncheckedSideEffectImports 的项目会报错）
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const walk = (d) =>
  readdirSync(d).flatMap((f) => {
    const p = join(d, f)
    return statSync(p).isDirectory() ? walk(p) : p.endsWith('.d.ts') ? [p] : []
  })
for (const f of walk(new URL('../dist/types', import.meta.url).pathname)) {
  const s = readFileSync(f, 'utf8')
  const t = s.replace(/^import ['"][^'"]+\.css['"];\n/gm, '')
  if (t !== s) writeFileSync(f, t)
}

// `import '@open-agent-docs/editor/style.css'` 在开了 noUncheckedSideEffectImports（TS 6 起默认开）的项目里也要有类型
writeFileSync(new URL('../dist/style.css.d.ts', import.meta.url), '// 样式文件，没有导出\nexport {}\n')
