import { defineConfig } from 'vitest/config'

export default defineConfig({ resolve: { conditions: ['oad-source', 'module', 'node', 'import', 'default'] }, test: { include: ['test/**/*.test.ts'] } })
