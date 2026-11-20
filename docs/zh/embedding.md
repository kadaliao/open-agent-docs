# 嵌入到自己的应用

[English](../en/embedding.md) · [文档目录](README.md)

网页这边三种做法：

- **iframe**：放一个 iframe 指向 Open Agent Docs 的页面，不用写前端代码。
- **Web Component**：`<oad-editor>`、`<oad-viewer>`、`<oad-database>`（`@open-agent-docs/elements`），宿主用什么框架都行，纯 HTML 也行（[Web Component](#web-component)）。
- **JS 包**：`@open-agent-docs/client`（接口、实时事件、协作文档、本地数据库）和 `@open-agent-docs/editor`（React 组件：编辑器、评论栏、数据库视图），
  编辑器直接长在你的页面里，跟着你的样式。

服务器这边三种跑法：单独跑一个 `open-agent-docs serve`（宿主旁挂、反向代理过去）；在宿主的 Node / Python 进程里启动
（[进程内启动](#进程内启动node--python)）；Rust 服务把它的路由挂进自己的 axum（[挂进 Rust 服务](#挂进-rust-服务)）。

登录有四种接法：同域名下用 Open Agent Docs 自己的账号（cookie）；宿主自己管登录、转发时带用户头（[header 登录](#header-登录)）；
宿主签 JWT（[JWT](#jwt)）；Rust 宿主自己实现 `AuthProvider`（[挂进 Rust 服务](#挂进-rust-服务)）。

## iframe

```html
<iframe src="https://docs.example.com/p/<页面 id>?embed=1&theme=light&lang=zh" style="width:100%;border:0"></iframe>
```

嵌入模式没有侧栏和顶栏，只有正文（或数据库），里面的链接在 iframe 里跳转。

| 参数 | 说明 |
| --- | --- |
| `embed=1` | 嵌入模式 |
| `theme=light` / `dark` | 主题，不给跟系统 |
| `lang=zh` / `en` | 界面语言，不给跟浏览器和账号（不会存下来） |

和宿主用 `postMessage` 说话：

| 方向 | 消息 |
| --- | --- |
| iframe → 宿主 | `{ type: 'oad:ready', pageId }` 内容出来了 |
| | `{ type: 'oad:navigate', pageId, title }` 在里面换了页 |
| | `{ type: 'oad:height', height }` 内容高度变了（想让 iframe 跟着长高时用） |
| | `{ type: 'oad:auth-required', reason }` 要新的 token（`reason`：`'unauthorized'` 收到 401，`'expiring'` 快过期了），见下面的[跨域 + JWT](#跨域--jwt) |
| 宿主 → iframe | `{ type: 'oad:navigate', pageId }` 换一页 |
| | `{ type: 'oad:theme', theme: 'light' \| 'dark' \| 'system' }` |
| | `{ type: 'oad:locale', locale: 'zh' \| 'en' }` |
| | `{ type: 'oad:token', token }` 换一个 token（只在嵌入模式里认，见下面） |

```js
const frame = document.querySelector('iframe')
addEventListener('message', (e) => {
  if (e.origin !== 'https://docs.example.com') return
  if (e.data?.type === 'oad:height') frame.style.height = e.data.height + 'px'
})
frame.contentWindow.postMessage({ type: 'oad:theme', theme: 'dark' }, 'https://docs.example.com')
```

iframe 里默认用的是 Open Agent Docs 自己的登录（cookie）。宿主和它不同域名时浏览器会拦第三方 cookie：
让两边同域（反向代理到同一个域名下），或者用下面的 JWT。完整的例子：[examples/embed-iframe](../../examples/embed-iframe)（同域的、跨域 + JWT 的都有）。

### 跨域 + JWT

宿主在 `app.example.com`，Open Agent Docs 在 `docs.example.com`，不用 cookie：宿主的后端签一个短期 JWT（[JWT](#jwt)），放在 iframe 地址的 `#token=` 后面。

```sh
OAD_AUTH=jwt OAD_JWT_SECRET='…' OAD_ALLOWED_ORIGINS=https://app.example.com open-agent-docs serve
```

```html
<iframe id="oad" src="https://docs.example.com/p/<页面 id>?embed=1#token=<JWT>"></iframe>
<script>
  const OAD = 'https://docs.example.com'
  const frame = document.getElementById('oad')
  addEventListener('message', async (e) => {
    if (e.origin !== OAD) return
    // 401 或者 token 快过期了：问自己的后端要个新的，发进去
    if (e.data?.type === 'oad:auth-required') {
      const { token } = await (await fetch('/api/oad-token')).json()
      frame.contentWindow.postMessage({ type: 'oad:token', token }, OAD)   // targetOrigin 写死，别用 '*'
    }
  })
</script>
```

- token 放在 `#` 后面：不会发给服务器、不进访问日志、不进 Referer。网页读走后只放在内存里（不进 localStorage），马上从地址栏抹掉。
  之后接口带 `Authorization: Bearer`，协同编辑的 WebSocket 带 `?token=`，实时事件也带上。
- 宿主发 `oad:token` 换新的：只认 `OAD_ALLOWED_ORIGINS` 里的 origin（还有 `OAD_PUBLIC_URL` 的，和同源的），而且得是嵌着它的那个窗口发的；
  这个列表网页从 `GET /api/v1/auth/config` 的 `allowed_origins` 拿。
- iframe 收到 401、或者 token 离过期（`exp`）不到 60 秒时，发一次 `{ type: 'oad:auth-required', reason }`（同一个 token 每种原因只发一次），宿主回 `oad:token`。
  一开始没给 token、或者给的不对，iframe 不出登录框，等宿主的 token 来了再登录。
- 不是嵌入模式（没有 `?embed=1`）时这些都不生效，照常用 cookie。
- 上传的图片、文件：`<img>`、`<video>`、链接带不了 token，iframe 会先把地址交给 `POST /api/v1/files/sign` 换成签名地址
  （`?exp=…&sig=…`，见[签名地址](api.md#文件的签名地址)），不靠 cookie 也打得开。

宿主后端签 JWT 用各语言 SDK 的 `sign_jwt` / `signJwt`，拼地址用 `embed_url` / `embedUrl`（见 [SDK](sdks.md)）。

## 只读 HTML

不要编辑器、只想把一页显示出来（文章页、邮件、搜索结果、服务器端渲染）时，要 HTML：

```sh
curl -H "Authorization: Bearer $TOKEN" 'https://docs.example.com/api/v1/pages/<页面 id>/content?format=html'
# { "html": "<div class=\"oad-content\"><h1>…</h1>…</div>", "version": "…" }
```

```html
<link rel="stylesheet" href="https://docs.example.com/embed/content.css">
<article>{html}</article>
```

- 外面包一层 `<div class="oad-content">`；样式表 `GET /embed/content.css` 不用登录、可以跨域取、带 ETag，只在 `.oad-content` 里生效，长得和编辑器一样。
  颜色、字体用 `--oad-*` 变量换（和编辑器同名）；明暗跟系统，祖先上放 `data-theme="light|dark"` 就按那个。
- 安全：只出固定的一组标签和属性，文字和属性值都转义；没有 `<script>`、事件属性（`onclick`……）；链接只认 http、https、mailto、tel 和相对地址，
  图片只认 http、https 和相对地址（`javascript:`、`data:` 之类去掉链接、留下文字）；Markdown 里写的原始 HTML 当文字显示。可以直接 `innerHTML`。
- 需要脚本的块出成带 `data-*` 的占位，文字留着：公式 `<span class="oad-math-inline" data-latex="…">`、`<div class="oad-math-block" data-latex="…">`，
  链接卡片 `<div class="oad-embed" data-url="…" data-view="embed|bookmark">`（里面是一个普通链接），上传的文件 `<div class="oad-embed oad-file" data-type="file" data-url="…">`。
  想画公式就照着 `data-latex` 用 KaTeX 自己画。
- 图片、文件、站内页面的地址是服务器上的路径（`/api/v1/files/…`、`/p/…`）；设了 `OAD_PUBLIC_URL` 就是完整地址，嵌在别的域名下也指得对。
- 调用者看得到的图片、文件，地址直接是**签名地址**（`/api/v1/files/<id>/<文件名>?exp=…&sig=…`）：`<img>` 带不了 token 也打得开，
  宿主在别的域名、用 JWT 或 Agent token 取的都一样。响应里的 `assets_expire_at`（毫秒时间戳）是这些签名的过期时间，没有签名地址时是 `null`。
  签名不含缩略图宽度，想要小图自己加 `&w=640`。
- `limit` 照常能用（先拿前 N 块）。
- 浏览器里或者 Node 里自己转：`@open-agent-docs/markdown`（WASM，和服务器同一份代码，输出逐字一样），样式表在包里：

```js
import init, { toHtml } from '@open-agent-docs/markdown'
import '@open-agent-docs/markdown/content.css'

await init()
el.innerHTML = toHtml('# 标题\n\n正文')                 // Markdown
el.innerHTML = toHtml(doc, { baseUrl: 'https://docs.example.com' })   // 文档 JSON（format=json 拿到的）
```

  自己转出来的 HTML 没有签名（签名要服务器的密钥），跨域、token 登录时图片要自己换：`client.signFileUrls(urls)`（`POST /files/sign`）。

> [!WARNING]
> **签名地址的两个后果**（`format=html`、`POST /files/sign` 都一样）：
>
> 1. **有有效期**：默认 24 小时（`OAD_SIGNED_URL_TTL`，见[配置](configuration.md#服务器)）。宿主把 HTML 缓存起来（知识库前台常这么做）的话，
>    缓存时间别超过有效期：过期前重新取一次，或者按响应里的 `assets_expire_at` 安排刷新。过期的地址回 403，图片就裂了。
> 2. **拿到地址就能打开**：拿到这段 HTML 的任何人，在有效期内不登录也能打开里面的图片和文件。文档不该外泄的场景把有效期调短；
>    要长期公开的内容用分享链接（`POST /pages/{id}/share`）。

## Web Component

宿主不用 React（Vue、Svelte、Angular、纯 HTML、后端模板都行）、又想要完整的编辑器时，用 `@open-agent-docs/elements`：三个自定义元素，
React、编辑器、Yjs 都打在包里，样式在 Shadow DOM 里，和宿主互不影响。完整的说明：[packages/elements/README.md](../../packages/elements/README.md)。

```html
<!-- 不用打包工具：从 CDN 引，引进来就注册好了（npm i @open-agent-docs/elements 后 import '@open-agent-docs/elements' 也行） -->
<script type="module" src="https://cdn.jsdelivr.net/npm/@open-agent-docs/elements@<版本>/dist/elements.js"></script>

<oad-editor base-url="https://docs.example.com" page-id="<页面 id>" comments></oad-editor>
<oad-viewer base-url="https://docs.example.com" page-id="<页面 id>"></oad-viewer>
<oad-database base-url="https://docs.example.com" db-id="<库 id>"></oad-database>

<script type="module">
  for (const el of document.querySelectorAll('oad-editor, oad-viewer, oad-database')) {
    // 宿主后端签的 JWT；没有 token、快过期、遇到 401 时元素自己来要
    el.getToken = () => fetch('/api/oad-token').then((r) => r.json()).then((j) => j.token)
    el.addEventListener('oad-change', (e) => console.log(e.detail.kind, e.detail.title))
  }
</script>
```

| 元素 | 属性 | 说明 |
| --- | --- | --- |
| `<oad-editor>` | `page-id`、`comments`、`readonly` | 标题 + 协作编辑器（WebSocket 实时同步）；`comments` 加评论栏 |
| `<oad-viewer>` | `page-id`、`content-css` | 只读正文，用 `format=html`，不连 WebSocket、不加载编辑器，最轻 |
| `<oad-database>` | `db-id`、`view-id`、`readonly` | 数据库视图：表格、看板、日历、画廊、列表 |

- 三个都有：`base-url`（服务器地址，不给是同源）、`token`（JWT 或 Agent token，也可以 `el.token = …`）、`el.getToken = async () => …`
  （没有 token、JWT 离过期不到 60 秒、收到 401 时调它）、`theme`（`light` / `dark`）、`lang`（`zh` / `en`）、`el.client`、`el.reload()`。
- 事件（`CustomEvent`，冒泡、穿过 Shadow DOM）：`oad-ready`（内容到了）、`oad-change`（标题、正文变了）、`oad-navigate`（要去另一页，
  可以 `preventDefault()` 自己处理）、`oad-error`（401、403、404……）。
- **跨域直连**：`base-url` 是别的域名时，元素直接调 Open Agent Docs 的接口（CORS）、连 WebSocket，服务器要把宿主的 origin 写进
  `OAD_ALLOWED_ORIGINS`：

  ```sh
  OAD_AUTH=jwt OAD_JWT_SECRET='…' OAD_ALLOWED_ORIGINS=https://app.example.com open-agent-docs serve
  ```

  图片、文件用签名地址显示（元素自己调 `POST /files/sign`，缓存到过期前 60 秒）。不想让浏览器直连的，宿主把 `/api/*`（含 WebSocket）转发到自己的域名下，
  `base-url` 不给。
- 换样式：颜色、字体用 CSS 变量，写在元素上（`oad-editor { --oad-accent: #7c3aed }`）。

例子：[examples/embed-elements](../../examples/embed-elements)（纯 HTML + 一个小 Node 服务签 JWT，元素直连；`PROXY=1` 是转发的写法）。

## JS 包

> 还没发布到 npm。现在用仓库里打的包：`pnpm pack:all` 生成 `dist-packs/*.tgz`，在你的项目里 `npm i ./oad-client-x.y.z.tgz` 等。

```sh
npm i @open-agent-docs/client @open-agent-docs/editor react react-dom yjs
```

### @open-agent-docs/client

```ts
import { createClient } from '@open-agent-docs/client'

const client = createClient({
  baseUrl: 'https://docs.example.com',  // 同源时给 ''；别的域名要服务器的 OAD_ALLOWED_ORIGINS 里有你的页面（CORS）
  token: () => getToken(),              // 可选：Agent token、会话令牌或 JWT；不给就用 cookie
  locale: 'zh',                          // 错误信息的语言
})

const me = await client.me()
const ws = me.workspaces[0].id
const pages = await client.pages(ws)

// 实时事件（SSE，断了自动重连，从断的地方续上）
const events = client.events(ws)
events.on('page.updated', (p) => console.log(p.title))

// 协作文档（Yjs + WebSocket；浏览器里带 IndexedDB 缓存）
const conn = client.openPage(pages[0].id)
await conn.ready

// 数据库：本机缓存 + 增量同步，筛选排序分组在 WASM 里算（和服务器同一份代码）
const db = client.database(dbId)
await db.open(events)
const { ids } = db.engine!.query({ sorts: [{ prop: 'n', dir: 'desc' }] })
await db.setCells([{ row_id: ids[0], key: 'n', value: 3 }])   // 先改本地，服务器失败会退回
```

入口：`@open-agent-docs/client`（全部）、`@open-agent-docs/client/core`（只有接口和事件，不带 Yjs、WASM，最小）、`@open-agent-docs/client/page`、`@open-agent-docs/client/db`。
只用 core 时要显式注册：`registerPlugin('openPage', openPage)`。

### @open-agent-docs/editor

```tsx
import { OadEditor, OadTitle, OadComments, OadDatabaseView, useOadPage, colorFor } from '@open-agent-docs/editor'
import '@open-agent-docs/editor/style.css'

function Doc({ pageId, me }) {
  const user = { id: me.user.id, name: me.user.name, color: colorFor(me.user.id) }
  const doc = useOadPage(client, pageId, { user })
  if (!doc.ydoc) return null
  return (
    <>
      <OadTitle ydoc={doc.ydoc} />
      <OadEditor ydoc={doc.ydoc} provider={doc.provider} user={user} />
    </>
  )
}
```

`react`、`react-dom`、`yjs`、`@open-agent-docs/client` 是 peer 依赖。组件的样式只在 `.oad-root` 里生效，不会影响宿主；颜色、字体用 CSS 变量换。

图片、视频、上传的文件：同域 cookie 登录时不用管。服务器在别的域名、或者用 token 登录（JWT、Agent token）时，`<img>` 带不了 token，
给编辑器一个解析地址的钩子，显示前换成签名地址（批量调 `POST /files/sign`，缓存到过期前 60 秒）：

```tsx
import { clientUrlResolver } from '@open-agent-docs/editor'

const resolveUrl = clientUrlResolver(client, { token: true })   // 同域 cookie 登录时返回 null（不用换）
<OadEditor ydoc={doc.ydoc} provider={doc.provider} user={user} media={{ resolveUrl }} />
<OadDatabaseView client={client} dbId={dbId} media={{ resolveUrl }} … />
```
界面语言：`import { setLocale } from '@open-agent-docs/editor/i18n'`。完整的例子（编辑器 + 评论栏 + 数据库视图 + 两个用户）：[examples/embed-react](../../examples/embed-react)。

## header 登录

宿主自己做登录，在服务端把 `/api/*`（包括 WebSocket `/api/v1/sync/*`）转给 Open Agent Docs，转发时带上：

| 请求头 | 说明 |
| --- | --- |
| `X-OAD-User-Id` | 宿主那边的用户编号（必填，第一次出现时自动建用户、加进默认工作区） |
| `X-OAD-User-Name` | 显示的名字；非 ASCII 的用百分号编码（`encodeURIComponent`） |
| `X-OAD-User-Email` | 可选 |
| `X-OAD-Secret` | 和 `OAD_HEADER_SECRET` 一样才认 |

```sh
OAD_AUTH=header OAD_HEADER_SECRET='一串长的随机字符' OAD_HOST=127.0.0.1 open-agent-docs serve
```

- Open Agent Docs 只监听内网，浏览器连不到它，只能经过宿主；密钥只在宿主的服务端。
- 宿主和浏览器看到的域名不一样时，把宿主的地址写进 `OAD_ALLOWED_ORIGINS`（WebSocket 握手要检查 Origin）。
- Agent 照常用自己的 token 连 `/mcp`，不受 header 登录影响。

## JWT

宿主用自己的密钥签一个短期 JWT 交给浏览器，浏览器直接连 Open Agent Docs（不用代理）：

```sh
OAD_AUTH=jwt OAD_JWT_SECRET='…' open-agent-docs serve          # HS256 / HS384 / HS512
OAD_AUTH=jwt OAD_JWT_JWKS_URL=https://auth.example.com/.well-known/jwks.json open-agent-docs serve   # RS* / ES256 / ES384 / EdDSA
# 可选：OAD_JWT_ISSUER、OAD_JWT_AUDIENCE、OAD_JWT_LEEWAY（秒，默认 60）
```

- 带法：`Authorization: Bearer <jwt>`、WebSocket 用 `?token=<jwt>`，或 cookie `oad_token`。`@open-agent-docs/client` 的 `token` 选项都替你处理了。
- 浏览器在别的域名上直连：`OAD_ALLOWED_ORIGINS` 写上宿主的 origin（接口的 CORS、WebSocket 的 Origin 检查都看它）；图片、文件用签名地址（[只读 HTML](#只读-html)）。
- claims：`sub`（必填，用户编号）、`exp`（必填）、`name`、`email`、`workspaces`（工作区 id 的数组，或 `[{ "id": "…", "role": "editor" }]`；不给就进默认工作区）。
- 用户按 `sub` 认，第一次见到时自动建；名字跟着 claims 变；claims 里给了角色就以它为准，没给的新成员是 editor。
- JWKS 按 `kid` 缓存 10 分钟，宿主换了钥匙（新的 kid）会自动再取；签名算法必须和那把钥匙的一致。
- Agent 照常用自己的 token，不受影响。

## 进程内启动（Node / Python）

不想另外起一个进程时，在宿主自己的进程里启动服务器。服务器跑在它自己的线程和 tokio 运行时上，不占 Node 的事件循环、不占 Python 的 GIL；
配置项和命令行完全一样（`OAD_*`，见[配置](configuration.md)），写成 `dataDir` / `data_dir` 也行，没给的再看环境变量；不给端口时随便挑一个空端口。

**Node**（`@open-agent-docs/server`，napi-rs，`packages/server-node`）：

```js
import { start } from '@open-agent-docs/server'

const oad = await start({ dataDir: './data', auth: 'header', headerSecret: process.env.OAD_HEADER_SECRET })
console.log(oad.url)            // http://127.0.0.1:<端口>
app.use(oad.handler)            // Express / Node http：把请求转给它（流式，SSE 也行）
server.on('upgrade', oad.upgrade)  // 协同编辑的 WebSocket
await oad.close()               // 页面都存好后 resolve
```

`handler` 是把请求原样转给进程里的服务器，不是在 Node 里直接调 Rust：服务器是异步的、有自己的运行时，
按请求过一遍 FFI 要拷贝请求和响应的正文，还接不上 SSE、WebSocket 这种长连接；转发走本机回环，开销很小。
网页里的链接默认在根路径上，所以要么挂在根上，要么只把 `/api`、`/s` 转过去、网页另外处理。
例子：[examples/node-inprocess](../../examples/node-inprocess)（Express，header 登录）。

**Python**（`pip install "open-agent-docs[server]"`：Python SDK 加上进程内服务器 `open-agent-docs-server`，PyO3 的 abi3 wheel，源码在 `crates/oad-py`）：

```python
import open_agent_docs as oad

with oad.start(data_dir="./data", auth="header", header_secret=SECRET) as server:   # 退出 with 时关掉（页面都存好）
    admin = oad.Client(base_url=server.url, header_secret=SECRET)
    alice = admin.with_user("alice", name="Alice")                                   # 按宿主认出的用户调
    alice.pages.create(alice.default_workspace(), title="周报", markdown="# 本周")
```

只装 `open-agent-docs`（不带 `[server]`）时，`oad.start()` 会下载发布页的二进制，作为子进程启动。
SDK 的完整用法见 [SDK](sdks.md)。例子：[examples/python-inprocess](../../examples/python-inprocess)（在 FastAPI 的 lifespan 里起服务器，每个请求按当前用户 `with_user`）。

## 挂进 Rust 服务

Rust（axum）宿主直接依赖 `oad-core`、`oad-server`：

1. 实现自己的 `AuthProvider`：从宿主的会话认出是谁，按外部 id 在存储里找到 / 建用户（`Storage::user_by_external` / `create_user`），
   用 `oad_core::join_default` 加进默认工作区；
2. `Core::open_full(storage, AuthConfig::Custom(Arc::new(你的实现)), …)`；
3. `oad_server::router(AppState::new(core, ServerConfig::default()))` 和自己的路由 `merge`（只要接口不要网页用 `oad_server::api_router`）。
   多租户用 `oad_server::tenant::multi_tenant`（按 Host 或请求头选租户，每个租户一套）。

例子：[examples/rust-mount](../../examples/rust-mount)。
