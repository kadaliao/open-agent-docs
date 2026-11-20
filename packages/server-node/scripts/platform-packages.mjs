// 发 npm 前（CI 里）：把各平台编好的 .node 拆成一个平台一个包，主包用 optionalDependencies 引它们，
// 这样 npm 只下载当前平台那个（每个 30MB 上下，四个放一起太大）。
//   node packages/server-node/scripts/platform-packages.mjs <放 .node 的目录> <输出目录>
// 输入目录里是 oad-server.<平台>-<架构>.node；输出目录里每个平台一个可以直接 npm publish 的目录。会改主包的 package.json（只在 CI 里跑）。
import { copyFileSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(dirname(fileURLToPath(import.meta.url)))
const [inDir, outDir] = process.argv.slice(2)
if (!inDir || !outDir) {
  console.error('用法：platform-packages.mjs <.node 目录> <输出目录>')
  process.exit(2)
}

const mainPath = join(here, 'package.json')
const main = JSON.parse(readFileSync(mainPath, 'utf8'))
const files = readdirSync(inDir).filter((f) => /^oad-server\.[a-z0-9]+-[a-z0-9]+\.node$/.test(f))
if (!files.length) throw new Error(`${inDir} 里没有 oad-server.*.node`)

const optional = {}
for (const f of files) {
  const plat = f.slice('oad-server.'.length, -'.node'.length)
  const [os, cpu] = plat.split('-')
  const name = `${main.name}-${plat}`
  const dir = join(outDir, `server-${plat}`)
  mkdirSync(dir, { recursive: true })
  copyFileSync(join(inDir, f), join(dir, 'oad-server.node'))
  const pkg = {
    name,
    version: main.version,
    description: `Native ${plat} module of ${main.name}. Don't install it directly: installing ${main.name} pulls in the right one`,
    license: main.license,
    repository: main.repository,
    main: 'oad-server.node',
    files: ['oad-server.node'],
    os: [os],
    cpu: [cpu],
    engines: main.engines,
  }
  if (os === 'linux') pkg.libc = ['glibc']
  writeFileSync(join(dir, 'package.json'), JSON.stringify(pkg, null, 2) + '\n')
  optional[name] = main.version
  console.log(`→ ${dir}（${name}）`)
}

main.optionalDependencies = optional
// 发出去的主包不带 .node（在平台包里）
main.files = main.files.filter((f) => f !== '*.node')
writeFileSync(mainPath, JSON.stringify(main, null, 2) + '\n')
console.log(`${main.name} 的 optionalDependencies：${Object.keys(optional).join('、')}`)
