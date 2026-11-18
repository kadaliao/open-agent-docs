// 端到端测试：默认自己起一个真服务器（cargo run -p oad-bin -- serve，OAD_AUTH=none，临时数据目录），网页用 apps/web/dist。
// 先 `pnpm --filter @oad/web build`。已经有服务器在跑：OAD_URL=http://127.0.0.1:8848 pnpm e2e。
// 有编好的二进制：OAD_BIN=/path/to/open-agent-docs，省掉 cargo run。
import { defineConfig, devices } from '@playwright/test'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const port = Number(process.env.OAD_PORT ?? 18858)
const external = process.env.OAD_URL
const baseURL = external ?? `http://127.0.0.1:${port}`
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const bin = process.env.OAD_BIN ? `"${process.env.OAD_BIN}"` : 'cargo run -q -p oad-bin --'

export default defineConfig({
  testDir: './e2e',
  timeout: 120_000,
  fullyParallel: false,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL,
    ...devices['Desktop Chrome'],
    viewport: { width: 1320, height: 900 },
    headless: true,
    // service worker 默认关掉（它发的请求 Playwright 拦不到）；测离线的那个测试自己打开
    serviceWorkers: 'block',
    // 界面跟浏览器语言：测试默认中文（英文的测试自己切）
    locale: 'zh-CN',
  },
  webServer: external
    ? undefined
    : {
        command: `${bin} serve --port ${port}`,
        cwd: root,
        url: `${baseURL}/api/v1/auth/config`,
        timeout: 600_000,
        reuseExistingServer: false,
        stdout: 'ignore',
        stderr: 'pipe',
        env: {
          OAD_AUTH: 'none',
          OAD_DATA_DIR: mkdtempSync(join(tmpdir(), 'oad-e2e-')),
          OAD_WEB_DIR: join(root, 'apps/web/dist'),
          RUST_LOG: 'warn',
        },
      },
})
