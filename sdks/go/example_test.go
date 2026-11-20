package oad_test

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net/http"
	"os"
	"time"

	oad "github.com/kadaliao/open-agent-docs/sdks/go"
)

func ExampleNewClient() {
	ctx := context.Background()
	// 不给选项就读环境变量 OAD_BASE_URL、OAD_TOKEN
	client := oad.NewClient(
		oad.WithBaseURL("https://docs.example.com"),
		oad.WithToken(os.Getenv("OAD_TOKEN")),
		oad.WithLocale("zh"),
	)
	ws, err := client.DefaultWorkspace(ctx)
	if err != nil {
		log.Fatal(err)
	}
	pages, err := client.Pages.List(ctx, ws)
	if err != nil {
		log.Fatal(err)
	}
	for _, p := range pages {
		fmt.Println(p.ID, p.Title)
	}
}

func ExamplePagesService_Create() {
	ctx := context.Background()
	client := oad.NewClient()
	ws, _ := client.DefaultWorkspace(ctx)
	page, err := client.Pages.Create(ctx, ws, oad.PagesCreateParams{
		Title:    oad.Some("周报"),
		Markdown: "# 本周\n\n- 上线了 Go SDK",
	})
	if err != nil {
		log.Fatal(err)
	}
	content, err := client.Pages.Content.Get(ctx, page.ID, oad.PagesContentGetParams{})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println(content.Markdown.Value())
	// 改正文时带上 base，别人同时改了会是冲突（409）
	err = client.Pages.Content.Replace(ctx, page.ID, oad.PagesContentReplaceParams{
		Markdown: oad.Some("# 本周\n\n- 上线了 Go SDK\n- 写了文档"),
		Base:     oad.Some(content.Version),
	})
	if errors.Is(err, oad.ErrConflict) {
		log.Println("别人刚改过，重新取一遍再改")
	}
}

func ExamplePagesService_Get_errors() {
	client := oad.NewClient()
	_, err := client.Pages.Get(context.Background(), "nope")
	var ae *oad.APIError
	switch {
	case errors.Is(err, oad.ErrNotFound):
		fmt.Println("没有这个页面")
	case errors.As(err, &ae):
		fmt.Println(ae.StatusCode, ae.Code, ae.Message)
	case errors.Is(err, oad.ErrConnection):
		fmt.Println("连不上服务器")
	}
}

func ExamplePagesVersionsService_ListAll() {
	ctx := context.Background()
	client := oad.NewClient()
	// 自动带 cursor 翻页；break 出循环就不再取
	for v, err := range client.Pages.Versions.ListAll(ctx, "page-id", oad.PagesVersionsListParams{Limit: 50}) {
		if err != nil {
			log.Fatal(err)
		}
		fmt.Println(v.ID, v.Label.Or("（没有标签）"))
	}
}

func ExampleWorkspacesService_Events() {
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute) // ctx 取消就停
	defer cancel()
	client := oad.NewClient()
	ws, _ := client.DefaultWorkspace(ctx)
	for ev, err := range client.Workspaces.Events(ctx, ws) {
		if err != nil {
			log.Fatal(err) // 401、403、404 这类不重连的错误
		}
		switch d := ev.Data.(type) {
		case *oad.Page:
			fmt.Println(ev.Name, d.Title)
		case *oad.DbChangedEvent:
			fmt.Println("库变了", d.DbID.Value())
		}
		if ev.Name == oad.EventResync {
			fmt.Println("落下了事件，重新拉一遍列表")
		}
	}
}

func ExampleAgentNotificationsService_Stream() {
	ctx := context.Background()
	agent := oad.NewClient(oad.WithToken(os.Getenv("OAD_TOKEN"))) // Agent 的 token
	// 取 → 处理 → 确认。没确认的断线重连、重启以后都会再收到
	for note, err := range agent.Agent.Notifications.Stream(ctx, oad.AgentNotificationsStreamParams{}) {
		if err != nil {
			log.Fatal(err)
		}
		fmt.Println(note.Page.Title, note.Comment.Body)
		if _, err := agent.Threads.Reply(ctx, note.ThreadID, oad.ThreadsReplyParams{Body: "收到，在看"}); err != nil {
			log.Println(err)
			continue
		}
		if _, err := agent.Agent.Notifications.Ack(ctx, oad.AgentNotificationsAckParams{IDs: oad.Some([]int64{note.ID})}); err != nil {
			log.Println(err)
		}
	}
}

func ExampleClient_WithUser() {
	ctx := context.Background()
	// 服务器 OAD_AUTH=header：宿主自己做登录，后端替当前用户调
	admin := oad.NewClient(oad.WithBaseURL("http://127.0.0.1:8848"), oad.WithHeaderSecret(os.Getenv("OAD_HEADER_SECRET")))
	u, err := admin.WithUser("u-42", oad.UserName("王小明"), oad.UserEmail("x@example.com"))
	if err != nil {
		log.Fatal(err)
	}
	ws, err := u.DefaultWorkspace(ctx)
	if err != nil {
		log.Fatal(err)
	}
	pages, _ := u.Pages.List(ctx, ws)
	fmt.Println(len(pages))
}

func ExampleEmbedURL() {
	token, _ := oad.SignJWT("jwt-secret", "u-42", oad.JWTOptions{Name: "王小明", TTL: 10 * time.Minute})
	_ = token
	fmt.Println(oad.EmbedURL("https://docs.example.com/", "p1", oad.EmbedOptions{Theme: "dark", Lang: "en"}))
	fmt.Println(oad.EmbedURL("https://docs.example.com", "a b/c", oad.EmbedOptions{Token: "eyJ.x.y"}))
	// Output:
	// https://docs.example.com/p/p1?embed=1&theme=dark&lang=en
	// https://docs.example.com/p/a%20b%2Fc?embed=1#token=eyJ.x.y
}

func ExampleVerifyWebhookAt() {
	h := http.Header{}
	h.Set("X-OAD-Timestamp", "1700000000000")
	h.Set("X-OAD-Signature", "sha256=617e1ec5d26477925814def08f9f6c1ad7df5646b5710557abe2dee3ff1dc040")
	body := []byte(`{"id":42,"type":"mention"}`)

	note, err := oad.VerifyWebhookAt("whsec_test_0123456789", h, body, time.UnixMilli(1700000060000), oad.DefaultWebhookTolerance)
	fmt.Println(note.ID, note.Type, err)

	_, err = oad.VerifyWebhookAt("whsec_wrong", h, body, time.UnixMilli(1700000060000), oad.DefaultWebhookTolerance)
	fmt.Println(errors.Is(err, oad.ErrWebhookSignature))
	// Output:
	// 42 mention <nil>
	// true
}

func ExampleWebhookHandler() {
	http.Handle("/oad/webhook", oad.WebhookHandler(os.Getenv("OAD_WEBHOOK_SECRET"), func(ctx context.Context, n *oad.AgentNotification) error {
		log.Printf("%s 叫我：%s", n.Comment.Body, n.Page.URL)
		return nil // 返回错误时回 500，服务器过一会儿重发
	}))
}

func ExampleStart() {
	ctx := context.Background()
	dir, _ := os.MkdirTemp("", "oad-data")
	defer os.RemoveAll(dir)
	// 二进制：StartOptions.Binary → OAD_BINARY → 缓存 → 从 GitHub release 下载（SHA256SUMS 校验）
	srv, err := oad.Start(ctx, oad.StartOptions{DataDir: dir, Auth: "header", HeaderSecret: "dev-secret"})
	if err != nil {
		log.Fatal(err)
	}
	defer srv.Close()
	u, _ := oad.NewClient(oad.WithBaseURL(srv.URL), oad.WithHeaderSecret("dev-secret")).WithUser("dev")
	me, _ := u.Me.Get(ctx)
	fmt.Println(me.User.ID)
}

func ExampleOpt() {
	// 没设置的不发；Some 发值；Null 明确发 null（pages.update 的 after: null = 挪到最前）
	b, _ := json.Marshal(oad.PatchPage{Title: oad.Some("新标题"), After: oad.Null[string]()})
	fmt.Println(string(b))

	var p oad.Page
	_ = json.Unmarshal([]byte(`{"id":"p1","title":"T","parent_id":null}`), &p)
	fmt.Println(p.ParentID.IsNull(), p.Icon.IsSet(), p.Icon.Or("📄"))
	// Output:
	// {"after":null,"title":"新标题"}
	// true false 📄
}
