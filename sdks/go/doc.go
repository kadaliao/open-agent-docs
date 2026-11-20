// Package oad is the Go client for Open Agent Docs: a document server that people and AI agents write in together (HTTP API under /api/v1).
//
// Standard library only. Endpoints and models are generated from docs/openapi.json (zz_generated_*.go) and named after sdks/operations.json:
// the dotted path pages.versions.list is client.Pages.Versions.List(ctx, …) here.
//
// # Create a client
//
//	client := oad.NewClient(
//		oad.WithBaseURL("https://docs.example.com"),
//		oad.WithToken(os.Getenv("OAD_TOKEN")), // an agent token, a session token or a JWT
//	)
//	// oad.NewClient() reads the environment variables OAD_BASE_URL and OAD_TOKEN
//
// # Call endpoints
//
// Path parameters are positional; query parameters and request-body fields go in the last argument, XxxParams (all optional, the zero value is fine):
//
//	page, err := client.Pages.Create(ctx, ws, oad.PagesCreateParams{
//		Title:    oad.Some("Weekly"),
//		Markdown: "# This week\n\n- Shipped the SDK",
//	})
//	content, err := client.Pages.Content.Get(ctx, page.ID, oad.PagesContentGetParams{})
//
// Optional fields use Opt[T]: the zero value means "not set" (not sent), oad.Some(v) sends a value and oad.Null[T]() sends an explicit null.
// Response fields that aren't required, or can be null, are Opt too; read them with Get / Or / Value.
//
// # Errors
//
// An error returned by the server is an *APIError; check it with the sentinels:
//
//	if errors.Is(err, oad.ErrNotFound) { … }
//	var ae *oad.APIError
//	if errors.As(err, &ae) { log.Println(ae.StatusCode, ae.Code, ae.Message) }
//
// Failing to connect or timing out is a *ConnectionError (ErrConnection, ErrTimeout; a timeout is also context.DeadlineExceeded).
// 429, 502/503/504 on GET/PUT/DELETE, and dropped connections are retried automatically (WithMaxRetries, 2 by default).
//
// # Pagination and events
//
//	for v, err := range client.Pages.Versions.ListAll(ctx, page.ID, oad.PagesVersionsListParams{}) { … }
//	for ev, err := range client.Workspaces.Events(ctx, ws) { … } // stops when ctx is cancelled
//
// # Host backends
//
// WithUser calls on behalf of a user logged in to your own system (server with OAD_AUTH=header or jwt); SignJWT and EmbedURL build the iframe URL;
// VerifyWebhook and WebhookHandler receive an agent's webhooks; Start runs a server on this machine (for development and tests).
package oad

//go:generate go run ./internal/codegen
