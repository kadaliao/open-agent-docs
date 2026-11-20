# 快速开始

[English](../en/quickstart.md) · [文档目录](README.md)

服务器默认监听 `127.0.0.1:8848`，数据放在当前目录的 `./data`。

## 系统要求

在本机跑服务器的几种方式（发布页的二进制、Homebrew、Python 的 `open-agent-docs[server]`、Node 的 `@open-agent-docs/server`、
各 SDK 的 `start()`）用的是同一份编好的程序，要求一样：

| 系统 | 要求 |
| --- | --- |
| macOS | 11 起，Apple Silicon 和 Intel |
| Linux | x86_64、arm64，**glibc 2.34 起**：Ubuntu 22.04+、Debian 12+、RHEL / Rocky / AlmaLinux 9+、Fedora 35+、Amazon Linux 2023 |
| Windows | 没有原生版本，用 Docker 或 WSL2（Ubuntu 22.04+） |

`ldd --version` 第一行能看到本机的 glibc 版本。Ubuntu 20.04、Debian 11、CentOS 7 / 8、RHEL 8、Amazon Linux 2 的 glibc 太老，
Alpine 用的是 musl，这些系统上请用 [Docker](#docker)（镜像自带运行环境，不挑系统）。

只是**连**服务器的话没有这个要求：各语言的 SDK 本身（Python、Go、Rust、JS 客户端）走 HTTP，哪里都能跑。

## Homebrew

macOS、Linux（arm64 和 x86_64）：

```sh
brew install kadaliao/tap/open-agent-docs
# 先建管理员（密码在终端里输入），数据目录和下面的服务一致
open-agent-docs user add --email me@example.com --name 我 --role owner --data-dir "$(brew --prefix)/var/open-agent-docs"
brew services start open-agent-docs      # 当服务跑，开机自启；数据在 $(brew --prefix)/var/open-agent-docs
```

打开 http://127.0.0.1:8848 登录。日志在 `$(brew --prefix)/var/log/open-agent-docs.log`，升级用 `brew upgrade open-agent-docs`。

想在前台跑、直接看日志，先 `brew services stop open-agent-docs`，再 `open-agent-docs serve --data-dir "$(brew --prefix)/var/open-agent-docs"`。
不加 `--data-dir` 用的是当前目录下的 `./data`，上面建的账号不在那里，登录会说密码不对。

## 二进制

从[发布页](https://github.com/kadaliao/open-agent-docs/releases/latest)下载对应平台的压缩包（macOS、Linux，arm64 和 x86_64 都有），或者[从源码编](#从源码编)。

```sh
tar xzf open-agent-docs-*.tar.gz
cd open-agent-docs-*/
# 只有 macOS 要：程序没做公证，第一次运行前去掉隔离标记
xattr -d com.apple.quarantine open-agent-docs 2>/dev/null
```

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

不用下源码，直接用发布好的镜像（`ghcr.io/kadaliao/open-agent-docs`，amd64 和 arm64；`latest` 是最新版，也可以写死版本号，比如 `:0.1.0`）：

```sh
docker run -d --name oad -p 8848:8848 -v oad-data:/data \
  -e OAD_ADMIN_EMAIL=me@example.com -e OAD_ADMIN_PASSWORD='至少 8 位' \
  ghcr.io/kadaliao/open-agent-docs
```

用 Docker Compose：

```sh
curl -fsSLO https://raw.githubusercontent.com/kadaliao/open-agent-docs/main/docker-compose.yml
# 改一下里面的 OAD_ADMIN_EMAIL
OAD_ADMIN_PASSWORD='至少 8 位' docker compose up -d
```

想自己编镜像，在源码目录里 `docker build -t open-agent-docs .`。

镜像里已经设好 `OAD_HOST=0.0.0.0`、`OAD_DATA_DIR=/data`，以非 root 用户（uid 65532）运行。用自己的目录挂载时先
`chown 65532:65532` 那个目录。镜像里只有这一个程序，没有 shell；加账号这样跑：

```sh
docker run --rm -it -v oad-data:/data ghcr.io/kadaliao/open-agent-docs user add --email you@example.com --role editor
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
