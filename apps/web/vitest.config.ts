import { defineConfig } from 'vitest/config'

// 仓库里的包（@open-agent-docs/*）按 oad-source 用源码，不用先构建 dist（CI 里没有 dist）。
// vitest 按 SSR 跑测试，Vite 6 起 SSR 的解析条件单独配（ssr.resolve），所以两处都写。
const conditions = ['oad-source', 'module', 'node', 'import', 'default']
export default defineConfig({
  resolve: { conditions },
  ssr: { resolve: { conditions, externalConditions: ['oad-source'] } },
  test: { include: ['test/**/*.test.ts'], server: { deps: { inline: [/@open-agent-docs\//] } } },
})
