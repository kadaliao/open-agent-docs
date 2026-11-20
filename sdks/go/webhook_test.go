package oad

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"testing"
	"time"
)

func signWebhook(secret, ts, body string) string {
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(ts + "." + body))
	return "sha256=" + hex.EncodeToString(mac.Sum(nil))
}

func TestWebhookVectors(t *testing.T) { testWebhookVectors(t) }

func TestWebhookHandler(t *testing.T) {
	const secret = "whsec_x"
	var got *AgentNotification
	h := WebhookHandler(secret, func(ctx context.Context, n *AgentNotification) error {
		if n.Prompt == "fail" {
			return errors.New("处理失败")
		}
		got = n
		return nil
	})
	send := func(method, body string, sign bool) int {
		r := httptest.NewRequest(method, "/oad", strings.NewReader(body))
		ts := strconv.FormatInt(time.Now().UnixMilli(), 10)
		r.Header.Set("X-OAD-Timestamp", ts)
		if sign {
			r.Header.Set("X-OAD-Signature", signWebhook(secret, ts, body))
		} else {
			r.Header.Set("X-OAD-Signature", signWebhook("wrong", ts, body))
		}
		w := httptest.NewRecorder()
		h.ServeHTTP(w, r)
		return w.Code
	}
	if c := send("POST", `{"id":3,"prompt":"hi","comment":{"id":"c","body":"看看","author":{}}}`, true); c != 204 || got == nil || got.ID != 3 || got.Comment.Body != "看看" {
		t.Fatalf("code = %d, note = %+v", c, got)
	}
	if c := send("POST", `{"id":3}`, false); c != http.StatusUnauthorized {
		t.Fatalf("签名不对 = %d", c)
	}
	if c := send("POST", `not json`, true); c != http.StatusBadRequest {
		t.Fatalf("不是 JSON = %d", c)
	}
	if c := send("POST", `{"id":4,"prompt":"fail"}`, true); c != 500 {
		t.Fatalf("处理失败 = %d", c)
	}
	if c := send("GET", ``, true); c != http.StatusMethodNotAllowed {
		t.Fatalf("GET = %d", c)
	}
}
