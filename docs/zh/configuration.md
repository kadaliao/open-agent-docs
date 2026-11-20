# 配置

[English](../en/configuration.md) · [文档目录](README.md)

都是环境变量；`serve` 的前几项也能写成命令行参数（`open-agent-docs serve --help`）。
进程内启动时（Node 的 `@open-agent-docs/server`、Python 的 `open_agent_docs`）同样的名字写成选项，`OAD_DATA_DIR` 也可以写成 `dataDir` / `data_dir`，
选项里没给的再看环境变量（见[嵌入](embedding.md#进程内启动node--python)）。
空值等于没设。下表是代码里读到的全部 `OAD_*`（`crates/` 下）。

## 服务器

| 变量 | 参数 | 默认 | 说明 |
| --- | --- | --- | --- |
| `OAD_PORT` | `--port` | `8848` | 端口 |
| `OAD_HOST` | `--host` | `127.0.0.1` | 监听地址；对外提供服务设 `0.0.0.0`（Docker 镜像里已设） |
| `OAD_DATA_DIR` | `--data-dir` | `./data` | 数据目录：`oad.db`（SQLite）、`blobs/`（上传的文件）、`search/`（搜索索引）。`user add` 也读它 |
| `OAD_WEB_DIR` | `--web-dir` | 无 | 从这个目录读网页（开发用）；不设就用打进二进制的 |
| `OAD_PUBLIC_URL` | `--public-url` | 无 | 对外的地址，如 `https://docs.example.com`。通知、webhook、分享里的链接用它；它的 Host 和 Origin 自动允许 |
| `OAD_COOKIE_SECURE` | `--cookie-secure` | 关 | 会话 cookie 加 `Secure`。走 HTTPS 时打开（`true` / `1`） |
| `OAD_ALLOWED_HOSTS` | `--allowed-hosts` | 无 | 额外允许的 Host，逗号分隔；`*` 表示不检查。见下面的“请求来源” |
| `OAD_ALLOWED_ORIGINS` | `--allowed-origins` | 无 | 允许跨源的 Origin（嵌入到别的域名时），逗号分隔，如 `https://app.example.com` |
| `OAD_RATE_LIMIT` | | `1200` | 每个 token / 会话（没有就按 IP）每分钟最多多少个请求；`0` 不限 |
| `OAD_MAX_UPLOAD_MB` | | `100` | 上传文件的大小上限（MB） |
| `OAD_TZ` | | 系统时区 | 服务器时区，IANA 名字，如 `Asia/Shanghai`。数据库里的“今天”、相对日期、时间的显示按它算；认不出时用系统的 |

## 存储

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `OAD_DATABASE_URL` | 无 | `postgres://user:pass@host:5432/db`：数据库改用 Postgres（不设就是数据目录里的 SQLite）。上传的文件和搜索索引还在 `OAD_DATA_DIR`。地址里加 `?sslmode=require` 走 TLS（按公共 CA 验证书），否则不加密 |
| `OAD_BLOB_URL` | 无 | `s3://<bucket>/<前缀>?region=<区域>&endpoint=<地址>`：上传的文件、缩略图放进 S3 兼容的存储（AWS S3、MinIO、R2……）。给了 `endpoint` 用路径式地址（MinIO 这样），不给用 AWS 的 |
| `AWS_ACCESS_KEY_ID`、`AWS_SECRET_ACCESS_KEY` | 无 | `OAD_BLOB_URL` 用的密钥（必填）；可选 `AWS_SESSION_TOKEN`、`AWS_REGION`（地址里没写 region 时） |

## 多租户

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `OAD_TENANTS` | 无（单租户） | `host:<域名>`：按子域名分租户（`acme.docs.example.com` 是租户 `acme`，`docs.example.com` 本身是 `main`）；`header:<请求头名>`：按请求头（如 `header:x-oad-tenant`，只能放在可信的反向代理后面）。每个租户的数据在 `<OAD_DATA_DIR>/tenants/<租户>/`；用 Postgres 时是 schema `t_<租户>`，用 S3 时键前缀后面加 `/<租户>`。租户名只能是小写字母、数字、`-` |
| `OAD_TENANT_SECRET` | 无 | `header:` 方式时可选：还要请求头 `X-OAD-Tenant-Secret` 和它一样 |

日志级别用标准的 `RUST_LOG`（默认 `info`），如 `RUST_LOG=warn`、`RUST_LOG=oad_server=debug`。

## 登录

| 变量 | 参数 | 默认 | 说明 |
| --- | --- | --- | --- |
| `OAD_AUTH` | `--auth` | `local` | `local`：内置账号；`header`：宿主在前面登录、转发时带用户头；`none`：本机单人，不登录；`jwt`：宿主签 JWT（见[嵌入](embedding.md#jwt)） |
| `OAD_ADMIN_EMAIL`、`OAD_ADMIN_PASSWORD` | | 无 | `local` 模式：还没有任何用户时，启动时用它们建管理员（工作区所有者）。已经有用户就不管 |
| `OAD_SIGNUP` | | `invite` | `local` 模式的注册：`invite` 只能凭邀请链接；`open` 谁都能注册；`off` 不能注册 |
| `OAD_USER_PASSWORD` | | 无 | `open-agent-docs user add` 用的密码；没设就从终端读一行 |
| `OAD_HEADER_SECRET` | | 无 | `header` 模式必填：请求头 `X-OAD-Secret` 要和它一样才认 |
| `OAD_JWT_SECRET` | | 无 | `jwt` 模式：HS256 / HS384 / HS512 的共享密钥（和 `OAD_JWT_JWKS_URL` 至少给一个） |
| `OAD_JWT_JWKS_URL` | | 无 | `jwt` 模式：JWKS 地址（RS256 / RS384 / RS512、ES256 / ES384、EdDSA）。按 kid 缓存 10 分钟，遇到不认识的 kid 再取（最多 30 秒一次） |
| `OAD_JWT_ISSUER`、`OAD_JWT_AUDIENCE` | | 无 | `jwt` 模式：设了就检查 `iss` / `aud` |
| `OAD_JWT_LEEWAY` | | `60` | `jwt` 模式：过期时间允许的误差（秒） |

## 通知和抓取外部网页

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `OAD_WEBHOOK_ALLOW_PRIVATE` | 关 | `1` / `true`：webhook 可以投到内网地址。只在本机开发时用；默认只投公网地址（防 SSRF） |
| `OAD_FETCH_PROXY` | 无 | 抓链接预览（标题、图、播放器）时走的代理，如 `http://127.0.0.1:7890` |
| `OAD_FETCH_ALLOW_CIDRS` | 无 | 抓链接预览时额外允许的内网网段，逗号分隔，如 `10.0.0.0/8`。默认不抓内网地址 |
| `OAD_GITHUB_TOKEN` | 无 | GitHub 链接的预览用这个 token 调 GitHub API（额度更高） |

## 只在测试里用的

`OAD_UNFURL_SKIP`（跳过链接预览的某几步）、`OAD_FUZZ`（Markdown 随机测试的轮数）、`OAD_WRITE_OPENAPI=1`（重新生成 `docs/openapi.json`）、
`OAD_TEST_POSTGRES_URL`（设了就把服务器的整套测试跑在 Postgres 上）、`OAD_TEST_S3_URL`（设了就对真的 S3 / MinIO 测文件存储）。部署时不用管。

## 请求来源（Host 和 Origin）

- **Host**：防 DNS rebinding。`none` 模式一直检查；别的模式配了 `OAD_ALLOWED_HOSTS` 或 `OAD_PUBLIC_URL` 才检查。总是允许
  `localhost`、`127.0.0.1`、`[::1]`。用别的域名或 IP 访问 `none` 模式的服务器时，要把它写进 `OAD_ALLOWED_HOSTS`。
- **Origin**：改数据的请求（POST / PUT / PATCH / DELETE）和 WebSocket 握手，带了 Origin 的必须同源、在 `OAD_ALLOWED_ORIGINS` 里，
  或者是 `OAD_PUBLIC_URL`。不带 Origin 的（Agent、curl、服务器之间）照常放行。
- 反向代理后面：设 `OAD_PUBLIC_URL`，代理要传原来的 `Host`（或 `X-Forwarded-Host`）。见[部署](deployment.md)。
