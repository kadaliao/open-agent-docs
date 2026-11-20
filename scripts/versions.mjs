// 发布的 JS 包（packages/*，新加的包自动算进来）版本号统一：检查一遍；`node scripts/versions.mjs 0.2.0` 一起改。
// @open-agent-docs/client 的 src/version.ts（User-Agent 里的版本）跟着改。
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const root = new URL('..', import.meta.url).pathname
const dirs = readdirSync(join(root, 'packages')).map((d) => join(root, 'packages', d, 'package.json'))
const pkgs = dirs.map((f) => ({ f, j: JSON.parse(readFileSync(f, 'utf8')) })).filter((p) => !p.j.private)
const want = process.argv[2]
const versionTs = join(root, 'packages/client/src/version.ts')
const versionLine = (v) => `export const VERSION = '${v}'`
if (want) {
  for (const p of pkgs) {
    p.j.version = want
    writeFileSync(p.f, JSON.stringify(p.j, null, 2) + '\n')
  }
  writeFileSync(versionTs, readFileSync(versionTs, 'utf8').replace(/export const VERSION = '[^']*'/, versionLine(want)))
  console.log(`改成 ${want}：` + pkgs.map((p) => p.j.name).join('、'))
} else {
  const vs = new Set(pkgs.map((p) => p.j.version))
  for (const p of pkgs) console.log(`${p.j.name}@${p.j.version}`)
  if (vs.size > 1) {
    console.error('版本号不一致')
    process.exit(1)
  }
  const [v] = vs
  if (!readFileSync(versionTs, 'utf8').includes(versionLine(v))) {
    console.error(`packages/client/src/version.ts 不是 ${v}（node scripts/versions.mjs ${v} 一起改）`)
    process.exit(1)
  }
  // 互相依赖的一律 workspace:*（pnpm pack 时换成真的版本号）
  for (const p of pkgs)
    for (const k of ['dependencies', 'peerDependencies'])
      for (const [n, v] of Object.entries(p.j[k] ?? {}))
        if (n.startsWith('@open-agent-docs/') && v !== 'workspace:*') {
          console.error(`${p.j.name} 的 ${k}.${n} 应该是 workspace:*`)
          process.exit(1)
        }
}
