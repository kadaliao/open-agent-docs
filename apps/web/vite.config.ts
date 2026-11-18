import react from '@vitejs/plugin-react'
import { existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'

// WASM 版 oad-markdown（packages/markdown-wasm）还没编出来时，用占位模块
const wasm = fileURLToPath(new URL('../../packages/markdown-wasm', import.meta.url))
const markdownAlias = { '@oad/markdown': existsSync(wasm + '/dist/oad_markdown.js') ? wasm + '/dist/oad_markdown.js' : fileURLToPath(new URL('./src/markdown-fallback.ts', import.meta.url)) }

const target = process.env.OAD_SERVER ?? 'http://localhost:8848'

export default defineConfig({
  plugins: [react()],
  resolve: { alias: markdownAlias },
  server: {
    port: 5173,
    proxy: {
      '/api': { target, ws: true, changeOrigin: false },
    },
  },
  preview: {
    proxy: {
      '/api': { target, ws: true },
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
    sourcemap: false,
    reportCompressedSize: true,
    chunkSizeWarningLimit: 800,
  },
})
