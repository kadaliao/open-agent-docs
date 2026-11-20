# @open-agent-docs/elements

Open Agent Docs 的 Web Components：`<oad-editor>`、`<oad-viewer>`、`<oad-database>`。宿主不用 React（React、编辑器、Yjs 都打在包里），
Vue、Svelte、Angular、纯 HTML、后端模板都能用。[English](README.en.md)

## 引进来

```html
<!-- 不用打包工具：直接从 CDN 引，引进来就注册好了 -->
<script type="module" src="https://cdn.jsdelivr.net/npm/@open-agent-docs/elements@0.3.0/dist/elements.js"></script>

<oad-editor base-url="https://docs.example.com" page-id="Qj1y9Gufp11" token="eyJ…"></oad-editor>
```

```sh
npm i @open-agent-docs/elements
```

```ts
import '@open-agent-docs/elements' // 注册三个元素；TS 里 document.querySelector('oad-editor') 就有类型
```

`dist/elements.js` 是入口（gzip 后约 13 KB）。编辑器、数据库视图、代码高亮、公式、图表这些按需加载的部分在 `dist/chunks/` 下，
第一次用到时按 `elements.js` 的地址去取（CDN 上也一样）：页面上有 `<oad-editor>` 时再下载约 290 KB；`<oad-database>` 约 570 KB（含约 250 KB 的 WASM 计算引擎，和编辑器共用的部分只下载一次）；
只有 `<oad-viewer>` 的页面不用下载编辑器。

## 三个元素

| 元素 | 属性 | 说明 |
| --- | --- | --- |
| `<oad-editor>` | `page-id`、`comments`、`readonly` | 标题 + 协作编辑器（WebSocket 实时同步，别人的光标也看得到）。`comments`：右边加评论栏（窄了放到下面）；`readonly`：只读（只有查看权限时自动只读） |
| `<oad-viewer>` | `page-id`、`content-css` | 只读正文，最轻：一次 `GET /pages/{id}/content?format=html`，不连 WebSocket、不加载编辑器。样式表默认 `<base-url>/embed/content.css`，`content-css` 可以换。服务器太旧、没有 `format=html` 时取 Markdown 在浏览器里转（`@open-agent-docs/markdown`，用到才下载） |
| `<oad-database>` | `db-id`、`view-id`、`readonly` | 数据库视图：表格、看板、日历、画廊、列表。`view-id` 只显示这一个视图 |

三个都有：

| 属性 / JS 属性 | 说明 |
| --- | --- |
| `base-url` / `el.baseUrl` | 服务器地址；不给是同源（宿主把 `/api` 转给 Open Agent Docs 时） |
| `token` / `el.token` | JWT 或 Agent token，请求带 `Authorization: Bearer`，WebSocket 带 `?token=`。不给就靠 cookie（同源、登录过） |
| `el.getToken = async () => …` | 换 token：没有 token、JWT 离过期不到 60 秒、收到 401 时调它（同时只调一次），WebSocket 重连时用新的 |
| `theme` | `light` / `dark` / 不给跟系统 |
| `lang` | `zh` / `en`：界面语言（不给跟浏览器） |
| `el.client` | 元素用的 `@open-agent-docs/client` 客户端（登录方式一样），想自己调接口时用 |
| `el.reload()` | 断开重来（比如 `<oad-viewer>` 想看最新的正文） |

`page-id`、`db-id`、`base-url` 变了会断开重连；`comments`、`readonly`、`view-id` 变了只重画。元素从页面上拿掉时关掉 WebSocket、事件流，释放 Y.Doc；
在页面里挪位置（先移除再加上）不会重连。元素定义之前就设了 `el.token` / `el.getToken`（脚本后加载）也没关系。

## 事件

都是 `CustomEvent`，冒泡、穿过 Shadow DOM，数据在 `detail` 里：

| 事件 | `detail` | 什么时候 |
| --- | --- | --- |
| `oad-ready` | `{ id, title }` | 内容到了（编辑器：本机缓存或服务器同步完；只读：画好了；数据库：数据到了） |
| `oad-change` | `{ id, kind: 'title' \| 'content', title, local }` | 标题、正文变了（正文合并 300 毫秒内的改动）。`local`：自己改的还是别人（协作者、Agent）改的 |
| `oad-navigate` | `{ href, pageId? }` | 要去另一页（点了内部链接、数据库里打开一行）。可以 `preventDefault()` 自己处理，不拦就跳过去 |
| `oad-error` | `{ error, status?, code?, message }` | 出错（401、403、404……），界面上也会写一行 |

```js
const el = document.querySelector('oad-editor')
el.getToken = () => fetch('/my-backend/oad-token').then((r) => r.json()).then((j) => j.token)
el.addEventListener('oad-change', (e) => console.log(e.detail.kind, e.detail.title))
el.addEventListener('oad-navigate', (e) => {
  e.preventDefault()
  router.push('/docs/' + e.detail.pageId)
})
```

## 登录

- **JWT**（推荐，`OAD_AUTH=jwt`）：宿主后端用 `@open-agent-docs/client/node` 的 `signJwt(secret, userId, { name, email })` 签，经 `getToken` 给元素；元素带着它直连服务器（见下面的跨域直连）。
- **同源 + 宿主转发**：宿主把 `/api/*`（含 WebSocket）转给 Open Agent Docs，转发时加上 `X-OAD-User-*`（`OAD_AUTH=header`）；元素不用给 token，`base-url` 也不用给。
- **Agent token**：`token="oad_…"`，适合内部工具。

**跨域直连**（`base-url` 是别的域名）：服务器的 `OAD_ALLOWED_ORIGINS` 写上宿主页面的 origin，接口（CORS）、事件流、WebSocket 握手（Origin 检查）就都放行了：

```sh
OAD_AUTH=jwt OAD_JWT_SECRET='…' OAD_ALLOWED_ORIGINS=https://app.example.com open-agent-docs serve
```

**图片和文件**：`<img>`、`<video>`、链接带不了 token。用 token 登录时，元素显示前先把服务器上的地址交给 `POST /api/v1/files/sign` 换成签名地址
（`?exp=…&sig=…`，一批一起签，缓存到过期前 60 秒；过期了、加载失败了重签），跨域时再拼上 `base-url`；`<oad-viewer>` 拿到的 `format=html` 里本来就是签名地址。
签名地址在有效期内（默认 24 小时，`OAD_SIGNED_URL_TTL`）谁拿到都能打开，见[嵌入文档](../../docs/zh/embedding.md#只读-html)。

不想让浏览器直连（服务器只听内网）：宿主把 `/api/*`（含 WebSocket）转发到自己的域名下，`base-url` 不给。
完整的例子见仓库里的 `examples/embed-elements`（纯 HTML + 一个小 Node 服务签 JWT，元素直连；`PROXY=1` 是转发的写法）。

## 换样式

样式都在 Shadow DOM 里，宿主页面的 CSS 进不去，元素的 CSS 也不会漏出来。颜色、字体用 CSS 变量改，**写在元素上**（不是 `:root` 上）：

```css
oad-editor, oad-viewer, oad-database {
  --oad-accent: #7c3aed;
  --oad-accent-soft: rgba(124, 58, 237, 0.14);
  --oad-font: 'Inter', -apple-system, 'PingFang SC', sans-serif;
  --oad-fg: #1f1e1c; --oad-bg: #fff; --oad-radius: 8px;
}
```

变量名和 `@open-agent-docs/editor` 的一样（`--oad-fg`、`--oad-fg-2`、`--oad-bg-2`、`--oad-line`……）。深色：`theme="dark"`，或者不给跟系统。
`::part(root)` 能选到元素里最外层的那个 `div`。

## 和 React 版的区别

`@open-agent-docs/editor` 是 React 组件，宿主自己管 React、样式表；这个包是同一套组件包了一层：

- 弹出来的东西（数据库的弹出菜单、公式编辑器、图片的大图预览）都挂在元素自己的 Shadow DOM 里，样式跟着进去，不往宿主页面里插；代码块的语言提示也在 Shadow DOM 里。
- 界面语言整个页面只有一份：一个元素改了 `lang`，页面上别的元素（还有宿主自己用 `@open-agent-docs/editor` 画的组件）都跟着换，同一页上没法一个中文一个英文。
  编辑器、数据库视图里几百处文字按全局语言取，不少在 ProseMirror 的节点视图里（不归 React 管），按元素分开改动太大，先不做。
  另外设了 `lang` 也会改宿主页面的 `<html lang>`，并记进本机 localStorage 的 `oad:locale`。

## 开发

```sh
pnpm --filter @open-agent-docs/elements build   # dist/：vite 库模式 + 类型声明（scripts/dts.mjs）
pnpm --filter @open-agent-docs/elements size    # 看各部分 gzip 后多大
pnpm --filter @open-agent-docs/elements test    # jsdom：注册、属性变化、事件、token、断开
```

打包时每个 `import './x.css'` 都换成 `addCss(样式文字)`（`vite.config.ts` 里的插件，`src/styles.ts`），`:root` 改成 `:host`，放进各个元素的 Shadow DOM。
