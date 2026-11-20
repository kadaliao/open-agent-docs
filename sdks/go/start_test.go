package oad

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func fakeTarball(t *testing.T, dir string, content []byte) []byte {
	var buf bytes.Buffer
	gz := gzip.NewWriter(&buf)
	tw := tar.NewWriter(gz)
	for _, f := range []struct {
		name string
		body []byte
	}{{dir + "/README.md", []byte("readme")}, {dir + "/open-agent-docs", content}} {
		if err := tw.WriteHeader(&tar.Header{Name: f.name, Mode: 0o755, Size: int64(len(f.body)), Typeflag: tar.TypeReg}); err != nil {
			t.Fatal(err)
		}
		tw.Write(f.body)
	}
	tw.Close()
	gz.Close()
	return buf.Bytes()
}

func TestResolveBinaryDownload(t *testing.T) {
	target, err := releaseTarget()
	if err != nil {
		t.Skip(err)
	}
	t.Setenv("OAD_BINARY", "")
	name := fmt.Sprintf("open-agent-docs-v9.9.9-%s", target)
	tarball := fakeTarball(t, name, []byte("#!/bin/sh\necho hi\n"))
	sum := sha256.Sum256(tarball)
	sums := hex.EncodeToString(sum[:]) + "  " + name + ".tar.gz\n"
	bad := false
	f := newFake(t, func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/v9.9.9/SHA256SUMS":
			if bad {
				w.Write([]byte(strings.Repeat("0", 64) + "  " + name + ".tar.gz\n"))
				return
			}
			w.Write([]byte(sums))
		case "/v9.9.9/" + name + ".tar.gz":
			w.Write(tarball)
		default:
			http.NotFound(w, r)
		}
	})
	oldBase, oldCache := releaseBaseURL, cacheRoot
	defer func() { releaseBaseURL, cacheRoot = oldBase, oldCache }()
	releaseBaseURL = f.URL
	cache := t.TempDir()
	cacheRoot = func() (string, error) { return cache, nil }

	ctx := context.Background()
	bin, err := resolveBinary(ctx, StartOptions{Version: "v9.9.9"})
	if err != nil {
		t.Fatal(err)
	}
	if bin != filepath.Join(cache, "v9.9.9", "open-agent-docs") {
		t.Fatalf("bin = %s", bin)
	}
	b, _ := os.ReadFile(bin)
	st, _ := os.Stat(bin)
	if string(b) != "#!/bin/sh\necho hi\n" || st.Mode().Perm()&0o100 == 0 {
		t.Fatalf("解出来的不对：%q %v", b, st.Mode())
	}
	// 第二次走缓存，不下载
	n := f.n.Load()
	if _, err := resolveBinary(ctx, StartOptions{Version: "9.9.9"}); err != nil || f.n.Load() != n {
		t.Fatalf("没走缓存：%v", err)
	}
	// Binary、OAD_BINARY 优先
	if b, _ := resolveBinary(ctx, StartOptions{Binary: "/x"}); b != "/x" {
		t.Fatal(b)
	}
	t.Setenv("OAD_BINARY", "/y")
	if b, _ := resolveBinary(ctx, StartOptions{}); b != "/y" {
		t.Fatal(b)
	}
	t.Setenv("OAD_BINARY", "")

	// SHA256 对不上：报错，不留文件
	bad = true
	cache = t.TempDir()
	if _, err := resolveBinary(ctx, StartOptions{Version: "9.9.9"}); err == nil || !strings.Contains(err.Error(), "SHA256") {
		t.Fatalf("err = %v", err)
	}
	if _, err := os.Stat(filepath.Join(cache, "v9.9.9", "open-agent-docs")); err == nil {
		t.Fatal("校验失败还留下了二进制")
	}
}

func TestStartFailsFast(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "fake-server")
	if err := os.WriteFile(bin, []byte("#!/bin/sh\necho 'boom: 端口被占了' >&2\nexit 3\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	_, err := Start(context.Background(), StartOptions{Binary: bin})
	if err == nil || !strings.Contains(err.Error(), "boom: 端口被占了") {
		t.Fatalf("err = %v", err)
	}
}
