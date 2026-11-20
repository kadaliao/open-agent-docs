package oad

// 统一测试的场景 S1–S15（docs/sdk-contract.md §14）。要服务器二进制：设 OAD_BINARY，没设就跳过（S10、S11、S14 不用二进制，照常跑）。
//
//	OAD_BINARY=/path/to/open-agent-docs go test ./... -run Conformance

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"reflect"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

const confSecret = "conformance-header-secret"

// startServer 起一个服务器（自己的临时数据目录），测试结束时关掉。
func startServer(t *testing.T, opts StartOptions) *Server {
	t.Helper()
	if opts.Binary == "" && getenv("OAD_BINARY") == "" {
		t.Skip("没设 OAD_BINARY，跳过要服务器的场景")
	}
	if opts.DataDir == "" {
		opts.DataDir = t.TempDir()
	}
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()
	srv, err := Start(ctx, opts)
	if err != nil {
		t.Fatalf("Start: %v", err)
	}
	t.Cleanup(func() { srv.Close() })
	return srv
}

// headerUser：header 模式的服务器 + 一个用户的客户端。
func headerUser(t *testing.T, userID string, extra ...Option) (*Server, *Client) {
	t.Helper()
	srv := startServer(t, StartOptions{Auth: "header", HeaderSecret: confSecret})
	admin := NewClient(append([]Option{WithBaseURL(srv.URL), WithHeaderSecret(confSecret)}, extra...)...)
	u, err := admin.WithUser(userID, UserName("测试用户"))
	if err != nil {
		t.Fatal(err)
	}
	return srv, u
}

func ctxT(t *testing.T) context.Context {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	t.Cleanup(cancel)
	return ctx
}

// must 取值，出错就让测试失败：配合每个测试开头的 defer catch(t)（Go 不能把 f() 的两个返回值和 t 一起传）。
func must[T any](v T, err error) T {
	if err != nil {
		panic(mustFailed{err})
	}
	return v
}

type mustFailed struct{ err error }

func catch(t *testing.T) {
	if r := recover(); r != nil {
		if m, ok := r.(mustFailed); ok {
			t.Fatal(m.err)
		}
		panic(r)
	}
}

func TestConformanceS1Start(t *testing.T) {
	t.Parallel()
	defer catch(t)
	srv := startServer(t, StartOptions{Auth: "header", HeaderSecret: confSecret})
	c := NewClient(WithBaseURL(srv.URL))
	cfg := must(c.Auth.Config(ctxT(t)))
	if cfg.Mode != AuthModeHeader {
		t.Fatalf("mode = %q", cfg.Mode)
	}
	if err := srv.Close(); err != nil {
		t.Fatal(err)
	}
	if conn, err := net.DialTimeout("tcp", fmt.Sprintf("127.0.0.1:%d", srv.Port), time.Second); err == nil {
		conn.Close()
		t.Fatal("Close 以后端口还能连")
	}
}

func TestConformanceS2WithUser(t *testing.T) {
	t.Parallel()
	defer catch(t)
	srv := startServer(t, StartOptions{Auth: "header", HeaderSecret: confSecret})
	admin := NewClient(WithBaseURL(srv.URL), WithHeaderSecret(confSecret))
	u := must(admin.WithUser("u1", UserName("王小明")))
	ctx := ctxT(t)
	me := must(u.Me.Get(ctx))
	if me.User.Name != "王小明" {
		t.Fatalf("name = %q", me.User.Name)
	}
	ws := must(u.DefaultWorkspace(ctx))
	if len(me.Workspaces) == 0 || ws != me.Workspaces[0].ID {
		t.Fatalf("DefaultWorkspace = %q, /me = %+v", ws, me.Workspaces)
	}
}

func TestConformanceS3Pages(t *testing.T) {
	t.Parallel()
	defer catch(t)
	_, u := headerUser(t, "u3")
	ctx := ctxT(t)
	ws := must(u.DefaultWorkspace(ctx))
	page := must(u.Pages.Create(ctx, ws, PagesCreateParams{Title: Some("周报"), Markdown: "# 标题\n\n正文"}))
	content := must(u.Pages.Content.Get(ctx, page.ID, PagesContentGetParams{}))
	md := content.Markdown.Value()
	if !strings.Contains(md, "# 标题") || !strings.Contains(md, "正文") {
		t.Fatalf("markdown = %q", md)
	}
	pages := must(u.Pages.List(ctx, ws))
	found := false
	for _, p := range pages {
		found = found || p.ID == page.ID
	}
	if !found {
		t.Fatal("pages.list 里没有新页面")
	}
	up := must(u.Pages.Update(ctx, page.ID, PagesUpdateParams{Title: Some("周报（改）")}))
	if up.Title != "周报（改）" {
		t.Fatalf("title = %q", up.Title)
	}
	if got := must(u.Pages.Get(ctx, page.ID)); got.Title != "周报（改）" {
		t.Fatalf("get 的 title = %q", got.Title)
	}
	if err := u.Pages.Content.Replace(ctx, page.ID, PagesContentReplaceParams{Markdown: Some("# 新的\n\n内容"), Base: Some(content.Version)}); err != nil {
		t.Fatal(err)
	}
	if _, err := u.Pages.Delete(ctx, page.ID); err != nil {
		t.Fatal(err)
	}
	if _, err := u.Pages.Get(ctx, page.ID); !errors.Is(err, ErrNotFound) {
		t.Fatalf("删了以后 get: %v", err)
	}
}

func TestConformanceS4Errors(t *testing.T) {
	t.Parallel()
	defer catch(t)
	srv, u := headerUser(t, "u4")
	ctx := ctxT(t)
	_, err := u.Pages.Get(ctx, "nope")
	var ae *APIError
	if !errors.As(err, &ae) || !errors.Is(err, ErrNotFound) {
		t.Fatalf("err = %v", err)
	}
	if ae.StatusCode != 404 || ae.Code != "not_found" || ae.Message == "" {
		t.Fatalf("APIError = %+v", ae)
	}
	bogus := NewClient(WithBaseURL(srv.URL), WithToken("oad_bogus"))
	if _, err := bogus.Me.Get(ctx); !errors.Is(err, ErrUnauthorized) {
		t.Fatalf("假 token: %v", err)
	}
}

func TestConformanceS5Agent(t *testing.T) {
	t.Parallel()
	defer catch(t)
	srv, u := headerUser(t, "u5")
	ctx := ctxT(t)
	ws := must(u.DefaultWorkspace(ctx))
	created := must(u.Agents.Create(ctx, ws, AgentsCreateParams{Name: "bot", Scope: "write"}))
	bot := NewClient(WithBaseURL(srv.URL), WithToken(created.Token))
	me := must(bot.Me.Get(ctx))
	if me.User.Kind != PrincipalKindAgent {
		t.Fatalf("kind = %q", me.User.Kind)
	}
}

// countingTransport 数一数发了几个请求（按路径前缀）。
type countingTransport struct {
	prefix string
	n      atomic.Int64
	onResp func(*http.Request, *http.Response)
}

func (c *countingTransport) RoundTrip(r *http.Request) (*http.Response, error) {
	if strings.HasPrefix(r.URL.Path, c.prefix) {
		c.n.Add(1)
	}
	resp, err := http.DefaultTransport.RoundTrip(r)
	if err == nil && c.onResp != nil {
		c.onResp(r, resp)
	}
	return resp, err
}

func TestConformanceS6Paginate(t *testing.T) {
	t.Parallel()
	defer catch(t)
	srv := startServer(t, StartOptions{Auth: "header", HeaderSecret: confSecret})
	ctx := ctxT(t)
	ct := &countingTransport{}
	admin := NewClient(WithBaseURL(srv.URL), WithHeaderSecret(confSecret), WithHTTPClient(&http.Client{Transport: ct}))
	u := must(admin.WithUser("u6"))
	ws := must(u.DefaultWorkspace(ctx))
	page := must(u.Pages.Create(ctx, ws, PagesCreateParams{Title: Some("翻页")}))
	for i := range 5 {
		if err := u.Pages.Content.Replace(ctx, page.ID, PagesContentReplaceParams{Markdown: Some(fmt.Sprintf("第 %d 版", i))}); err != nil {
			t.Fatal(err)
		}
		must(u.Pages.Versions.Create(ctx, page.ID, PagesVersionsCreateParams{Label: Some(fmt.Sprintf("v%d", i))}))
	}
	ct.prefix = "/api/v1/pages/" + page.ID + "/versions"
	seen := map[int64]bool{}
	for v, err := range u.Pages.Versions.ListAll(ctx, page.ID, PagesVersionsListParams{Limit: 2}) {
		if err != nil {
			t.Fatal(err)
		}
		if seen[v.ID] {
			t.Fatalf("版本 %d 重复", v.ID)
		}
		seen[v.ID] = true
	}
	if len(seen) < 5 {
		t.Fatalf("只取到 %d 个版本", len(seen))
	}
	if n := ct.n.Load(); n < 3 {
		t.Fatalf("只发了 %d 个请求，没翻页", n)
	}
}

func TestConformanceS7Events(t *testing.T) {
	t.Parallel()
	defer catch(t)
	srv := startServer(t, StartOptions{Auth: "header", HeaderSecret: confSecret})
	connected := make(chan struct{}, 4)
	ct := &countingTransport{onResp: func(r *http.Request, resp *http.Response) {
		if strings.HasSuffix(r.URL.Path, "/events") && resp.StatusCode == 200 {
			connected <- struct{}{}
		}
	}}
	admin := NewClient(WithBaseURL(srv.URL), WithHeaderSecret(confSecret), WithHTTPClient(&http.Client{Transport: ct}))
	u := must(admin.WithUser("u7"))
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	ws := must(u.DefaultWorkspace(ctx))

	got := make(chan *Page, 8)
	errc := make(chan error, 1)
	go func() {
		for ev, err := range u.Workspaces.Events(ctx, ws) {
			if err != nil {
				errc <- err
				return
			}
			if p, ok := ev.Data.(*Page); ok && ev.Name == EventPageCreated {
				got <- p
			}
		}
	}()
	select {
	case <-connected:
	case err := <-errc:
		t.Fatal(err)
	case <-time.After(10 * time.Second):
		t.Fatal("事件流 10 秒内没连上")
	}
	page := must(u.Pages.Create(ctx, ws, PagesCreateParams{Title: Some("事件")}))
	deadline := time.After(10 * time.Second)
	for {
		select {
		case p := <-got:
			if p.ID == page.ID {
				return
			}
		case err := <-errc:
			t.Fatal(err)
		case <-deadline:
			t.Fatal("10 秒内没收到 page.created")
		}
	}
}

func TestConformanceS8AgentNotifications(t *testing.T) {
	t.Parallel()
	defer catch(t)
	srv, u := headerUser(t, "u8")
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	ws := must(u.DefaultWorkspace(ctx))
	created := must(u.Agents.Create(ctx, ws, AgentsCreateParams{Name: "bot", Scope: "write"}))
	bot := NewClient(WithBaseURL(srv.URL), WithToken(created.Token))
	page := must(u.Pages.Create(ctx, ws, PagesCreateParams{Title: Some("评论"), Markdown: "一段话"}))
	must(u.Threads.Create(ctx, page.ID, ThreadsCreateParams{Body: "@bot 看看"}))

	var note *AgentNotification
	for n, err := range bot.Agent.Notifications.Stream(ctx, AgentNotificationsStreamParams{}) {
		if err != nil {
			t.Fatal(err)
		}
		note = n
		break
	}
	if note == nil || !strings.Contains(note.Comment.Body, "看看") {
		t.Fatalf("通知 = %+v", note)
	}
	acked := must(bot.Agent.Notifications.Ack(ctx, AgentNotificationsAckParams{IDs: Some([]int64{note.ID})}))
	if acked.Acked != 1 {
		t.Fatalf("acked = %d", acked.Acked)
	}
	left := must(bot.Agent.Notifications.List(ctx, AgentNotificationsListParams{}))
	for _, n := range left.Notifications {
		if n.ID == note.ID {
			t.Fatal("ack 以后 list 里还有它")
		}
	}
}

func TestConformanceS9JWT(t *testing.T) {
	t.Parallel()
	defer catch(t)
	const secret = "conformance-jwt-secret-0123456789"
	srv := startServer(t, StartOptions{Auth: "jwt", JWTSecret: secret})
	ctx := ctxT(t)
	u := must(NewClient(WithBaseURL(srv.URL), WithJWTSecret(secret)).WithUser("u9", UserName("小九")))
	me := must(u.Me.Get(ctx))
	if me.User.Name != "小九" {
		t.Fatalf("name = %q", me.User.Name)
	}
	other := must(NewClient(WithBaseURL(srv.URL), WithJWTSecret("another-secret-0123456789abcdef")).WithUser("u9"))
	if _, err := other.Me.Get(ctx); !errors.Is(err, ErrUnauthorized) {
		t.Fatalf("别的密钥签的: %v", err)
	}
}

func TestConformanceS10Webhook(t *testing.T) { testWebhookVectors(t) }

func TestConformanceS11Embed(t *testing.T) {
	testEmbedVectors(t)
	testJWTVector(t)
}

func TestConformanceS12Share(t *testing.T) {
	t.Parallel()
	defer catch(t)
	_, u := headerUser(t, "u12")
	ctx := ctxT(t)
	ws := must(u.DefaultWorkspace(ctx))
	page := must(u.Pages.Create(ctx, ws, PagesCreateParams{Title: Some("分享")}))
	if s := must(u.Pages.Share.Get(ctx, page.ID)); s != nil {
		t.Fatalf("新页面就有分享: %+v", s)
	}
	s := must(u.Pages.Share.Enable(ctx, page.ID, PagesShareEnableParams{}))
	if !strings.Contains(s.URL, "/s/") {
		t.Fatalf("url = %q", s.URL)
	}
	if err := u.Pages.Share.Disable(ctx, page.ID); err != nil {
		t.Fatal(err)
	}
	if s := must(u.Pages.Share.Get(ctx, page.ID)); s != nil {
		t.Fatalf("关了以后还有: %+v", s)
	}
}

func TestConformanceS13HTML(t *testing.T) {
	t.Parallel()
	defer catch(t)
	// Content 的 HTML 字段要 openapi.json 更新后重新 go generate 才有；用反射取，生成前后都能编译。
	if _, ok := reflect.TypeOf(Content{}).FieldByName("HTML"); !ok {
		t.Skip("Content 还没有 HTML 字段：openapi.json 还没加 format=html（W-WEB），加了以后在 sdks/go 下 go generate")
	}
	_, u := headerUser(t, "u13")
	ctx := ctxT(t)
	ws := must(u.DefaultWorkspace(ctx))
	page := must(u.Pages.Create(ctx, ws, PagesCreateParams{Title: Some("HTML"), Markdown: "# 标题\n\n正文 <script>alert(1)</script>"}))
	c, err := u.Pages.Content.Get(ctx, page.ID, PagesContentGetParams{Format: "html"})
	if errors.Is(err, ErrBadRequest) {
		t.Skipf("服务器还不支持 format=html：%v", err)
	}
	if err != nil {
		t.Fatal(err)
	}
	f := reflect.ValueOf(c).Elem().FieldByName("HTML")
	html := fmt.Sprint(f.Interface())
	if v, ok := f.Interface().(interface{ Value() string }); ok {
		html = v.Value()
	}
	if html == "" {
		t.Skip("服务器还不支持 format=html（响应里没有 html）")
	}
	if !strings.Contains(html, "<h1") || strings.Contains(html, "<script") {
		t.Fatalf("html = %q", html)
	}
}

func TestConformanceS14Retry(t *testing.T) { testRetryAfter(t) }

func TestConformanceS15Files(t *testing.T) {
	t.Parallel()
	defer catch(t)
	_, u := headerUser(t, "u15")
	ctx := ctxT(t)
	ws := must(u.DefaultWorkspace(ctx))
	up := must(u.Files.Upload(ctx, ws, "a.txt", bytes.NewReader([]byte("hello"))))
	rc := must(u.Files.Download(ctx, up.ID, FilesDownloadParams{}))
	defer rc.Close()
	got := must(io.ReadAll(rc))
	if string(got) != "hello" {
		t.Fatalf("下载的 = %q", got)
	}
}
