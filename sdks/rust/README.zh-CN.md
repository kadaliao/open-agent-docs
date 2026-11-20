# open-agent-docs（Rust）

[English](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/rust/README.md) | 简体中文

[Open Agent Docs](https://github.com/kadaliao/open-agent-docs) 的 Rust 客户端：建页面、读写 Markdown 正文、评论、数据库、文件、
实时事件、Agent 的通知和 webhook，还有宿主嵌入要用的 JWT、iframe 地址。全是 async（tokio + reqwest，TLS 用 rustls）。

和 Python、Go、JS 的 SDK 是同一份契约（[docs/sdk-contract.md](https://github.com/kadaliao/open-agent-docs/blob/main/docs/sdk-contract.md)），名字、行为、错误都对得上。

## 安装

```toml
[dependencies]
open-agent-docs = "0.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
futures-util = "0.3"   # 用 list_all、事件流时要 StreamExt / TryStreamExt
```

最低 Rust 1.85。SDK 和服务器同一个版本号；新 SDK 连旧服务器时，旧服务器没有的接口报 404。

## 建客户端

```rust
use open_agent_docs as oad;

let client = oad::Client::builder()
    .base_url("https://docs.example.com")   // /api/v1 由 SDK 拼
    .token("oad_…")                          // Agent 的 token、会话令牌或 JWT
    .build()?;

// 或者读环境变量 OAD_BASE_URL（默认 http://127.0.0.1:8848）、OAD_TOKEN
let client = oad::Client::from_env()?;
```

其它配置：`timeout`（默认 60 秒）、`max_retries`（默认 2）、`locale("zh" | "en")`（服务器错误信息的语言）、
`header(k, v)`（每个请求多带的头）、`http_client(reqwest::Client)`（自己配代理、证书）。`Client` 克隆很便宜，可以到处传。

## 调接口

按资源调，名字和 [sdks/operations.json](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/operations.json) 一样：`pages.versions.list` → `client.pages().versions().list(id)`。
每个接口返回一个请求构造器：位置参数是路径参数（和必填的查询参数）、请求体；可选的查询参数链式设；`.await` 发出去。

```rust
use oad::types::{CreatePage, PatchPage, PutContent};

let ws = client.default_workspace().await?;          // /me 里第一个工作区

// 建页面，顺手写正文
let page = client
    .pages()
    .create_with_markdown(&ws, CreatePage { title: "周报".into(), ..Default::default() }, "# 本周\n\n- 上线了 SDK")
    .await?;

// 读正文（带 version，改的时候当 base）
let content = client.pages().content().get(&page.id).await?;
client
    .pages()
    .content()
    .replace(&page.id, PutContent { markdown: "# 改过了".into(), base: content.version.into(), ..Default::default() })
    .await?;

let pages = client.pages().list(&ws).await?;           // 只包一层的响应直接给数组
let hits = client.search(&ws, "周报").limit(5).await?;
let share = client.pages().share().get(&page.id).await?; // 没分享时是 None
let file = client.files().upload(&ws, "a.txt", b"hello".to_vec()).await?;
let bytes = client.files().download(&file.id).await?;    // bytes::Bytes
```

模型在 `oad::types` 里（`oad::types::CreatePage`）。

### 请求体：不发、发 null、发值

请求体是实现了 `Default` 的结构体，没写的字段不发。可空可省的字段是 `oad::Nullable<T>`，三种状态：

| 写法 | 发出去的 |
| --- | --- |
| `Nullable::Omit`（默认，`..Default::default()`） | 不发这个字段（不改） |
| `Nullable::Null` | `null`（比如 `PatchPage.after: null` = 挪到最前，`icon: null` = 清掉图标） |
| `Nullable::Value(v)` 或 `v.into()` | 值 |

```rust
use oad::Nullable;
client.pages().update(&page.id, PatchPage { after: Nullable::Null, ..Default::default() }).await?;   // 挪到最前
client.pages().update(&page.id, PatchPage { title: "新标题".into(), ..Default::default() }).await?; // 只改标题
```

只能省、不能为 `null` 的字段是普通的 `Option<T>`（`None` 不发）。响应模型里可空的字段也都是 `Option<T>`。

### 翻页

带 `next_cursor` 的列表（版本、回收站、库的历史、通知）：`list(...)` 是一页，`list_all(...)` 是一个
`Stream<Item = Result<T>>`，自动带 `cursor` 取完所有页（停在没有下一页、空页、游标没变）。

```rust
use futures_util::TryStreamExt;

let first = client.pages().versions().list(&page.id).limit(50).await?;   // VersionList { versions, next_cursor }
let all: Vec<_> = client.pages().versions().list_all(&page.id).limit(50).try_collect().await?;

let mut s = client.trash().list_all(&ws);
while let Some(item) = s.try_next().await? {
    println!("{}", item.title);
}
```

### 错误

```rust
match client.pages().get("nope").await {
    Ok(p) => println!("{}", p.title),
    Err(e) if e.is_not_found() => println!("没有这一页"),
    Err(oad::Error::Api(e)) => println!("{} {}：{}", e.status, e.code, e.message),
    Err(e) => return Err(e),
}
```

- `Error::Api(ApiError { status, code, message, request_id, retry_after })`：服务器回的错误。`code` 取响应的
  `{ "error": { "code" } }`，不是 JSON 时是 `http_<状态码>`。判断用 `is_bad_request()`、`is_unauthorized()`、`is_forbidden()`、
  `is_not_found()`、`is_conflict()`、`is_gone()`、`is_too_large()`、`is_rate_limited()`、`is_bad_gateway()`、`is_server_error()`。
- `Error::Connection`（连不上）、`Error::Timeout`、`Error::Decode`（响应看不懂）、`Error::Config`（配置不对）、
  `Error::Webhook(kind)`（验签失败）、`Error::Launch`（本地启动失败）。

### 重试

最多 `max_retries` 次（默认 2）：429；502、503、504；连不上。有 `Retry-After` 就照它等（最多 60 秒），没有就
0.5 秒 × 2ⁿ（最多 8 秒）加一点随机。POST、PATCH 遇到 5xx 不重试（可能已经生效了）。

## 替用户调（宿主后端）

宿主自己做登录，后端以当前用户的身份调。服务器是 `OAD_AUTH=header` 就配 `header_secret`，是 `OAD_AUTH=jwt` 就配 `jwt_secret`：

```rust
let admin = oad::Client::builder()
    .base_url("https://docs.example.com")
    .jwt_secret(std::env::var("OAD_JWT_SECRET").expect("OAD_JWT_SECRET"))   // 或 .header_secret(...)
    .build()?;

let u = admin.with_user(oad::User::new("u-42").name("王小明").email("x@example.com"))?;
let pages = u.pages().list(u.default_workspace().await?).await?;
```

`with_user` 返回新的客户端（共享连接池），原来的不变。header 模式带 `X-OAD-Secret`、`X-OAD-User-Id/Name/Email`（百分号编码）；
JWT 模式签 HS256（`jwt_issuer`、`jwt_audience`、`jwt_ttl` 可配），缓存到过期前 60 秒。JWT 模式能给
`.workspaces(["ws1"])` 或 `[oad::JwtWorkspace::with_role("ws1", "viewer")]`。两个密钥都配了或都没配，`with_user` 返回 `Error::Config`。

## 实时事件

```rust
use futures_util::TryStreamExt;
use oad::types::WorkspaceEvent;

let mut events = client.workspaces().events(&ws).await?;   // 连上了才返回
while let Some(ev) = events.try_next().await? {
    match ev.data {
        WorkspaceEvent::PageCreated(p) => println!("新页面：{}", p.title),
        WorkspaceEvent::DbChanged(d) => println!("库 {:?} 变了", d.db_id),
        WorkspaceEvent::Resync(_) => println!("落下了事件，重新拉一遍"),
        other => println!("{}：{other:?}", ev.name),          // 不认识的事件是 Other(原始 JSON)
    }
}
```

断了自动重连（1 秒起翻倍、最多 30 秒，连上后重置），带 `Last-Event-ID`。401、403、404 不重连：流里给出错误然后结束。
drop 掉流就断开。

### Agent：取 → 处理 → 确认

```rust
use futures_util::TryStreamExt;
use oad::types::Ack;

let bot = oad::Client::builder().base_url(url).token(agent_token).build()?;
// 先补发没确认的，再实时推
let mut notes = bot.agent().notifications().stream().await?;
while let Some(ev) = notes.try_next().await? {
    let n = ev.data;                                   // AgentNotification
    println!("{} 在《{}》里说：{}", n.comment.body, n.page.title, n.prompt);
    // ……让 Agent 干活，回评论：bot.threads().reply(&n.thread_id, …)
    bot.agent().notifications().ack(Ack { upto: n.id.into(), ..Default::default() }).await?;
}
```

不确认的下次连上会再给。不想常连就用 `bot.agent().notifications().list().wait(30)`（长轮询）。

## 验 webhook

```rust
// axum：headers 是 HeaderMap，body 是收到的原始字节
let note = oad::verify_webhook(&secret, &headers, &body)?;   // AgentNotification；默认允许前后 5 分钟
```

`headers` 可以是 `http::HeaderMap`、`HashMap<String, String>`、`[(k, v)]`，名字大小写不论。失败是
`Error::Webhook(WebhookErrorKind::Signature | Stale | Malformed)`。测试里用 `verify_webhook_at(.., now, tolerance)`；
只验签不解析用 `verify_webhook_signature`。完整的例子见 [examples/webhook.rs](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/rust/open-agent-docs/examples/webhook.rs)。

## 嵌入

```rust
let token = oad::sign_jwt(&secret, "u-42", &oad::JwtOptions { name: Some("王小明".into()), ..Default::default() });
let src = oad::embed_url("https://docs.example.com", &page_id, &oad::EmbedOptions { token: Some(token), ..Default::default() });
// → https://docs.example.com/p/<id>?embed=1#token=<jwt>，放进 <iframe src>
```

`sign_jwt` 只做 HS256，claims 是 `sub`、`name`、`email`、`workspaces`、`iat`、`exp`、`iss`、`aud`；要 RS256 / ES256 用自己的 JWT 库签同样的 claims。
宿主的完整例子（axum 签 JWT、iframe、快过期时换 token）见 [examples/embed_host.rs](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/rust/open-agent-docs/examples/embed_host.rs)。

## 本地启动（feature `launcher`）

```toml
open-agent-docs = { version = "0.3", features = ["launcher"] }
```

```rust
let server = oad::start(oad::StartOptions::new().data_dir("./data").auth("header").header_secret("dev")).await?;
let admin = server.client_builder().header_secret("dev").build()?;
// ……
server.shutdown().await?;   // SIGTERM，等页面存好；drop 也会尽力关
```

`set("rate_limit", "0")` 这样的任意一项转成环境变量 `OAD_RATE_LIMIT`。二进制按顺序找：`.binary(path)` → `OAD_BINARY` →
缓存（`<用户缓存目录>/open-agent-docs/v<版本>/`）→ 从 GitHub 发布页下载并用 `SHA256SUMS` 校验。支持 macOS 11+、Linux（x86_64、aarch64，glibc 2.34 起：Ubuntu 22.04+、Debian 12+、RHEL 9+），
更老的系统和别的平台用 Docker，见[系统要求](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/quickstart.md#系统要求)。

## 开发

```sh
cd sdks/rust
cargo run -p codegen            # 从 docs/openapi.json + sdks/operations.json 重新生成 open-agent-docs/src/generated/
cargo test                      # 单元测试；codegen 的测试检查生成结果是最新的
OAD_BINARY=../../target/debug/open-agent-docs cargo test --all-features --test conformance   # 统一场景 S1–S15
cargo clippy --all-features --all-targets -- -D warnings
```

`src/generated/` 是生成的，不要手改；其它都是手写的运行时（重试、翻页、事件流、验签、JWT、本地启动）。
