//! [Open Agent Docs](https://github.com/kadaliao/open-agent-docs) 的 Rust 客户端：人和 Agent 一起写的文档。
//!
//! 接口都是 async 的（tokio + reqwest）。建一个 [`Client`]，按资源调：
//! `client.pages().versions().list(id)`。每个接口返回一个请求构造器，可选参数链式设，`.await` 发出去。
//!
//! ```no_run
//! use open_agent_docs as oad;
//! use oad::types::CreatePage;
//! use futures_util::TryStreamExt;
//!
//! # async fn run() -> oad::Result<()> {
//! // 读环境变量 OAD_BASE_URL、OAD_TOKEN（Agent 的 token，oad_…）
//! let client = oad::Client::from_env()?;
//! let ws = client.default_workspace().await?;
//!
//! // 建页面，顺手写进 Markdown 正文
//! let page = client
//!     .pages()
//!     .create_with_markdown(&ws, CreatePage { title: "周报".into(), ..Default::default() }, "# 本周\n\n- 上线了 SDK")
//!     .await?;
//!
//! // 读正文
//! let content = client.pages().content().get(&page.id).await?;
//! println!("{}", content.markdown.unwrap_or_default());
//!
//! // 翻页：list 是一页，list_all 逐条取完
//! let versions: Vec<_> = client.pages().versions().list_all(&page.id).limit(50).try_collect().await?;
//! println!("{} 个版本", versions.len());
//!
//! // 404 之类的错误
//! match client.pages().get("nope").await {
//!     Err(e) if e.is_not_found() => println!("没有这一页"),
//!     other => { other?; }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # 替用户调（宿主后端）
//!
//! 宿主自己做登录，后端用 [`Client::with_user`] 以当前用户的身份调：服务器是 `OAD_AUTH=header` 时配
//! [`header_secret`](ClientBuilder::header_secret)，是 `OAD_AUTH=jwt` 时配 [`jwt_secret`](ClientBuilder::jwt_secret)。
//!
//! ```no_run
//! # use open_agent_docs as oad;
//! # async fn run() -> oad::Result<()> {
//! let admin = oad::Client::builder()
//!     .base_url("https://docs.example.com")
//!     .jwt_secret(std::env::var("OAD_JWT_SECRET").unwrap())
//!     .build()?;
//! let u = admin.with_user(oad::User::new("u-42").name("王小明").email("x@example.com"))?;
//! let pages = u.pages().list(u.default_workspace().await?).await?;
//! # let _ = pages;
//! # Ok(())
//! # }
//! ```
//!
//! # 请求体：不发、发 null、发值
//!
//! 请求体是实现了 `Default` 的结构体，没写的字段不发。可空可省的字段（比如 [`PatchPage`](types::PatchPage) 的 `after`）
//! 是 [`Nullable`]：`Nullable::Omit`（默认，不发）、`Nullable::Null`（发 `null`）、`Nullable::Value(v)`（发值，
//! 也可以写 `v.into()`）。只能省、不能为 null 的字段是 `Option`。
//!
//! ```
//! use open_agent_docs::{Nullable, types::PatchPage};
//! // 挪到最前：after 发 null
//! let p = PatchPage { after: Nullable::Null, ..Default::default() };
//! assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"after":null}"#);
//! // 改标题，别的不动
//! let p = PatchPage { title: "新标题".into(), ..Default::default() };
//! assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"title":"新标题"}"#);
//! ```
//!
//! # 事件、webhook、嵌入
//!
//! - [`workspaces().events(ws)`](resources::WorkspacesApi::events)：工作区的实时事件（SSE），断了自动重连；
//!   [`agent().notifications().stream()`](resources::AgentNotificationsApi::stream)：Agent 的通知。
//! - [`verify_webhook`]：验 Agent webhook 的签名。
//! - [`sign_jwt`]、[`embed_url`]：宿主签 JWT、拼 iframe 的地址。
//! - feature `launcher`：`oad::start(...)` 在本机拉起一个服务器（测试、桌面应用用）。
//!
//! 和其它语言 SDK 共用的契约见仓库里的 `docs/sdk-contract.md`。

mod client;
mod embed;
mod error;
pub mod events;
mod extras;
mod generated;
#[cfg(feature = "launcher")]
mod launcher;
mod nullable;
mod pagination;
mod request;
mod webhook;

pub use client::{Client, ClientBuilder, User};
pub use embed::{EmbedOptions, JwtOptions, JwtWorkspace, embed_url, sign_jwt};
pub use error::{ApiError, Error, Result, WebhookErrorKind};
pub use events::{Event, EventStream};
pub use generated::{resources, types};
#[cfg(feature = "launcher")]
pub use launcher::{Server, StartOptions, start};
pub use nullable::Nullable;
pub use pagination::{ListAll, PageFuture, Paginated};
pub use webhook::{
    DEFAULT_TOLERANCE, WebhookHeaders, verify_webhook, verify_webhook_at, verify_webhook_signature,
};

/// 这个 SDK 的版本（和服务器同一个版本号）
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 重新导出用到的 crate，免得版本对不上：`oad::reqwest::Client`、`oad::bytes::Bytes`。
pub use bytes;
pub use reqwest;
