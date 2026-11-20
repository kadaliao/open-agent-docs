package oad

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"strings"
	"time"
)

// WorkspaceGrant 是 JWT 的 workspaces claim 里的一项：工作区 id，加上可选的角色（viewer / editor / admin / owner）。
// Role 是空的时写成单独的 id 字符串。
type WorkspaceGrant struct {
	ID   string
	Role string
}

// MarshalJSON：没有角色时是 "ws1"，有角色时是 {"id":"ws1","role":"viewer"}。
func (w WorkspaceGrant) MarshalJSON() ([]byte, error) {
	if w.Role == "" {
		return json.Marshal(w.ID)
	}
	return json.Marshal(struct {
		ID   string `json:"id"`
		Role string `json:"role"`
	}{w.ID, w.Role})
}

// JWTOptions 是 SignJWT 的可选项；零值的字段不写进 claims。
type JWTOptions struct {
	Name       string
	Email      string
	Workspaces []WorkspaceGrant
	// TTL 多久过期，默认 1 小时。
	TTL      time.Duration
	Issuer   string
	Audience string
	// Now 当成“现在”（测试用），零值是 time.Now()。
	Now time.Time
}

// SignJWT 签一个给 OAD_AUTH=jwt 的服务器用的 JWT（HS256，secret 是 OAD_JWT_SECRET）。
// claims：sub、name、email、workspaces、iat、exp、iss、aud（没给的不写）。
// 只做 HS256；要 RS256 / ES256 的用自己的 JWT 库签同样的 claims，服务器配 OAD_JWT_JWKS_URL。
func SignJWT(secret, userID string, opts JWTOptions) (string, error) {
	if secret == "" {
		return "", configError("SignJWT 的 secret 不能是空的")
	}
	if userID == "" {
		return "", configError("SignJWT 的 userID 不能是空的")
	}
	now := opts.Now
	if now.IsZero() {
		now = time.Now()
	}
	ttl := opts.TTL
	if ttl <= 0 {
		ttl = time.Hour
	}
	claims := struct {
		Sub        string           `json:"sub"`
		Name       string           `json:"name,omitempty"`
		Email      string           `json:"email,omitempty"`
		Workspaces []WorkspaceGrant `json:"workspaces,omitempty"`
		Iat        int64            `json:"iat"`
		Exp        int64            `json:"exp"`
		Iss        string           `json:"iss,omitempty"`
		Aud        string           `json:"aud,omitempty"`
	}{
		Sub: userID, Name: opts.Name, Email: opts.Email, Workspaces: opts.Workspaces,
		Iat: now.Unix(), Exp: now.Unix() + int64(ttl/time.Second), Iss: opts.Issuer, Aud: opts.Audience,
	}
	payload, err := json.Marshal(claims)
	if err != nil {
		return "", err
	}
	enc := base64.RawURLEncoding
	signing := enc.EncodeToString([]byte(`{"alg":"HS256","typ":"JWT"}`)) + "." + enc.EncodeToString(payload)
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(signing))
	return signing + "." + enc.EncodeToString(mac.Sum(nil)), nil
}

// EmbedOptions 是 EmbedURL 的可选项；空的不加。
type EmbedOptions struct {
	// Theme 是 light / dark。
	Theme string
	// Lang 是 zh / en。
	Lang string
	// Token 放在 # 后面（#token=…）：不进服务器日志、不进 Referer，网页读走后从地址栏抹掉。
	Token string
}

// EmbedURL 拼嵌入页面用的地址：<base>/p/<page_id>?embed=1[&theme=][&lang=][#token=]，放进 iframe 的 src。
func EmbedURL(baseURL, pageID string, opts EmbedOptions) string {
	var b strings.Builder
	b.WriteString(strings.TrimRight(baseURL, "/"))
	b.WriteString("/p/")
	b.WriteString(encodeURIComponent(pageID))
	b.WriteString("?embed=1")
	if opts.Theme != "" {
		b.WriteString("&theme=" + encodeURIComponent(opts.Theme))
	}
	if opts.Lang != "" {
		b.WriteString("&lang=" + encodeURIComponent(opts.Lang))
	}
	if opts.Token != "" {
		b.WriteString("#token=" + encodeURIComponent(opts.Token))
	}
	return b.String()
}
