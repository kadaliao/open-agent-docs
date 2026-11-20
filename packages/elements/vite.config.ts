// 打包：dist/elements.js 是入口（一个 ES 模块，<script type="module"> 直接能用），依赖（React、编辑器、Yjs、客户端）全打在包里，宿主什么都不用装。
// 入口只有元素本身和客户端的 core（gzip 后十几 KB）；编辑器、数据库视图、代码高亮、公式、图表、WASM 放在 dist/chunks/、dist/assets/，
// 第一次用到时按 elements.js 自己的地址去取（从 CDN 引也一样）。
//
// CSS：每个 `import './x.css'` 换成 addCss(样式文字)（src/styles.ts），放进各个元素的 Shadow DOM，不往宿主页面里插。
import react from '@vitejs/plugin-react'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig, type Plugin } from 'vite'

const registry = fileURLToPath(new URL('./src/styles.ts', import.meta.url))
// 不用 \0 开头：那样的虚拟模块在这版 vite 里不走 load
const PREFIX = 'virtual:oad-css:'

function cssToShadow(): Plugin {
  return {
    name: 'oad-css-to-shadow',
    enforce: 'pre',
    async resolveId(source, importer, opts) {
      if (!/\.css$/.test(source) || source.startsWith(PREFIX)) return null
      const r = await this.resolve(source, importer, { ...opts, skipSelf: true })
      // 虚拟模块的名字不能以 .css 结尾，不然 vite 又把它当样式处理
      return r ? { id: PREFIX + r.id + '.js', moduleSideEffects: true } : null
    },
    load(id) {
      if (!id.startsWith(PREFIX)) return null
      const real = id.slice(PREFIX.length, -'.js'.length)
      // moduleSideEffects：编辑器的 package.json 只把 *.css 标成有副作用，这个虚拟模块名不是 .css，不标的话会被摇掉
      return { code: `import css from ${JSON.stringify(real + '?inline')}\nimport { addCss } from ${JSON.stringify(registry)}\naddCss(css)\n`, moduleSideEffects: true }
    },
  }
}

// 库模式会把 new URL('x.wasm', import.meta.url) 塞成 base64（大三分之一，也不能边下边编译）：
// wasm-bindgen 生成的那句换成单独的文件 assets/x-<hash>.wasm
function wasmAsFile(): Plugin {
  return {
    name: 'oad-wasm-as-file',
    enforce: 'pre',
    transform(code, id) {
      const m = /new URL\('([\w-]+_bg\.wasm)', import\.meta\.url\)/.exec(code)
      if (!m) return null
      const ref = this.emitFile({ type: 'asset', name: m[1], source: readFileSync(join(dirname(id.split('?')[0]), m[1])) })
      return code.replace(m[0], `new URL(import.meta.ROLLUP_FILE_URL_${ref})`)
    },
  }
}

export default defineConfig({
  plugins: [cssToShadow(), wasmAsFile(), react()],
  // 资源地址按 elements.js 自己的位置算（CDN 上也对）
  base: './',
  resolve: { conditions: ['oad-source', 'module', 'browser', 'production', 'default'] },
  define: { 'process.env.NODE_ENV': JSON.stringify('production') },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
    sourcemap: false,
    minify: true,
    lib: {
      entry: { elements: 'src/index.ts' },
      formats: ['es'],
    },
    rolldownOptions: {
      // 允许入口多导出一些内部的东西：按需加载的块从 elements.js 里拿客户端、样式表登记这些公共的，不再另拆公共块（入口只有一个文件）
      preserveEntrySignatures: 'allow-extension',
      // lib 模式默认不压空白（为了让用的人再摇树）；这个包是直接给浏览器用的，压到最小
      output: { minify: true, chunkFileNames: 'chunks/[name]-[hash].js', assetFileNames: 'assets/[name]-[hash][extname]' },
    },
  },
})
