// webhook：收 Agent 的 webhook（有人在评论里 @ 这个 Agent 时服务器 POST 过来），验签后回一条评论。
//
//	OAD_WEBHOOK_SECRET=whsec_... OAD_TOKEN=oad_... go run ./examples/webhook
//
// 建 Agent 时把 webhook 设成 http://<这台机器>:8080/oad/webhook。
package main

import (
	"context"
	"log"
	"net/http"
	"os"

	oad "github.com/kadaliao/open-agent-docs/sdks/go"
)

func main() {
	secret := os.Getenv("OAD_WEBHOOK_SECRET")
	if secret == "" {
		log.Fatal("要设 OAD_WEBHOOK_SECRET（建 Agent 时给的 webhook 密钥）")
	}
	agent := oad.NewClient() // OAD_BASE_URL、OAD_TOKEN（Agent 的 token）

	http.Handle("/oad/webhook", oad.WebhookHandler(secret, func(ctx context.Context, n *oad.AgentNotification) error {
		log.Printf("通知 %d：%s 在《%s》里说：%s", n.ID, n.Comment.ID, n.Page.Title, n.Comment.Body)
		// 返回错误时回 500，服务器过一会儿重发；处理慢的活最好放到后台，先回 2xx
		_, err := agent.Threads.Reply(ctx, n.ThreadID, oad.ThreadsReplyParams{Body: "收到，在看"})
		if err != nil {
			return err
		}
		_, err = agent.Agent.Notifications.Ack(ctx, oad.AgentNotificationsAckParams{IDs: oad.Some([]int64{n.ID})})
		return err
	}))
	log.Println("在 :8080 等 webhook")
	log.Fatal(http.ListenAndServe(":8080", nil))
}
