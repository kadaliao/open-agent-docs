# 第 6 阶段约定（SDK 与切换）

目标：别人能把 Open Agent Docs 嵌进自己的服务。接口定稿，发 JS 包、Rust crate、Node / Python 绑定，支持 Postgres，写好文档，next 合回 main，删掉 `legacy/`。

## 接法（三种，按宿主的语言挑）

1. **Rust 服务**：依赖 `oad-server`，`oad_server::router(state)` 挂到自己的 axum 里；登录用自己的 `AuthProvider` 实现（宿主已经知道用户是谁）。
2. **任何语言，旁挂**：跑 `open-agent-docs serve --auth header`，宿主的反向代理在转发时带 `X-OAD-User-*` 和 `X-OAD-Secret`；
   或者宿主签 JWT（`OAD_AUTH=jwt`，HS256 共享密钥或 JWKS 地址，claims：`sub`、`name`、`email`、`workspaces`），浏览器直接连。
3. **Node / Python 进程内**：`@oad/server`（napi-rs）/ `open-agent-docs`（PyO3 wheel）启动内嵌服务器（自己的线程和 tokio 运行时），
   返回端口或一个能挂到宿主 HTTP 框架上的处理函数；同一套配置项。

网页这边：宿主页面里放 `@oad/editor`（只要编辑器）或 iframe 整个 `apps/web`（`?embed=1` 去掉侧栏和顶栏）。

## 接口定稿

- 服务器用 `utoipa` 从代码生成 OpenAPI 3.1（`GET /api/v1/openapi.json`，也写进 `docs/openapi.json`），CI 检查和代码一致。
- 所有接口加 `/api/v1` 版本前缀（已经是）；破坏性改动以后走 `/api/v2`。错误码表、分页（`cursor`）、限流头统一。
- Webhook 和 SSE 事件的格式也写进 OpenAPI（`x-webhooks`）。

## JS 包（`packages/`）

- `@oad/client`：类型从 OpenAPI 生成（`openapi-typescript`）+ 手写的薄封装：`createClient({ baseUrl, token? | cookie, fetch? })`，
  每个接口一个方法；`client.events(ws)`（SSE，自动重连、Last-Event-ID）；`client.openPage(id)` 返回 `{ ydoc, provider, awareness, destroy }`
  （y-websocket + IndexedDB 缓存，和 apps/web 用的是同一份代码）；`client.database(id)` 返回带 WASM 引擎的本地数据库对象（rows、query、patch、订阅变化）。
  浏览器和 Node（18+）都能用。
- `@oad/editor`：能单独发布（库模式打包，`react`、`react-dom` 是 peer 依赖，样式单独一个 css 文件，可用 CSS 变量换主题），
  `<OadEditor>`、`<OadTitle>`、`<OadComments>`、`<OadDatabaseView>` 几个组件 + `useOadPage(client, id)` hook。
- `@oad/markdown`、`@oad/db`：WASM 包，单独发布。
- 版本号统一，`pnpm -r publish` 前有 `prepack` 构建；先不真的发到 npm（用户说了再发），只确保 `pnpm pack` 出来的包能在一个干净项目里装上能用。
- `examples/`：`embed-react`（Vite + React 宿主页面嵌编辑器，宿主用一个几十行的 Node 代理做 header 登录）、`embed-iframe`（纯 HTML iframe 嵌整个应用）、
  `node-inprocess`（Express 里进程内启动）、`python-inprocess`（FastAPI 里进程内启动）、`rust-mount`（axum 挂 Router）。每个都有 README 和能跑的冒烟测试。

## 服务器

- `Storage` 的 Postgres 实现（`sqlx` 或 `tokio-postgres`，`OAD_DATABASE_URL=postgres://…`），和 SQLite 跑同一套存储测试；
  文件存储加 S3 兼容实现（`OAD_BLOB_URL=s3://…`）。
- `OAD_AUTH=jwt`。
- napi-rs 绑定 `crates/oad-node` → `packages/server-node`（@oad/server）；PyO3 绑定 `crates/oad-py`（maturin）。
- 多工作区 / 多租户：`TenantResolver`（按 Host 或请求头选数据目录 / 数据库 schema），默认单租户。

## 文档（`docs/`，中英双语）

- 快速开始（单个二进制、Docker）、配置项全表、三种接法、Agent 接入（MCP、通知）、HTTP API（从 OpenAPI 生成的参考）、数据模型、
  Markdown 约定（Agent 看到的样子）、部署（反代、HTTPS、备份）、从旧版迁移（旧版数据导出成 Markdown + CSV 再导入的脚本）。
- 仓库根的 README / README.zh-CN 换成新版。

## 切换

- 旧版数据迁移脚本（`legacy` 的 SQLite → 新版），测一份真实形状的数据。
- 全部测试过、性能目标复测一遍写进 `docs/baseline.md`（新旧对照）后，next 合回 main，删掉 `legacy/` 和 `spikes/`（用户确认后再合、再推）。

## JS 包（实际的样子，网页这边）

- `@oad/client`：`createClient({ baseUrl, token?, credentials?, fetch?, locale?, headers?, WebSocket? })`。入口：
  `@oad/client`（全部）、`@oad/client/core`（只有 HTTP 方法和事件，没有 Yjs / WASM）、`@oad/client/page`（`openPage`）、`@oad/client/db`（`DbStore`、引擎）。
  只用 core 时要显式 `registerPlugin('openPage', openPage)` / `registerPlugin('database', databasePlugin)`（只为副作用的 import 会被打包工具摇掉）。
  类型在 `src/types.ts` 手写；有了 `docs/openapi.json` 以后 `pnpm --filter @oad/client gen:types` 生成 `src/openapi.ts`，types.ts 改成从那里取。
  WebSocket 带 token 用 `?token=`。apps/web 的 HTTP、协作文档连接、数据库本地对象都用这一份。
- `@oad/editor`：入口 `@oad/editor`（编辑器、标题、`OadComments`、按需加载的 `OadDatabaseView`、`useOadPage`、`OadProvider`）、
  `@oad/editor/database`（数据库视图直接打进去的版本、行的属性头）、`@oad/editor/i18n`（`setLocale`）、`@oad/editor/schema`、`@oad/editor/style.css`。
  `react`、`react-dom`、`yjs`、`@oad/client` 是 peer 依赖。组件里的通用控件样式只在 `.oad-root` 里生效。apps/web 的评论栏、数据库界面、界面语言都从这里拿。
- 工作区里的包 `exports` 多一个 `oad-source` 条件指向源码（apps/web、examples 开发时用，不用先打包）；发出去的包用 `dist/`。`pnpm pack` 前 `prepack` 构建。
- 版本号：`node scripts/versions.mjs` 检查（`node scripts/versions.mjs 0.2.0` 一起改）；互相依赖写 `workspace:*`，pack 时换成真的版本号。
- iframe 嵌入（`?embed=1`）：没有侧栏、顶栏；参数 `theme`、`lang`；`postMessage`：发 `oad:ready`、`oad:navigate`、`oad:height`，收 `oad:navigate`、`oad:theme`、`oad:locale`。
  服务器那边不需要改（没有 X-Frame-Options）。
