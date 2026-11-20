// 库模式打包：dist/ 下是 ES 模块（每个入口一个文件，按需加载的部分各自一块）+ 一个 style.css；
// 依赖（react、tiptap、yjs、@open-agent-docs/client……）都不打进去，由用的人装。类型声明另外用 tsc 出到 dist/types。
import react from '@vitejs/plugin-react'
import { isAbsolute } from 'node:path'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [react()],
  resolve: { conditions: ['oad-source', 'module', 'browser', 'production', 'default'] },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
    sourcemap: true,
    minify: false,
    cssCodeSplit: false,
    lib: {
      entry: {
        index: 'src/index.ts',
        database: 'src/database/index.ts',
        schema: 'src/schema/index.ts',
        i18n: 'src/i18n.ts',
      },
      formats: ['es'],
      cssFileName: 'style',
    },
    rolldownOptions: {
      // 自己的源码打进去，别的（npm 包、css 以外的裸模块名）都留给宿主装
      external: (id) => (!id.startsWith('.') && !isAbsolute(id) && !id.startsWith('\0') && !id.endsWith('.css')) || id.startsWith('katex/'),
      output: { chunkFileNames: 'chunks/[name]-[hash].js' },
    },
  },
})
