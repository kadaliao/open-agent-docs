package oad

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// DefaultWebhookTolerance：webhook 的时间戳和现在差多少以内算新鲜（防重放）。
const DefaultWebhookTolerance = 5 * time.Minute

// webhook 验签失败的几种，用 errors.Is 判断；errors.Is(err, oad.ErrWebhook) 是任何一种。
var (
	ErrWebhook          = errors.New("oad: webhook 验签失败")
	ErrWebhookSignature = errors.New("oad: webhook 签名不对")
	ErrWebhookStale     = errors.New("oad: webhook 时间戳太旧或太新")
	ErrWebhookMalformed = errors.New("oad: webhook 格式不对")
)

// WebhookError 是 VerifyWebhook 的错误。Kind 是 signature、stale 或 malformed。
type WebhookError struct {
	Kind   string
	Reason string
}

func (e *WebhookError) Error() string {
	return "oad: webhook 验签失败（" + e.Kind + "）：" + e.Reason
}

// Is：ErrWebhook，以及按 Kind 对应的 ErrWebhookSignature / ErrWebhookStale / ErrWebhookMalformed。
func (e *WebhookError) Is(target error) bool {
	switch target {
	case ErrWebhook:
		return true
	case ErrWebhookSignature:
		return e.Kind == "signature"
	case ErrWebhookStale:
		return e.Kind == "stale"
	case ErrWebhookMalformed:
		return e.Kind == "malformed"
	}
	return false
}

// VerifyWebhook 验 Agent 的 webhook（X-OAD-Timestamp、X-OAD-Signature），对了返回解析好的通知。
// secret 是建 Agent 时给的 webhook 密钥；body 是原始请求体（别先解析再序列化）。时间差超过 5 分钟算 stale。
func VerifyWebhook(secret string, header http.Header, body []byte) (*AgentNotification, error) {
	return VerifyWebhookAt(secret, header, body, time.Now(), DefaultWebhookTolerance)
}

// VerifyWebhookAt 和 VerifyWebhook 一样，但“现在”和容许的时间差自己给（测试用）。
//
// 签名 = "sha256=" + 小写十六进制 HMAC-SHA256(secret, "<X-OAD-Timestamp>.<body>")，常量时间比较；时间戳是毫秒。
// 请求头名字大小写不论。缺头、时间戳不是整数、签名格式不对 → malformed；签名不对 → signature；时间差超过 tolerance → stale。
func VerifyWebhookAt(secret string, header http.Header, body []byte, now time.Time, tolerance time.Duration) (*AgentNotification, error) {
	ts := headerGet(header, "X-OAD-Timestamp")
	sig := headerGet(header, "X-OAD-Signature")
	if ts == "" {
		return nil, &WebhookError{"malformed", "没有 X-OAD-Timestamp 头"}
	}
	if sig == "" {
		return nil, &WebhookError{"malformed", "没有 X-OAD-Signature 头"}
	}
	ms, err := strconv.ParseInt(strings.TrimSpace(ts), 10, 64)
	if err != nil {
		return nil, &WebhookError{"malformed", "X-OAD-Timestamp 不是整数"}
	}
	hexSig, ok := strings.CutPrefix(strings.TrimSpace(sig), "sha256=")
	if !ok {
		return nil, &WebhookError{"malformed", "X-OAD-Signature 不是 sha256=<hex>"}
	}
	got, err := hex.DecodeString(hexSig)
	if err != nil || len(got) != sha256.Size {
		return nil, &WebhookError{"malformed", "X-OAD-Signature 不是 sha256=<hex>"}
	}
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(strings.TrimSpace(ts)))
	mac.Write([]byte("."))
	mac.Write(body)
	if !hmac.Equal(got, mac.Sum(nil)) {
		return nil, &WebhookError{"signature", "签名对不上（密钥不对，或者正文被改过）"}
	}
	diff := now.UnixMilli() - ms
	if diff < 0 {
		diff = -diff
	}
	if diff > tolerance.Milliseconds() {
		return nil, &WebhookError{"stale", fmt.Sprintf("时间戳和现在差了 %d 秒（最多 %d 秒）", diff/1000, int64(tolerance/time.Second))}
	}
	var note AgentNotification
	if err := json.Unmarshal(body, &note); err != nil {
		return nil, &WebhookError{"malformed", "正文不是 AgentNotification 的 JSON：" + err.Error()}
	}
	return &note, nil
}

// 名字大小写不论（http.Header 是手拼的、没规范化也能找到）。
func headerGet(h http.Header, name string) string {
	if v := h.Get(name); v != "" {
		return v
	}
	for k, vs := range h {
		if strings.EqualFold(k, name) && len(vs) > 0 {
			return vs[0]
		}
	}
	return ""
}

// WebhookHandler 返回一个收 Agent webhook 的 http.Handler：验签，对了调 handle。
//
//   - 不是 POST：405。验签失败：malformed 回 400，signature / stale 回 401。
//
//   - handle 返回错误：500，服务器过一会儿重发（按 X-OAD-Delivery 去重）。成功：204。
//
//     http.Handle("/oad", oad.WebhookHandler(secret, func(ctx context.Context, n *oad.AgentNotification) error {
//     log.Println(n.Comment.Body)
//     return nil
//     }))
func WebhookHandler(secret string, handle func(ctx context.Context, note *AgentNotification) error) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			w.Header().Set("Allow", http.MethodPost)
			http.Error(w, "只收 POST", http.StatusMethodNotAllowed)
			return
		}
		body, err := io.ReadAll(http.MaxBytesReader(w, r.Body, 8<<20))
		if err != nil {
			http.Error(w, "读不了请求体", http.StatusBadRequest)
			return
		}
		note, err := VerifyWebhook(secret, r.Header, body)
		if err != nil {
			status := http.StatusUnauthorized
			if errors.Is(err, ErrWebhookMalformed) {
				status = http.StatusBadRequest
			}
			http.Error(w, err.Error(), status)
			return
		}
		if err := handle(r.Context(), note); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		w.WriteHeader(http.StatusNoContent)
	})
}
