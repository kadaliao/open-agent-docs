# open-agent-docs (Rust)

[中文](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/rust/README.md)

Rust client for [Open Agent Docs](https://github.com/kadaliao/open-agent-docs), docs that people and AI agents write together.
It covers pages, Markdown content, comments, databases, files, real-time events, agent notifications and webhooks, and the
JWT / iframe helpers a host app needs for embedding. Everything is async (tokio + reqwest, TLS via rustls).

The Python, Go and JS SDKs follow the same contract ([docs/sdk-contract.md](https://github.com/kadaliao/open-agent-docs/blob/main/docs/sdk-contract.md)):
same names, same behavior, same errors.

## Install

```toml
[dependencies]
open-agent-docs = "0.2"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
futures-util = "0.3"   # StreamExt / TryStreamExt for list_all and event streams
```

Rust 1.85 or newer. The SDK and the server share one version number; a newer SDK talking to an older server gets a 404 for
endpoints the server doesn't have yet.

## Create a client

```rust
use open_agent_docs as oad;

let client = oad::Client::builder()
    .base_url("https://docs.example.com")   // the SDK appends /api/v1
    .token("oad_…")                          // agent token, session token or JWT
    .build()?;

// or read OAD_BASE_URL (default http://127.0.0.1:8848) and OAD_TOKEN
let client = oad::Client::from_env()?;
```

Other options: `timeout` (60 s), `max_retries` (2), `locale("zh" | "en")` (language of server error messages), `header(k, v)`
(extra header on every request), `http_client(reqwest::Client)` (your own proxy / certificates). `Client` is cheap to clone.

## Calling endpoints

Endpoints are grouped by resource, named after [sdks/operations.json](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/operations.json):
`pages.versions.list` → `client.pages().versions().list(id)`. Each call returns a request builder: positional arguments are path
parameters (and required query parameters) and the request body; optional query parameters are chained; `.await` sends it.

```rust
use oad::types::{CreatePage, PatchPage, PutContent};

let ws = client.default_workspace().await?;          // first workspace in /me

// create a page and write its Markdown body
let page = client
    .pages()
    .create_with_markdown(&ws, CreatePage { title: "Weekly".into(), ..Default::default() }, "# This week\n\n- shipped the SDK")
    .await?;

// read the body (with its version, used as `base` when writing)
let content = client.pages().content().get(&page.id).await?;
client
    .pages()
    .content()
    .replace(&page.id, PutContent { markdown: "# Edited".into(), base: content.version.into(), ..Default::default() })
    .await?;

let pages = client.pages().list(&ws).await?;            // single-field responses are unwrapped
let hits = client.search(&ws, "weekly").limit(5).await?;
let share = client.pages().share().get(&page.id).await?; // None when not shared
let file = client.files().upload(&ws, "a.txt", b"hello".to_vec()).await?;
let bytes = client.files().download(&file.id).await?;    // bytes::Bytes
```

Models live in `oad::types` (`oad::types::CreatePage`).

### Request bodies: omit, null, or a value

Request bodies are `Default` structs; fields you don't set are not sent. Fields that are both optional and nullable are
`oad::Nullable<T>`:

| Rust | On the wire |
| --- | --- |
| `Nullable::Omit` (default, `..Default::default()`) | field left out (unchanged) |
| `Nullable::Null` | `null` (e.g. `PatchPage.after: null` moves the page to the top, `icon: null` clears the icon) |
| `Nullable::Value(v)` or `v.into()` | the value |

```rust
use oad::Nullable;
client.pages().update(&page.id, PatchPage { after: Nullable::Null, ..Default::default() }).await?;     // move to top
client.pages().update(&page.id, PatchPage { title: "New title".into(), ..Default::default() }).await?; // title only
```

Fields that may be omitted but never `null` are plain `Option<T>` (`None` is not sent). Nullable fields in response models are `Option<T>`.

### Pagination

Lists with `next_cursor` (versions, trash, database history, notifications): `list(...)` is one page; `list_all(...)` is a
`Stream<Item = Result<T>>` that follows `cursor` through every page (it stops when there's no next cursor, the page is empty,
or the cursor didn't move).

```rust
use futures_util::TryStreamExt;

let first = client.pages().versions().list(&page.id).limit(50).await?;   // VersionList { versions, next_cursor }
let all: Vec<_> = client.pages().versions().list_all(&page.id).limit(50).try_collect().await?;
```

### Errors

```rust
match client.pages().get("nope").await {
    Ok(p) => println!("{}", p.title),
    Err(e) if e.is_not_found() => println!("no such page"),
    Err(oad::Error::Api(e)) => println!("{} {}: {}", e.status, e.code, e.message),
    Err(e) => return Err(e),
}
```

- `Error::Api(ApiError { status, code, message, request_id, retry_after })`: the server returned an error. `code` comes from
  `{ "error": { "code" } }`, or `http_<status>` when the body isn't JSON. Helpers: `is_bad_request()`, `is_unauthorized()`,
  `is_forbidden()`, `is_not_found()`, `is_conflict()`, `is_gone()`, `is_too_large()`, `is_rate_limited()`, `is_bad_gateway()`, `is_server_error()`.
- `Error::Connection`, `Error::Timeout`, `Error::Decode` (unexpected response), `Error::Config`, `Error::Webhook(kind)`, `Error::Launch`.

### Retries

Up to `max_retries` (2) on 429, 502/503/504 and connection failures. `Retry-After` is honored (capped at 60 s); otherwise
0.5 s × 2ⁿ (capped at 8 s) plus jitter. POST and PATCH are not retried on 5xx (they may have taken effect).

## Acting as a user (host backends)

Your app does the login; your backend calls Open Agent Docs as the current user. Configure `header_secret` for an
`OAD_AUTH=header` server or `jwt_secret` for `OAD_AUTH=jwt`:

```rust
let admin = oad::Client::builder()
    .base_url("https://docs.example.com")
    .jwt_secret(std::env::var("OAD_JWT_SECRET").expect("OAD_JWT_SECRET"))   // or .header_secret(...)
    .build()?;

let u = admin.with_user(oad::User::new("u-42").name("Alice").email("alice@example.com"))?;
let pages = u.pages().list(u.default_workspace().await?).await?;
```

`with_user` returns a new client sharing the connection pool. Header mode sends `X-OAD-Secret` and `X-OAD-User-Id/Name/Email`
(percent-encoded); JWT mode signs an HS256 token (`jwt_issuer`, `jwt_audience`, `jwt_ttl`) and caches it until 60 s before
it expires. In JWT mode you can pass `.workspaces(["ws1"])` or `[oad::JwtWorkspace::with_role("ws1", "viewer")]`.
Configuring both secrets, or neither, makes `with_user` return `Error::Config`.

## Real-time events

```rust
use futures_util::TryStreamExt;
use oad::types::WorkspaceEvent;

let mut events = client.workspaces().events(&ws).await?;   // returns once connected
while let Some(ev) = events.try_next().await? {
    match ev.data {
        WorkspaceEvent::PageCreated(p) => println!("new page: {}", p.title),
        WorkspaceEvent::Resync(_) => println!("missed events, refetch"),
        other => println!("{}: {other:?}", ev.name),          // unknown events are Other(raw JSON)
    }
}
```

The stream reconnects on its own (1 s doubling up to 30 s, reset after a successful connect) and sends `Last-Event-ID`.
401, 403 and 404 are not retried: the stream yields the error and ends. Drop the stream to disconnect.

### Agents: receive → handle → acknowledge

```rust
use futures_util::TryStreamExt;
use oad::types::Ack;

let bot = oad::Client::builder().base_url(url).token(agent_token).build()?;
let mut notes = bot.agent().notifications().stream().await?;   // replays unacknowledged ones first
while let Some(ev) = notes.try_next().await? {
    let n = ev.data;                                           // AgentNotification
    println!("{} on \"{}\": {}", n.comment.body, n.page.title, n.prompt);
    // ...do the work, reply with bot.threads().reply(&n.thread_id, ...)
    bot.agent().notifications().ack(Ack { upto: n.id.into(), ..Default::default() }).await?;
}
```

Unacknowledged notifications are delivered again on the next connect. For long polling use `bot.agent().notifications().list().wait(30)`.

## Verifying webhooks

```rust
// axum: headers is a HeaderMap, body the raw bytes you received
let note = oad::verify_webhook(&secret, &headers, &body)?;   // AgentNotification; ±5 minutes by default
```

`headers` can be an `http::HeaderMap`, a `HashMap<String, String>` or `[(k, v)]`; names are case-insensitive. Failures are
`Error::Webhook(WebhookErrorKind::Signature | Stale | Malformed)`. Use `verify_webhook_at(.., now, tolerance)` in tests and
`verify_webhook_signature` to check the signature without parsing. See `examples/webhook.rs`.

## Embedding

```rust
let token = oad::sign_jwt(&secret, "u-42", &oad::JwtOptions { name: Some("Alice".into()), ..Default::default() });
let src = oad::embed_url("https://docs.example.com", &page_id, &oad::EmbedOptions { token: Some(token), ..Default::default() });
// → https://docs.example.com/p/<id>?embed=1#token=<jwt>, put it in <iframe src>
```

`sign_jwt` does HS256 only, with claims `sub`, `name`, `email`, `workspaces`, `iat`, `exp`, `iss`, `aud`; for RS256 / ES256 sign
the same claims with your own JWT library. `examples/embed_host.rs` is a complete axum host (signs the JWT, renders the iframe,
hands out a fresh token when the old one is about to expire).

## Running a server locally (feature `launcher`)

```toml
open-agent-docs = { version = "0.2", features = ["launcher"] }
```

```rust
let server = oad::start(oad::StartOptions::new().data_dir("./data").auth("header").header_secret("dev")).await?;
let admin = server.client_builder().header_secret("dev").build()?;
// ...
server.shutdown().await?;   // SIGTERM, waits for pages to be saved; dropping also tries to stop it
```

Any `set("rate_limit", "0")` becomes the environment variable `OAD_RATE_LIMIT`. The binary is looked up in order:
`.binary(path)` → `OAD_BINARY` → cache (`<user cache dir>/open-agent-docs/v<version>/`) → downloaded from the GitHub release
and checked against `SHA256SUMS`. Prebuilt for macOS and Linux (x86_64, aarch64); use Docker elsewhere.

## Development

```sh
cd sdks/rust
cargo run -p codegen            # regenerate open-agent-docs/src/generated/ from docs/openapi.json + sdks/operations.json
cargo test                      # unit tests; the codegen test checks the generated code is up to date
OAD_BINARY=../../target/debug/open-agent-docs cargo test --all-features --test conformance   # shared scenarios S1–S15
cargo clippy --all-features --all-targets -- -D warnings
```

`src/generated/` is generated, don't edit it by hand; everything else is the hand-written runtime (retries, pagination,
event streams, webhook verification, JWT, local launcher).

## License

MIT
