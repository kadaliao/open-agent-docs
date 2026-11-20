// src/openapi.ts 要和 docs/openapi.json 对得上：服务器改了接口、文档重新生成以后，这里也要重新生成（pnpm --filter @open-agent-docs/client gen:types）。
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { expect, it } from 'vitest'

it('src/openapi.ts 是从现在的 docs/openapi.json 生成的', () => {
  const script = fileURLToPath(new URL('../scripts/gen-types.mjs', import.meta.url))
  const r = spawnSync(process.execPath, [script, '--check'], { encoding: 'utf8' })
  expect(r.stderr.trim(), r.stderr).toBe('')
  expect(r.status).toBe(0)
}, 60_000)
