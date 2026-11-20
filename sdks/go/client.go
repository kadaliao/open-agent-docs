package oad

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/rand/v2"
	"net"
	"net/http"
	"net/url"
	"os"
	"strings"
	"sync"
	"time"
)

// DefaultBaseURL 是没给 WithBaseURL、也没有环境变量 OAD_BASE_URL 时连的地址。
const DefaultBaseURL = "http://127.0.0.1:8848"

type config struct {
	baseURL      string
	token        string
	headerSecret string
	jwtSecret    string
	jwtIssuer    string
	jwtAudience  string
	jwtTTL       time.Duration
	timeout      time.Duration
	maxRetries   int
	locale       string
	headers      http.Header
	httpClient   *http.Client

	// 测试用：事件流第一次重连等多久、重试的等待
	sseMinRetry time.Duration
	sleep       func(ctx context.Context, d time.Duration) error
}

// Option 是 NewClient 的选项。
type Option func(*config)

// WithBaseURL 设服务器地址，比如 https://docs.example.com（末尾的 / 去掉，/api/v1 由 SDK 拼）。
// 不给就读环境变量 OAD_BASE_URL，再没有就是 http://127.0.0.1:8848。
func WithBaseURL(u string) Option { return func(c *config) { c.baseURL = u } }

// WithToken 设 token：Agent token（oad_…）、会话令牌或 JWT，带成 Authorization: Bearer。不给就读 OAD_TOKEN。
func WithToken(t string) Option { return func(c *config) { c.token = t } }

// WithHeaderSecret 设 OAD_AUTH=header 的服务器的密钥（X-OAD-Secret），配合 Client.WithUser 用。
func WithHeaderSecret(s string) Option { return func(c *config) { c.headerSecret = s } }

// WithJWTSecret 设 OAD_AUTH=jwt 的服务器的 HS256 共享密钥，配合 Client.WithUser 用（SDK 替用户签 JWT）。
func WithJWTSecret(s string) Option { return func(c *config) { c.jwtSecret = s } }

// WithJWTIssuer 设 WithUser 签的 JWT 的 iss（服务器配了 OAD_JWT_ISSUER 时要一样）。
func WithJWTIssuer(iss string) Option { return func(c *config) { c.jwtIssuer = iss } }

// WithJWTAudience 设 WithUser 签的 JWT 的 aud（服务器配了 OAD_JWT_AUDIENCE 时要一样）。
func WithJWTAudience(aud string) Option { return func(c *config) { c.jwtAudience = aud } }

// WithJWTTTL 设 WithUser 签的 JWT 多久过期，默认 1 小时。快过期（剩不到 60 秒）时自动换新的。
func WithJWTTTL(d time.Duration) Option { return func(c *config) { c.jwtTTL = d } }

// WithTimeout 设单个请求的超时，默认 60 秒。事件流和下载只管到收到响应头，之后不限时。
func WithTimeout(d time.Duration) Option { return func(c *config) { c.timeout = d } }

// WithMaxRetries 设最多重试几次，默认 2；0 是不重试。
// 只在这些情况重试：429；GET、PUT、DELETE 遇到 502、503、504 或连接断了；连不上（请求还没发出去）。
// 等待：有 Retry-After 照它（最多 60 秒），没有就 0.5 秒 × 2^n（最多 8 秒）加 0–25% 的随机。
func WithMaxRetries(n int) Option { return func(c *config) { c.maxRetries = n } }

// WithLocale 设服务器错误信息的语言：zh 或 en（Accept-Language）。
func WithLocale(l string) Option { return func(c *config) { c.locale = l } }

// WithHeader 让每个请求多带一个头（可以给多次）。
func WithHeader(key, value string) Option {
	return func(c *config) {
		if c.headers == nil {
			c.headers = http.Header{}
		}
		c.headers.Add(key, value)
	}
}

// WithHTTPClient 换掉 HTTP 客户端（代理、自定义 Transport、测试）。不给就用一个新的 &http.Client{}。
// 它自己的 Timeout 也会生效，最好留空，用 WithTimeout。
func WithHTTPClient(h *http.Client) Option { return func(c *config) { c.httpClient = h } }

// core 是客户端的内部状态：配置（多个 Client 共享）、身份、缓存。
type core struct {
	cfg  *config
	user *userIdentity

	jwtMu  sync.Mutex
	jwt    string
	jwtExp time.Time

	wsMu sync.Mutex
	ws   string
}

// NewClient 建一个客户端。没给的选项读环境变量：OAD_BASE_URL、OAD_TOKEN。
//
//	client := oad.NewClient(oad.WithBaseURL("https://docs.example.com"), oad.WithToken(os.Getenv("OAD_TOKEN")))
//	pages, err := client.Pages.List(ctx, ws)
func NewClient(opts ...Option) *Client {
	cfg := &config{
		baseURL:     os.Getenv("OAD_BASE_URL"),
		token:       os.Getenv("OAD_TOKEN"),
		jwtTTL:      time.Hour,
		timeout:     60 * time.Second,
		maxRetries:  2,
		sseMinRetry: time.Second,
		sleep:       sleepCtx,
	}
	for _, o := range opts {
		if o != nil {
			o(cfg)
		}
	}
	if cfg.baseURL == "" {
		cfg.baseURL = DefaultBaseURL
	}
	cfg.baseURL = strings.TrimRight(cfg.baseURL, "/")
	if cfg.httpClient == nil {
		cfg.httpClient = &http.Client{}
	}
	if cfg.jwtTTL <= 0 {
		cfg.jwtTTL = time.Hour
	}
	if cfg.maxRetries < 0 {
		cfg.maxRetries = 0
	}
	return newClient(&core{cfg: cfg})
}

func newClient(c *core) *Client {
	cl := &Client{core: c}
	cl.initResources()
	return cl
}

// BaseURL 是这个客户端连的服务器地址（没有末尾的 /）。
func (c *Client) BaseURL() string { return c.core.cfg.baseURL }

// ---------- 请求 ----------

// request 是生成的方法交给 core 的一个请求。
type request struct {
	method, path string
	query        url.Values
	header       http.Header
	body         []byte
	reader       io.Reader
	readerStart  int64
	readerUsed   bool
	contentType  string
}

func (r *request) setHeader(k, v string) {
	if r.header == nil {
		r.header = http.Header{}
	}
	r.header.Set(k, v)
}

func (r *request) setJSON(v any) error {
	b, err := json.Marshal(v)
	if err != nil {
		return fmt.Errorf("oad: %s %s: 请求体转 JSON 失败: %w", r.method, r.path, err)
	}
	r.body, r.contentType = b, "application/json"
	return nil
}

func (r *request) setReader(rd io.Reader, contentType string) {
	r.reader, r.contentType = rd, contentType
}

var errNotReplayable = errors.New("请求体不能重放")

// bodyReader 返回这次要发的正文。io.Reader 只能读一次：能 Seek 的倒回去重发，不能的就不重试了。
func (r *request) bodyReader() (io.Reader, error) {
	if r.reader == nil {
		if r.body == nil {
			return nil, nil
		}
		return bytes.NewReader(r.body), nil
	}
	s, seekable := r.reader.(io.Seeker)
	if !r.readerUsed {
		r.readerUsed = true
		if seekable {
			if pos, err := s.Seek(0, io.SeekCurrent); err == nil {
				r.readerStart = pos
			} else {
				seekable = false
			}
		}
		return readerWithLen(r.reader), nil
	}
	if !seekable {
		return nil, errNotReplayable
	}
	if _, err := s.Seek(r.readerStart, io.SeekStart); err != nil {
		return nil, errNotReplayable
	}
	return readerWithLen(r.reader), nil
}

// 包一层只留 Read：*os.File 之类本身是 ReadCloser，直接给 http.Client 会被它关掉（就不能倒回去重发了，调用方的文件也被关了）。
// 长度未知时按块传，服务器照样收。
func readerWithLen(r io.Reader) io.Reader {
	switch r.(type) {
	case *bytes.Reader, *bytes.Buffer, *strings.Reader:
		return r
	}
	return struct{ io.Reader }{r}
}

func (r *request) clone() *request {
	n := *r
	if r.query != nil {
		n.query = url.Values{}
		for k, v := range r.query {
			n.query[k] = append([]string(nil), v...)
		}
	}
	if r.header != nil {
		n.header = r.header.Clone()
	}
	return &n
}

func (c *core) url(r *request) string {
	u := c.cfg.baseURL + "/api/v1" + r.path
	if len(r.query) > 0 {
		u += "?" + r.query.Encode()
	}
	return u
}

func (c *core) userAgent() string { return "open-agent-docs-go/" + Version }

// 每个请求都带的头。
func (c *core) setHeaders(h *http.Request, r *request, accept string) error {
	for k, vs := range c.cfg.headers {
		for _, v := range vs {
			h.Header.Add(k, v)
		}
	}
	h.Header.Set("User-Agent", c.userAgent())
	h.Header.Set("Accept", accept)
	if l := c.cfg.locale; l != "" {
		switch l {
		case "zh":
			l = "zh-CN"
		}
		h.Header.Set("Accept-Language", l)
	}
	if err := c.auth(h.Header); err != nil {
		return err
	}
	for k, vs := range r.header {
		h.Header[k] = vs
	}
	if r.contentType != "" {
		h.Header.Set("Content-Type", r.contentType)
	}
	return nil
}

// response 是 send 拿到的成功响应；用完调 done。
type response struct {
	*http.Response
	ctx  context.Context
	done func()
}

// send 发请求，按 §5 的规则重试。返回 2xx 的响应；≥400 返回 *APIError。
// stream 为 true 时超时只管到响应头（事件流、下载）。retries < 0 用配置里的。
func (c *core) send(ctx context.Context, r *request, accept string, stream bool, retries int) (*response, error) {
	if retries < 0 {
		retries = c.cfg.maxRetries
	}
	full := c.url(r)
	var lastErr error
	for attempt := 0; ; attempt++ {
		body, err := r.bodyReader()
		if err != nil {
			if lastErr != nil {
				return nil, lastErr
			}
			return nil, err
		}
		actx, cancel := context.WithCancelCause(ctx)
		var timer *time.Timer
		if c.cfg.timeout > 0 {
			timer = time.AfterFunc(c.cfg.timeout, func() { cancel(errAttemptTimeout) })
		}
		stop := func() {
			if timer != nil {
				timer.Stop()
			}
		}
		hr, err := http.NewRequestWithContext(actx, r.method, full, body)
		if err != nil {
			stop()
			cancel(nil)
			return nil, fmt.Errorf("oad: %s %s: %w", r.method, r.path, err)
		}
		if err := c.setHeaders(hr, r, accept); err != nil {
			stop()
			cancel(nil)
			return nil, err
		}
		resp, err := c.cfg.httpClient.Do(hr)
		if err != nil {
			stop()
			cerr := c.transportError(ctx, actx, r, full, err)
			cancel(nil)
			if ctx.Err() != nil {
				return nil, cerr
			}
			if attempt < retries && retryableConnErr(r.method, err, actx) {
				lastErr = cerr
				if err := c.cfg.sleep(ctx, backoff(attempt)); err != nil {
					return nil, cerr
				}
				continue
			}
			return nil, cerr
		}
		if resp.StatusCode >= 400 {
			data, rerr := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
			resp.Body.Close()
			stop()
			if rerr != nil && actx.Err() != nil {
				cerr := c.transportError(ctx, actx, r, full, rerr)
				cancel(nil)
				return nil, cerr
			}
			cancel(nil)
			apiErr := newAPIError(r.method, "/api/v1"+r.path, resp, data)
			if attempt < retries && retryableStatus(r.method, resp.StatusCode) {
				lastErr = apiErr
				wait := backoff(attempt)
				if apiErr.RetryAfter > 0 {
					wait = min(apiErr.RetryAfter, 60*time.Second)
				}
				if err := c.cfg.sleep(ctx, wait); err != nil {
					return nil, apiErr
				}
				continue
			}
			return nil, apiErr
		}
		if stream {
			stop()
		}
		return &response{Response: resp, ctx: actx, done: func() { stop(); cancel(nil) }}, nil
	}
}

var errAttemptTimeout = errors.New("oad: 单个请求超时")

func (c *core) transportError(parent, actx context.Context, r *request, full string, err error) error {
	if parent.Err() != nil {
		// 调用方取消的：原样往上给（errors.Is(err, context.Canceled) 能用）
		return fmt.Errorf("oad: %s %s: %w", r.method, r.path, err)
	}
	timeout := context.Cause(actx) == errAttemptTimeout
	var ne net.Error
	if errors.As(err, &ne) && ne.Timeout() {
		timeout = true
	}
	if timeout {
		return &ConnectionError{Method: r.method, URL: full, Timeout: true, Err: fmt.Errorf("%w（%s 内没有响应）", context.DeadlineExceeded, c.cfg.timeout)}
	}
	return &ConnectionError{Method: r.method, URL: full, Err: err}
}

func idempotent(m string) bool {
	return m == http.MethodGet || m == http.MethodPut || m == http.MethodDelete || m == http.MethodHead
}

func retryableStatus(method string, status int) bool {
	if status == http.StatusTooManyRequests {
		return true
	}
	// POST、PATCH 遇到 5xx 不重试：可能已经生效了
	return (status == 502 || status == 503 || status == 504) && idempotent(method)
}

func retryableConnErr(method string, err error, actx context.Context) bool {
	if context.Cause(actx) == errAttemptTimeout {
		return false
	}
	var ne net.Error
	if errors.As(err, &ne) && ne.Timeout() {
		return false
	}
	if idempotent(method) {
		return true
	}
	// 请求还没发出去（连不上）：什么方法都能重试
	var op *net.OpError
	if errors.As(err, &op) && op.Op == "dial" {
		return true
	}
	var dns *net.DNSError
	return errors.As(err, &dns)
}

// backoff：0.5 秒 × 2^n，最多 8 秒，加 0–25% 的随机。
func backoff(attempt int) time.Duration {
	d := 500 * time.Millisecond << attempt
	if d > 8*time.Second || d <= 0 {
		d = 8 * time.Second
	}
	return d + time.Duration(rand.Float64()*0.25*float64(d))
}

func sleepCtx(ctx context.Context, d time.Duration) error {
	t := time.NewTimer(d)
	defer t.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-t.C:
		return nil
	}
}

// do 发请求，把 JSON 响应解到 out（nil 就丢掉响应体）。
func (c *core) do(ctx context.Context, r *request, out any) error {
	resp, err := c.send(ctx, r, "application/json", false, -1)
	if err != nil {
		return err
	}
	defer resp.done()
	defer resp.Body.Close()
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return c.transportError(ctx, resp.ctx, r, c.url(r), err)
	}
	if out == nil || resp.StatusCode == http.StatusNoContent || len(bytes.TrimSpace(data)) == 0 {
		return nil
	}
	if err := json.Unmarshal(data, out); err != nil {
		return fmt.Errorf("oad: %s %s: 解析响应失败: %w", r.method, r.path, err)
	}
	return nil
}

// download 返回响应正文（调用方 Close）。
func (c *core) download(ctx context.Context, r *request) (io.ReadCloser, error) {
	resp, err := c.send(ctx, r, "*/*", true, -1)
	if err != nil {
		return nil, err
	}
	return &doneCloser{ReadCloser: resp.Body, done: resp.done}, nil
}

type doneCloser struct {
	io.ReadCloser
	done func()
	once sync.Once
}

func (d *doneCloser) Close() error {
	err := d.ReadCloser.Close()
	d.once.Do(d.done)
	return err
}

// encodeURIComponent 和 JS 的一样：除了 A-Z a-z 0-9 - _ . ! ~ * ' ( ) 都按 UTF-8 百分号编码。
func encodeURIComponent(s string) string {
	const hex = "0123456789ABCDEF"
	var b strings.Builder
	for i := 0; i < len(s); i++ {
		ch := s[i]
		if ch >= 'a' && ch <= 'z' || ch >= 'A' && ch <= 'Z' || ch >= '0' && ch <= '9' || strings.IndexByte("-_.!~*'()", ch) >= 0 {
			b.WriteByte(ch)
		} else {
			b.WriteByte('%')
			b.WriteByte(hex[ch>>4])
			b.WriteByte(hex[ch&15])
		}
	}
	return b.String()
}
