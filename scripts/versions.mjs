// 发布的东西版本号统一（和 Cargo.toml 的 [workspace.package] version 一样）：检查一遍；`node scripts/versions.mjs 0.3.0` 一起改。
//   JS 包（packages/*，新加的包自动算进来），@open-agent-docs/client 的 src/version.ts（User-Agent 里的版本）；
//   Python SDK（pyproject、_version.py、[server] 依赖的版本）、进程内服务器 open-agent-docs-server、Rust SDK、服务器的 Cargo workspace。
// Go SDK 的版本由 go generate 从 docs/openapi.json 的 info.version 来：改完版本号要重新生成 openapi.json 和各 SDK 的代码（见 docs/sdk-contract.md §15）。
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const root = new URL('..', import.meta.url).pathname
const dirs = readdirSync(join(root, 'packages')).map((d) => join(root, 'packages', d, 'package.json'))
const pkgs = dirs.map((f) => ({ f, j: JSON.parse(readFileSync(f, 'utf8')) })).filter((p) => !p.j.private)

// 别的语言：文件 + 找版本号的正则（第一个分组是版本号）
const V = '(\\d+\\.\\d+\\.\\d+(?:-[0-9A-Za-z.-]+)?)'
const others = [
  ['Cargo.toml', new RegExp(`(?<=\\[workspace\\.package\\][^\\[]*?\\nversion = ")${V}(?=")`)],
  ['sdks/rust/Cargo.toml', new RegExp(`(?<=\\[workspace\\.package\\][^\\[]*?\\nversion = ")${V}(?=")`)],
  ['sdks/python/pyproject.toml', new RegExp(`(?<=\\nversion = ")${V}(?=")`)],
  ['sdks/python/pyproject.toml', new RegExp(`(?<="open-agent-docs-server==)${V}(?=")`)],
  ['sdks/python/src/open_agent_docs/_version.py', new RegExp(`(?<=__version__ = ")${V}(?=")`)],
  ['crates/oad-py/pyproject.toml', new RegExp(`(?<=\\nversion = ")${V}(?=")`)],
  ['packages/client/src/version.ts', new RegExp(`(?<=export const VERSION = ')${V}(?=')`)],
]

const want = process.argv[2]
if (want) {
  if (!new RegExp(`^${V}$`).test(want)) throw new Error(`版本号 ${want} 不对，要 X.Y.Z`)
  for (const p of pkgs) {
    p.j.version = want
    writeFileSync(p.f, JSON.stringify(p.j, null, 2) + '\n')
  }
  for (const [f, re] of others) {
    const path = join(root, f)
    const s = readFileSync(path, 'utf8')
    if (!re.test(s)) throw new Error(`${f} 里找不到版本号`)
    writeFileSync(path, s.replace(re, want))
  }
  // crates 之间的依赖写的是 version = "X", path = ...：一起改，不然 cargo 解析不了
  for (const d of readdirSync(join(root, 'crates'))) {
    const path = join(root, 'crates', d, 'Cargo.toml')
    const s = readFileSync(path, 'utf8')
    writeFileSync(path, s.replace(new RegExp(`(oad-[a-z-]+ = \\{ version = ")${V}(", path)`, 'g'), `$1${want}$3`))
  }
  console.log(`改成 ${want}：` + [...pkgs.map((p) => p.j.name), ...new Set(others.map(([f]) => f))].join('、'))
  console.log('接着：OAD_WRITE_OPENAPI=1 cargo test -p oad-server --test openapi，再重新生成各 SDK（sdks/regen.sh）')
} else {
  let bad = false
  const cargo = readFileSync(join(root, 'Cargo.toml'), 'utf8').match(others[0][1])?.[1]
  console.log(`服务器（Cargo.toml）：${cargo}`)
  for (const p of pkgs) {
    if (p.j.version !== cargo) {
      console.error(`✗ ${p.j.name}@${p.j.version}，应该是 ${cargo}`)
      bad = true
    }
  }
  for (const [f, re] of others) {
    const v = readFileSync(join(root, f), 'utf8').match(re)?.[1]
    if (v !== cargo) {
      console.error(`✗ ${f}：${v ?? '找不到版本号'}，应该是 ${cargo}`)
      bad = true
    }
  }
  const spec = JSON.parse(readFileSync(join(root, 'docs/openapi.json'), 'utf8')).info.version
  if (spec !== cargo) {
    console.error(`✗ docs/openapi.json 的 info.version 是 ${spec}（OAD_WRITE_OPENAPI=1 cargo test -p oad-server --test openapi）`)
    bad = true
  }
  const go = readFileSync(join(root, 'sdks/go/zz_generated_version.go'), 'utf8').match(new RegExp(`const Version = "${V}"`))?.[1]
  if (go !== cargo) {
    console.error(`✗ Go SDK 的 Version 是 ${go}（cd sdks/go && go generate ./...）`)
    bad = true
  }
  // 互相依赖的一律 workspace:*（pnpm pack 时换成真的版本号）
  for (const p of pkgs)
    for (const k of ['dependencies', 'peerDependencies'])
      for (const [n, v] of Object.entries(p.j[k] ?? {}))
        if (n.startsWith('@open-agent-docs/') && v !== 'workspace:*') {
          console.error(`✗ ${p.j.name} 的 ${k}.${n} 应该是 workspace:*`)
          bad = true
        }
  if (bad) process.exit(1)
  console.log(`✓ ${pkgs.length} 个 JS 包、Python、Rust、Go SDK 都是 ${cargo}`)
}
