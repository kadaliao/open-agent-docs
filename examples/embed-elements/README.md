# embed-elements：纯 HTML 里放编辑器（Web Components）

宿主是一个纯 HTML 页面（`index.html`，没有 React、没有打包），用 `@open-agent-docs/elements` 的 `<oad-editor>`、`<oad-viewer>`、`<oad-database>`；
加一个几十行的 Node 服务（`server.mjs`）：宿主自己管登录，给浏览器发 Open Agent Docs 的 JWT，把 `/api/*`（包括协作编辑的 WebSocket）原样转过去。

```
浏览器 ──► server.mjs（:5184）─┬ /api/*：原样转发（JWT 在 Authorization 和 ?token= 里）──► open-agent-docs serve（OAD_AUTH=jwt，:8848）
                               ├ /token：当前用户的 JWT（signJwt）
                               ├ /host/page：后端替当前用户建好演示的页面和库（withUser）
                               └ /oad/*：@open-agent-docs/elements 的 dist（真实项目换成 CDN）
```

## 跑起来

```sh
# 1. Open Agent Docs（JWT 登录）
OAD_AUTH=jwt OAD_JWT_SECRET=change-me-change-me-change-me open-agent-docs serve --port 8848

# 2. 宿主
pnpm install
pnpm --filter @open-agent-docs/client build && pnpm --filter @open-agent-docs/elements build
OAD_JWT_SECRET=change-me-change-me-change-me OAD_URL=http://127.0.0.1:8848 pnpm start   # http://localhost:5184
```

打开 http://localhost:5184 是 Alice，右上角能换成 Bob（演示用的“登录”，真的服务换成自己的会话）。页面底下的日志能看到 `oad-ready`、`oad-change` 这些事件。

`pnpm smoke`（`OAD_BINARY=` 指向编好的 `open-agent-docs`）把上面整个走一遍：起服务器和宿主，浏览器里两个人写字、看到对方写的，检查样式进了 Shadow DOM、
数据库的弹出菜单能用、拿掉编辑器后 WebSocket 断开。

## 看点

- `index.html`：元素的 `getToken` 指到宿主的 `/token`；JWT 快过期（这里故意只给 10 分钟）、遇到 401 时元素自己来换。
- `server.mjs`：`signJwt` 给浏览器签 JWT；`createClient({ jwtSecret }).withUser(...)` 是后端替用户调接口。两个都在 `@open-agent-docs/client/node`。
- 为什么转发 `/api` 而不让浏览器直连 :8848：跨域直连要服务器给 `/api` 回 CORS 头，现在还没有。上线时 `OAD_ALLOWED_ORIGINS` 设成宿主的域名（WebSocket 握手检查 Origin）。
- 换主题：`index.html` 里 `oad-editor { --oad-accent: … }`（CSS 变量写在元素上）。
