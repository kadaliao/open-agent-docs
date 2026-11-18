import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

const target = process.env.OAD_SERVER ?? 'http://localhost:8848'

export default defineConfig({
  plugins: [react()],
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
