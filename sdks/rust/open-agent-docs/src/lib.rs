//! Rust client for [Open Agent Docs](https://github.com/kadaliao/open-agent-docs): docs that people and AI agents write together.
//!
//! Every call is async (tokio + reqwest). Build a [`Client`] and call by resource:
//! `client.pages().versions().list(id)`. Each call returns a request builder: set optional parameters by chaining, then `.await` to send it.
//!
//! ```no_run
//! use open_agent_docs as oad;
//! use oad::types::CreatePage;
//! use futures_util::TryStreamExt;
//!
//! # async fn run() -> oad::Result<()> {
//! // reads OAD_BASE_URL and OAD_TOKEN (an agent token, oad_…) from the environment
//! let client = oad::Client::from_env()?;
//! let ws = client.default_workspace().await?;
//!
//! // create a page and write its Markdown body in one go
//! let page = client
//!     .pages()
//!     .create_with_markdown(&ws, CreatePage { title: "Weekly".into(), ..Default::default() }, "# This week\n\n- Shipped the SDK")
//!     .await?;
//!
//! // read the body
//! let content = client.pages().content().get(&page.id).await?;
//! println!("{}", content.markdown.unwrap_or_default());
//!
//! // pagination: list returns one page, list_all walks every item
//! let versions: Vec<_> = client.pages().versions().list_all(&page.id).limit(50).try_collect().await?;
//! println!("{} versions", versions.len());
//!
//! // errors such as 404
//! match client.pages().get("nope").await {
//!     Err(e) if e.is_not_found() => println!("no such page"),
//!     other => { other?; }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Calling on behalf of a user (host backends)
//!
//! The host does its own login; its backend calls as the current user with [`Client::with_user`]. Set
//! [`header_secret`](ClientBuilder::header_secret) when the server runs with `OAD_AUTH=header`, or
//! [`jwt_secret`](ClientBuilder::jwt_secret) with `OAD_AUTH=jwt`.
//!
//! ```no_run
//! # use open_agent_docs as oad;
//! # async fn run() -> oad::Result<()> {
//! let admin = oad::Client::builder()
//!     .base_url("https://docs.example.com")
//!     .jwt_secret(std::env::var("OAD_JWT_SECRET").unwrap())
//!     .build()?;
//! let u = admin.with_user(oad::User::new("u-42").name("Ming Wang").email("x@example.com"))?;
//! let pages = u.pages().list(u.default_workspace().await?).await?;
//! # let _ = pages;
//! # Ok(())
//! # }
//! ```
//!
//! # Request bodies: omit, send null, send a value
//!
//! A request body is a struct that implements `Default`; fields you don't set aren't sent. Fields that can be omitted or null (such as `after` on
//! [`PatchPage`](types::PatchPage)) are [`Nullable`]: `Nullable::Omit` (the default, not sent), `Nullable::Null` (sends `null`) and
//! `Nullable::Value(v)` (sends the value; `v.into()` works too). Fields that can be omitted but never null are `Option`.
//!
//! ```
//! use open_agent_docs::{Nullable, types::PatchPage};
//! // move to the top: send null for after
//! let p = PatchPage { after: Nullable::Null, ..Default::default() };
//! assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"after":null}"#);
//! // change the title and nothing else
//! let p = PatchPage { title: "New title".into(), ..Default::default() };
//! assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"title":"New title"}"#);
//! ```
//!
//! # Events, webhooks, embedding
//!
//! - [`workspaces().events(ws)`](resources::WorkspacesApi::events): a workspace's real-time events (SSE), reconnecting automatically;
//!   [`agent().notifications().stream()`](resources::AgentNotificationsApi::stream): an agent's notifications.
//! - [`verify_webhook`]: verify the signature of an agent webhook.
//! - [`sign_jwt`], [`embed_url`]: the host signs a JWT and builds the iframe URL.
//! - feature `launcher`: `oad::start(...)` starts a server on this machine (for tests and desktop apps).
//!
//! The contract shared with the SDKs in the other languages is `docs/sdk-contract.md` in the repository.

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

/// The version of this SDK (the same version number as the server).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Re-exports of the crates used in the API, so versions can't drift apart: `oad::reqwest::Client`, `oad::bytes::Bytes`.
pub use bytes;
pub use reqwest;
