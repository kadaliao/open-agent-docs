# node-inprocess：在 Express 里进程内跑 Open Agent Docs

`@oad/server` 在 Node 进程里起服务器（Rust，自己的线程和运行时，不占事件循环），`start()` 返回 `{ url, port, close(), handler, upgrade }`：

- `handler` 挂到 Express / Node http 上，把请求原样转给进程里的服务器（流式，SSE 也行）；
- `upgrade` 处理协同编辑的 WebSocket：`server.on('upgrade', oad.upgrade)`；
- 宿主用 header 登录（`auth: 'header'`）：转发前加上 `X-OAD-Secret` 和 `X-OAD-User-Id / -Name / -Email`。

```sh
pnpm --filter @oad/server build        # 编 Rust 绑定（第一次要几分钟）
pnpm --filter @oad/example-node-inprocess start   # http://127.0.0.1:3001/host
pnpm --filter @oad/example-node-inprocess smoke   # 冒烟测试
```

配置项和命令行一样（`OAD_*`，写成 `dataDir`、`auth`、`headerSecret` 也行），见 docs/zh/configuration.md。
