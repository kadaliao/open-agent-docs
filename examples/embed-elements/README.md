# embed-elements：纯 HTML 里放编辑器（Web Components）

宿主是一个纯 HTML 页面（`index.html`，没有 React、没有打包），用 `@open-agent-docs/elements` 的 `<oad-editor>`、`<oad-viewer>`、`<oad-database>`；
加一个几十行的 Node 服务（`server.mjs`）：宿主自己管登录，给浏览器发 Open Agent Docs 的 JWT。元素带着 JWT **直连** Open Agent Docs（另一个端口，跨域：
服务器对 `OAD_ALLOWED_ORIGINS` 里的宿主回 CORS 头），图片、文件用签名地址显示。

```
浏览器 ──► server.mjs（:5184）─┬ /token：当前用户的 JWT（signJwt）
   │                           ├ /host/page：后端替当前用户建好演示的页面（带一张图）和库（withUser）
   │                           └ /oad/*：@open-agent-docs/elements 的 dist（真实项目换成 CDN）
   └──── 直连（CORS）────────► open-agent-docs serve（OAD_AUTH=jwt，OAD_ALLOWED_ORIGINS=http://localhost:5184，:8848）
                                 /api/*、协作编辑的 WebSocket、图片（?exp=&sig= 签名地址）
```

第二种写法：`PROXY=1 pnpm start`，宿主把 `/api/*`（含 WebSocket）原样转给 Open Agent Docs，元素不给 `base-url`（同源）。
Open Agent Docs 只听内网、或者不想让浏览器看到它的地址时用。

## 跑起来

```sh
# 1. Open Agent Docs（JWT 登录，允许宿主页面跨域）
OAD_AUTH=jwt OAD_JWT_SECRET=change-me-change-me-change-me OAD_ALLOWED_ORIGINS=http://localhost:5184 \
  open-agent-docs serve --port 8848

# 2. 宿主
pnpm install
pnpm --filter @open-agent-docs/client build && pnpm --filter @open-agent-docs/elements build
OAD_JWT_SECRET=change-me-change-me-change-me OAD_URL=http://127.0.0.1:8848 pnpm start   # http://localhost:5184
```

打开 http://localhost:5184 是 Alice，右上角能换成 Bob（演示用的“登录”，真的服务换成自己的会话）。页面底下的日志能看到 `oad-ready`、`oad-change` 这些事件。

`pnpm smoke`（`OAD_BINARY=` 指向编好的 `open-agent-docs`）把上面整个走一遍：起服务器和宿主，检查接口、WebSocket 都直连服务器（没走宿主），
编辑器和只读视图里的图片显示出来（签名地址），浏览器里两个人写字、看到对方写的，样式进了 Shadow DOM、数据库的弹出菜单能用、拿掉编辑器后 WebSocket 断开；
最后用 `PROXY=1` 再起一个宿主，看转发的写法图片也出得来。

## 看点

- `index.html`：元素的 `getToken` 指到宿主的 `/token`；JWT 快过期（这里故意只给 10 分钟）、遇到 401 时元素自己来换。
- `server.mjs`：`signJwt` 给浏览器签 JWT；`createClient({ jwtSecret }).withUser(...)` 是后端替用户调接口。两个都在 `@open-agent-docs/client/node`。
- 直连要服务器的 `OAD_ALLOWED_ORIGINS` 里有宿主页面的 origin（`http://localhost:5184`）：CORS、WebSocket 握手的 Origin 检查都看它。
  上线时设成宿主的域名，`OAD_PUBLIC_URL` 设成浏览器连 Open Agent Docs 的地址（`server.mjs` 也读 `OAD_PUBLIC_URL` 告诉页面）。
- 图片：`<img>` 带不了 JWT。元素先把图片地址交给 `POST /api/v1/files/sign` 换成签名地址（`?exp=&sig=`，默认 24 小时有效），`<oad-viewer>` 拿到的 `format=html` 里本来就是签名地址。
- 换主题：`index.html` 里 `oad-editor { --oad-accent: … }`（CSS 变量写在元素上）。
