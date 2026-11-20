import { defineConfig } from 'vitest/config'

export default defineConfig({
  resolve: { conditions: ['oad-source', 'module', 'browser', 'development', 'default'] },
  test: {
    environment: 'jsdom',
    include: ['test/**/*.test.ts'],
    testTimeout: 20_000,
  },
})
