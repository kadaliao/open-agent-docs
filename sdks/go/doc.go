// Package oad 是 Open Agent Docs 的 Go 客户端：人和 Agent 一起写的文档服务器（HTTP 接口 /api/v1）。
//
// 只用标准库。接口和模型从 docs/openapi.json 生成（zz_generated_*.go），名字按 sdks/operations.json：
// 点路径 pages.versions.list 在这里是 client.Pages.Versions.List(ctx, …)。
//
// # 建客户端
//
//	client := oad.NewClient(
//		oad.WithBaseURL("https://docs.example.com"),
//		oad.WithToken(os.Getenv("OAD_TOKEN")), // Agent token、会话令牌或 JWT
//	)
//	// oad.NewClient() 读环境变量 OAD_BASE_URL、OAD_TOKEN
//
// # 调接口
//
// 路径参数是位置参数，查询参数和请求体字段放在最后一个参数 XxxParams 里（全是可选的可以传零值）：
//
//	page, err := client.Pages.Create(ctx, ws, oad.PagesCreateParams{
//		Title:    oad.Some("周报"),
//		Markdown: "# 本周\n\n- 上线了 SDK",
//	})
//	content, err := client.Pages.Content.Get(ctx, page.ID, oad.PagesContentGetParams{})
//
// 可选字段用 Opt[T]：零值是“没设置”（不发），oad.Some(v) 发值，oad.Null[T]() 明确发 null。
// 响应里不是必填、或者可以是 null 的字段也是 Opt，用 Get / Or / Value 取。
//
// # 错误
//
// 服务器回的错误是 *APIError，可以用哨兵判断：
//
//	if errors.Is(err, oad.ErrNotFound) { … }
//	var ae *oad.APIError
//	if errors.As(err, &ae) { log.Println(ae.StatusCode, ae.Code, ae.Message) }
//
// 连不上、超时是 *ConnectionError（ErrConnection、ErrTimeout，超时也是 context.DeadlineExceeded）。
// 429、GET/PUT/DELETE 的 502/503/504 和断线会自动重试（WithMaxRetries，默认 2 次）。
//
// # 翻页、事件
//
//	for v, err := range client.Pages.Versions.ListAll(ctx, page.ID, oad.PagesVersionsListParams{}) { … }
//	for ev, err := range client.Workspaces.Events(ctx, ws) { … } // ctx 取消就停
//
// # 宿主后端
//
// WithUser 替自己系统里登录的用户调（服务器 OAD_AUTH=header 或 jwt）；SignJWT、EmbedURL 拼 iframe 地址；
// VerifyWebhook、WebhookHandler 收 Agent 的 webhook；Start 在本机起一个服务器（开发、测试用）。
package oad

//go:generate go run ./internal/codegen
