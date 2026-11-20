package oad

import (
	"context"
	"errors"
	"net/http"
	"strings"
	"testing"
	"time"
)

func meHandler(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, 200, map[string]any{
		"user":       map[string]any{"id": "u", "name": "n", "kind": "user"},
		"workspaces": []any{map[string]any{"id": "ws-first", "name": "A", "role": "owner"}, map[string]any{"id": "ws-2", "name": "B", "role": "editor"}},
	})
}

func TestWithUserHeader(t *testing.T) {
	f := newFake(t, meHandler)
	admin := NewClient(WithBaseURL(f.URL), WithHeaderSecret("s3cret"), WithToken("oad_admin"))
	u, err := admin.WithUser("u-42", UserName("王小明"), UserEmail("x@example.com"))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := u.Me.Get(context.Background()); err != nil {
		t.Fatal(err)
	}
	r, _ := f.last()
	for k, want := range map[string]string{
		"X-OAD-Secret": "s3cret", "X-OAD-User-Id": "u-42", "X-OAD-User-Name": "%E7%8E%8B%E5%B0%8F%E6%98%8E",
		"X-OAD-User-Email": "x@example.com", "Authorization": "",
	} {
		if got := r.Header.Get(k); got != want {
			t.Errorf("%s = %q，应该是 %q", k, got, want)
		}
	}
	// 原来的客户端不变
	if _, err := admin.Me.Get(context.Background()); err != nil {
		t.Fatal(err)
	}
	r, _ = f.last()
	if r.Header.Get("X-OAD-User-Id") != "" || r.Header.Get("Authorization") != "Bearer oad_admin" {
		t.Fatalf("原来的客户端变了：%v", r.Header)
	}
	if _, err := admin.WithUser("u", UserWorkspaces(WorkspaceGrant{ID: "ws"})); !errors.Is(err, ErrConfig) {
		t.Fatalf("header 模式给工作区应该报错：%v", err)
	}
}

func TestWithUserJWT(t *testing.T) {
	f := newFake(t, meHandler)
	admin := NewClient(WithBaseURL(f.URL), WithJWTSecret("jwt-secret"), WithJWTIssuer("https://app"), WithJWTAudience("oad"), WithJWTTTL(10*time.Minute))
	u, err := admin.WithUser("u9", UserName("小九"), UserWorkspaces(WorkspaceGrant{ID: "ws1"}, WorkspaceGrant{ID: "ws2", Role: "viewer"}))
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	u.Me.Get(ctx)
	r1, _ := f.last()
	u.Me.Get(ctx)
	r2, _ := f.last()
	tok, ok := strings.CutPrefix(r1.Header.Get("Authorization"), "Bearer ")
	if !ok || r2.Header.Get("Authorization") != r1.Header.Get("Authorization") {
		t.Fatal("JWT 没缓存")
	}
	if !verifyHS256(tok, "jwt-secret") {
		t.Fatal("签名不对")
	}
	_, claims := decodeJWT(t, tok)
	if claims["sub"] != "u9" || claims["name"] != "小九" || claims["iss"] != "https://app" || claims["aud"] != "oad" {
		t.Fatalf("claims = %v", claims)
	}
	if claims["exp"].(float64)-claims["iat"].(float64) != 600 {
		t.Fatalf("ttl 不对：%v", claims)
	}
	ws := claims["workspaces"].([]any)
	if ws[0] != "ws1" || ws[1].(map[string]any)["role"] != "viewer" {
		t.Fatalf("workspaces = %v", ws)
	}

	// 快过期了：换新的
	u.core.jwtExp = time.Now().Add(30 * time.Second)
	old := u.core.jwt
	u.core.jwt = old + "x"
	u.Me.Get(ctx)
	r3, _ := f.last()
	if r3.Header.Get("Authorization") == "Bearer "+old+"x" {
		t.Fatal("快过期的 JWT 没换")
	}
}

func TestWithUserConfigErrors(t *testing.T) {
	for name, c := range map[string]*Client{
		"都没配":  NewClient(),
		"都配了":  NewClient(WithHeaderSecret("a"), WithJWTSecret("b")),
		"空的id": NewClient(WithHeaderSecret("a")),
	} {
		id := "u"
		if name == "空的id" {
			id = ""
		}
		if _, err := c.WithUser(id); !errors.Is(err, ErrConfig) {
			t.Errorf("%s：%v", name, err)
		}
	}
}

func TestDefaultWorkspace(t *testing.T) {
	f := newFake(t, meHandler)
	u, _ := NewClient(WithBaseURL(f.URL), WithHeaderSecret("s")).WithUser("u")
	for range 3 {
		ws, err := u.DefaultWorkspace(context.Background())
		if err != nil || ws != "ws-first" {
			t.Fatalf("%q %v", ws, err)
		}
	}
	if f.n.Load() != 1 {
		t.Fatalf("/me 查了 %d 次", f.n.Load())
	}
}

func TestSignJWTErrors(t *testing.T) {
	if _, err := SignJWT("", "u", JWTOptions{}); !errors.Is(err, ErrConfig) {
		t.Fatal(err)
	}
	tok, err := SignJWT("s", "u", JWTOptions{Now: time.Unix(100, 0)})
	if err != nil {
		t.Fatal(err)
	}
	_, c := decodeJWT(t, tok)
	if len(c) != 3 || c["exp"] != float64(3700) {
		t.Fatalf("没给的不该写：%v", c)
	}
}
