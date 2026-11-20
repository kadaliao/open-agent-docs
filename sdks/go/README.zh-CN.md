# Open Agent Docs Go SDK

[English](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/go/README.md) | 简体中文

Open Agent Docs 的 Go 客户端。只用标准库，Go 1.23 起。

```sh
go get github.com/kadaliao/open-agent-docs/sdks/go
```

```go
import oad "github.com/kadaliao/open-agent-docs/sdks/go"

client := oad.NewClient(
	oad.WithBaseURL("https://docs.example.com"),
	oad.WithToken(os.Getenv("OAD_TOKEN")), // Agent token（oad_…）、会话令牌或 JWT
)
// oad.NewClient() 读环境变量 OAD_BASE_URL、OAD_TOKEN

ws, err := client.DefaultWorkspace(ctx) // /me 里第一个工作区
page, err := client.Pages.Create(ctx, ws, oad.PagesCreateParams{
	Title:    oad.Some("周报"),
	Markdown: "# 本周\n\n- 上线了 Go SDK", // 建好以后顺手写正文
})
content, err := client.Pages.Content.Get(ctx, page.ID, oad.PagesContentGetParams{})
fmt.Println(content.Markdown.Value())
```

完整的例子在 [`examples/`](https://github.com/kadaliao/open-agent-docs/tree/main/sdks/go/examples)：`basic`（建页面写 Markdown）、`embed-host`（宿主登录后签 JWT、返回 iframe 地址）、`webhook`（收 Agent 的 webhook）。
godoc 里每个主要功能都有 `Example`。

## 名字

接口的名字按 [`sdks/operations.json`](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/operations.json)：点路径 `pages.versions.list` 是 `client.Pages.Versions.List(ctx, …)`，只有一段的（`search`）挂在客户端上：`client.Search(ctx, ws, …)`。
缩写按 Go 的习惯：`ID`、`URL`、`CSV`、`HTML`（`Databases.ImportCSV`、`Files.DownloadNamed`）。模型和 openapi.json 里同名（`oad.Page`、`oad.CreatePage`）。

- 路径参数是位置参数；查询参数和请求体字段放进最后一个参数 `XxxParams`（名字是点路径拼起来：`PagesVersionsListParams`）。全是可选的可以传零值 `oad.XxxParams{}`；没有参数的接口没有这个参数。
- 查询参数是普通类型，零值不发（`Limit: 2`）。
- 返回解析好的模型（`*oad.Page`）；204 只返回 `error`；`pages.list` 这类只包一层的直接返回数组；`pages.share.get` 没分享时返回 `nil, nil`；下载返回 `io.ReadCloser`（用完 `Close`）。
- 上传：`client.Files.Upload(ctx, ws, "a.txt", r)`、`client.Databases.ImportCSV(ctx, ws, r, oad.DatabasesImportCSVParams{Title: "表"})`。
- 请求体不是对象的（库的视图）用 `Body` 字段，整个当 JSON 发。

## 可选字段：`Opt[T]`

请求里要分清“没给”（不发这个字段）和“明确给 null”，所以可选字段是 `oad.Opt[T]`：

```go
oad.PagesUpdateParams{}                              // {}
oad.PagesUpdateParams{Title: oad.Some("新标题")}      // {"title":"新标题"}
oad.PagesUpdateParams{After: oad.Null[string]()}     // {"after":null}：挪到最前
```

响应的模型也是同一套写法（全包一致）：必填的字段是普通类型，不是必填或者可以是 null 的字段是 `Opt[T]`：

```go
page.Title                 // string，必填
page.ParentID.Get()        // (string, bool)：有值才是 true
page.ParentID.Or("")       // 没有就用默认值
page.ParentID.IsNull()     // 服务器给的是 null
page.ParentID.IsSet()      // 服务器给了这个字段（值或 null）
page.ParentID.Ptr()        // *string，没有值是 nil
```

选 `Opt` 不选指针：指针分不出“没设置”和“null”，而且请求、响应里嵌套的模型（`AddRow`、`DocNode`、`Anchor`）要能来回用。
Go 1.24 以后 `Opt` 也配合 `json:",omitzero"`。

## 错误

```go
_, err := client.Pages.Get(ctx, "nope")
if errors.Is(err, oad.ErrNotFound) { … }

var ae *oad.APIError
if errors.As(err, &ae) {
	log.Println(ae.StatusCode, ae.Code, ae.Message, ae.RetryAfter)
}
```

| 情况 | 怎么判断 |
| --- | --- |
| 服务器回的错误 | `*oad.APIError{StatusCode, Code, Message, RetryAfter}`；`ErrBadRequest` `ErrUnauthorized` `ErrForbidden` `ErrNotFound` `ErrConflict` `ErrGone` `ErrTooLarge` `ErrRateLimited` `ErrServer`（≥500） |
| 连不上、连接断了 | `*oad.ConnectionError`，`errors.Is(err, oad.ErrConnection)` |
| 超时（`WithTimeout`） | 再加上 `oad.ErrTimeout`、`context.DeadlineExceeded` |
| 调用方取消 ctx | `context.Canceled` |
| 配置不对（`WithUser` 没配密钥之类） | `oad.ErrConfig` |
| webhook 验签失败 | `*oad.WebhookError{Kind}`；`ErrWebhook`，`ErrWebhookSignature` / `ErrWebhookStale` / `ErrWebhookMalformed` |

`Code` 取响应的 `error.code`，响应不是 JSON 时是 `http_<状态码>`。

## 客户端选项

| 选项 | 默认 | 说明 |
| --- | --- | --- |
| `WithBaseURL` | `OAD_BASE_URL`，再没有是 `http://127.0.0.1:8848` | `/api/v1` 由 SDK 拼 |
| `WithToken` | `OAD_TOKEN` | 带成 `Authorization: Bearer` |
| `WithHeaderSecret` | | `OAD_AUTH=header` 的服务器，配合 `WithUser` |
| `WithJWTSecret`、`WithJWTIssuer`、`WithJWTAudience`、`WithJWTTTL` | ttl 1 小时 | `OAD_AUTH=jwt` 的服务器，配合 `WithUser` |
| `WithTimeout` | 60 秒 | 单个请求；事件流和下载只管到收到响应头 |
| `WithMaxRetries` | 2 | 见下 |
| `WithLocale` | | `zh` / `en`：服务器错误信息的语言 |
| `WithHeader` | | 每个请求多带的头 |
| `WithHTTPClient` | `&http.Client{}` | 代理、自定义 Transport |

**重试**：429；GET、PUT、DELETE 遇到 502、503、504 或连接断了；连不上（请求还没发出去，什么方法都行）。
有 `Retry-After` 照它等（最多 60 秒），没有就 0.5 秒 × 2^n（最多 8 秒）加 0–25% 的随机。POST、PATCH 遇到 5xx 不重试（可能已经生效了）。
上传的 `io.Reader` 能 `Seek` 的才会重发，不能的不重试。

gzip 由 Go 的 HTTP 库自动处理（不声明 br：标准库解不了）。每个请求带 `User-Agent: open-agent-docs-go/<版本>`。

## 翻页

带 `next_cursor` 的列表（版本、库的历史、回收站、收件箱、Agent 通知）：`List` 返回一页，`ListAll` 逐条取完所有页：

```go
for v, err := range client.Pages.Versions.ListAll(ctx, pageID, oad.PagesVersionsListParams{Limit: 50}) {
	if err != nil { return err }
	fmt.Println(v.ID, v.Label.Or(""))
}
```

`break` 出循环就不再取。`next_cursor` 是空、这页没有条目、或者游标没变时停。

## 事件

```go
for ev, err := range client.Workspaces.Events(ctx, ws) { // ctx 取消就停
	if err != nil { return err }                          // 401、403、404 不重连，给一个错误后停
	switch d := ev.Data.(type) {
	case *oad.Page:           fmt.Println(ev.Name, d.Title) // page.created、page.updated……
	case *oad.DbChangedEvent: fmt.Println(d.DbID.Value())
	}
	if ev.Name == oad.EventResync { /* 落下了，重新拉一遍 */ }
}
```

`Data` 按事件名解析成模型（表在 `EventPageCreated` 那组常量的注释里，从 openapi.json 的 `webhooks` 生成），不认识的事件名是 `json.RawMessage`；`Raw` 总是原始 JSON。
断了自动重连：1 秒起翻倍、最多 30 秒，连上后重置，带 `Last-Event-ID`。

Agent 的通知：取 → 处理 → 确认。没确认的断线重连、重启以后都会再收到：

```go
for note, err := range agent.Agent.Notifications.Stream(ctx, oad.AgentNotificationsStreamParams{}) {
	if err != nil { return err }
	handle(note) // note.Prompt、note.Comment.Body、note.Page……
	agent.Agent.Notifications.Ack(ctx, oad.AgentNotificationsAckParams{IDs: oad.Some([]int64{note.ID})})
}
```

## 替用户调（宿主后端）

宿主自己做登录，后端替当前用户调：

```go
admin := oad.NewClient(oad.WithBaseURL(base), oad.WithHeaderSecret(secret)) // 或 oad.WithJWTSecret(secret)
u, err := admin.WithUser("u-42", oad.UserName("王小明"), oad.UserEmail("x@example.com"))
ws, _ := u.DefaultWorkspace(ctx)
pages, _ := u.Pages.List(ctx, ws)
```

- header 模式带 `X-OAD-Secret`、`X-OAD-User-Id`、`X-OAD-User-Name`（UTF-8 百分号编码）、`X-OAD-User-Email`；不支持 `UserWorkspaces`。
- JWT 模式用 `SignJWT` 签，缓存到过期前 60 秒再换；`UserWorkspaces(oad.WorkspaceGrant{ID: "ws1", Role: "viewer"})` 写进 `workspaces`。
- `WithUser` 返回新的客户端，和原来的共享连接池、配置；原来的不变。两个密钥都配了或都没配时返回 `ErrConfig`。

## 嵌入

```go
tok, _ := oad.SignJWT(secret, "u-42", oad.JWTOptions{Name: "王小明", TTL: 15 * time.Minute})
src := oad.EmbedURL("https://docs.example.com", pageID, oad.EmbedOptions{Theme: "dark", Lang: "zh", Token: tok})
// https://docs.example.com/p/<id>?embed=1&theme=dark&lang=zh#token=…  放进 <iframe src>
```

token 放在 `#` 后面：不进服务器日志、不进 Referer。`SignJWT` 只做 HS256（claims：`sub`、`name`、`email`、`workspaces`、`iat`、`exp`、`iss`、`aud`）；
要 RS256 / ES256 的用自己的 JWT 库签同样的 claims，服务器配 `OAD_JWT_JWKS_URL`。跨域 iframe 的换 token 流程见 [`docs/zh/embedding.md`](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/embedding.md) 和 `examples/embed-host`。

## Webhook

```go
http.Handle("/oad/webhook", oad.WebhookHandler(secret, func(ctx context.Context, n *oad.AgentNotification) error {
	log.Println(n.Comment.Body)
	return nil // 返回错误时回 500，服务器过一会儿重发
}))
```

自己处理请求的用 `oad.VerifyWebhook(secret, r.Header, body)`（body 是原始字节）；时间差超过 5 分钟算 stale，测试时用 `oad.VerifyWebhookAt` 给“现在”。
验签失败：malformed 回 400，signature / stale 回 401；成功回 204。

## 本地启动

开发、测试时在本机起一个服务器：

```go
srv, err := oad.Start(ctx, oad.StartOptions{DataDir: dir, Auth: "header", HeaderSecret: "dev"})
defer srv.Close()
client := oad.NewClient(oad.WithBaseURL(srv.URL), oad.WithHeaderSecret("dev"))
```

- 二进制从哪来：`StartOptions.Binary` → 环境变量 `OAD_BINARY` → 缓存（`os.UserCacheDir()/open-agent-docs/v<版本>/`）→ 从 GitHub release 下载 tar.gz，用同一个 release 的 `SHA256SUMS` 校验。
  支持 macOS 11+（arm64、x86_64）、Linux（x86_64、arm64，glibc 2.34 起：Ubuntu 22.04+、Debian 12+、RHEL 9+）；更老的系统和别的平台用 Docker，见[系统要求](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/quickstart.md#系统要求)。默认下载和 SDK 同版本（`oad.Version`）的。
- `Env` 里的键转成 `OAD_<键大写>`（`"public_url"` → `OAD_PUBLIC_URL`）。`Port` 不给时挑一个空端口。
- 等 `GET /api/v1/auth/config` 回 200（最多 30 秒）；没起来就退出了，错误里带它 stderr 的末尾。
- `Close()`：SIGTERM，等它把页面存好；10 秒内没退出再强制结束。

## 开发

```sh
cd sdks/go
go generate ./...   # openapi.json / operations.json 改了以后重新生成 zz_generated_*.go（生成的代码要提交）
go test ./...       # 单元测试；TestGeneratedUpToDate 检查生成结果是最新的
OAD_BINARY=../../target/debug/open-agent-docs go test ./... -run Conformance   # 统一测试的场景 S1–S15
```

生成器在 `internal/codegen`，手写的运行时（客户端、重试、事件流、webhook、启动）和生成的代码分开放。
