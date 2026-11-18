import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// 构建产物放进 open_agent_docs/static，由 FastAPI 直接提供（server.py 的 web）；开发时把接口和 MCP 代理到 8790
export default defineConfig({
  plugins: [react()],
  build: { outDir: '../open_agent_docs/static', emptyOutDir: true },
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:8790',
      '/mcp': 'http://127.0.0.1:8790',
    },
  },
})
