"""事件流：SSE 解析（多行 data、心跳、id、\\r\\n）、按事件名解析成模型、断线重连带 Last-Event-ID、401 不重连、异步。"""
from __future__ import annotations

import threading
from typing import AsyncIterator, List

import httpx
import pytest

import open_agent_docs as oad
from open_agent_docs._sse import SSEDecoder, _Lines
from oad_test_helpers import make_async_client, make_client

PAGE = '{"id":"p1","workspace_id":"w1","position":"a","title":"周报","created_at":1,"updated_at":2}'
NOTE = (
    '{"id":%d,"created_at":1,"type":"comment","agent":{"id":"a1","name":"bot"},"page":{"id":"p1","title":"x","url":"/p/p1"},'
    '"thread_id":"t1","comment":{"id":"c1","body":"@bot 看看","author":{"id":"u1","kind":"user","name":"甲"}},"prompt":"看看"}'
)


def decode(text: str) -> List[tuple]:
    d, lines, out = SSEDecoder(), _Lines(), []
    for i in range(0, len(text), 3):  # 故意切碎：一块三个字符
        for ln in lines.feed(text[i : i + 3]):
            got = d.feed(ln)
            if got:
                out.append(got)
    for ln in lines.flush() + [""]:
        got = d.feed(ln)
        if got:
            out.append(got)
    return out


def test_decoder() -> None:
    text = ": ok\n\nevent: page.created\ndata: {\"a\":\ndata: 1}\nid: 7\n\r\n:heartbeat\r\ndata:no-space\r\n\r\nevent: x\nid: 9\n\nretry: 10\ndata: last\n"
    assert decode(text) == [
        ("page.created", '{"a":\n1}', "7"),
        ("message", "no-space", "7"),  # 没有 id 的事件：沿用上一个
        ("message", "last", "9"),  # 只有 id 没有 data 的不出，但 id 记下了
    ]
    assert _Lines().feed("a\rb\r") == ["a"]


def test_events_parsed_into_models() -> None:
    sse = (
        f"event: page.created\ndata: {PAGE}\n\n"
        'event: db.changed\ndata: {"db_id":"d1","seq":3}\n\n'
        "event: resync\ndata: {}\n\n"
        'event: brand.new\ndata: {"x":1}\n\n'
        "event: page.updated\ndata: not json\n\n"
    )
    c, rec = make_client(lambda r: httpx.Response(200, headers={"content-type": "text/event-stream"}, content=sse.encode()))
    got = []
    with c.workspaces.events("w1") as events:
        assert len(rec.requests) == 1  # 进 with 就连上了
        for ev in events:
            got.append(ev)
            if len(got) == 5:
                break
    assert isinstance(got[0].data, oad.Page) and got[0].data.title == "周报"
    assert isinstance(got[1].data, oad.DbChangedEvent) and got[1].data.seq == 3
    assert got[2].name == "resync" and got[2].data == {}
    assert got[3].data == {"x": 1}  # 不认识的事件：原始 JSON
    assert got[4].data == "not json"
    assert rec.requests[0].headers["accept"] == "text/event-stream"
    assert rec.requests[0].url.path == "/api/v1/workspaces/w1/events"


def test_reconnect_with_last_event_id() -> None:
    bodies = [
        f": ok\n\nid: 1\nevent: notification\ndata: {NOTE % 1}\n\nid: 2\nevent: notification\ndata: {NOTE % 2}\n\n",
        None,  # 第二次连：503，过一会儿再连
        f"id: 3\nevent: notification\ndata: {NOTE % 3}\n\n",
    ]
    n = {"i": 0}

    def h(r: httpx.Request) -> httpx.Response:
        i = n["i"]
        n["i"] += 1
        if i >= len(bodies):
            return httpx.Response(200, content=b"")
        b = bodies[i]
        if b is None:
            return httpx.Response(503)
        return httpx.Response(200, headers={"content-type": "text/event-stream"}, content=b.encode())

    c, rec = make_client(h, token="oad_agent")
    ids = []
    stream = c.agent.notifications.stream(after=0)
    for ev in stream:
        assert isinstance(ev.data, oad.AgentNotification)
        assert ev.data.comment.body == "@bot 看看"
        ids.append(ev.data.id)
        if len(ids) == 3:
            break
    stream.close()
    assert ids == [1, 2, 3]
    assert stream.last_event_id == "3"
    first, second, third = rec.requests[:3]
    assert first.url.params["after"] == "0" and "last-event-id" not in first.headers
    # 重连：带 Last-Event-ID，去掉 ?after=（服务器先看 after）
    assert second.headers["last-event-id"] == "2" and "after" not in second.url.params
    assert third.headers["last-event-id"] == "2"


@pytest.mark.parametrize("status,cls", [(401, oad.AuthenticationError), (403, oad.PermissionDeniedError), (404, oad.NotFoundError)])
def test_fatal_status_does_not_reconnect(status: int, cls: type) -> None:
    c, rec = make_client(lambda r: httpx.Response(status, json={"error": {"code": "x", "message": "不行"}}))
    with pytest.raises(cls):
        with c.workspaces.events("w1"):
            pass
    assert len(rec.requests) == 1
    # 断了以后重连时遇到也一样：抛出来，不再连
    n = {"i": 0}

    def h(r: httpx.Request) -> httpx.Response:
        n["i"] += 1
        if n["i"] == 1:
            return httpx.Response(200, content=f"event: page.created\ndata: {PAGE}\n\n".encode())
        return httpx.Response(status)

    c, rec = make_client(h)
    events = c.workspaces.events("w1")
    it = iter(events)
    assert next(it).name == "page.created"
    with pytest.raises(cls):
        next(it)
    assert len(rec.requests) == 2


def test_close_from_another_thread_stops_waiting(monkeypatch: pytest.MonkeyPatch) -> None:
    import open_agent_docs._sse as sse

    monkeypatch.setattr(sse, "RECONNECT_INITIAL", 30.0)
    c, _ = make_client(lambda r: httpx.Response(503))
    events = c.workspaces.events("w1")
    threading.Timer(0.2, events.close).start()
    assert list(events) == []  # 关了：迭代结束，不用等 30 秒
    assert events.closed


def test_connect_error_then_ok() -> None:
    n = {"i": 0}

    def h(r: httpx.Request) -> httpx.Response:
        n["i"] += 1
        if n["i"] < 3:
            raise httpx.ConnectError("refused", request=r)
        return httpx.Response(200, content=f"event: page.created\ndata: {PAGE}\n\n".encode())

    c, _ = make_client(h)
    with c.workspaces.events("w1") as events:
        ev = next(iter(events))
    assert ev.name == "page.created" and n["i"] == 3


async def test_async_events_and_reconnect() -> None:
    n = {"i": 0}

    async def chunks(text: str) -> AsyncIterator[bytes]:
        for i in range(0, len(text), 7):
            yield text[i : i + 7].encode()

    def h(r: httpx.Request) -> httpx.Response:
        n["i"] += 1
        if n["i"] == 1:
            return httpx.Response(200, content=chunks(f"id: 5\nevent: notification\ndata: {NOTE % 5}\n\n"))
        if n["i"] == 2:
            assert r.headers["last-event-id"] == "5"
            return httpx.Response(200, content=chunks(f"id: 6\nevent: notification\ndata: {NOTE % 6}\n\n"))
        return httpx.Response(401)

    c, rec = make_async_client(h)
    got = []
    async with c.agent.notifications.stream() as events:
        assert len(rec.requests) == 1
        with pytest.raises(oad.AuthenticationError):
            async for ev in events:
                got.append(ev.data.id)
    assert got == [5, 6]
    await c.close()

