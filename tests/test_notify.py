"""给 Agent 的通知（notify.py）和接口的鉴权（server.guard）：评论通知谁、webhook 的签名和重试、轮询和 SSE、ack、admin 管 Agent。"""
import asyncio
import hashlib
import hmac
import json

import pytest
from conftest import asgi, settle, until
from open_agent_docs import agents, library, notify, pagecomments, server
from starlette.requests import Request


class FakeHook:
    """冒充 webhook 的接收方：记下收到的请求，按 codes 依次回状态码。"""

    def __init__(self, *codes: int):
        self.codes = list(codes)
        self.got: list[tuple[str, bytes, dict]] = []

    async def post(self, url, content, headers):
        self.got.append((url, content, headers))
        code = self.codes.pop(0) if self.codes else 200

        class R:
            status_code = code
        return R()


@pytest.fixture
def hook(monkeypatch):
    h = FakeHook()
    monkeypatch.setattr(notify, "client", h)
    monkeypatch.setattr(notify, "RETRY", (0.01, 0.01, 0.01))
    return h


@pytest.fixture
def page():
    made = []

    def make(by: str = "user", content: str = "# 标题\n\n第一段。\n\n第二段。") -> dict:
        p = library.save_page(None, "测试页", content, by=by)
        made.append(p["id"])
        return p
    yield make
    for pid in made:
        library.delete_page(pid, trash=False)


async def test_who_gets_notified(make_agent, page, hook):
    writer, _ = make_agent("写手")
    other, _ = make_agent("审稿")
    reader, _ = make_agent("只读的", scope="read")
    p = page(by=f"agent:{writer['id']}")
    # 不 @ 谁：通知建页面的 Agent
    c = await pagecomments.post(p["id"], "这一页再短一点")
    assert c["woke"] == [writer["id"]]
    # @ 到的优先（只读的 Agent 回不了评论，不通知）
    c = await pagecomments.post(p["id"], "@审稿 @只读的 看一下")
    assert c["woke"] == [other["id"]]
    # 回复：这串里最后说话的 Agent
    pagecomments.add(p["id"], f"agent:{other['id']}", "看过了", reply_to=c["id"])
    c2 = await pagecomments.post(p["id"], "那就这样", reply_to=c["id"])
    assert c2["woke"] == [other["id"]]
    # wake=False 谁也不通知；Agent 写的评论（add）不通知
    assert (await pagecomments.post(p["id"], "备注", wake=False))["woke"] == []
    n = notify.listing(writer["id"])
    assert len(n) == 1 and n[0]["comment"]["text"] == "这一页再短一点"
    assert "comment_page" in n[0]["prompt"] and f"page_id={p['id']}" in n[0]["prompt"]
    assert notify.listing(reader["id"]) == []
    await settle()
    assert hook.got == []   # 都没设 webhook


async def test_user_page_no_target(make_agent, page):
    make_agent("闲着的")
    p = page()
    c = await pagecomments.post(p["id"], "没人管的评论")
    assert c["woke"] == []


async def test_webhook_signed_and_retried(make_agent, page, hook):
    a, _ = make_agent("钩子", webhook="https://bots.example/hook")
    hook.codes = [500, 200]
    p = page(by=f"agent:{a['id']}")
    await pagecomments.post(p["id"], "改一下第一段")
    await until(lambda: len(hook.got) == 2, what="重试一次")
    await settle()
    url, body, headers = hook.got[-1]
    assert url == "https://bots.example/hook" and headers["x-oad-event"] == "comment"
    want = "sha256=" + hmac.new(agents.secret(a["id"]).encode(), headers["x-oad-timestamp"].encode() + b"." + body, hashlib.sha256).hexdigest()
    assert headers["x-oad-signature"] == want
    data = json.loads(body)
    assert data["page"]["id"] == p["id"] and data["agent"]["id"] == a["id"] and data["comment"]["text"] == "改一下第一段"
    n = notify.listing(a["id"])[-1]
    assert n["done"] and n["attempts"] == 2 and n["error"] == ""


async def test_poll_stream_ack(make_agent, page):
    a, tok = make_agent("轮询的")
    b, tok_b = make_agent("别人")
    p = page(by=f"agent:{a['id']}")
    auth_a = {"authorization": f"Bearer {tok}"}
    async with asgi(headers=auth_a) as c:
        assert (await c.get("/api/agent/notifications")).json() == []
        # 长轮询：等着的时候来了一条
        poll = asyncio.create_task(c.get("/api/agent/notifications", params={"wait": 5}))
        await asyncio.sleep(0.1)
        await pagecomments.post(p["id"], "第一条")
        got = (await poll).json()
        assert [n["comment"]["text"] for n in got] == ["第一条"]
        first = got[0]["id"]
        await pagecomments.post(p["id"], "第二条")
        assert [n["comment"]["text"] for n in (await c.get("/api/agent/notifications", params={"after": first})).json()] == ["第二条"]
        # 别的 Agent 看不到
        async with asgi(headers={"authorization": f"Bearer {tok_b}"}) as cb:
            assert (await cb.get("/api/agent/notifications")).json() == []
        # SSE（httpx 的 ASGITransport 不支持流式响应：直接读响应的生成器）：没给游标就补发还没处理的，再实时推
        req = Request({"type": "http", "method": "GET", "path": "/api/agent/notifications/stream", "query_string": b"",
                       "headers": [(b"authorization", f"Bearer {tok}".encode())]})
        req.state.agent = None
        r = await server.stream_notifications(req)
        assert r.media_type == "text/event-stream"
        it = r.body_iterator
        chunks = [await anext(it) for _ in range(3)]   # retry、两条补发的
        await pagecomments.post(p["id"], "第三条")
        chunks.append(await asyncio.wait_for(anext(it), 2))
        await it.aclose()
        datas = [json.loads(x.split("data: ", 1)[1]) for x in chunks if "data: " in x]
        assert [d["comment"]["text"] for d in datas] == ["第一条", "第二条", "第三条"]
        assert chunks[1].startswith(f"id: {first}\nevent: notification\n")
        assert (await c.post("/api/agent/notifications/ack", json={"upto": first})).json() == {"done": 1}
        assert [n["comment"]["text"] for n in (await c.get("/api/agent/notifications", params={"pending": True})).json()] == ["第二条", "第三条"]
        assert (await c.get("/api/agent/me")).json()["pending"] == 2


async def test_guard(make_agent, client):
    _, tok = make_agent("读的", scope="read")
    _, wtok = make_agent("写的")
    async with asgi() as anon:
        assert (await anon.get("/api/pages")).status_code == 401
        assert (await anon.get("/api/bootstrap")).json()["authed"] is False
        assert (await anon.post("/api/login", json={"password": "wrong"})).status_code == 401
        assert (await anon.get("/api/agent/me")).status_code == 401
    # 登录了：网页接口能用；跨站来的写请求拒掉
    assert (await client.get("/api/pages")).status_code == 200
    assert (await client.get("/api/bootstrap")).json()["authed"] is True
    r = await client.post("/api/pages", json={"title": "x"}, headers={"origin": "https://evil.example"})
    assert r.status_code == 403
    # Agent 的 token 不能用网页接口，也不能管 Agent（admin 才行）
    async with asgi(headers={"authorization": f"Bearer {wtok}"}) as c:
        assert (await c.get("/api/pages")).status_code == 401
        assert (await c.get("/api/agents")).status_code == 403
        assert (await c.get("/api/agent/me")).json()["name"] == "写的"
    # 只读的 token 不能传图
    async with asgi(headers={"authorization": f"Bearer {tok}"}) as c:
        assert (await c.put("/api/media", content=b"x")).status_code == 403


async def test_admin_manages_agents(client, make_agent):
    r = await client.post("/api/agents", json={"name": "dsh 宿主", "scope": "admin"})
    assert r.status_code == 200
    admin = r.json()
    try:
        async with asgi(headers={"authorization": f"Bearer {admin['token']}"}) as c:
            r = await c.post("/api/agents", json={"name": "Atlas", "scope": "write", "webhook": "http://127.0.0.1:9/hook"})
            assert r.status_code == 200, r.text
            atlas = r.json()
            assert atlas["token"].startswith("oad_") and atlas["secret"]
            assert (await c.post("/api/agents", json={"name": "x", "scope": "admin"})).status_code == 403
            assert (await c.patch(f"/api/agents/{atlas['id']}", json={"webhook": ""})).json()["webhook"] == ""
            names = [a["name"] for a in (await c.get("/api/agents")).json()]
            assert "Atlas" in names
            # admin 能收所有 Agent 的通知
            p = library.save_page(None, "交接", "要做的事。", by=f"agent:{atlas['id']}")
            await pagecomments.post(p["id"], "@Atlas 开始吧")
            got = (await c.get("/api/agent/notifications")).json()
            assert got[-1]["agent"]["name"] == "Atlas"
            assert (await c.delete(f"/api/agents/{atlas['id']}")).json()["revoked"]
            library.delete_page(p["id"], trash=False)
        # 网页上能撤销 admin
        assert (await client.delete(f"/api/agents/{admin['id']}")).json()["revoked"]
    finally:
        agents.revoke(admin["id"])
