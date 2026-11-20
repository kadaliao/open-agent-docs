package oad

// sdks/conformance/vectors.json 的固定用例：webhook（S10）、embed_url 和 jwt（S11）。

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"net/http"
	"os"
	"reflect"
	"strings"
	"testing"
	"time"
)

func getenv(k string) string { return os.Getenv(k) }

type vectors struct {
	Webhook []struct {
		Name       string            `json:"name"`
		Secret     string            `json:"secret"`
		Headers    map[string]string `json:"headers"`
		Body       string            `json:"body"`
		NowMs      int64             `json:"now_ms"`
		ToleranceS int64             `json:"tolerance_s"`
		Expect     string            `json:"expect"`
	} `json:"webhook"`
	EmbedURL []struct {
		BaseURL string            `json:"base_url"`
		PageID  string            `json:"page_id"`
		Options map[string]string `json:"options"`
		Expect  string            `json:"expect"`
	} `json:"embed_url"`
	JWT struct {
		Secret string `json:"secret"`
		NowS   int64  `json:"now_s"`
		Input  struct {
			UserID     string `json:"user_id"`
			Name       string `json:"name"`
			Email      string `json:"email"`
			Workspaces []struct {
				ID   string `json:"id"`
				Role string `json:"role"`
			} `json:"workspaces"`
			TTLS     int64  `json:"ttl_s"`
			Issuer   string `json:"issuer"`
			Audience string `json:"audience"`
		} `json:"input"`
		ExpectHeader map[string]any `json:"expect_header"`
		ExpectClaims map[string]any `json:"expect_claims"`
	} `json:"jwt"`
}

func loadVectors(t *testing.T) *vectors {
	t.Helper()
	raw, err := os.ReadFile("../conformance/vectors.json")
	if err != nil {
		t.Fatal(err)
	}
	var v vectors
	if err := json.Unmarshal(raw, &v); err != nil {
		t.Fatal(err)
	}
	return &v
}

func testWebhookVectors(t *testing.T) {
	v := loadVectors(t)
	if len(v.Webhook) == 0 {
		t.Fatal("没有 webhook 用例")
	}
	for _, c := range v.Webhook {
		t.Run(c.Name, func(t *testing.T) {
			// 头按原样放进 map（不规范化名字），检查大小写不论
			h := http.Header{}
			for k, val := range c.Headers {
				h[k] = []string{val}
			}
			note, err := VerifyWebhookAt(c.Secret, h, []byte(c.Body), time.UnixMilli(c.NowMs), time.Duration(c.ToleranceS)*time.Second)
			if c.Expect == "ok" {
				if err != nil || note == nil {
					t.Fatalf("应该通过：%v", err)
				}
				return
			}
			var we *WebhookError
			if !errors.As(err, &we) || we.Kind != c.Expect {
				t.Fatalf("应该是 %s，得到 %v", c.Expect, err)
			}
			want := map[string]error{"signature": ErrWebhookSignature, "stale": ErrWebhookStale, "malformed": ErrWebhookMalformed}[c.Expect]
			if !errors.Is(err, want) || !errors.Is(err, ErrWebhook) {
				t.Fatalf("errors.Is 不对：%v", err)
			}
		})
	}
}

func testEmbedVectors(t *testing.T) {
	v := loadVectors(t)
	for _, c := range v.EmbedURL {
		got := EmbedURL(c.BaseURL, c.PageID, EmbedOptions{Theme: c.Options["theme"], Lang: c.Options["lang"], Token: c.Options["token"]})
		if got != c.Expect {
			t.Errorf("EmbedURL(%q, %q, %v) = %q，应该是 %q", c.BaseURL, c.PageID, c.Options, got, c.Expect)
		}
	}
}

func decodeJWT(t *testing.T, tok string) (header, claims map[string]any) {
	t.Helper()
	parts := strings.Split(tok, ".")
	if len(parts) != 3 {
		t.Fatalf("不是 JWT：%q", tok)
	}
	dec := func(s string) map[string]any {
		raw, err := base64.RawURLEncoding.DecodeString(s)
		if err != nil {
			t.Fatal(err)
		}
		var m map[string]any
		if err := json.Unmarshal(raw, &m); err != nil {
			t.Fatal(err)
		}
		return m
	}
	return dec(parts[0]), dec(parts[1])
}

func testJWTVector(t *testing.T) {
	v := loadVectors(t).JWT
	var ws []WorkspaceGrant
	for _, w := range v.Input.Workspaces {
		ws = append(ws, WorkspaceGrant{ID: w.ID, Role: w.Role})
	}
	tok, err := SignJWT(v.Secret, v.Input.UserID, JWTOptions{
		Name: v.Input.Name, Email: v.Input.Email, Workspaces: ws,
		TTL: time.Duration(v.Input.TTLS) * time.Second, Issuer: v.Input.Issuer, Audience: v.Input.Audience,
		Now: time.Unix(v.NowS, 0),
	})
	if err != nil {
		t.Fatal(err)
	}
	h, c := decodeJWT(t, tok)
	if !reflect.DeepEqual(h, v.ExpectHeader) {
		t.Errorf("header = %v，应该是 %v", h, v.ExpectHeader)
	}
	if !reflect.DeepEqual(c, v.ExpectClaims) {
		t.Errorf("claims = %v，应该是 %v", c, v.ExpectClaims)
	}
	// 签名：HS256 能自己验
	if !verifyHS256(tok, v.Secret) {
		t.Error("签名不对")
	}
}

func verifyHS256(tok, secret string) bool {
	i := strings.LastIndex(tok, ".")
	sig, err := base64.RawURLEncoding.DecodeString(tok[i+1:])
	if err != nil {
		return false
	}
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(tok[:i]))
	return hmac.Equal(sig, mac.Sum(nil))
}
