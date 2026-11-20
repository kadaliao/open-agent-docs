# @open-agent-docs/server

English | [简体中文](https://github.com/kadaliao/open-agent-docs/blob/main/packages/server-node/README.zh-CN.md)

Run an [Open Agent Docs](https://github.com/kadaliao/open-agent-docs) server inside your Node process (Rust via napi-rs). The server runs on its own threads and runtime,
so it doesn't use Node's event loop. Supports macOS 11+ (arm64, x64) and Linux (arm64, x64, glibc 2.34+: Ubuntu 22.04+, Debian 12+, RHEL 9+);
on install, npm only downloads the native module for your platform. [System requirements](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/quickstart.md#system-requirements)

```sh
npm i @open-agent-docs/server @open-agent-docs/client
```

```js
import { start } from '@open-agent-docs/server'
import { createClient } from '@open-agent-docs/client/node'

const server = await start({ dataDir: './data', auth: 'header', headerSecret: process.env.OAD_HEADER_SECRET })
console.log(server.url)                 // http://127.0.0.1:<port>; a free port is picked when you don't pass port

app.use(server.handler)                 // Express / Node http: forward requests to it (SSE works too)
httpServer.on('upgrade', server.upgrade) // the WebSocket for collaborative editing

const client = createClient({ baseUrl: server.url, headerSecret: process.env.OAD_HEADER_SECRET })
const alice = client.withUser('alice', { name: 'Alice' })
await alice.createPage(await alice.defaultWorkspace(), { title: 'Weekly' })

await server.close()                    // resolves once all pages are saved
```

Options are the same as on the command line (`OAD_*`; `dataDir`, `auth` and so on work too), see [configuration](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/configuration.md).
On older Linux (glibc below 2.34), Windows and the like, run the server separately with the Docker image `ghcr.io/kadaliao/open-agent-docs` (the binaries on the releases page have the same requirements as this package).

More: [Embedding in your own app](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/embedding.md) · [SDKs](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/sdks.md) · [简体中文文档](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/embedding.md)
