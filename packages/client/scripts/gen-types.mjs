// 从 docs/openapi.json 生成 src/openapi.ts（openapi-typescript）。
//   node scripts/gen-types.mjs            写文件
//   node scripts/gen-types.mjs --check    只比较，过期了退出码 1（测试里也用）
// openapi-typescript 要 TypeScript 5 的编译器接口，仓库用的 TypeScript 7 没有：这里把它的 `typescript` 换成 typescript5（5.9）。
import { registerHooks } from 'node:module'
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const ts5 = import.meta.resolve('typescript5')
registerHooks({
  resolve(spec, ctx, next) {
    return spec === 'typescript' ? { url: ts5, shortCircuit: true, format: 'commonjs' } : next(spec, ctx)
  },
})
const { default: openapiTS, astToString } = await import('openapi-typescript')

const specPath = fileURLToPath(new URL('../../../docs/openapi.json', import.meta.url))
const outPath = fileURLToPath(new URL('../src/openapi.ts', import.meta.url))

export async function generate() {
  const spec = JSON.parse(readFileSync(specPath, 'utf8'))
  const ast = await openapiTS(spec, { alphabetize: false, exportType: false, emptyObjectsUnknown: true })
  return '// 由 scripts/gen-types.mjs 从 docs/openapi.json 生成，不要手改（pnpm --filter @oad/client gen:types）\n' + astToString(ast)
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const code = await generate()
  if (process.argv.includes('--check')) {
    let cur = ''
    try {
      cur = readFileSync(outPath, 'utf8')
    } catch {
      /* 还没生成 */
    }
    if (cur !== code) {
      console.error('src/openapi.ts 和 docs/openapi.json 对不上：pnpm --filter @oad/client gen:types')
      process.exit(1)
    }
    console.log('src/openapi.ts 是最新的')
  } else {
    writeFileSync(outPath, code)
    console.log(`写了 ${outPath}（${code.split('\n').length} 行）`)
  }
}
