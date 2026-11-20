// embed-host：宿主网站把 Open Agent Docs 的页面嵌进自己的页面。
//
// 宿主自己做登录（这里用一个假的 cookie 登录代替）；登录后后端签一个 JWT，返回 iframe 地址（token 放在 # 后面）。
// Open Agent Docs 服务器这样起：
//
//	OAD_AUTH=jwt OAD_JWT_SECRET=<同一个密钥> OAD_ALLOWED_ORIGINS=http://127.0.0.1:8080 open-agent-docs serve
//
// 然后：
//
//	OAD_BASE_URL=http://127.0.0.1:8848 OAD_JWT_SECRET=<同一个密钥> go run ./examples/embed-host
//	打开 http://127.0.0.1:8080/login?user=u-42&name=王小明
package main

import (
	"context"
	"encoding/json"
	"html/template"
	"log"
	"net/http"
	"net/url"
	"os"
	"time"

	oad "github.com/kadaliao/open-agent-docs/sdks/go"
)

var page = template.Must(template.New("p").Parse(`<!doctype html>
<meta charset="utf-8"><title>宿主</title>
<style>body{font-family:system-ui;margin:24px} iframe{width:100%;height:80vh;border:1px solid #ddd;border-radius:8px}</style>
<p>你好，{{.Name}}。下面是嵌进来的文档：</p>
<iframe src="{{.Src}}" allow="clipboard-write"></iframe>
<script>
// token 快过期时 iframe 会发 oad:auth-required，宿主去后端换一个新的发回去
const frame = document.querySelector('iframe')
const origin = new URL(frame.src).origin
addEventListener('message', async (e) => {
  if (e.origin !== origin || e.data?.type !== 'oad:auth-required') return
  const r = await fetch('/token')
  if (r.ok) frame.contentWindow.postMessage({ type: 'oad:token', token: (await r.json()).token }, origin)
})
</script>`))

func main() {
	base := os.Getenv("OAD_BASE_URL")
	secret := os.Getenv("OAD_JWT_SECRET")
	if base == "" || secret == "" {
		log.Fatal("要设 OAD_BASE_URL 和 OAD_JWT_SECRET")
	}
	admin := oad.NewClient(oad.WithBaseURL(base), oad.WithJWTSecret(secret))

	type user struct{ ID, Name string }
	current := func(r *http.Request) (user, bool) {
		id, err1 := r.Cookie("uid")
		name, err2 := r.Cookie("name")
		if err1 != nil || err2 != nil {
			return user{}, false
		}
		n, _ := url.QueryUnescape(name.Value)
		return user{id.Value, n}, true
	}
	sign := func(u user) (string, error) {
		return oad.SignJWT(secret, u.ID, oad.JWTOptions{Name: u.Name, TTL: 15 * time.Minute})
	}

	// 假登录：真实系统里换成自己的登录
	http.HandleFunc("/login", func(w http.ResponseWriter, r *http.Request) {
		http.SetCookie(w, &http.Cookie{Name: "uid", Value: r.URL.Query().Get("user"), Path: "/", HttpOnly: true})
		// cookie 的值只能是 ASCII：中文名字先编码
		http.SetCookie(w, &http.Cookie{Name: "name", Value: url.QueryEscape(r.URL.Query().Get("name")), Path: "/", HttpOnly: true})
		http.Redirect(w, r, "/", http.StatusFound)
	})

	// 首页：找（或建）这个用户的一个页面，返回带 iframe 的页面
	http.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		u, ok := current(r)
		if !ok {
			http.Error(w, "先登录：/login?user=u-42&name=王小明", http.StatusUnauthorized)
			return
		}
		ctx, cancel := context.WithTimeout(r.Context(), 10*time.Second)
		defer cancel()
		as, err := admin.WithUser(u.ID, oad.UserName(u.Name))
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		ws, err := as.DefaultWorkspace(ctx)
		if err != nil {
			http.Error(w, err.Error(), http.StatusBadGateway)
			return
		}
		pages, err := as.Pages.List(ctx, ws)
		if err != nil {
			http.Error(w, err.Error(), http.StatusBadGateway)
			return
		}
		var id string
		if len(pages) > 0 {
			id = pages[0].ID
		} else {
			p, err := as.Pages.Create(ctx, ws, oad.PagesCreateParams{Title: oad.Some("我的笔记"), Markdown: "在这里写点什么"})
			if err != nil {
				http.Error(w, err.Error(), http.StatusBadGateway)
				return
			}
			id = p.ID
		}
		tok, err := sign(u)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		src := oad.EmbedURL(base, id, oad.EmbedOptions{Lang: "zh", Token: tok})
		page.Execute(w, map[string]any{"Name": u.Name, "Src": template.URL(src)})
	})

	// 换新 token（iframe 发 oad:auth-required 时调）
	http.HandleFunc("/token", func(w http.ResponseWriter, r *http.Request) {
		u, ok := current(r)
		if !ok {
			http.Error(w, "没登录", http.StatusUnauthorized)
			return
		}
		tok, err := sign(u)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"token": tok})
	})

	log.Println("打开 http://127.0.0.1:8080/login?user=u-42&name=王小明")
	log.Fatal(http.ListenAndServe("127.0.0.1:8080", nil))
}
