// 发布的 JS 包（packages/*）版本号统一：检查一遍；`node scripts/versions.mjs 0.2.0` 一起改。
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const root = new URL('..', import.meta.url).pathname
const dirs = readdirSync(join(root, 'packages')).map((d) => join(root, 'packages', d, 'package.json'))
const pkgs = dirs.map((f) => ({ f, j: JSON.parse(readFileSync(f, 'utf8')) })).filter((p) => !p.j.private)
const want = process.argv[2]
if (want) {
  for (const p of pkgs) {
    p.j.version = want
    writeFileSync(p.f, JSON.stringify(p.j, null, 2) + '\n')
  }
  console.log(`改成 ${want}：` + pkgs.map((p) => p.j.name).join('、'))
} else {
  const vs = new Set(pkgs.map((p) => p.j.version))
  for (const p of pkgs) console.log(`${p.j.name}@${p.j.version}`)
  if (vs.size > 1) {
    console.error('版本号不一致')
    process.exit(1)
  }
  // 互相依赖的一律 workspace:*（pnpm pack 时换成真的版本号）
  for (const p of pkgs)
    for (const k of ['dependencies', 'peerDependencies'])
      for (const [n, v] of Object.entries(p.j[k] ?? {}))
        if (n.startsWith('@oad/') && v !== 'workspace:*') {
          console.error(`${p.j.name} 的 ${k}.${n} 应该是 workspace:*`)
          process.exit(1)
        }
}
