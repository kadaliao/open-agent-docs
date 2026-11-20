package oad

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestReadSSE(t *testing.T) {
	in := ": ok\r\n\r\n" +
		"event: page.created\r\ndata: {\"id\":\"p1\",\r\ndata: \"title\":\"T\"}\r\n\r\n" +
		"id: 7\nevent: notification\ndata:{\"id\":7}\n\n" +
		"data: plain\n\n" +
		"event: empty\n\n" +
		"id: 9\n\n"
	var got []sseMsg
	err := readSSE(strings.NewReader(in), func(m sseMsg) bool { got = append(got, m); return true })
	if err != io.EOF {
		t.Fatal(err)
	}
	want := []sseMsg{
		{name: "page.created", data: "{\"id\":\"p1\",\n\"title\":\"T\"}"},
		{name: "notification", id: "7", data: "{\"id\":7}"},
		{name: "message", data: "plain"},
		{id: "9"},
	}
	if len(got) != len(want) {
		t.Fatalf("got %+v", got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("第 %d 条 = %+v，应该是 %+v", i, got[i], want[i])
		}
	}
}

func sseServer(t *testing.T, h func(w http.ResponseWriter, r *http.Request, n int64)) *fake {
	var n atomic.Int64
	return newFake(t, func(w http.ResponseWriter, r *http.Request) {
		h(w, r, n.Add(1))
	})
}

func flushWrite(w http.ResponseWriter, s string) {
	io.WriteString(w, s)
	w.(http.Flusher).Flush()
}

func TestWorkspaceEvents(t *testing.T) {
	lastIDs := make(chan string, 4)
	f := sseServer(t, func(w http.ResponseWriter, r *http.Request, n int64) {
		lastIDs <- r.Header.Get("Last-Event-ID")
		if r.Header.Get("Accept") != "text/event-stream" {
			t.Errorf("Accept = %q", r.Header.Get("Accept"))
		}
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(200)
		switch n {
		case 1:
			flushWrite(w, ": ok\n\nid: 1\nevent: page.created\ndata: {\"id\":\"p1\",\"title\":\"一\"}\n\n")
			// 断开：重连
		case 2:
			flushWrite(w, "event: something.new\ndata: {\"x\":1}\n\nevent: resync\ndata: {}\n\nevent: db.changed\ndata: {\"db_id\":\"d\",\"seq\":3}\n\n")
			<-r.Context().Done()
		}
	})
	c := NewClient(WithBaseURL(f.URL))
	c.core.cfg.sseMinRetry = 10 * time.Millisecond
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	var evs []Event
	for ev, err := range c.Workspaces.Events(ctx, "ws") {
		if err != nil {
			t.Fatal(err)
		}
		evs = append(evs, ev)
		if len(evs) == 4 {
			break
		}
	}
	if len(evs) != 4 {
		t.Fatalf("evs = %+v", evs)
	}
	if p, ok := evs[0].Data.(*Page); !ok || p.ID != "p1" || p.Title != "一" || evs[0].ID != "1" {
		t.Fatalf("第一条 = %+v", evs[0])
	}
	if raw, ok := evs[1].Data.(json.RawMessage); !ok || string(raw) != `{"x":1}` {
		t.Fatalf("不认识的事件 = %#v", evs[1].Data)
	}
	if evs[2].Name != EventResync {
		t.Fatalf("resync = %+v", evs[2])
	}
	if d, ok := evs[3].Data.(*DbChangedEvent); !ok || d.DbID.Value() != "d" || d.Seq.Value() != 3 {
		t.Fatalf("db.changed = %#v", evs[3].Data)
	}
	if first, second := <-lastIDs, <-lastIDs; first != "" || second != "1" {
		t.Fatalf("Last-Event-ID：%q、%q", first, second)
	}
}

func TestEventsFatalAndCancel(t *testing.T) {
	f := sseServer(t, func(w http.ResponseWriter, r *http.Request, n int64) { apiErr(w, 403, "forbidden", "不是成员") })
	c := NewClient(WithBaseURL(f.URL))
	var errs []error
	for _, err := range c.Workspaces.Events(context.Background(), "ws") {
		errs = append(errs, err)
	}
	if len(errs) != 1 || !errors.Is(errs[0], ErrForbidden) || f.n.Load() != 1 {
		t.Fatalf("errs = %v，请求 %d 个", errs, f.n.Load())
	}

	// 500：重连；ctx 取消：停，不给错误
	f5 := sseServer(t, func(w http.ResponseWriter, r *http.Request, n int64) { apiErr(w, 500, "internal", "x") })
	c = NewClient(WithBaseURL(f5.URL))
	c.core.cfg.sseMinRetry = 5 * time.Millisecond
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()
	for _, err := range c.Workspaces.Events(ctx, "ws") {
		t.Fatalf("不该给东西：%v", err)
	}
	if f5.n.Load() < 3 {
		t.Fatalf("500 应该重连，只连了 %d 次", f5.n.Load())
	}
}

func TestAgentStreamReconnect(t *testing.T) {
	type seen struct{ after, cursor, last string }
	reqs := make(chan seen, 4)
	f := sseServer(t, func(w http.ResponseWriter, r *http.Request, n int64) {
		reqs <- seen{r.URL.Query().Get("after"), r.URL.Query().Get("cursor"), r.Header.Get("Last-Event-ID")}
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(200)
		if n == 1 {
			flushWrite(w, "id: 5\nevent: notification\ndata: {\"id\":5,\"prompt\":\"a\",\"comment\":{\"id\":\"c\",\"body\":\"看看\",\"author\":{}}}\n\n")
			return
		}
		flushWrite(w, "id: 6\nevent: notification\ndata: {\"id\":6,\"prompt\":\"b\"}\n\n")
		<-r.Context().Done()
	})
	c := NewClient(WithBaseURL(f.URL), WithToken("oad_bot"))
	c.core.cfg.sseMinRetry = 10 * time.Millisecond
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	var notes []*AgentNotification
	for n, err := range c.Agent.Notifications.Stream(ctx, AgentNotificationsStreamParams{Cursor: "2"}) {
		if err != nil {
			t.Fatal(err)
		}
		notes = append(notes, n)
		if len(notes) == 2 {
			break
		}
	}
	if notes[0].ID != 5 || notes[0].Comment.Body != "看看" || notes[1].ID != 6 {
		t.Fatalf("notes = %+v %+v", notes[0], notes[1])
	}
	if a, b := <-reqs, <-reqs; a != (seen{"", "2", ""}) || b != (seen{"5", "", "5"}) {
		t.Fatalf("请求 = %+v、%+v", a, b)
	}
}
