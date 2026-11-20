package oad

import (
	"context"
	"errors"
	"net/http"
	"time"
)

// userIdentity 是 WithUser 给的用户。
type userIdentity struct {
	id, name, email string
	workspaces      []WorkspaceGrant
}

// UserOption 是 WithUser 的选项。
type UserOption func(*userIdentity)

// UserName 设用户的显示名字（header 模式按 UTF-8 百分号编码放进 X-OAD-User-Name；JWT 模式是 name）。
func UserName(name string) UserOption { return func(u *userIdentity) { u.name = name } }

// UserEmail 设用户的邮箱。
func UserEmail(email string) UserOption { return func(u *userIdentity) { u.email = email } }

// UserWorkspaces 设用户能进哪些工作区（只有 JWT 模式支持，写进 workspaces claim；不给就进默认工作区）。
func UserWorkspaces(ws ...WorkspaceGrant) UserOption {
	return func(u *userIdentity) { u.workspaces = append(u.workspaces, ws...) }
}

// WithUser 返回一个以这个用户身份调的新客户端（宿主后端替当前登录的用户调用）。原来的客户端不变，两者共享连接池和配置。
//
//   - 配了 WithHeaderSecret（服务器 OAD_AUTH=header）：带 X-OAD-Secret、X-OAD-User-Id、X-OAD-User-Name、X-OAD-User-Email。
//   - 配了 WithJWTSecret（服务器 OAD_AUTH=jwt）：用 SignJWT 签一个，缓存到过期前 60 秒再换。
//
// 两个都配了或者都没配、userID 是空的、header 模式给了 UserWorkspaces：返回 ErrConfig。
func (c *Client) WithUser(userID string, opts ...UserOption) (*Client, error) {
	cfg := c.core.cfg
	if userID == "" {
		return nil, configError("WithUser 的 userID 不能是空的")
	}
	switch {
	case cfg.headerSecret != "" && cfg.jwtSecret != "":
		return nil, configError("WithHeaderSecret 和 WithJWTSecret 只能配一个")
	case cfg.headerSecret == "" && cfg.jwtSecret == "":
		return nil, configError("WithUser 要先配 WithHeaderSecret（OAD_AUTH=header）或 WithJWTSecret（OAD_AUTH=jwt）")
	}
	u := &userIdentity{id: userID}
	for _, o := range opts {
		if o != nil {
			o(u)
		}
	}
	if cfg.headerSecret != "" && len(u.workspaces) > 0 {
		return nil, configError("header 模式不支持 UserWorkspaces（只有 JWT 能带工作区）")
	}
	return newClient(&core{cfg: cfg, user: u}), nil
}

// auth 加上身份相关的头。
func (c *core) auth(h http.Header) error {
	cfg := c.cfg
	if c.user == nil {
		if cfg.token != "" {
			h.Set("Authorization", "Bearer "+cfg.token)
		}
		return nil
	}
	if cfg.headerSecret != "" {
		h.Set("X-OAD-Secret", cfg.headerSecret)
		h.Set("X-OAD-User-Id", c.user.id)
		if c.user.name != "" {
			h.Set("X-OAD-User-Name", encodeURIComponent(c.user.name))
		}
		if c.user.email != "" {
			h.Set("X-OAD-User-Email", c.user.email)
		}
		return nil
	}
	tok, err := c.userJWT()
	if err != nil {
		return err
	}
	h.Set("Authorization", "Bearer "+tok)
	return nil
}

// userJWT 返回缓存的 JWT，过期前 60 秒以内就重新签。
func (c *core) userJWT() (string, error) {
	c.jwtMu.Lock()
	defer c.jwtMu.Unlock()
	now := time.Now()
	if c.jwt != "" && now.Before(c.jwtExp.Add(-60*time.Second)) {
		return c.jwt, nil
	}
	ttl := c.cfg.jwtTTL
	tok, err := SignJWT(c.cfg.jwtSecret, c.user.id, JWTOptions{
		Name: c.user.name, Email: c.user.email, Workspaces: c.user.workspaces,
		TTL: ttl, Issuer: c.cfg.jwtIssuer, Audience: c.cfg.jwtAudience, Now: now,
	})
	if err != nil {
		return "", err
	}
	c.jwt, c.jwtExp = tok, now.Truncate(time.Second).Add(ttl.Truncate(time.Second))
	return tok, nil
}

// DefaultWorkspace 返回 /me 里第一个工作区的 id。每个客户端只查一次，之后用缓存。
func (c *Client) DefaultWorkspace(ctx context.Context) (string, error) {
	c.core.wsMu.Lock()
	defer c.core.wsMu.Unlock()
	if c.core.ws != "" {
		return c.core.ws, nil
	}
	me, err := c.Me.Get(ctx)
	if err != nil {
		return "", err
	}
	if len(me.Workspaces) == 0 {
		return "", errors.New("oad: 这个用户还不在任何工作区里")
	}
	c.core.ws = me.Workspaces[0].ID
	return c.core.ws, nil
}
