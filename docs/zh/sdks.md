# SDK

[English](../en/sdks.md) · [文档目录](README.md)

Python、Go、Rust、JS/TS 四种语言的官方 SDK。四个 SDK 遵守同一份契约：名字一一对应，行为一致，
用同一套场景测试对着真服务器验证（[契约](../sdk-contract.md)）。

| 语言 | 安装 | 说明 |
| --- | --- | --- |
| Python | `pip install open-agent-docs` | 同步、异步各一套客户端，pydantic 模型，Python 3.9 起。[README](../../sdks/python/README.md) |
| Go | `go get github.com/kadaliao/open-agent-docs/sdks/go` | 只用标准库，Go 1.23 起。[README](../../sdks/go/README.md) |
| Rust | `cargo add open-agent-docs` | async（tokio + reqwest），Rust 1.85 起。[README](../../sdks/rust/README.md) |
| JS / TS | `npm i @open-agent-docs/client` | 浏览器和 Node 18+；协作文档、本地数据库只有它有。[README](../../packages/client/README.md) |

前端的包：`@open-agent-docs/elements` 是 Web Component，任何前端框架都能用；`@open-agent-docs/editor` 是 React 组件；
`@open-agent-docs/markdown` 把文档转成只读 HTML。见[嵌入到自己的应用](embedding.md)。

SDK 和服务器版本号相同，一起发布。服务器的 `/api/v1` 只加不删，所以新 SDK 也能连旧服务器，只是旧服务器没有的接口会报 404。

## 用哪个

| 你要做的 | 用什么 |
| --- | --- |
| 自己的后端替登录的用户读写文档 | SDK 加 `with_user`（下面的[替用户调](#替用户调)） |
| 写一个 Agent：有人在评论里 @ 它就干活 | MCP 最省事（见 [Agent](agents.md)）；要自己写循环就用 SDK 的通知流或 webhook |
| 在自己的网页里给人编辑 | `<oad-editor>`（Web Component）、React 的 `@open-agent-docs/editor`，或者 iframe |
| 只给人看 | `pages.content.get(id, format="html")` 拿 HTML 自己放进页面，或者 `<oad-viewer>` |
| 测试、本机开发要一个服务器 | 各 SDK 的 `start()` |

## 建客户端

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

不传参数时，各 SDK 读环境变量 `OAD_BASE_URL`（默认 `http://127.0.0.1:8848`）和 `OAD_TOKEN`。token 可以是 Agent 的 token（`oad_…`）、
登录后的会话令牌，或者宿主签的 JWT。

## 建页面、写 Markdown

```python
ws = client.default_workspace()
page = client.pages.create(ws, title="周报", markdown="# 本周\n\n- 上线了 SDK")
print(client.pages.content.get(page.id).markdown)
```

```go
ws, _ := client.DefaultWorkspace(ctx)
page, _ := client.Pages.Create(ctx, ws, oad.PagesCreateParams{Title: oad.Some("周报"), Markdown: "# 本周\n\n- 上线了 SDK"})
content, _ := client.Pages.Content.Get(ctx, page.ID, oad.PagesContentGetParams{})
```

```rust
let ws = client.default_workspace().await?;
let page = client.pages().create_with_markdown(&ws, oad::types::CreatePage { title: "周报".into(), ..Default::default() }, "# 本周").await?;
let content = client.pages().content().get(&page.id).await?;
```

```ts
const ws = await client.defaultWorkspace()
const page = await client.createPage(ws, { title: '周报' })
await client.putContent(page.id, { markdown: '# 本周\n\n- 上线了 SDK' })
```

## 替用户调

宿主自己做登录，后端用当前用户的身份调 Open Agent Docs。服务器配 `OAD_AUTH=header` 和 `OAD_HEADER_SECRET`，
或者 `OAD_AUTH=jwt` 和 `OAD_JWT_SECRET`，SDK 里给对应的密钥：

```python
admin = oad.Client(base_url=URL, jwt_secret=SECRET)          # 或 header_secret=
u = admin.with_user("u-42", name="王小明", email="x@example.com")
u.pages.list(u.default_workspace())
```

```go
admin := oad.NewClient(oad.WithBaseURL(url), oad.WithJWTSecret(secret))
u, _ := admin.WithUser("u-42", oad.UserName("王小明"))
```

```rust
let u = admin.with_user(oad::User::new("u-42").name("王小明"))?;
```

```ts
import { createClient } from '@open-agent-docs/client/node'
const u = createClient({ baseUrl, jwtSecret }).withUser('u-42', { name: '王小明' })
```

`with_user` 返回一个新的客户端，和原来的共享连接池，原来的客户端不受影响。JWT 模式下 SDK 自己签 token，并在过期前 60 秒换新的。
用户第一次出现时，服务器自动建账号，并把他加进默认工作区（JWT 可以用 `workspaces` 指定工作区和角色）。

## 嵌入：签 JWT、iframe 地址

浏览器直接连 Open Agent Docs 时，后端给它签一个短期 JWT：

```python
token = oad.sign_jwt(SECRET, "u-42", name="王小明", ttl=3600)
src = oad.embed_url("https://docs.example.com", page_id, theme="dark", token=token)
# https://docs.example.com/p/<id>?embed=1&theme=dark#token=…
```

Go 用 `oad.SignJWT`、`oad.EmbedURL`，Rust 用 `oad::sign_jwt`、`oad::embed_url`，JS 用 `signJwt`（`/node` 入口）、`embedUrl`。
token 放在 `#` 后面，不会进服务器日志和 Referer。iframe、Web Component、token 快过期时怎么续，见[嵌入](embedding.md)。

> **只读 HTML 里的图片地址有有效期。** `format=html` 返回的 HTML 里，图片和文件的地址带签名，不登录也能打开，
> 有效期默认 24 小时（`OAD_SIGNED_URL_TTL`）。所以：
> - 把 HTML 缓存起来的（比如知识库前台），缓存时间别超过有效期，到期前重新取一次；
> - 拿到这段 HTML 的人，在有效期内都能打开里面的文件。内容不该外传的，把有效期调短；要长期公开的，用分享链接。
>
> 细节见[嵌入 · 只读 HTML](embedding.md)。

## 实时事件

```python
with client.workspaces.events(ws) as events:
    for ev in events:                       # ev.name、ev.id、ev.data（按事件名解析成模型）
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

断线后自动重连，等待时间从 1 秒起翻倍、最多 30 秒，带上 `Last-Event-ID` 接着收；收到 401、403、404 就停止并报错。
收到 `resync` 表示漏掉了事件，要重新拉一遍数据。

## Agent：收通知

有人在评论里 @ Agent 时，Agent 会收到通知。处理流程是“取 → 处理 → 确认”：没确认的通知，断线重连或重启后会再推一遍。

```python
bot = oad.Client(token=AGENT_TOKEN)
with bot.agent.notifications.stream() as stream:
    for ev in stream:
        note = ev.data                           # AgentNotification；note.prompt 能直接当消息发给你的 Agent
        bot.threads.reply(note.thread_id, body=handle(note.prompt))
        bot.agent.notifications.ack(ids=[note.id])
```

不想维持长连接，有两个选择：用 `agent.notifications.list(wait=30)` 长轮询；或者给 Agent 配一个 webhook 地址，服务器把通知 POST 过来。
收到后用 `verify_webhook(secret, headers, body)` 验签，`body` 必须是原始字节：

```python
note = oad.verify_webhook(WEBHOOK_SECRET, request.headers, await request.body())
```

Go 另有 `oad.WebhookHandler(secret, func(ctx, note) error)`，可以直接当 `http.Handler` 挂上。

## 本地起服务器

测试、开发时在本机起一个服务器：

```python
with oad.start(data_dir="./data", auth="header", header_secret="dev") as server:
    client = oad.Client(base_url=server.url, header_secret="dev")
```

- Python 装了 `open-agent-docs[server]` 就在进程内跑；Node 用 `@open-agent-docs/server`，同样在进程内跑。
- 其他情况（包括 Go、Rust 的 `launcher` feature）会去发布页下载同版本的二进制，用 `SHA256SUMS` 校验，然后作为子进程启动。
  设了 `OAD_BINARY` 就直接用它。
- 支持 macOS 11+ 和 Linux（x86_64、arm64，**glibc 2.34 起**，即 Ubuntu 22.04+、Debian 12+、RHEL 9+），更老的系统和 Windows 请用 Docker，见[系统要求](quickstart.md#系统要求)。
  SDK 本身（只连服务器）没有这个限制。

## 各语言都一样的地方

- **名字**：每个接口有一个点路径名，比如 `pages.versions.list`，见 [sdks/operations.json](../../sdks/operations.json)。
  Python 写成 `client.pages.versions.list`，Go 写成 `client.Pages.Versions.List`，Rust 写成 `client.pages().versions().list()`；
  JS 保持原来的平铺方法名，对应关系在下面的表里。
- **重试**：429、502、503、504 和连不上时最多重试 2 次（`max_retries`）。有 `Retry-After` 就按它等。POST、PATCH 遇到 5xx 不重试，因为请求可能已经生效。
- **错误**：服务器返回的错误带 `status`、`code`（如 `not_found`）、`message`。各语言都有按类型判断的写法：Python 是 `NotFoundError` 等异常类，
  Go 是 `errors.Is(err, oad.ErrNotFound)`，Rust 是 `err.is_not_found()`，JS 是 `isNotFound(e)`。
- **翻页**：带游标的列表能自动一页页取完。Python 直接 `for x in client.pages.versions.list(id)`；Go 用 `ListAll`；Rust 用 `list_all`；JS 用 `each*`。
- **可选字段**：要区分“不发这个字段”和“发 `null`”。比如移动页面时 `after: null` 表示放到最前。Python 用 `NOT_GIVEN` 和 `None`，Go 用 `Opt[T]`，
  Rust 用 `Nullable<T>`，JS 用 `undefined` 和 `null`。
- **新增字段**：服务器加了字段或枚举值，旧 SDK 不会报错。

## 接口对照表

<!-- operations:start -->
<!-- 由 node sdks/gen-docs.mjs 从 sdks/operations.json 生成，不要手改 -->
83 个接口。Go 的方法第一个参数是 `ctx`；Rust 的要 `.await`；翻页的接口 Go 另有 `ListAll`、Rust 另有 `list_all`、JS 另有 `each*`。

| 点路径 | Python | Go | Rust | JS | HTTP |
| --- | --- | --- | --- | --- | --- |
| `auth.config` | `client.auth.config()` | `client.Auth.Config()` | `client.auth().config()` | `client.authConfig()` | `GET /auth/config` |
| `auth.login` | `client.auth.login()` | `client.Auth.Login()` | `client.auth().login()` | `client.login()` | `POST /auth/login` |
| `auth.logout` | `client.auth.logout()` | `client.Auth.Logout()` | `client.auth().logout()` | `client.logout()` | `POST /auth/logout` |
| `auth.signup` | `client.auth.signup()` | `client.Auth.Signup()` | `client.auth().signup()` | `client.signup()` | `POST /auth/signup` |
| `me.get` | `client.me.get()` | `client.Me.Get()` | `client.me().get()` | `client.me()` | `GET /me` |
| `me.update` | `client.me.update()` | `client.Me.Update()` | `client.me().update()` | `client.patchMe()` | `PATCH /me` |
| `me.change_password` | `client.me.change_password()` | `client.Me.ChangePassword()` | `client.me().change_password()` | `client.changePassword()` | `POST /me/password` |
| `me.notifications.list` (翻页) | `client.me.notifications.list()` | `client.Me.Notifications.List()` | `client.me().notifications().list()` | `client.inbox()` | `GET /me/notifications` |
| `me.notifications.mark_read` | `client.me.notifications.mark_read()` | `client.Me.Notifications.MarkRead()` | `client.me().notifications().mark_read()` | `client.markRead()` | `POST /me/notifications/read` |
| `workspaces.update` | `client.workspaces.update()` | `client.Workspaces.Update()` | `client.workspaces().update()` | `client.patchWorkspace()` | `PATCH /workspaces/{ws}` |
| `workspaces.members.list` | `client.workspaces.members.list()` | `client.Workspaces.Members.List()` | `client.workspaces().members().list()` | `client.members()` | `GET /workspaces/{ws}/members` |
| `workspaces.members.set_role` | `client.workspaces.members.set_role()` | `client.Workspaces.Members.SetRole()` | `client.workspaces().members().set_role()` | `client.patchMember()` | `PATCH /workspaces/{ws}/members/{user}` |
| `workspaces.members.remove` | `client.workspaces.members.remove()` | `client.Workspaces.Members.Remove()` | `client.workspaces().members().remove()` | `client.removeMember()` | `DELETE /workspaces/{ws}/members/{user}` |
| `workspaces.events` (事件流) | `client.workspaces.events()` | `client.Workspaces.Events()` | `client.workspaces().events()` | `client.events()` | `GET /workspaces/{ws}/events` |
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
| `pages.versions.list` (翻页) | `client.pages.versions.list()` | `client.Pages.Versions.List()` | `client.pages().versions().list()` | `client.versions()` | `GET /pages/{id}/versions` |
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
| `databases.import_csv` (上传) | `client.databases.import_csv()` | `client.Databases.ImportCSV()` | `client.databases().import_csv()` | `client.importCsv()` | `POST /workspaces/{ws}/databases/import` |
| `databases.get` | `client.databases.get()` | `client.Databases.Get()` | `client.databases().get()` | `client.getDatabase()` | `GET /databases/{id}` |
| `databases.query` | `client.databases.query()` | `client.Databases.Query()` | `client.databases().query()` | `client.queryDatabase()` | `POST /databases/{id}/query` |
| `databases.export_csv` (下载) | `client.databases.export_csv()` | `client.Databases.ExportCSV()` | `client.databases().export_csv()` | `client.exportCsv()` | `GET /databases/{id}/export.csv` |
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
| `databases.history.list` (翻页) | `client.databases.history.list()` | `client.Databases.History.List()` | `client.databases().history().list()` | `client.dbHistory()` | `GET /databases/{id}/history` |
| `databases.history.undo` | `client.databases.history.undo()` | `client.Databases.History.Undo()` | `client.databases().history().undo()` | `client.dbUndo()` | `POST /databases/{id}/history/undo` |
| `files.upload` (上传) | `client.files.upload()` | `client.Files.Upload()` | `client.files().upload()` | `client.upload()` | `POST /workspaces/{ws}/files` |
| `files.download` (下载) | `client.files.download()` | `client.Files.Download()` | `client.files().download()` | `client.download()` | `GET /files/{id}` |
| `files.download_named` (下载) | `client.files.download_named()` | `client.Files.DownloadNamed()` | `client.files().download_named()` | `client.downloadNamed()` | `GET /files/{id}/{name}` |
| `files.media` (下载) | `client.files.media()` | `client.Files.Media()` | `client.files().media()` | `client.media()` | `GET /media/{hash}` |
| `files.sign` | `client.files.sign()` | `client.Files.Sign()` | `client.files().sign()` | `client.signFileUrls()` | `POST /files/sign` |
| `links.unfurl` | `client.links.unfurl()` | `client.Links.Unfurl()` | `client.links().unfurl()` | `client.unfurl()` | `GET /unfurl` |
| `trash.list` (翻页) | `client.trash.list()` | `client.Trash.List()` | `client.trash().list()` | `client.trash()` | `GET /workspaces/{ws}/trash` |
| `trash.restore` | `client.trash.restore()` | `client.Trash.Restore()` | `client.trash().restore()` | `client.restoreTrash()` | `POST /trash/{id}/restore` |
| `trash.purge` | `client.trash.purge()` | `client.Trash.Purge()` | `client.trash().purge()` | `client.purgeTrash()` | `DELETE /trash/{id}` |
| `agents.list` | `client.agents.list()` | `client.Agents.List()` | `client.agents().list()` | `client.agents()` | `GET /workspaces/{ws}/agents` |
| `agents.create` | `client.agents.create()` | `client.Agents.Create()` | `client.agents().create()` | `client.createAgent()` | `POST /workspaces/{ws}/agents` |
| `agents.update` | `client.agents.update()` | `client.Agents.Update()` | `client.agents().update()` | `client.patchAgent()` | `PATCH /agents/{id}` |
| `agents.revoke` | `client.agents.revoke()` | `client.Agents.Revoke()` | `client.agents().revoke()` | `client.revokeAgent()` | `DELETE /agents/{id}` |
| `agents.rotate_token` | `client.agents.rotate_token()` | `client.Agents.RotateToken()` | `client.agents().rotate_token()` | `client.reissueToken()` | `POST /agents/{id}/token` |
| `agents.rotate_webhook_secret` | `client.agents.rotate_webhook_secret()` | `client.Agents.RotateWebhookSecret()` | `client.agents().rotate_webhook_secret()` | `client.reissueWebhookSecret()` | `POST /agents/{id}/webhook-secret` |
| `agent.notifications.list` (翻页) | `client.agent.notifications.list()` | `client.Agent.Notifications.List()` | `client.agent().notifications().list()` | `client.agentNotifications()` | `GET /agent/notifications` |
| `agent.notifications.ack` | `client.agent.notifications.ack()` | `client.Agent.Notifications.Ack()` | `client.agent().notifications().ack()` | `client.ackAgentNotifications()` | `POST /agent/notifications/ack` |
| `agent.notifications.stream` (事件流) | `client.agent.notifications.stream()` | `client.Agent.Notifications.Stream()` | `client.agent().notifications().stream()` | `client.agentEvents()` | `GET /agent/notifications/stream` |
<!-- operations:end -->
