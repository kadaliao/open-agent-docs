# Open Agent Docs Go SDK

[中文](README.md)

The Go client for Open Agent Docs. Standard library only, Go 1.23+.

```sh
go get github.com/kadaliao/open-agent-docs/sdks/go
```

```go
import oad "github.com/kadaliao/open-agent-docs/sdks/go"

client := oad.NewClient(
	oad.WithBaseURL("https://docs.example.com"),
	oad.WithToken(os.Getenv("OAD_TOKEN")), // agent token (oad_…), session token or JWT
)
// oad.NewClient() reads OAD_BASE_URL and OAD_TOKEN

ws, err := client.DefaultWorkspace(ctx) // first workspace in /me
page, err := client.Pages.Create(ctx, ws, oad.PagesCreateParams{
	Title:    oad.Some("Weekly"),
	Markdown: "# This week\n\n- Shipped the Go SDK", // written as the body right after creating
})
content, err := client.Pages.Content.Get(ctx, page.ID, oad.PagesContentGetParams{})
fmt.Println(content.Markdown.Value())
```

Full examples live in [`examples/`](examples): `basic` (create a page with Markdown), `embed-host` (sign a JWT after the host's own login and return an iframe URL), `webhook` (receive agent webhooks).
Every major feature has an `Example` in godoc.

## Names

Operation names follow [`sdks/operations.json`](../operations.json): the dotted path `pages.versions.list` becomes `client.Pages.Versions.List(ctx, …)`; single-segment names (`search`) live on the client: `client.Search(ctx, ws, …)`.
Initialisms follow Go style: `ID`, `URL`, `CSV`, `HTML` (`Databases.ImportCSV`, `Files.DownloadNamed`). Models keep their openapi.json names (`oad.Page`, `oad.CreatePage`).

- Path parameters are positional; query parameters and body fields go into the last argument, `XxxParams` (named after the dotted path: `PagesVersionsListParams`). When everything is optional, pass the zero value `oad.XxxParams{}`; operations without parameters have no such argument.
- Query parameters are plain types; zero values are not sent (`Limit: 2`).
- Results are parsed models (`*oad.Page`); 204 returns just `error`; single-field wrappers like `pages.list` return the array; `pages.share.get` returns `nil, nil` when not shared; downloads return an `io.ReadCloser` (close it).
- Uploads: `client.Files.Upload(ctx, ws, "a.txt", r)`, `client.Databases.ImportCSV(ctx, ws, r, oad.DatabasesImportCSVParams{Title: "Table"})`.
- Bodies that are not objects (database views) use a `Body` field sent as the whole JSON body.

## Optional fields: `Opt[T]`

Requests must distinguish "not given" (field omitted) from "explicitly null", so optional fields are `oad.Opt[T]`:

```go
oad.PagesUpdateParams{}                              // {}
oad.PagesUpdateParams{Title: oad.Some("New title")}  // {"title":"New title"}
oad.PagesUpdateParams{After: oad.Null[string]()}     // {"after":null}: move to the top
```

Response models use the same convention everywhere: required fields are plain types; fields that are optional or nullable are `Opt[T]`:

```go
page.Title                 // string, required
page.ParentID.Get()        // (string, bool): true only when there is a value
page.ParentID.Or("")       // value or a default
page.ParentID.IsNull()     // server sent null
page.ParentID.IsSet()      // server sent the field (value or null)
page.ParentID.Ptr()        // *string, nil without a value
```

Why `Opt` and not pointers: pointers cannot tell "unset" from "null", and nested models (`AddRow`, `DocNode`, `Anchor`) are used both in requests and responses.
On Go 1.24+ `Opt` also works with `json:",omitzero"`.

## Errors

```go
_, err := client.Pages.Get(ctx, "nope")
if errors.Is(err, oad.ErrNotFound) { … }

var ae *oad.APIError
if errors.As(err, &ae) {
	log.Println(ae.StatusCode, ae.Code, ae.Message, ae.RetryAfter)
}
```

| Case | How to check |
| --- | --- |
| Server returned an error | `*oad.APIError{StatusCode, Code, Message, RetryAfter}`; `ErrBadRequest` `ErrUnauthorized` `ErrForbidden` `ErrNotFound` `ErrConflict` `ErrGone` `ErrTooLarge` `ErrRateLimited` `ErrServer` (≥500) |
| Cannot connect / connection dropped | `*oad.ConnectionError`, `errors.Is(err, oad.ErrConnection)` |
| Timeout (`WithTimeout`) | additionally `oad.ErrTimeout` and `context.DeadlineExceeded` |
| Caller cancelled ctx | `context.Canceled` |
| Bad configuration (`WithUser` without a secret, …) | `oad.ErrConfig` |
| Webhook verification failed | `*oad.WebhookError{Kind}`; `ErrWebhook`, `ErrWebhookSignature` / `ErrWebhookStale` / `ErrWebhookMalformed` |

`Code` is the response's `error.code`, or `http_<status>` when the body is not JSON.

## Client options

| Option | Default | Notes |
| --- | --- | --- |
| `WithBaseURL` | `OAD_BASE_URL`, then `http://127.0.0.1:8848` | the SDK appends `/api/v1` |
| `WithToken` | `OAD_TOKEN` | sent as `Authorization: Bearer` |
| `WithHeaderSecret` | | servers with `OAD_AUTH=header`, used by `WithUser` |
| `WithJWTSecret`, `WithJWTIssuer`, `WithJWTAudience`, `WithJWTTTL` | ttl 1 hour | servers with `OAD_AUTH=jwt`, used by `WithUser` |
| `WithTimeout` | 60 s | per request; event streams and downloads only until response headers |
| `WithMaxRetries` | 2 | see below |
| `WithLocale` | | `zh` / `en`: language of server error messages |
| `WithHeader` | | extra header on every request |
| `WithHTTPClient` | `&http.Client{}` | proxies, custom transports |

**Retries**: 429; 502, 503, 504 or a dropped connection for GET, PUT, DELETE; connection refused (request never sent, any method).
Waits follow `Retry-After` (capped at 60 s), otherwise 0.5 s × 2^n (capped at 8 s) plus 0–25% jitter. POST and PATCH are not retried on 5xx (they may have taken effect).
Upload readers are only re-sent when they implement `io.Seeker`.

gzip is handled by Go's HTTP stack (br is not advertised: the standard library cannot decode it). Every request sends `User-Agent: open-agent-docs-go/<version>`.

## Pagination

Lists with `next_cursor` (versions, database history, trash, inbox, agent notifications): `List` returns one page, `ListAll` iterates every item across pages:

```go
for v, err := range client.Pages.Versions.ListAll(ctx, pageID, oad.PagesVersionsListParams{Limit: 50}) {
	if err != nil { return err }
	fmt.Println(v.ID, v.Label.Or(""))
}
```

Breaking out of the loop stops fetching. Iteration stops when `next_cursor` is empty, a page is empty, or the cursor does not advance.

## Events

```go
for ev, err := range client.Workspaces.Events(ctx, ws) { // stops when ctx is cancelled
	if err != nil { return err }                          // 401/403/404: no reconnect, one error then stop
	switch d := ev.Data.(type) {
	case *oad.Page:           fmt.Println(ev.Name, d.Title) // page.created, page.updated, …
	case *oad.DbChangedEvent: fmt.Println(d.DbID.Value())
	}
	if ev.Name == oad.EventResync { /* missed events: reload */ }
}
```

`Data` is decoded by event name (the table is in the comment on the `EventPageCreated` constants, generated from openapi.json `webhooks`); unknown names give `json.RawMessage`; `Raw` is always the original JSON.
Reconnects automatically: 1 s doubling up to 30 s, reset after connecting, with `Last-Event-ID`.

Agent notifications: fetch → handle → ack. Unacked notifications are redelivered after reconnects and restarts:

```go
for note, err := range agent.Agent.Notifications.Stream(ctx, oad.AgentNotificationsStreamParams{}) {
	if err != nil { return err }
	handle(note) // note.Prompt, note.Comment.Body, note.Page, …
	agent.Agent.Notifications.Ack(ctx, oad.AgentNotificationsAckParams{IDs: oad.Some([]int64{note.ID})})
}
```

## Acting as a user (host backends)

The host does its own login and calls Open Agent Docs on behalf of the current user:

```go
admin := oad.NewClient(oad.WithBaseURL(base), oad.WithHeaderSecret(secret)) // or oad.WithJWTSecret(secret)
u, err := admin.WithUser("u-42", oad.UserName("Ming Wang"), oad.UserEmail("x@example.com"))
ws, _ := u.DefaultWorkspace(ctx)
pages, _ := u.Pages.List(ctx, ws)
```

- Header mode sends `X-OAD-Secret`, `X-OAD-User-Id`, `X-OAD-User-Name` (UTF-8 percent-encoded), `X-OAD-User-Email`; `UserWorkspaces` is not supported.
- JWT mode signs with `SignJWT` and caches the token until 60 s before expiry; `UserWorkspaces(oad.WorkspaceGrant{ID: "ws1", Role: "viewer"})` goes into `workspaces`.
- `WithUser` returns a new client sharing the connection pool and configuration; the original is unchanged. Configuring both secrets or neither returns `ErrConfig`.

## Embedding

```go
tok, _ := oad.SignJWT(secret, "u-42", oad.JWTOptions{Name: "Ming Wang", TTL: 15 * time.Minute})
src := oad.EmbedURL("https://docs.example.com", pageID, oad.EmbedOptions{Theme: "dark", Lang: "en", Token: tok})
// https://docs.example.com/p/<id>?embed=1&theme=dark&lang=en#token=…  put it in <iframe src>
```

The token goes after `#`: it never reaches server logs or the Referer. `SignJWT` only does HS256 (claims: `sub`, `name`, `email`, `workspaces`, `iat`, `exp`, `iss`, `aud`);
for RS256 / ES256 sign the same claims with your own JWT library and set `OAD_JWT_JWKS_URL` on the server. The cross-origin iframe token refresh is described in [`docs/en/embedding.md`](../../docs/en/embedding.md) and shown in `examples/embed-host`.

## Webhooks

```go
http.Handle("/oad/webhook", oad.WebhookHandler(secret, func(ctx context.Context, n *oad.AgentNotification) error {
	log.Println(n.Comment.Body)
	return nil // returning an error responds 500 and the server retries later
}))
```

To handle requests yourself use `oad.VerifyWebhook(secret, r.Header, body)` (raw body bytes); a timestamp more than 5 minutes off is stale; tests can pass "now" with `oad.VerifyWebhookAt`.
Verification failures respond 400 (malformed) or 401 (signature / stale); success responds 204.

## Running a server locally

For development and tests:

```go
srv, err := oad.Start(ctx, oad.StartOptions{DataDir: dir, Auth: "header", HeaderSecret: "dev"})
defer srv.Close()
client := oad.NewClient(oad.WithBaseURL(srv.URL), oad.WithHeaderSecret("dev"))
```

- Binary lookup: `StartOptions.Binary` → `OAD_BINARY` → cache (`os.UserCacheDir()/open-agent-docs/v<version>/`) → download the tar.gz from the GitHub release, verified with that release's `SHA256SUMS`.
  macOS (arm64, x86_64) and Linux (x86_64, arm64) are supported; use Docker elsewhere. The default version matches the SDK (`oad.Version`).
- Keys in `Env` become `OAD_<KEY>` (`"public_url"` → `OAD_PUBLIC_URL`). Without `Port` a free port is picked.
- Waits for `GET /api/v1/auth/config` to return 200 (up to 30 s); if the process exits first, the error includes the tail of its stderr.
- `Close()`: SIGTERM so open pages are saved; killed if still running after 10 s.

## Development

```sh
cd sdks/go
go generate ./...   # regenerate zz_generated_*.go after openapi.json / operations.json change (commit the output)
go test ./...       # unit tests; TestGeneratedUpToDate checks the generated code is current
OAD_BINARY=../../target/debug/open-agent-docs go test ./... -run Conformance   # conformance scenarios S1–S15
```

The generator is in `internal/codegen`; the hand-written runtime (client, retries, event streams, webhooks, launcher) is kept separate from generated code.
