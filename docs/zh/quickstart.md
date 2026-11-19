# 快速开始

[English](../en/quickstart.md) · [文档目录](README.md)

服务器默认监听 `127.0.0.1:8848`，数据放在当前目录的 `./data`。

## 二进制

从发布页下载对应平台的 `open-agent-docs`（发布以后），或者[从源码编](#从源码编)。

```sh
# 还没有任何用户时，用这两个变量建管理员（已经有用户了就不管它们）
OAD_ADMIN_EMAIL=me@example.com OAD_ADMIN_PASSWORD='至少 8 位' \
  ./open-agent-docs serve --data-dir ~/oad-data
```

打开 http://127.0.0.1:8848 ，用上面的邮箱和密码登录。

也可以先建账号再启动：

```sh
./open-agent-docs user add --email me@example.com --name 我 --role owner --data-dir ~/oad-data
# 密码从 OAD_USER_PASSWORD 读，没设就在终端里输入
```

只在自己电脑上用、不想登录：

```sh
OAD_AUTH=none ./open-agent-docs serve
```

`none` 模式下所有请求都是同一个本地用户，只能监听本机地址（监听别的地址时会警告：能连上的人都能读写）。

## Docker

```sh
docker build -t open-agent-docs .
docker run -d --name oad -p 8848:8848 -v oad-data:/data \
  -e OAD_ADMIN_EMAIL=me@example.com -e OAD_ADMIN_PASSWORD='至少 8 位' \
  open-agent-docs
```

镜像里已经设好 `OAD_HOST=0.0.0.0`、`OAD_DATA_DIR=/data`，以非 root 用户（uid 65532）运行。用自己的目录挂载时先
`chown 65532:65532` 那个目录。镜像里只有这一个程序，没有 shell；加账号这样跑：

```sh
docker run --rm -it -v oad-data:/data open-agent-docs user add --email you@example.com --role editor
```

对外提供服务前看[部署](deployment.md)：放在 HTTPS 反向代理后面，设 `OAD_PUBLIC_URL` 和 `OAD_COOKIE_SECURE`。

## 从源码编

要 Node 22+、pnpm 8、Rust（stable）、`wasm32-unknown-unknown` 和 `wasm-bindgen-cli 0.2.130`。

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked

pnpm install
sh packages/db-wasm/build.sh          # 网页里的数据库引擎（WASM）
pnpm build                            # 网页 → apps/web/dist
cargo build --release -p oad-bin      # 网页打进二进制 → target/release/open-agent-docs
```

开发时：`cargo run -p oad-bin -- serve` 起服务器，`pnpm dev` 起网页（Vite，把 `/api` 转给 8848）。
测试：`pnpm -r test`、`cargo test`。

## 下一步

- 接上 Agent：[Agent](agents.md)
- 所有设置：[配置](configuration.md)
