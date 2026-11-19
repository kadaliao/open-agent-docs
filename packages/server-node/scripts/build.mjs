// 编 crates/oad-node，把动态库拷成 oad-server.<平台>-<架构>.node 放在包里。
// --release 编发布版；--if-missing 已经有了就不编（测试前用）。
import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(dirname(fileURLToPath(import.meta.url)))
const root = join(here, '..', '..')
const out = join(here, `oad-server.${process.platform}-${process.arch}.node`)
const release = process.argv.includes('--release')
if (process.argv.includes('--if-missing') && existsSync(out)) process.exit(0)
execFileSync('cargo', ['build', '-p', 'oad-node', ...(release ? ['--release'] : [])], { cwd: root, stdio: 'inherit' })
const lib = { darwin: 'liboad_node.dylib', linux: 'liboad_node.so', win32: 'oad_node.dll' }[process.platform]
if (!lib) throw new Error(`不支持的平台 ${process.platform}`)
const target = process.env.CARGO_TARGET_DIR ?? join(root, 'target')
copyFileSync(join(target, release ? 'release' : 'debug', lib), out)
console.log(`→ ${out}`)
