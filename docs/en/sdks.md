# SDKs

[中文](../zh/sdks.md) · [Docs index](README.md)

Official SDKs for Python, Go, Rust and JS/TS. All four follow one contract: the same names, the same behavior, and one shared set of scenarios run
against a real server ([contract](../sdk-contract.md), Chinese).

| Language | Install | Notes |
| --- | --- | --- |
| Python | `pip install open-agent-docs` | Sync and async clients, pydantic models, Python 3.9+. [README](../../sdks/python/README.en.md) |
| Go | `go get github.com/kadaliao/open-agent-docs/sdks/go` | Standard library only, Go 1.23+. [README](../../sdks/go/README.en.md) |
| Rust | `cargo add open-agent-docs` | Async (tokio + reqwest), Rust 1.85+. [README](../../sdks/rust/README.en.md) |
| JS / TS | `npm i @open-agent-docs/client` | Browsers and Node 18+; the only SDK with live collaborative documents and the local database engine. [README](../../packages/client/README.md) |

For the front end: `@open-agent-docs/elements` provides Web Components (any framework), `@open-agent-docs/editor` provides React components,
and `@open-agent-docs/markdown` renders documents to read-only HTML. See [Embedding](embedding.md).

The SDKs share the server's version number and ship with it. The server's `/api/v1` only ever gains things, so a newer SDK works against an older
server; endpoints the older server doesn't have return 404.

## Which one

| You want to | Use |
| --- | --- |
| Let your back end read and write documents as the signed-in user | An SDK plus `with_user` ([acting as a user](#acting-as-a-user)) |
| Build an agent that acts when someone @-mentions it in a comment | MCP is easiest ([Agents](agents.md)); for your own loop, use the SDK's notification stream or a webhook |
| Let people edit inside your web app | `<oad-editor>` (Web Component), `@open-agent-docs/editor` for React, or an iframe |
| Only show documents | `pages.content.get(id, format="html")` and put the HTML in your page, or `<oad-viewer>` |
| Get a server for tests or local development | Each SDK's `start()` |

## Create a client

```python
import open_agent_docs as oad
client = oad.Client(base_url="https://docs.example.com", token="oad_…")
```

```go
client := oad.NewClient(oad.WithBaseURL("https://docs.example.com"), oad.WithToken("oad_…"))
```

```rust
let client = oad::Client::builder().base_url("https://docs.example.com").token("oad_…").build()?;
```

```ts
const client = createClient({ baseUrl: 'https://docs.example.com', token: 'oad_…' })
```

Without arguments, every SDK reads `OAD_BASE_URL` (default `http://127.0.0.1:8848`) and `OAD_TOKEN`. The token can be an agent token (`oad_…`),
a session token, or a JWT signed by your host.

## Create a page and write Markdown

```python
ws = client.default_workspace()
page = client.pages.create(ws, title="Weekly", markdown="# This week\n\n- Shipped the SDKs")
print(client.pages.content.get(page.id).markdown)
```

```go
ws, _ := client.DefaultWorkspace(ctx)
page, _ := client.Pages.Create(ctx, ws, oad.PagesCreateParams{Title: oad.Some("Weekly"), Markdown: "# This week"})
content, _ := client.Pages.Content.Get(ctx, page.ID, oad.PagesContentGetParams{})
```

```rust
let ws = client.default_workspace().await?;
let page = client.pages().create_with_markdown(&ws, oad::types::CreatePage { title: "Weekly".into(), ..Default::default() }, "# This week").await?;
let content = client.pages().content().get(&page.id).await?;
```

```ts
const ws = await client.defaultWorkspace()
const page = await client.createPage(ws, { title: 'Weekly' })
await client.putContent(page.id, { markdown: '# This week' })
```

## Acting as a user

Your host handles sign-in, and your back end calls Open Agent Docs as the current user. Run the server with `OAD_AUTH=header` and
`OAD_HEADER_SECRET`, or with `OAD_AUTH=jwt` and `OAD_JWT_SECRET`, and give the SDK the matching secret:

```python
admin = oad.Client(base_url=URL, jwt_secret=SECRET)          # or header_secret=
u = admin.with_user("u-42", name="Alice", email="alice@example.com")
u.pages.list(u.default_workspace())
```

```go
admin := oad.NewClient(oad.WithBaseURL(url), oad.WithJWTSecret(secret))
u, _ := admin.WithUser("u-42", oad.UserName("Alice"))
```

```rust
let u = admin.with_user(oad::User::new("u-42").name("Alice"))?;
```

```ts
import { createClient } from '@open-agent-docs/client/node'
const u = createClient({ baseUrl, jwtSecret }).withUser('u-42', { name: 'Alice' })
```

`with_user` returns a new client that shares the connection pool; the original client is unchanged. In JWT mode the SDK signs the token itself and
renews it 60 seconds before it expires. The first time a user shows up, the server creates the account and adds it to the default workspace
(a JWT can name the workspaces and roles with `workspaces`).

## Embedding: sign a JWT, build an iframe URL

When the browser talks to Open Agent Docs directly, your back end signs it a short-lived JWT:

```python
token = oad.sign_jwt(SECRET, "u-42", name="Alice", ttl=3600)
src = oad.embed_url("https://docs.example.com", page_id, theme="dark", token=token)
# https://docs.example.com/p/<id>?embed=1&theme=dark#token=…
```

The equivalents are `oad.SignJWT` / `oad.EmbedURL` in Go, `oad::sign_jwt` / `oad::embed_url` in Rust, and `signJwt` (from `/node`) / `embedUrl` in JS.
The token goes after `#`, so it never reaches server logs or the Referer header. See [Embedding](embedding.md) for iframes, Web Components, and
renewing tokens before they expire.

> **Image links in read-only HTML expire.** In the HTML from `format=html`, image and file links carry a signature so they open without signing in.
> The signature lasts 24 hours by default (`OAD_SIGNED_URL_TTL`). So:
> - If you cache the HTML (a knowledge-base front end, for example), don't keep it longer than that; fetch it again before it expires.
> - Anyone holding the HTML can open the files in it until the signature expires. Shorten the lifetime for content that shouldn't leak;
>   use a share link for content that should stay public.
>
> Details: [Embedding · read-only HTML](embedding.md).

## Live events

```python
with client.workspaces.events(ws) as events:
    for ev in events:                       # ev.name, ev.id, ev.data (parsed into a model by event name)
        if ev.name == "page.created": print(ev.data.title)
```

```go
for ev, err := range client.Workspaces.Events(ctx, ws) { ... }
```

```rust
let mut events = client.workspaces().events(&ws).await?;
while let Some(ev) = events.try_next().await? { ... }
```

```ts
client.events(ws).on('page.created', (p) => console.log(p.title))
```

On disconnect, the stream reconnects on its own. It waits 1 second, doubling up to 30 seconds, and resumes with `Last-Event-ID`.
A 401, 403 or 404 stops the stream with an error. A `resync` event means events were missed: fetch your data again.

## Agents: receiving notifications

When someone @-mentions an agent in a comment, the agent gets a notification. Handle it as fetch → process → acknowledge.
Notifications you haven't acknowledged are sent again after a reconnect or a restart.

```python
bot = oad.Client(token=AGENT_TOKEN)
with bot.agent.notifications.stream() as stream:
    for ev in stream:
        note = ev.data                           # AgentNotification; note.prompt can go straight to your agent
        bot.threads.reply(note.thread_id, body=handle(note.prompt))
        bot.agent.notifications.ack(ids=[note.id])
```

If you'd rather not hold a connection open, you have two options. Long-poll with `agent.notifications.list(wait=30)`, or give the agent a webhook URL
and let the server POST notifications to you. Verify each webhook with `verify_webhook(secret, headers, body)`, passing the raw body bytes:

```python
note = oad.verify_webhook(WEBHOOK_SECRET, request.headers, await request.body())
```

In Go, `oad.WebhookHandler(secret, func(ctx, note) error)` gives you a ready-made `http.Handler`.

## Running a server locally

For tests and development, start a server on your machine:

```python
with oad.start(data_dir="./data", auth="header", header_secret="dev") as server:
    client = oad.Client(base_url=server.url, header_secret="dev")
```

- In Python with `open-agent-docs[server]` installed, the server runs inside your process. In Node, `@open-agent-docs/server` does the same.
- Otherwise (including Go, and Rust with the `launcher` feature), the SDK downloads the same-version binary from the release page,
  checks it against `SHA256SUMS`, and runs it as a child process. If `OAD_BINARY` is set, that binary is used instead.
- Supported on macOS and Linux (x86_64, arm64). On other platforms, use Docker.

## Shared across languages

- **Names:** every operation has a dotted name, such as `pages.versions.list` (see [sdks/operations.json](../../sdks/operations.json)).
  That becomes `client.pages.versions.list` in Python, `client.Pages.Versions.List` in Go, and `client.pages().versions().list()` in Rust.
  JS keeps its existing flat method names; the table below maps them.
- **Retries:** up to 2 retries (`max_retries`) on 429, 502, 503, 504 and failed connections, honoring `Retry-After` when present.
  POST and PATCH are not retried on 5xx, since the request may already have taken effect.
- **Errors:** server errors carry `status`, `code` (such as `not_found`) and `message`. Each language lets you check the kind:
  exception classes like `NotFoundError` in Python, `errors.Is(err, oad.ErrNotFound)` in Go, `err.is_not_found()` in Rust, `isNotFound(e)` in JS.
- **Pagination:** cursor-based lists can walk every page for you. In Python, iterate the result directly
  (`for x in client.pages.versions.list(id)`); Go has `ListAll`, Rust has `list_all`, and JS has `each*`.
- **Optional fields:** you can tell "don't send this field" apart from "send `null`". For example, `after: null` moves a page to the top.
  That's `NOT_GIVEN` vs `None` in Python, `Opt[T]` in Go, `Nullable<T>` in Rust, and `undefined` vs `null` in JS.
- **New fields:** when the server adds fields or enum values, older SDKs keep working.

## Operation table

<!-- operations:start -->
<!-- generated by node sdks/gen-docs.mjs from sdks/operations.json; do not edit -->
82 operations. Go methods take `ctx` first; Rust calls need `.await`; paginated lists also have `ListAll` (Go), `list_all` (Rust) and `each*` (JS).

| Name | Python | Go | Rust | JS | HTTP |
| --- | --- | --- | --- | --- | --- |
| `auth.config` | `client.auth.config()` | `client.Auth.Config()` | `client.auth().config()` | `client.authConfig()` | `GET /auth/config` |
| `auth.login` | `client.auth.login()` | `client.Auth.Login()` | `client.auth().login()` | `client.login()` | `POST /auth/login` |
| `auth.logout` | `client.auth.logout()` | `client.Auth.Logout()` | `client.auth().logout()` | `client.logout()` | `POST /auth/logout` |
| `auth.signup` | `client.auth.signup()` | `client.Auth.Signup()` | `client.auth().signup()` | `client.signup()` | `POST /auth/signup` |
| `me.get` | `client.me.get()` | `client.Me.Get()` | `client.me().get()` | `client.me()` | `GET /me` |
| `me.update` | `client.me.update()` | `client.Me.Update()` | `client.me().update()` | `client.patchMe()` | `PATCH /me` |
| `me.change_password` | `client.me.change_password()` | `client.Me.ChangePassword()` | `client.me().change_password()` | `client.changePassword()` | `POST /me/password` |
| `me.notifications.list` (paging) | `client.me.notifications.list()` | `client.Me.Notifications.List()` | `client.me().notifications().list()` | `client.inbox()` | `GET /me/notifications` |
| `me.notifications.mark_read` | `client.me.notifications.mark_read()` | `client.Me.Notifications.MarkRead()` | `client.me().notifications().mark_read()` | `client.markRead()` | `POST /me/notifications/read` |
| `workspaces.update` | `client.workspaces.update()` | `client.Workspaces.Update()` | `client.workspaces().update()` | `client.patchWorkspace()` | `PATCH /workspaces/{ws}` |
| `workspaces.members.list` | `client.workspaces.members.list()` | `client.Workspaces.Members.List()` | `client.workspaces().members().list()` | `client.members()` | `GET /workspaces/{ws}/members` |
| `workspaces.members.set_role` | `client.workspaces.members.set_role()` | `client.Workspaces.Members.SetRole()` | `client.workspaces().members().set_role()` | `client.patchMember()` | `PATCH /workspaces/{ws}/members/{user}` |
| `workspaces.members.remove` | `client.workspaces.members.remove()` | `client.Workspaces.Members.Remove()` | `client.workspaces().members().remove()` | `client.removeMember()` | `DELETE /workspaces/{ws}/members/{user}` |
| `workspaces.events` (stream) | `client.workspaces.events()` | `client.Workspaces.Events()` | `client.workspaces().events()` | `client.events()` | `GET /workspaces/{ws}/events` |
| `search` | `client.search()` | `client.Search()` | `client.search()` | `client.search()` | `GET /workspaces/{ws}/search` |
| `invites.get` | `client.invites.get()` | `client.Invites.Get()` | `client.invites().get()` | `client.inviteInfo()` | `GET /invites/{token}` |
| `invites.accept` | `client.invites.accept()` | `client.Invites.Accept()` | `client.invites().accept()` | `client.acceptInvite()` | `POST /invites/{token}/accept` |
| `invites.list` | `client.invites.list()` | `client.Invites.List()` | `client.invites().list()` | `client.invites()` | `GET /workspaces/{ws}/invites` |
| `invites.create` | `client.invites.create()` | `client.Invites.Create()` | `client.invites().create()` | `client.createInvite()` | `POST /workspaces/{ws}/invites` |
| `invites.revoke` | `client.invites.revoke()` | `client.Invites.Revoke()` | `client.invites().revoke()` | `client.deleteInvite()` | `DELETE /workspaces/{ws}/invites/{token}` |
| `pages.list` | `client.pages.list()` | `client.Pages.List()` | `client.pages().list()` | `client.pages()` | `GET /workspaces/{ws}/pages` |
| `pages.create` | `client.pages.create()` | `client.Pages.Create()` | `client.pages().create()` | `client.createPage()` | `POST /workspaces/{ws}/pages` |
| `pages.get` | `client.pages.get()` | `client.Pages.Get()` | `client.pages().get()` | `client.page()` | `GET /pages/{id}` |
| `pages.update` | `client.pages.update()` | `client.Pages.Update()` | `client.pages().update()` | `client.patchPage()` | `PATCH /pages/{id}` |
| `pages.delete` | `client.pages.delete()` | `client.Pages.Delete()` | `client.pages().delete()` | `client.deletePage()` | `DELETE /pages/{id}` |
| `pages.authors` | `client.pages.authors()` | `client.Pages.Authors()` | `client.pages().authors()` | `client.authors()` | `GET /pages/{id}/authors` |
| `pages.content.get` | `client.pages.content.get()` | `client.Pages.Content.Get()` | `client.pages().content().get()` | `client.content()` | `GET /pages/{id}/content` |
| `pages.content.replace` | `client.pages.content.replace()` | `client.Pages.Content.Replace()` | `client.pages().content().replace()` | `client.putContent()` | `PUT /pages/{id}/content` |
| `pages.blocks.insert` | `client.pages.blocks.insert()` | `client.Pages.Blocks.Insert()` | `client.pages().blocks().insert()` | `client.insertBlocks()` | `POST /pages/{id}/blocks` |
| `pages.blocks.get` | `client.pages.blocks.get()` | `client.Pages.Blocks.Get()` | `client.pages().blocks().get()` | `client.block()` | `GET /pages/{id}/blocks/{block_id}` |
| `pages.blocks.replace` | `client.pages.blocks.replace()` | `client.Pages.Blocks.Replace()` | `client.pages().blocks().replace()` | `client.putBlock()` | `PUT /pages/{id}/blocks/{block_id}` |
| `pages.blocks.delete` | `client.pages.blocks.delete()` | `client.Pages.Blocks.Delete()` | `client.pages().blocks().delete()` | `client.deleteBlock()` | `DELETE /pages/{id}/blocks/{block_id}` |
| `pages.share.get` | `client.pages.share.get()` | `client.Pages.Share.Get()` | `client.pages().share().get()` | `client.getShare()` | `GET /pages/{id}/share` |
| `pages.share.enable` | `client.pages.share.enable()` | `client.Pages.Share.Enable()` | `client.pages().share().enable()` | `client.createShare()` | `POST /pages/{id}/share` |
| `pages.share.disable` | `client.pages.share.disable()` | `client.Pages.Share.Disable()` | `client.pages().share().disable()` | `client.revokeShare()` | `DELETE /pages/{id}/share` |
| `pages.versions.list` (paging) | `client.pages.versions.list()` | `client.Pages.Versions.List()` | `client.pages().versions().list()` | `client.versions()` | `GET /pages/{id}/versions` |
| `pages.versions.create` | `client.pages.versions.create()` | `client.Pages.Versions.Create()` | `client.pages().versions().create()` | `client.saveVersion()` | `POST /pages/{id}/versions` |
| `pages.versions.get` | `client.pages.versions.get()` | `client.Pages.Versions.Get()` | `client.pages().versions().get()` | `client.version()` | `GET /pages/{id}/versions/{v}` |
| `pages.versions.diff` | `client.pages.versions.diff()` | `client.Pages.Versions.Diff()` | `client.pages().versions().diff()` | `client.versionDiff()` | `GET /pages/{id}/versions/{v}/diff` |
| `pages.versions.restore` | `client.pages.versions.restore()` | `client.Pages.Versions.Restore()` | `client.pages().versions().restore()` | `client.restoreVersion()` | `POST /pages/{id}/versions/{v}/restore` |
| `threads.list` | `client.threads.list()` | `client.Threads.List()` | `client.threads().list()` | `client.threads()` | `GET /pages/{id}/comments` |
| `threads.create` | `client.threads.create()` | `client.Threads.Create()` | `client.threads().create()` | `client.createThread()` | `POST /pages/{id}/comments` |
| `threads.update` | `client.threads.update()` | `client.Threads.Update()` | `client.threads().update()` | `client.resolveThread()` | `PATCH /threads/{id}` |
| `threads.reply` | `client.threads.reply()` | `client.Threads.Reply()` | `client.threads().reply()` | `client.reply()` | `POST /threads/{id}/comments` |
| `comments.update` | `client.comments.update()` | `client.Comments.Update()` | `client.comments().update()` | `client.editComment()` | `PATCH /comments/{id}` |
| `comments.delete` | `client.comments.delete()` | `client.Comments.Delete()` | `client.comments().delete()` | `client.deleteComment()` | `DELETE /comments/{id}` |
| `databases.create` | `client.databases.create()` | `client.Databases.Create()` | `client.databases().create()` | `client.createDatabase()` | `POST /workspaces/{ws}/databases` |
| `databases.import_csv` (upload) | `client.databases.import_csv()` | `client.Databases.ImportCSV()` | `client.databases().import_csv()` | `client.importCsv()` | `POST /workspaces/{ws}/databases/import` |
| `databases.get` | `client.databases.get()` | `client.Databases.Get()` | `client.databases().get()` | `client.getDatabase()` | `GET /databases/{id}` |
| `databases.query` | `client.databases.query()` | `client.Databases.Query()` | `client.databases().query()` | `client.queryDatabase()` | `POST /databases/{id}/query` |
| `databases.export_csv` (download) | `client.databases.export_csv()` | `client.Databases.ExportCSV()` | `client.databases().export_csv()` | `client.exportCsv()` | `GET /databases/{id}/export.csv` |
| `databases.press_button` | `client.databases.press_button()` | `client.Databases.PressButton()` | `client.databases().press_button()` | `client.pressButton()` | `POST /databases/{id}/button/{row}/{key}` |
| `databases.rows.list` | `client.databases.rows.list()` | `client.Databases.Rows.List()` | `client.databases().rows().list()` | `client.dbRows()` | `GET /databases/{id}/rows` |
| `databases.rows.add` | `client.databases.rows.add()` | `client.Databases.Rows.Add()` | `client.databases().rows().add()` | `client.addRows()` | `POST /databases/{id}/rows` |
| `databases.rows.update` | `client.databases.rows.update()` | `client.Databases.Rows.Update()` | `client.databases().rows().update()` | `client.patchRows()` | `PATCH /databases/{id}/rows` |
| `databases.rows.delete` | `client.databases.rows.delete()` | `client.Databases.Rows.Delete()` | `client.databases().rows().delete()` | `client.deleteRows()` | `DELETE /databases/{id}/rows` |
| `databases.rows.delete_one` | `client.databases.rows.delete_one()` | `client.Databases.Rows.DeleteOne()` | `client.databases().rows().delete_one()` | `client.deleteRow()` | `DELETE /databases/{id}/rows/{row}` |
| `databases.schema.update` | `client.databases.schema.update()` | `client.Databases.Schema.Update()` | `client.databases().schema().update()` | `client.patchSchema()` | `PATCH /databases/{id}/schema` |
| `databases.schema.undo` | `client.databases.schema.undo()` | `client.Databases.Schema.Undo()` | `client.databases().schema().undo()` | `client.undoSchema()` | `POST /databases/{id}/schema/undo` |
| `databases.views.create` | `client.databases.views.create()` | `client.Databases.Views.Create()` | `client.databases().views().create()` | `client.addView()` | `POST /databases/{id}/views` |
| `databases.views.replace` | `client.databases.views.replace()` | `client.Databases.Views.Replace()` | `client.databases().views().replace()` | `client.putViews()` | `PUT /databases/{id}/views` |
| `databases.views.update` | `client.databases.views.update()` | `client.Databases.Views.Update()` | `client.databases().views().update()` | `client.patchView()` | `PATCH /databases/{id}/views/{view}` |
| `databases.views.delete` | `client.databases.views.delete()` | `client.Databases.Views.Delete()` | `client.databases().views().delete()` | `client.deleteView()` | `DELETE /databases/{id}/views/{view}` |
| `databases.history.list` (paging) | `client.databases.history.list()` | `client.Databases.History.List()` | `client.databases().history().list()` | `client.dbHistory()` | `GET /databases/{id}/history` |
| `databases.history.undo` | `client.databases.history.undo()` | `client.Databases.History.Undo()` | `client.databases().history().undo()` | `client.dbUndo()` | `POST /databases/{id}/history/undo` |
| `files.upload` (upload) | `client.files.upload()` | `client.Files.Upload()` | `client.files().upload()` | `client.upload()` | `POST /workspaces/{ws}/files` |
| `files.download` (download) | `client.files.download()` | `client.Files.Download()` | `client.files().download()` | `client.download()` | `GET /files/{id}` |
| `files.download_named` (download) | `client.files.download_named()` | `client.Files.DownloadNamed()` | `client.files().download_named()` | `client.downloadNamed()` | `GET /files/{id}/{name}` |
| `files.media` (download) | `client.files.media()` | `client.Files.Media()` | `client.files().media()` | `client.media()` | `GET /media/{hash}` |
| `links.unfurl` | `client.links.unfurl()` | `client.Links.Unfurl()` | `client.links().unfurl()` | `client.unfurl()` | `GET /unfurl` |
| `trash.list` (paging) | `client.trash.list()` | `client.Trash.List()` | `client.trash().list()` | `client.trash()` | `GET /workspaces/{ws}/trash` |
| `trash.restore` | `client.trash.restore()` | `client.Trash.Restore()` | `client.trash().restore()` | `client.restoreTrash()` | `POST /trash/{id}/restore` |
| `trash.purge` | `client.trash.purge()` | `client.Trash.Purge()` | `client.trash().purge()` | `client.purgeTrash()` | `DELETE /trash/{id}` |
| `agents.list` | `client.agents.list()` | `client.Agents.List()` | `client.agents().list()` | `client.agents()` | `GET /workspaces/{ws}/agents` |
| `agents.create` | `client.agents.create()` | `client.Agents.Create()` | `client.agents().create()` | `client.createAgent()` | `POST /workspaces/{ws}/agents` |
| `agents.update` | `client.agents.update()` | `client.Agents.Update()` | `client.agents().update()` | `client.patchAgent()` | `PATCH /agents/{id}` |
| `agents.revoke` | `client.agents.revoke()` | `client.Agents.Revoke()` | `client.agents().revoke()` | `client.revokeAgent()` | `DELETE /agents/{id}` |
| `agents.rotate_token` | `client.agents.rotate_token()` | `client.Agents.RotateToken()` | `client.agents().rotate_token()` | `client.reissueToken()` | `POST /agents/{id}/token` |
| `agents.rotate_webhook_secret` | `client.agents.rotate_webhook_secret()` | `client.Agents.RotateWebhookSecret()` | `client.agents().rotate_webhook_secret()` | `client.reissueWebhookSecret()` | `POST /agents/{id}/webhook-secret` |
| `agent.notifications.list` (paging) | `client.agent.notifications.list()` | `client.Agent.Notifications.List()` | `client.agent().notifications().list()` | `client.agentNotifications()` | `GET /agent/notifications` |
| `agent.notifications.ack` | `client.agent.notifications.ack()` | `client.Agent.Notifications.Ack()` | `client.agent().notifications().ack()` | `client.ackAgentNotifications()` | `POST /agent/notifications/ack` |
| `agent.notifications.stream` (stream) | `client.agent.notifications.stream()` | `client.Agent.Notifications.Stream()` | `client.agent().notifications().stream()` | `client.agentEvents()` | `GET /agent/notifications/stream` |
<!-- operations:end -->
