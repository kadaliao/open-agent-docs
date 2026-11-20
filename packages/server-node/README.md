# @open-agent-docs/server

在 Node 进程里起 [Open Agent Docs](https://github.com/kadaliao/open-agent-docs) 服务器（Rust，napi-rs）。服务器跑在自己的线程和运行时上，不占 Node 的事件循环。
支持 macOS（arm64、x64）和 Linux（glibc，arm64、x64）；装的时候 npm 只下载当前平台的原生模块。

Run an Open Agent Docs server inside your Node process (Rust via napi-rs). macOS (arm64, x64) and Linux (glibc, arm64, x64).

```sh
npm i @open-agent-docs/server @open-agent-docs/client
```

```js
import { start } from '@open-agent-docs/server'
import { createClient } from '@open-agent-docs/client/node'

const server = await start({ dataDir: './data', auth: 'header', headerSecret: process.env.OAD_HEADER_SECRET })
console.log(server.url)                 // http://127.0.0.1:<端口>，不给 port 时挑一个空端口

app.use(server.handler)                 // Express / Node http：把请求转给它（SSE 也行）
httpServer.on('upgrade', server.upgrade) // 协同编辑的 WebSocket

const client = createClient({ baseUrl: server.url, headerSecret: process.env.OAD_HEADER_SECRET })
const alice = client.withUser('alice', { name: 'Alice' })
await alice.createPage(await alice.defaultWorkspace(), { title: '周报' })

await server.close()                    // 页面都存好后 resolve
```

配置项和命令行一样（`OAD_*`，写成 `dataDir`、`auth` 也行），见[配置](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/configuration.md)。
别的平台（Windows 等）请单独跑服务器：Docker 镜像 `ghcr.io/kadaliao/open-agent-docs` 或发布页的二进制。

更多：[嵌入到自己的应用](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/embedding.md) · [SDK](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/sdks.md) · [English docs](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/embedding.md)
