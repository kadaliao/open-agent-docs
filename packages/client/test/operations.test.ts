// sdks/operations.json 里每个接口的 js 名字，createClient() 上都真有这个方法（改名、删方法时这里失败）。
import { readFileSync } from 'node:fs'
import { expect, it } from 'vitest'
import { createClient } from '../src/index.ts'

const table = JSON.parse(readFileSync(new URL('../../../sdks/operations.json', import.meta.url), 'utf8')) as {
  operations: Record<string, { name: string; js?: string }>
}

it('operations.json 的每个 js 名字都是客户端上的方法', () => {
  const c = createClient({ fetch: (async () => new Response('{}')) as typeof fetch }) as unknown as Record<string, unknown>
  const missing = Object.entries(table.operations)
    .filter(([, e]) => typeof c[e.js ?? ''] !== 'function')
    .map(([id, e]) => `${id}（${e.name}）→ ${e.js ?? '没有 js 字段'}`)
  expect(missing).toEqual([])
})
