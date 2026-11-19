# Open Agent Docs

[English](README.md) | 简体中文

人和 AI Agent 一起写的文档。

自己部署的文档服务器。Agent（Claude Code、Codex、各种 Bot、自己写的脚本）经 MCP 读写页面；人在浏览器里和它们同时编辑、评论、建数据库。

- **Agent 按块改。** 页面读出来是 Markdown，每块前面有块编号。Agent 只改想改的块，带回读到的版本，服务器合并，谁打的字都不会被覆盖。
- **人用的实时编辑器。** 多人协作的块编辑器，能看到别人在哪；历史版本和恢复、子页面、回收站、公开分享、中英文搜索、中英文界面，装成 PWA 能离线用。
- **评论能叫到 Agent。** 在评论里 @ 它，或者评论它写的内容，它就收到通知（webhook、SSE 或轮询），带着写好的提示词，在评论里回复。
- **数据库。** 表格、看板、日历、画廊、列表五种视图；单选、状态、日期、人员、文件、公式、关联、汇总、自动编号、按钮。每一行也是一个页面。
- **能嵌进自己的应用。** 放一个 iframe，或者用 `@oad/client`、`@oad/editor` 两个包，登录用自己的。
- **一个程序，一个数据目录。** Rust 写的服务器，网页打在里面；SQLite。

## 跑起来

```sh
docker build -t open-agent-docs .
docker run -d -p 8848:8848 -v oad-data:/data \
  -e OAD_ADMIN_EMAIL=me@example.com -e OAD_ADMIN_PASSWORD='自己定一个' open-agent-docs
```

打开 http://127.0.0.1:8848 。直接用二进制、从源码编，见[快速开始](docs/zh/quickstart.md)。

接上 Claude Code（在网页的 **Agent** 里建一个，拿到 token）：

```sh
claude mcp add --transport http docs http://127.0.0.1:8848/mcp --header "Authorization: Bearer oad_..."
```

## 文档

- [快速开始](docs/zh/quickstart.md) · [配置](docs/zh/configuration.md) · [部署](docs/zh/deployment.md)
- [Agent](docs/zh/agents.md)（MCP、通知、webhook） · [Agent 看到的 Markdown](docs/zh/markdown.md)
- [嵌入到自己的应用](docs/zh/embedding.md)（JS 包、iframe、header 登录、JWT）
- [从旧版（Python）迁移](docs/zh/migration.md)

## 开发

```sh
pnpm install && sh packages/db-wasm/build.sh
cargo run -p oad-bin -- serve        # 服务器，:8848
pnpm dev                             # 网页（热更新，/api 转给 :8848）
pnpm -r test && cargo test           # 测试
```

旧的 Python 版在 [legacy/](legacy/)，迁移做完以前留着。

## 许可

MIT
