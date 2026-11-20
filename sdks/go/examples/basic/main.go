// basic：建一个页面，写 Markdown，再读回来。
//
//	OAD_BASE_URL=http://127.0.0.1:8848 OAD_TOKEN=oad_... go run ./examples/basic
package main

import (
	"context"
	"fmt"
	"log"
	"time"

	oad "github.com/kadaliao/open-agent-docs/sdks/go"
)

func main() {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	client := oad.NewClient() // 读 OAD_BASE_URL、OAD_TOKEN
	ws, err := client.DefaultWorkspace(ctx)
	if err != nil {
		log.Fatal(err)
	}

	page, err := client.Pages.Create(ctx, ws, oad.PagesCreateParams{
		Title:    oad.Some("周报 " + time.Now().Format("2006-01-02")),
		Markdown: "# 本周\n\n- 接上了 Go SDK\n- 写了例子\n\n> 有问题在评论里 @ Agent",
	})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("建好了：", page.ID, page.Title)

	content, err := client.Pages.Content.Get(ctx, page.ID, oad.PagesContentGetParams{})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println(content.Markdown.Value())

	// 在文末加一段
	if _, err := client.Pages.Blocks.Insert(ctx, page.ID, oad.PagesBlocksInsertParams{Markdown: "下周：发布 v" + oad.Version}); err != nil {
		log.Fatal(err)
	}
	fmt.Println("网页里打开：", client.BaseURL()+"/p/"+page.ID)
}
