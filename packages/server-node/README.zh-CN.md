# @open-agent-docs/server

[English](https://github.com/kadaliao/open-agent-docs/blob/main/packages/server-node/README.md) | 简体中文

在 Node 进程里起 [Open Agent Docs](https://github.com/kadaliao/open-agent-docs) 服务器（Rust，napi-rs）。服务器跑在自己的线程和运行时上，不占 Node 的事件循环。
支持 macOS 11+（arm64、x64）和 Linux（arm64、x64，glibc 2.34 起：Ubuntu 22.04+、Debian 12+、RHEL 9+）；装的时候 npm 只下载当前平台的原生模块。[系统要求](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/quickstart.md#系统要求)

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
更老的 Linux（glibc 低于 2.34）、Windows 等请单独跑服务器，用 Docker 镜像 `ghcr.io/kadaliao/open-agent-docs`（发布页的二进制和这个包的要求一样）。

更多：[嵌入到自己的应用](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/embedding.md) · [SDK](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/sdks.md) · [English docs](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/embedding.md)
