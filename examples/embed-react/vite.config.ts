import react from '@vitejs/plugin-react'
import { defaultClientConditions, defineConfig } from 'vite'

// 在本仓库里：@open-agent-docs/client、@open-agent-docs/editor 直接用源码（oad-source），不用先打包它们。从 npm 装的话这一行可以删掉。
export default defineConfig({ plugins: [react()], resolve: { conditions: ['oad-source', ...defaultClientConditions] } })
