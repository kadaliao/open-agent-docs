package oad

import (
	"archive/tar"
	"bufio"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
)

// StartOptions 是 Start 的参数。
type StartOptions struct {
	// Binary 是服务器二进制的路径。不给就看环境变量 OAD_BINARY，再没有就用缓存的，缓存里也没有就从 GitHub release 下载。
	Binary string
	// Version 是要下载的服务器版本，默认和 SDK 同版本（Version）。
	Version string

	// DataDir → OAD_DATA_DIR（数据目录；不给是服务器的默认 ./data）。
	DataDir string
	// Port → OAD_PORT；不给就挑一个空端口。
	Port int
	// Auth → OAD_AUTH：local（默认）、header、jwt、none。
	Auth string
	// HeaderSecret → OAD_HEADER_SECRET（Auth 是 header 时）。
	HeaderSecret string
	// JWTSecret → OAD_JWT_SECRET（Auth 是 jwt 时）。
	JWTSecret string
	// Env 是别的配置：键转成 OAD_<键大写>（"public_url" → OAD_PUBLIC_URL），已经是 OAD_ 开头的照原样。
	Env map[string]string

	// Stdout、Stderr 接服务器的输出，nil 就丢掉（stderr 的末尾总会留着，起不来时放进错误信息）。
	Stdout, Stderr io.Writer
	// StartTimeout 是等服务器起来的时间，默认 30 秒。
	StartTimeout time.Duration
}

// Server 是 Start 起来的服务器。用完 Close。
type Server struct {
	// URL 是服务器地址，比如 http://127.0.0.1:52341，直接给 WithBaseURL。
	URL string
	// Port 是端口。
	Port int

	cmd     *exec.Cmd
	exited  chan struct{}
	waitErr error
	tail    *tailBuffer
	once    sync.Once
	cerr    error
}

// Start 在本机起一个 Open Agent Docs 服务器（开发、测试用），等它能用了再返回。
//
//	srv, err := oad.Start(ctx, oad.StartOptions{DataDir: dir, Auth: "header", HeaderSecret: "s3cret"})
//	if err != nil { … }
//	defer srv.Close()
//	client := oad.NewClient(oad.WithBaseURL(srv.URL), oad.WithHeaderSecret("s3cret"))
//
// 子进程是 `<binary> serve`，配置都走环境变量。等 GET /api/v1/auth/config 回 200（最多 StartTimeout）；
// 期间进程退出了，错误里带上它 stderr 的末尾。ctx 只管启动（下载、等待），服务器起来以后不受它影响。
func Start(ctx context.Context, opts StartOptions) (*Server, error) {
	bin, err := resolveBinary(ctx, opts)
	if err != nil {
		return nil, err
	}
	port := opts.Port
	if port == 0 {
		if port, err = freePort(); err != nil {
			return nil, fmt.Errorf("oad: 找空端口失败: %w", err)
		}
	}
	env := map[string]string{"OAD_HOST": "127.0.0.1"}
	for k, v := range opts.Env {
		k = strings.ToUpper(k)
		if !strings.HasPrefix(k, "OAD_") {
			k = "OAD_" + k
		}
		env[k] = v
	}
	set := func(k, v string) {
		if v != "" {
			env[k] = v
		}
	}
	set("OAD_DATA_DIR", opts.DataDir)
	set("OAD_AUTH", opts.Auth)
	set("OAD_HEADER_SECRET", opts.HeaderSecret)
	set("OAD_JWT_SECRET", opts.JWTSecret)
	env["OAD_PORT"] = strconv.Itoa(port)

	cmd := exec.Command(bin, "serve")
	cmd.Env = os.Environ()
	for _, k := range sortedMapKeys(env) {
		cmd.Env = append(cmd.Env, k+"="+env[k])
	}
	tail := &tailBuffer{max: 8 << 10}
	cmd.Stdout = teeOrTail(opts.Stdout, tail)
	cmd.Stderr = teeOrTail(opts.Stderr, tail)
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("oad: 启动 %s 失败: %w", bin, err)
	}
	s := &Server{URL: "http://127.0.0.1:" + strconv.Itoa(port), Port: port, cmd: cmd, exited: make(chan struct{}), tail: tail}
	go func() {
		s.waitErr = cmd.Wait()
		close(s.exited)
	}()

	limit := opts.StartTimeout
	if limit <= 0 {
		limit = 30 * time.Second
	}
	deadline := time.Now().Add(limit)
	hc := &http.Client{Timeout: 2 * time.Second}
	for {
		select {
		case <-s.exited:
			return nil, fmt.Errorf("oad: 服务器没起来就退出了（%v）：\n%s", s.waitErr, tail.String())
		case <-ctx.Done():
			s.Close()
			return nil, ctx.Err()
		default:
		}
		if resp, err := hc.Get(s.URL + "/api/v1/auth/config"); err == nil {
			io.Copy(io.Discard, resp.Body)
			resp.Body.Close()
			if resp.StatusCode == http.StatusOK {
				return s, nil
			}
		}
		if time.Now().After(deadline) {
			s.Close()
			return nil, fmt.Errorf("oad: 服务器 %s 内没起来：\n%s", limit, tail.String())
		}
		select {
		case <-s.exited:
		case <-ctx.Done():
		case <-time.After(50 * time.Millisecond):
		}
	}
}

// Close 关掉服务器：先 SIGTERM（等打开的页面都存好），10 秒内没退出再强制结束。可以调多次。
func (s *Server) Close() error {
	s.once.Do(func() {
		select {
		case <-s.exited:
			return
		default:
		}
		if err := s.cmd.Process.Signal(syscall.SIGTERM); err != nil {
			_ = s.cmd.Process.Kill()
		}
		select {
		case <-s.exited:
		case <-time.After(10 * time.Second):
			_ = s.cmd.Process.Kill()
			<-s.exited
			s.cerr = errors.New("oad: 服务器 10 秒内没退出，强制结束了")
		}
	})
	return s.cerr
}

func freePort() (int, error) {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return 0, err
	}
	defer l.Close()
	return l.Addr().(*net.TCPAddr).Port, nil
}

func sortedMapKeys(m map[string]string) []string {
	ks := make([]string, 0, len(m))
	for k := range m {
		ks = append(ks, k)
	}
	sort.Strings(ks)
	return ks
}

// tailBuffer 只留最后 max 个字节。
type tailBuffer struct {
	mu  sync.Mutex
	max int
	buf []byte
}

func (t *tailBuffer) Write(p []byte) (int, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.buf = append(t.buf, p...)
	if len(t.buf) > t.max {
		t.buf = append([]byte(nil), t.buf[len(t.buf)-t.max:]...)
	}
	return len(p), nil
}

func (t *tailBuffer) String() string {
	t.mu.Lock()
	defer t.mu.Unlock()
	return strings.TrimSpace(string(t.buf))
}

func teeOrTail(w io.Writer, tail *tailBuffer) io.Writer {
	if w == nil {
		return tail
	}
	return io.MultiWriter(w, tail)
}

// ---------- 二进制从哪来 ----------

// releaseBaseURL 是 GitHub release 的下载地址（测试时换成假的）。
var releaseBaseURL = "https://github.com/kadaliao/open-agent-docs/releases/download"

// cacheRoot 是缓存目录（测试时换掉）。
var cacheRoot = func() (string, error) {
	d, err := os.UserCacheDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(d, "open-agent-docs"), nil
}

func releaseTarget() (string, error) {
	switch runtime.GOOS + "/" + runtime.GOARCH {
	case "darwin/arm64":
		return "aarch64-apple-darwin", nil
	case "darwin/amd64":
		return "x86_64-apple-darwin", nil
	case "linux/amd64":
		return "x86_64-unknown-linux-gnu", nil
	case "linux/arm64":
		return "aarch64-unknown-linux-gnu", nil
	}
	return "", fmt.Errorf("oad: 没有 %s/%s 的服务器二进制；用 Docker（ghcr.io/kadaliao/open-agent-docs）跑服务器，或者用 StartOptions.Binary / OAD_BINARY 指定自己编的", runtime.GOOS, runtime.GOARCH)
}

// resolveBinary：Binary → OAD_BINARY → 缓存 → 下载。
func resolveBinary(ctx context.Context, opts StartOptions) (string, error) {
	if opts.Binary != "" {
		return opts.Binary, nil
	}
	if b := os.Getenv("OAD_BINARY"); b != "" {
		return b, nil
	}
	version := strings.TrimPrefix(opts.Version, "v")
	if version == "" {
		version = Version
	}
	target, err := releaseTarget()
	if err != nil {
		return "", err
	}
	root, err := cacheRoot()
	if err != nil {
		return "", fmt.Errorf("oad: 找不到缓存目录: %w", err)
	}
	dir := filepath.Join(root, "v"+version)
	bin := filepath.Join(dir, "open-agent-docs")
	if st, err := os.Stat(bin); err == nil && st.Mode().IsRegular() {
		return bin, nil
	}
	if err := downloadBinary(ctx, version, target, dir, opts.Stderr); err != nil {
		return "", err
	}
	return bin, nil
}

// downloadBinary 下载 open-agent-docs-v<版本>-<target>.tar.gz，用同一个 release 的 SHA256SUMS 验，解出二进制放进 dir。
func downloadBinary(ctx context.Context, version, target, dir string, log io.Writer) error {
	name := fmt.Sprintf("open-agent-docs-v%s-%s.tar.gz", version, target)
	base := releaseBaseURL + "/v" + version + "/"
	if log != nil {
		fmt.Fprintf(log, "oad: 下载服务器 %s\n", base+name)
	}
	sums, err := httpGet(ctx, base+"SHA256SUMS")
	if err != nil {
		return err
	}
	want, err := findSum(sums, name)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	archive, err := os.CreateTemp(dir, ".download-*.tar.gz")
	if err != nil {
		return err
	}
	defer os.Remove(archive.Name())
	defer archive.Close()
	body, err := httpOpen(ctx, base+name)
	if err != nil {
		return err
	}
	h := sha256.New()
	_, err = io.Copy(io.MultiWriter(archive, h), body)
	body.Close()
	if err != nil {
		return fmt.Errorf("oad: 下载 %s 失败: %w", name, err)
	}
	if got := hex.EncodeToString(h.Sum(nil)); got != want {
		return fmt.Errorf("oad: %s 的 SHA256 对不上（下载的 %s，SHA256SUMS 里是 %s）", name, got, want)
	}
	if _, err := archive.Seek(0, io.SeekStart); err != nil {
		return err
	}
	return extractBinary(archive, filepath.Join(dir, "open-agent-docs"))
}

func findSum(sums []byte, name string) (string, error) {
	sc := bufio.NewScanner(strings.NewReader(string(sums)))
	for sc.Scan() {
		f := strings.Fields(sc.Text())
		if len(f) == 2 && strings.TrimPrefix(f[1], "*") == name {
			return strings.ToLower(f[0]), nil
		}
	}
	return "", fmt.Errorf("oad: SHA256SUMS 里没有 %s", name)
}

// extractBinary 从 tar.gz 里找名字是 open-agent-docs 的文件，写到 dest（先写临时文件再改名，别的进程不会看到一半）。
func extractBinary(r io.Reader, dest string) error {
	gz, err := gzip.NewReader(r)
	if err != nil {
		return fmt.Errorf("oad: 解压失败: %w", err)
	}
	defer gz.Close()
	tr := tar.NewReader(gz)
	for {
		hdr, err := tr.Next()
		if err == io.EOF {
			return errors.New("oad: 压缩包里没有 open-agent-docs")
		}
		if err != nil {
			return fmt.Errorf("oad: 解压失败: %w", err)
		}
		if hdr.Typeflag != tar.TypeReg || path.Base(hdr.Name) != "open-agent-docs" {
			continue
		}
		tmp, err := os.CreateTemp(filepath.Dir(dest), ".open-agent-docs-*")
		if err != nil {
			return err
		}
		_, err = io.Copy(tmp, tr)
		if cerr := tmp.Close(); err == nil {
			err = cerr
		}
		if err == nil {
			err = os.Chmod(tmp.Name(), 0o755)
		}
		if err == nil {
			err = os.Rename(tmp.Name(), dest)
		}
		if err != nil {
			os.Remove(tmp.Name())
			return fmt.Errorf("oad: 写 %s 失败: %w", dest, err)
		}
		return nil
	}
}

func httpOpen(ctx context.Context, u string) (io.ReadCloser, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", "open-agent-docs-go/"+Version)
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("oad: 下载 %s 失败: %w", u, err)
	}
	if resp.StatusCode != http.StatusOK {
		resp.Body.Close()
		return nil, fmt.Errorf("oad: 下载 %s 失败: HTTP %d", u, resp.StatusCode)
	}
	return resp.Body, nil
}

func httpGet(ctx context.Context, u string) ([]byte, error) {
	body, err := httpOpen(ctx, u)
	if err != nil {
		return nil, err
	}
	defer body.Close()
	return io.ReadAll(io.LimitReader(body, 1<<20))
}
