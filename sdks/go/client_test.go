package oad

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

// fake 是一个假服务器：按顺序回 handlers 里的处理，记下收到的请求。
type fake struct {
	*httptest.Server
	mu   sync.Mutex
	reqs []*http.Request
	body [][]byte
	n    atomic.Int64
}

func newFake(t *testing.T, h http.HandlerFunc) *fake {
	f := &fake{}
	f.Server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		b, _ := io.ReadAll(r.Body)
		f.mu.Lock()
		f.reqs = append(f.reqs, r)
		f.body = append(f.body, b)
		f.mu.Unlock()
		f.n.Add(1)
		r.Body = io.NopCloser(bytes.NewReader(b))
		h(w, r)
	}))
	t.Cleanup(f.Close)
	return f
}

func (f *fake) last() (*http.Request, []byte) {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.reqs[len(f.reqs)-1], f.body[len(f.body)-1]
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(v)
}

func apiErr(w http.ResponseWriter, status int, code, msg string) {
	writeJSON(w, status, map[string]any{"error": map[string]string{"code": code, "message": msg}})
}

// 不真睡：记下要等多久
func noSleep(waits *[]time.Duration) Option {
	return func(c *config) {
		c.sleep = func(ctx context.Context, d time.Duration) error {
			*waits = append(*waits, d)
			return ctx.Err()
		}
	}
}

func TestNewClientDefaults(t *testing.T) {
	t.Setenv("OAD_BASE_URL", "http://env.example.com/")
	t.Setenv("OAD_TOKEN", "oad_env")
	c := NewClient()
	if c.BaseURL() != "http://env.example.com" || c.core.cfg.token != "oad_env" {
		t.Fatalf("没读环境变量：%q %q", c.BaseURL(), c.core.cfg.token)
	}
	if c.core.cfg.timeout != 60*time.Second || c.core.cfg.maxRetries != 2 {
		t.Fatal("默认值不对")
	}
	c = NewClient(WithBaseURL("http://x/"), WithToken("t"))
	if c.BaseURL() != "http://x" || c.core.cfg.token != "t" {
		t.Fatal("选项没盖过环境变量")
	}
	t.Setenv("OAD_BASE_URL", "")
	if NewClient().BaseURL() != DefaultBaseURL {
		t.Fatal("默认地址不对")
	}
}

func TestRequestHeaders(t *testing.T) {
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, 200, map[string]any{"user": map[string]any{"id": "u", "name": "n", "kind": "user"}, "workspaces": []any{}})
	})
	c := NewClient(WithBaseURL(f.URL), WithToken("oad_x"), WithLocale("zh"), WithHeader("X-Extra", "1"))
	if _, err := c.Me.Get(context.Background()); err != nil {
		t.Fatal(err)
	}
	r, _ := f.last()
	if r.URL.Path != "/api/v1/me" {
		t.Fatalf("path = %s", r.URL.Path)
	}
	for k, want := range map[string]string{
		"Authorization": "Bearer oad_x", "Accept-Language": "zh-CN", "X-Extra": "1",
		"User-Agent": "open-agent-docs-go/" + Version, "Accept": "application/json",
	} {
		if got := r.Header.Get(k); got != want {
			t.Errorf("%s = %q，应该是 %q", k, got, want)
		}
	}
}

// S14：429 + Retry-After: 1 → 等 ≥ 1 秒再成功；max_retries=0 → RateLimit 错误，RetryAfter == 1 秒。
func testRetryAfter(t *testing.T) {
	var calls atomic.Int64
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		if calls.Add(1) == 1 {
			w.Header().Set("Retry-After", "1")
			apiErr(w, 429, "too_many_requests", "慢点")
			return
		}
		writeJSON(w, 200, map[string]any{"mode": "header", "timezone": "UTC", "signup": "off"})
	})
	c := NewClient(WithBaseURL(f.URL))
	start := time.Now()
	cfg, err := c.Auth.Config(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Mode != AuthModeHeader {
		t.Fatalf("mode = %q", cfg.Mode)
	}
	if el := time.Since(start); el < time.Second {
		t.Fatalf("只等了 %s", el)
	}

	calls.Store(0)
	c0 := NewClient(WithBaseURL(f.URL), WithMaxRetries(0))
	_, err = c0.Auth.Config(context.Background())
	var ae *APIError
	if !errors.As(err, &ae) || !errors.Is(err, ErrRateLimited) {
		t.Fatalf("err = %v", err)
	}
	if ae.RetryAfter != time.Second || ae.StatusCode != 429 || ae.Code != "too_many_requests" {
		t.Fatalf("APIError = %+v", ae)
	}
}

func TestRetryRules(t *testing.T) {
	cases := []struct {
		name   string
		method func(c *Client) error
		status int
		calls  int64
	}{
		{"GET 503 重试", func(c *Client) error { _, err := c.Me.Get(context.Background()); return err }, 503, 3},
		{"GET 500 不重试", func(c *Client) error { _, err := c.Me.Get(context.Background()); return err }, 500, 1},
		{"POST 503 不重试", func(c *Client) error {
			_, err := c.Pages.Create(context.Background(), "ws", PagesCreateParams{})
			return err
		}, 503, 1},
		{"POST 429 重试", func(c *Client) error {
			_, err := c.Pages.Create(context.Background(), "ws", PagesCreateParams{})
			return err
		}, 429, 3},
		{"PUT 502 重试", func(c *Client) error {
			return c.Pages.Content.Replace(context.Background(), "p", PagesContentReplaceParams{Markdown: Some("x")})
		}, 502, 3},
		{"404 不重试", func(c *Client) error { _, err := c.Me.Get(context.Background()); return err }, 404, 1},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			f := newFake(t, func(w http.ResponseWriter, r *http.Request) { apiErr(w, tc.status, "x", "y") })
			var waits []time.Duration
			c := NewClient(WithBaseURL(f.URL), noSleep(&waits))
			err := tc.method(c)
			var ae *APIError
			if !errors.As(err, &ae) || ae.StatusCode != tc.status {
				t.Fatalf("err = %v", err)
			}
			if f.n.Load() != tc.calls {
				t.Fatalf("发了 %d 次，应该 %d 次", f.n.Load(), tc.calls)
			}
			for i, w := range waits {
				lo := 500 * time.Millisecond << i
				if w < lo || w > lo*5/4 {
					t.Fatalf("第 %d 次等了 %s", i, w)
				}
			}
		})
	}
}

func TestRetryAfterCapped(t *testing.T) {
	var n atomic.Int64
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		if n.Add(1) == 1 {
			w.Header().Set("Retry-After", "3600")
			apiErr(w, 429, "too_many_requests", "x")
			return
		}
		writeJSON(w, 200, map[string]any{"user": map[string]any{}, "workspaces": []any{}})
	})
	var waits []time.Duration
	c := NewClient(WithBaseURL(f.URL), noSleep(&waits))
	if _, err := c.Me.Get(context.Background()); err != nil {
		t.Fatal(err)
	}
	if len(waits) != 1 || waits[0] != 60*time.Second {
		t.Fatalf("waits = %v", waits)
	}
}

func TestRetryReplaysBody(t *testing.T) {
	var n atomic.Int64
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		if n.Add(1) == 1 {
			apiErr(w, 429, "too_many_requests", "x")
			return
		}
		writeJSON(w, 201, map[string]any{"id": "f1", "url": "/api/v1/files/f1", "name": "a.txt", "mime": "text/plain", "size": 5})
	})
	var waits []time.Duration
	c := NewClient(WithBaseURL(f.URL), noSleep(&waits))
	up, err := c.Files.Upload(context.Background(), "ws", "中 文.txt", strings.NewReader("hello"))
	if err != nil {
		t.Fatal(err)
	}
	if up.ID != "f1" {
		t.Fatalf("up = %+v", up)
	}
	r, body := f.last()
	if string(body) != "hello" {
		t.Fatalf("重发的正文 = %q", body)
	}
	if r.Header.Get("X-Filename") != "%E4%B8%AD%20%E6%96%87.txt" || r.Header.Get("Content-Type") != "application/octet-stream" {
		t.Fatalf("头 = %v", r.Header)
	}

	// 不能 Seek 的 reader：不重发
	n.Store(0)
	_, err = c.Files.Upload(context.Background(), "ws", "a", io.MultiReader(strings.NewReader("x")))
	if !errors.Is(err, ErrRateLimited) {
		t.Fatalf("err = %v", err)
	}
}

func TestAPIErrors(t *testing.T) {
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/v1/pages/nope":
			apiErr(w, 404, "not_found", "页面不存在")
		case "/api/v1/pages/html":
			w.WriteHeader(500)
			io.WriteString(w, "<html>oops</html>")
		default:
			w.WriteHeader(403)
		}
	})
	c := NewClient(WithBaseURL(f.URL), WithMaxRetries(0))
	_, err := c.Pages.Get(context.Background(), "nope")
	var ae *APIError
	if !errors.As(err, &ae) || ae.Code != "not_found" || ae.Message != "页面不存在" || ae.StatusCode != 404 {
		t.Fatalf("err = %#v", err)
	}
	if !errors.Is(err, ErrNotFound) || errors.Is(err, ErrServer) {
		t.Fatal("errors.Is 不对")
	}
	if !strings.Contains(err.Error(), "GET /api/v1/pages/nope") {
		t.Fatalf("Error() = %s", err)
	}
	_, err = c.Pages.Get(context.Background(), "html")
	if !errors.As(err, &ae) || ae.Code != "http_500" || !errors.Is(err, ErrServer) {
		t.Fatalf("err = %v", err)
	}
	_, err = c.Pages.Get(context.Background(), "x")
	if !errors.Is(err, ErrForbidden) {
		t.Fatalf("err = %v", err)
	}
}

func TestTimeout(t *testing.T) {
	release := make(chan struct{})
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		select {
		case <-release:
		case <-r.Context().Done():
		}
	})
	defer close(release)
	c := NewClient(WithBaseURL(f.URL), WithTimeout(100*time.Millisecond))
	_, err := c.Me.Get(context.Background())
	if !errors.Is(err, context.DeadlineExceeded) || !errors.Is(err, ErrTimeout) || !errors.Is(err, ErrConnection) {
		t.Fatalf("err = %v", err)
	}
	if f.n.Load() != 1 {
		t.Fatalf("超时不该重试，发了 %d 次", f.n.Load())
	}

	// 调用方取消
	ctx, cancel := context.WithCancel(context.Background())
	go func() { time.Sleep(50 * time.Millisecond); cancel() }()
	_, err = NewClient(WithBaseURL(f.URL)).Me.Get(ctx)
	if !errors.Is(err, context.Canceled) || errors.Is(err, ErrTimeout) {
		t.Fatalf("err = %v", err)
	}
}

func TestConnectionRefused(t *testing.T) {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	addr := l.Addr().String()
	l.Close()
	var waits []time.Duration
	c := NewClient(WithBaseURL("http://"+addr), noSleep(&waits))
	_, err = c.Pages.Create(context.Background(), "ws", PagesCreateParams{})
	if !errors.Is(err, ErrConnection) || errors.Is(err, ErrTimeout) {
		t.Fatalf("err = %v", err)
	}
	if len(waits) != 2 {
		t.Fatalf("连不上应该重试 2 次（POST 也行），等了 %v", waits)
	}
}

func TestNullOn404AndUnwrap(t *testing.T) {
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/v1/pages/p1/share":
			apiErr(w, 404, "not_found", "没分享")
		case "/api/v1/workspaces/ws/pages":
			writeJSON(w, 200, map[string]any{"pages": []any{map[string]any{"id": "a", "title": "A"}, map[string]any{"id": "b", "title": "B", "parent_id": nil}}})
		}
	})
	c := NewClient(WithBaseURL(f.URL))
	s, err := c.Pages.Share.Get(context.Background(), "p1")
	if err != nil || s != nil {
		t.Fatalf("share = %v, %v", s, err)
	}
	pages, err := c.Pages.List(context.Background(), "ws")
	if err != nil || len(pages) != 2 || pages[1].Title != "B" {
		t.Fatalf("pages = %+v, %v", pages, err)
	}
	if pages[0].ParentID.IsSet() || !pages[1].ParentID.IsNull() {
		t.Fatal("Opt 的三态没解对")
	}
}

func TestPagesCreateMarkdown(t *testing.T) {
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method == "POST" {
			writeJSON(w, 201, map[string]any{"id": "p1", "title": "周报"})
			return
		}
		w.WriteHeader(204)
	})
	c := NewClient(WithBaseURL(f.URL))
	p, err := c.Pages.Create(context.Background(), "ws 1", PagesCreateParams{Title: Some("周报"), After: Null[string](), Markdown: "# 标题"})
	if err != nil || p.ID != "p1" {
		t.Fatalf("%v %v", p, err)
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	if len(f.reqs) != 2 {
		t.Fatalf("发了 %d 个请求", len(f.reqs))
	}
	if f.reqs[0].URL.EscapedPath() != "/api/v1/workspaces/ws%201/pages" {
		t.Fatalf("path = %s", f.reqs[0].URL.EscapedPath())
	}
	if string(f.body[0]) != `{"after":null,"title":"周报"}` {
		t.Fatalf("建页面的正文 = %s", f.body[0])
	}
	if f.reqs[1].Method != "PUT" || f.reqs[1].URL.Path != "/api/v1/pages/p1/content" || string(f.body[1]) != `{"markdown":"# 标题"}` {
		t.Fatalf("写正文 = %s %s %s", f.reqs[1].Method, f.reqs[1].URL.Path, f.body[1])
	}
}

func TestPaginate(t *testing.T) {
	// 三页：c1 → c2 → 空
	pages := map[string]map[string]any{
		"":   {"versions": []any{map[string]any{"id": 1}, map[string]any{"id": 2}}, "next_cursor": "c1"},
		"c1": {"versions": []any{map[string]any{"id": 3}, map[string]any{"id": 4}}, "next_cursor": "c2"},
		"c2": {"versions": []any{map[string]any{"id": 5}}, "next_cursor": nil},
	}
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Query().Get("limit") != "2" {
			t.Errorf("limit = %q", r.URL.Query().Get("limit"))
		}
		writeJSON(w, 200, pages[r.URL.Query().Get("cursor")])
	})
	c := NewClient(WithBaseURL(f.URL))
	var ids []int64
	for v, err := range c.Pages.Versions.ListAll(context.Background(), "p", PagesVersionsListParams{Limit: 2}) {
		if err != nil {
			t.Fatal(err)
		}
		ids = append(ids, v.ID)
	}
	if fmt.Sprint(ids) != "[1 2 3 4 5]" || f.n.Load() != 3 {
		t.Fatalf("ids = %v，请求 %d 个", ids, f.n.Load())
	}

	// break 了就不再取
	f.n.Store(0)
	for range c.Pages.Versions.ListAll(context.Background(), "p", PagesVersionsListParams{Limit: 2}) {
		break
	}
	if f.n.Load() != 1 {
		t.Fatalf("break 以后又取了：%d", f.n.Load())
	}

	// 游标没变：停
	stuck := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, 200, map[string]any{"items": []any{map[string]any{"id": "x"}}, "next_cursor": "same"})
	})
	c = NewClient(WithBaseURL(stuck.URL))
	n := 0
	for _, err := range c.Trash.ListAll(context.Background(), "ws", TrashListParams{}) {
		if err != nil {
			t.Fatal(err)
		}
		n++
	}
	if n != 2 || stuck.n.Load() != 2 {
		t.Fatalf("游标没变还在取：%d 条，%d 个请求", n, stuck.n.Load())
	}

	// 出错：给一次错误
	bad := newFake(t, func(w http.ResponseWriter, r *http.Request) { apiErr(w, 403, "forbidden", "不行") })
	c = NewClient(WithBaseURL(bad.URL))
	var errs int
	for _, err := range c.Trash.ListAll(context.Background(), "ws", TrashListParams{}) {
		if errors.Is(err, ErrForbidden) {
			errs++
		}
	}
	if errs != 1 {
		t.Fatalf("errs = %d", errs)
	}
}

func TestDownload(t *testing.T) {
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Range") != "bytes=0-1" || r.URL.Query().Get("w") != "64" {
			t.Errorf("Range = %q, w = %q", r.Header.Get("Range"), r.URL.Query().Get("w"))
		}
		w.WriteHeader(206)
		io.WriteString(w, "he")
	})
	c := NewClient(WithBaseURL(f.URL), WithTimeout(50*time.Millisecond))
	rc, err := c.Files.Download(context.Background(), "f1", FilesDownloadParams{Range: "bytes=0-1", W: 64})
	if err != nil {
		t.Fatal(err)
	}
	time.Sleep(100 * time.Millisecond) // 超过 timeout 也还能读：下载的超时只管到响应头
	b, err := io.ReadAll(rc)
	rc.Close()
	if err != nil || string(b) != "he" {
		t.Fatalf("%q %v", b, err)
	}
}

func TestImportCSV(t *testing.T) {
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, 201, map[string]any{"page": map[string]any{"id": "db"}, "schema": map[string]any{"props": []any{}}, "views": []any{}, "seq": 1})
	})
	c := NewClient(WithBaseURL(f.URL))
	db, err := c.Databases.ImportCSV(context.Background(), "ws", strings.NewReader("a,b\n1,2\n"), DatabasesImportCSVParams{Title: "表"})
	if err != nil || db.Page.ID != "db" {
		t.Fatalf("%v %v", db, err)
	}
	r, body := f.last()
	if r.URL.Query().Get("title") != "表" || !strings.HasPrefix(r.Header.Get("Content-Type"), "text/csv") || string(body) != "a,b\n1,2\n" {
		t.Fatalf("%s %s %q", r.URL, r.Header.Get("Content-Type"), body)
	}
}

func TestEncodeURIComponent(t *testing.T) {
	for in, want := range map[string]string{
		"王小明":         "%E7%8E%8B%E5%B0%8F%E6%98%8E",
		"a b/c?d=e&f": "a%20b%2Fc%3Fd%3De%26f",
		"-_.!~*'()":   "-_.!~*'()",
	} {
		if got := encodeURIComponent(in); got != want {
			t.Errorf("%q → %q，应该是 %q", in, got, want)
		}
	}
}
