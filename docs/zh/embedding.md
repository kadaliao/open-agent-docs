# 嵌入到自己的应用

[English](../en/embedding.md) · [文档目录](README.md)

网页这边两种做法：

- **iframe**：放一个 iframe 指向 Open Agent Docs 的页面，不用写前端代码。
- **JS 包**：`@oad/client`（接口、实时事件、协作文档、本地数据库）和 `@oad/editor`（React 组件：编辑器、评论栏、数据库视图），
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
| 宿主 → iframe | `{ type: 'oad:navigate', pageId }` 换一页 |
| | `{ type: 'oad:theme', theme: 'light' \| 'dark' \| 'system' }` |
| | `{ type: 'oad:locale', locale: 'zh' \| 'en' }` |

```js
const frame = document.querySelector('iframe')
addEventListener('message', (e) => {
  if (e.origin !== 'https://docs.example.com') return
  if (e.data?.type === 'oad:height') frame.style.height = e.data.height + 'px'
})
frame.contentWindow.postMessage({ type: 'oad:theme', theme: 'dark' }, 'https://docs.example.com')
```

iframe 里用的是 Open Agent Docs 自己的登录（cookie）。宿主和它不同域名时浏览器可能拦第三方 cookie：
让两边同域（反向代理到同一个域名下），或者用 header / JWT 登录。完整的例子：[examples/embed-iframe](../../examples/embed-iframe)。

## JS 包

> 还没发布到 npm。现在用仓库里打的包：`pnpm pack:all` 生成 `dist-packs/*.tgz`，在你的项目里 `npm i ./oad-client-x.y.z.tgz` 等。

```sh
npm i @oad/client @oad/editor react react-dom yjs
```

### @oad/client

```ts
import { createClient } from '@oad/client'

const client = createClient({
  baseUrl: 'https://docs.example.com',  // 同源时给 ''
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

入口：`@oad/client`（全部）、`@oad/client/core`（只有接口和事件，不带 Yjs、WASM，最小）、`@oad/client/page`、`@oad/client/db`。
只用 core 时要显式注册：`registerPlugin('openPage', openPage)`。

### @oad/editor

```tsx
import { OadEditor, OadTitle, OadComments, OadDatabaseView, useOadPage, colorFor } from '@oad/editor'
import '@oad/editor/style.css'

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

`react`、`react-dom`、`yjs`、`@oad/client` 是 peer 依赖。组件的样式只在 `.oad-root` 里生效，不会影响宿主；颜色、字体用 CSS 变量换。
界面语言：`import { setLocale } from '@oad/editor/i18n'`。完整的例子（编辑器 + 评论栏 + 数据库视图 + 两个用户）：[examples/embed-react](../../examples/embed-react)。

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

- 带法：`Authorization: Bearer <jwt>`、WebSocket 用 `?token=<jwt>`，或 cookie `oad_token`。`@oad/client` 的 `token` 选项都替你处理了。
- claims：`sub`（必填，用户编号）、`exp`（必填）、`name`、`email`、`workspaces`（工作区 id 的数组，或 `[{ "id": "…", "role": "editor" }]`；不给就进默认工作区）。
- 用户按 `sub` 认，第一次见到时自动建；名字跟着 claims 变；claims 里给了角色就以它为准，没给的新成员是 editor。
- JWKS 按 `kid` 缓存 10 分钟，宿主换了钥匙（新的 kid）会自动再取；签名算法必须和那把钥匙的一致。
- Agent 照常用自己的 token，不受影响。

## 进程内启动（Node / Python）

不想另外起一个进程时，在宿主自己的进程里启动服务器。服务器跑在它自己的线程和 tokio 运行时上，不占 Node 的事件循环、不占 Python 的 GIL；
配置项和命令行完全一样（`OAD_*`，见[配置](configuration.md)），写成 `dataDir` / `data_dir` 也行，没给的再看环境变量；不给端口时随便挑一个空端口。

**Node**（`@oad/server`，napi-rs，`packages/server-node`）：

```js
import { start } from '@oad/server'

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

**Python**（`open-agent-docs`，PyO3 的 abi3 wheel，`crates/oad-py`，用 maturin 编）：

```python
import open_agent_docs as oad

with oad.start(data_dir="./data", auth="header", header_secret=SECRET) as server:
    print(server.url)           # 退出 with 时关掉（页面都存好）
# 或者：server = oad.start(...); ...; server.close()
```

例子：[examples/python-inprocess](../../examples/python-inprocess)（FastAPI 的 lifespan 里起，宿主经 httpx 以某个用户的身份调接口）。

## 挂进 Rust 服务

Rust（axum）宿主直接依赖 `oad-core`、`oad-server`：

1. 实现自己的 `AuthProvider`：从宿主的会话认出是谁，按外部 id 在存储里找到 / 建用户（`Storage::user_by_external` / `create_user`），
   用 `oad_core::join_default` 加进默认工作区；
2. `Core::open_full(storage, AuthConfig::Custom(Arc::new(你的实现)), …)`；
3. `oad_server::router(AppState::new(core, ServerConfig::default()))` 和自己的路由 `merge`（只要接口不要网页用 `oad_server::api_router`）。
   多租户用 `oad_server::tenant::multi_tenant`（按 Host 或请求头选租户，每个租户一套）。

例子：[examples/rust-mount](../../examples/rust-mount)。
