# embed-react：在自己的 React 应用里放编辑器

宿主是一个 Vite + React 页面，加一个几十行的 Node 服务（`server.mjs`）：宿主自己管登录，把 `/api/*`（包括协作编辑的 WebSocket）
转给 Open Agent Docs 时带上 `X-OAD-User-Id / Name / Email` 和共享密钥 `X-OAD-Secret`。浏览器不直接连 Open Agent Docs，也拿不到密钥。

```
浏览器 ──► server.mjs（:5180）── /api 加上用户头 ──► open-agent-docs serve --auth header（:8848）
           └ 其余：宿主页面（dist/）
```

## 跑起来

```sh
# 1. Open Agent Docs（header 登录）
OAD_AUTH=header OAD_HEADER_SECRET=change-me open-agent-docs serve --port 8848

# 2. 宿主
pnpm install
pnpm build
OAD_HEADER_SECRET=change-me OAD_URL=http://127.0.0.1:8848 pnpm start   # http://localhost:5180
```

打开 http://localhost:5180 是 Alice；右上角能换成 Bob（演示用的“登录”，真的服务换成自己的会话）。
两个人看到的是同一个工作区，第一次来时 Open Agent Docs 会自动建用户、加进工作区。

`src/main.tsx` 用到了 `@oad/client`（`createClient`、`client.events`）和 `@oad/editor`（`OadTitle`、`OadEditor`、`OadComments`、
`OadDatabaseView`、`useOadPage`）；`src/host.css` 演示用 CSS 变量换成宿主的强调色和字体。

上线时：Open Agent Docs 只监听内网（`OAD_HOST=127.0.0.1`），`OAD_ALLOWED_ORIGINS` 设成宿主的域名（WebSocket 握手要检查 Origin）。

## 冒烟测试

```sh
OAD_BIN=/path/to/open-agent-docs pnpm smoke   # 不给 OAD_BIN 就找仓库里 target/release、target/debug 下编好的
```

起服务器（header 登录）、打包宿主、起代理，用浏览器以 Alice 建一页写字，换成 Bob 看到并接着写，经代理读回正文和作者；
另外确认不带密钥直接打 Open Agent Docs 进不去。要装好 Playwright 的 Chromium（`npx playwright install chromium`）。
