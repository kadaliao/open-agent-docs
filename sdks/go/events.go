package oad

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"iter"
	"strings"
	"time"
)

// Event 是工作区事件流（Workspaces.Events）里的一条。
//
//	for ev, err := range client.Workspaces.Events(ctx, ws) {
//		if err != nil { return err }
//		if p, ok := ev.Data.(*oad.Page); ok && ev.Name == oad.EventPageCreated { fmt.Println(p.Title) }
//	}
type Event struct {
	// Name 是事件名（event: 字段），比如 page.created；没写是 "message"。常量见 EventPageCreated 等。
	Name string
	// ID 是 id: 字段（工作区事件没有，Agent 通知是通知编号）。
	ID string
	// Data 是按事件名解析好的 data：page.* 是 *Page，workspace.updated 是 *Workspace……（见 EventPageCreated 上面的表）；
	// 不认识的事件名是 json.RawMessage。
	Data any
	// Raw 是原始的 data（JSON）。
	Raw json.RawMessage
}

// sseMsg 是解析出来的一条 SSE 消息。
type sseMsg struct {
	name, id string
	data     string
}

// readSSE 逐行解析标准 SSE：event、data（多行用 \n 拼）、id；冒号开头的是注释（心跳）。emit 返回 false 就停。
func readSSE(r io.Reader, emit func(sseMsg) bool) error {
	br := bufio.NewReaderSize(r, 64<<10)
	var name, id string
	var data []string
	hasData := false
	for {
		line, err := br.ReadString('\n')
		if err != nil && line == "" {
			return err
		}
		line = strings.TrimSuffix(line, "\n")
		line = strings.TrimSuffix(line, "\r")
		switch {
		case line == "":
			if hasData {
				m := sseMsg{name: name, id: id, data: strings.Join(data, "\n")}
				if m.name == "" {
					m.name = "message"
				}
				if !emit(m) {
					return nil
				}
			} else if id != "" {
				// 只有 id 没有 data：也记下来（重连时带上）
				if !emit(sseMsg{id: id}) {
					return nil
				}
			}
			name, id, data, hasData = "", "", nil, false
		case strings.HasPrefix(line, ":"):
			// 心跳
		default:
			k, v, _ := strings.Cut(line, ":")
			v = strings.TrimPrefix(v, " ")
			switch k {
			case "event":
				name = v
			case "data":
				data = append(data, v)
				hasData = true
			case "id":
				if !strings.ContainsRune(v, 0) {
					id = v
				}
			}
		}
		if err != nil {
			return err
		}
	}
}

// sseFatal：这些状态码不重连，给错误后停。
func sseFatal(err error) bool {
	var ae *APIError
	if !errors.As(err, &ae) {
		return false
	}
	switch ae.StatusCode {
	case 400, 401, 403, 404, 410:
		return true
	}
	return false
}

// runSSE 连上、逐条交给 emit；断了按 1 秒起翻倍、最多 30 秒重连（连上后重置），带 Last-Event-ID。
// prepare 在每次连之前按收到的最后一个 id 改请求。返回不重连的错误；ctx 取消或 emit 返回 false 时返回 nil。
func (c *core) runSSE(ctx context.Context, base *request, prepare func(r *request, lastID string), emit func(sseMsg) bool) error {
	first := c.cfg.sseMinRetry
	if first <= 0 {
		first = time.Second
	}
	wait := first
	lastID := ""
	for {
		r := base.clone()
		if lastID != "" {
			r.setHeader("Last-Event-ID", lastID)
		}
		if prepare != nil {
			prepare(r, lastID)
		}
		resp, err := c.send(ctx, r, "text/event-stream", true, 0)
		if ctx.Err() != nil {
			return nil
		}
		if err != nil {
			if sseFatal(err) {
				return err
			}
		} else {
			wait = first
			stopped := false
			_ = readSSE(resp.Body, func(m sseMsg) bool {
				if m.id != "" {
					lastID = m.id
				}
				if m.data == "" && m.name == "" {
					return true
				}
				if !emit(m) {
					stopped = true
					return false
				}
				return true
			})
			resp.Body.Close()
			resp.done()
			if stopped || ctx.Err() != nil {
				return nil
			}
		}
		if err := sleepCtx(ctx, wait); err != nil {
			return nil
		}
		wait = min(wait*2, 30*time.Second)
	}
}

// workspaceEvents 是 Workspaces.Events 的实现。
func (c *core) workspaceEvents(ctx context.Context, req *request) iter.Seq2[Event, error] {
	return func(yield func(Event, error) bool) {
		err := c.runSSE(ctx, req, nil, func(m sseMsg) bool {
			raw := json.RawMessage(m.data)
			data, err := decodeEventData(m.name, raw)
			if err != nil {
				data = raw
			}
			return yield(Event{Name: m.name, ID: m.id, Data: data, Raw: raw}, nil)
		})
		if err != nil {
			yield(Event{}, err)
		}
	}
}

// agentStream 是 Agent.Notifications.Stream 的实现：每条 data 是一个 AgentNotification。
// 重连时把查询参数 after 换成收到的最后一个编号（服务器先看 after，再看 Last-Event-ID）。
func (c *core) agentStream(ctx context.Context, req *request) iter.Seq2[*AgentNotification, error] {
	return func(yield func(*AgentNotification, error) bool) {
		prepare := func(r *request, lastID string) {
			if lastID == "" {
				return
			}
			if r.query == nil {
				r.query = map[string][]string{}
			}
			r.query.Del("cursor")
			r.query.Set("after", lastID)
		}
		err := c.runSSE(ctx, req, prepare, func(m sseMsg) bool {
			var n AgentNotification
			if err := json.Unmarshal([]byte(m.data), &n); err != nil {
				return yield(nil, fmt.Errorf("oad: 通知（%s）解析失败: %w", m.id, err))
			}
			return yield(&n, nil)
		})
		if err != nil {
			yield(nil, err)
		}
	}
}
