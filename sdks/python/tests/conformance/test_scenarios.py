"""S1–S15：各语言 SDK 都要过的场景（docs/sdk-contract.md §14）。"""
from __future__ import annotations

import http.server
import json
import threading
import time
from pathlib import Path
from typing import Iterator, List

import httpx
import pytest

import open_agent_docs as oad
SECRET = "conformance-header-secret"  # 和 conftest.py 的一样
VECTORS = json.loads((Path(__file__).resolve().parents[4] / "sdks" / "conformance" / "vectors.json").read_text(encoding="utf-8"))


def test_s1_start_and_close(need_binary: str, tmp_path: Path) -> None:
    s = oad.start(data_dir=str(tmp_path), auth="header", header_secret=SECRET)
    try:
        assert s.url.startswith("http://127.0.0.1:") and s.port > 0
        c = oad.Client(base_url=s.url)
        assert c.auth.config().mode == "header"
    finally:
        s.close()
    s.close()  # 关两次没事
    assert s.closed
    with pytest.raises(httpx.ConnectError):
        httpx.get(f"{s.url}/api/v1/auth/config", timeout=2)


def test_s2_with_user(admin: oad.Client) -> None:
    u = admin.with_user("u1", name="王小明")
    me = u.me.get()
    assert me.user.name == "王小明"
    assert u.default_workspace() == me.workspaces[0].id


def test_s3_pages_markdown(admin: oad.Client) -> None:
    u = admin.with_user("u3", name="小三")
    ws = u.default_workspace()
    page = u.pages.create(ws, title="周报", markdown="# 标题\n\n正文")
    content = u.pages.content.get(page.id)
    assert content.markdown is not None and "# 标题" in content.markdown and "正文" in content.markdown
    assert page.id in [p.id for p in u.pages.list(ws)]
    assert u.pages.update(page.id, title="月报").title == "月报"
    assert u.pages.get(page.id).title == "月报"
    u.pages.content.replace(page.id, markdown="# 新的\n\n改过了", base=content.version)
    assert "改过了" in (u.pages.content.get(page.id).markdown or "")
    u.pages.delete(page.id)
    with pytest.raises(oad.NotFoundError):
        u.pages.get(page.id)


def test_s4_errors(server: oad.Server, admin: oad.Client) -> None:
    u = admin.with_user("u4")
    with pytest.raises(oad.NotFoundError) as ei:
        u.pages.get("nope")
    e = ei.value
    assert (e.status, e.code) == (404, "not_found") and e.message
    with pytest.raises(oad.AuthenticationError) as ei2:
        oad.Client(base_url=server.url, token="oad_bogus").me.get()
    assert ei2.value.status == 401
    # 错误信息的语言
    with pytest.raises(oad.NotFoundError) as ei3:
        oad.Client(base_url=server.url, header_secret=SECRET, locale="en").with_user("u4").pages.get("nope")
    assert ei3.value.message != e.message


def test_s5_agent_token(server: oad.Server, admin: oad.Client) -> None:
    u = admin.with_user("u5")
    created = u.agents.create(u.default_workspace(), name="bot", scope="write")
    assert created.token.startswith("oad_") and created.webhook_secret.startswith("whsec_")
    bot = oad.Client(base_url=server.url, token=created.token)
    assert bot.me.get().user.kind == "agent"


def test_s6_pagination(server: oad.Server) -> None:
    n: List[str] = []

    def count(req: httpx.Request) -> None:
        if req.url.path.endswith("/versions") and req.method == "GET":
            n.append(str(req.url))

    http = httpx.Client(event_hooks={"request": [count]})
    u = oad.Client(base_url=server.url, header_secret=SECRET, http_client=http).with_user("u6")
    page = u.pages.create(u.default_workspace(), title="版本")
    for i in range(5):
        u.pages.content.replace(page.id, markdown=f"# 第 {i} 版\n\n内容 {i}")
        u.pages.versions.create(page.id, label=f"v{i}")
    ids = [v.id for v in u.pages.versions.list(page.id, limit=2)]
    assert len(ids) >= 5
    assert len(set(ids)) == len(ids)
    assert len(n) >= 3


def test_s7_events(admin: oad.Client) -> None:
    u = admin.with_user("u7")
    ws = u.default_workspace()
    with u.workspaces.events(ws) as events:
        watchdog = threading.Timer(10, events.close)
        watchdog.start()
        page = u.pages.create(ws, title="有事件")
        got = None
        for ev in events:
            if ev.name == "page.created" and isinstance(ev.data, oad.Page) and ev.data.id == page.id:
                got = ev
                break
        watchdog.cancel()
    assert got is not None, "10 秒内没收到 page.created"
    assert got.data.title == "有事件"


def test_s8_agent_notifications(server: oad.Server, admin: oad.Client) -> None:
    u = admin.with_user("u8", name="小八")
    ws = u.default_workspace()
    created = u.agents.create(ws, name="bot", scope="write")
    bot = oad.Client(base_url=server.url, token=created.token)
    page = u.pages.create(ws, title="请 bot 看看")
    u.threads.create(page.id, body="@bot 看看")
    note = None
    with bot.agent.notifications.stream() as stream:
        watchdog = threading.Timer(10, stream.close)
        watchdog.start()
        for ev in stream:
            note = ev.data
            break
        watchdog.cancel()
    assert isinstance(note, oad.AgentNotification), "10 秒内没收到通知"
    assert "看看" in note.comment.body
    assert note.page.id == page.id
    # 取 → 处理 → 确认
    assert bot.agent.notifications.ack(ids=[note.id]).acked == 1
    assert note.id not in [x.id for x in bot.agent.notifications.list()]


def test_s9_jwt(need_binary: str, tmp_path: Path) -> None:
    secret = "jwt-secret-for-conformance-0123456789"
    with oad.start(data_dir=str(tmp_path), auth="jwt", jwt_secret=secret) as s:
        u = oad.Client(base_url=s.url, jwt_secret=secret).with_user("u9", name="小九")
        me = u.me.get()
        assert me.user.name == "小九"
        bad = oad.Client(base_url=s.url, jwt_secret="another-secret-0123456789abcdef").with_user("u9")
        with pytest.raises(oad.AuthenticationError):
            bad.me.get()
        # 手签的也认
        tok = oad.sign_jwt(secret, "u9b", name="九号", workspaces=[me.workspaces[0].id])
        assert oad.Client(base_url=s.url, token=tok).me.get().user.name == "九号"


@pytest.mark.parametrize("case", VECTORS["webhook"], ids=[c["name"] for c in VECTORS["webhook"]])
def test_s10_webhook(case: dict) -> None:
    def verify() -> oad.AgentNotification:
        return oad.verify_webhook(case["secret"], case["headers"], case["body"], tolerance=case["tolerance_s"], now=case["now_ms"] / 1000)

    if case["expect"] == "ok":
        verify()
    else:
        with pytest.raises(oad.WebhookVerificationError) as ei:
            verify()
        assert ei.value.kind == case["expect"]


def test_s11_embed_and_jwt() -> None:
    for case in VECTORS["embed_url"]:
        assert oad.embed_url(case["base_url"], case["page_id"], **case["options"]) == case["expect"]
    v = VECTORS["jwt"]
    i = v["input"]
    tok = oad.sign_jwt(v["secret"], i["user_id"], name=i["name"], email=i["email"], workspaces=i["workspaces"], ttl=i["ttl_s"], issuer=i["issuer"], audience=i["audience"], now=v["now_s"])
    import base64

    part = tok.split(".")[1]
    assert json.loads(base64.urlsafe_b64decode(part + "=" * (-len(part) % 4))) == v["expect_claims"]


def test_s12_share(admin: oad.Client) -> None:
    u = admin.with_user("u12")
    page = u.pages.create(u.default_workspace(), title="分享")
    assert u.pages.share.get(page.id) is None
    share = u.pages.share.enable(page.id)
    assert "/s/" in share.url
    assert u.pages.share.get(page.id) is not None
    u.pages.share.disable(page.id)
    assert u.pages.share.get(page.id) is None


def test_s13_html(admin: oad.Client) -> None:
    u = admin.with_user("u13")
    page = u.pages.create(u.default_workspace(), title="HTML", markdown="# 标题\n\n<script>alert(1)</script> 正文")
    try:
        content = u.pages.content.get(page.id, format="html")
    except oad.BadRequestError as e:
        pytest.skip(f"服务器还不支持 format=html（W-WEB 在做）：{e.message}")
    html = getattr(content, "html", None)
    assert isinstance(html, str)
    assert "<h1" in html and "<script" not in html


class _FakeHandler(http.server.BaseHTTPRequestHandler):
    calls = 0

    def do_GET(self) -> None:  # noqa: N802
        cls = type(self)
        cls.calls += 1
        if cls.calls % 2 == 1:
            body = b'{"error":{"code":"too_many_requests","message":"slow down"}}'
            self.send_response(429)
            self.send_header("Retry-After", "1")
        else:
            body = b'{"mode":"none","timezone":"UTC","signup":"off"}'
            self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a: object) -> None:
        pass


@pytest.fixture
def fake_server() -> Iterator[str]:
    _FakeHandler.calls = 0
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _FakeHandler)
    t = threading.Thread(target=srv.serve_forever, daemon=True)
    t.start()
    try:
        yield f"http://127.0.0.1:{srv.server_address[1]}"
    finally:
        srv.shutdown()
        srv.server_close()


def test_s14_retry(fake_server: str) -> None:
    t0 = time.monotonic()
    cfg = oad.Client(base_url=fake_server).auth.config()
    assert cfg.mode == "none"
    assert time.monotonic() - t0 >= 1.0
    assert _FakeHandler.calls == 2
    with pytest.raises(oad.RateLimitError) as ei:
        oad.Client(base_url=fake_server, max_retries=0).auth.config()
    assert ei.value.retry_after == 1


def test_s15_files(admin: oad.Client) -> None:
    u = admin.with_user("u15")
    up = u.files.upload(u.default_workspace(), b"hello", "a.txt")
    assert up.name == "a.txt" and up.size == 5
    assert u.files.download(up.id) == b"hello"
    assert u.files.download_named(up.id, "a.txt") == b"hello"


# ---------------------------------------------------------------- 异步版：同样的几步


async def test_async_client_scenarios(server: oad.Server) -> None:
    async with oad.AsyncClient(base_url=server.url, header_secret=SECRET) as admin:
        u = admin.with_user("ua", name="异步")
        assert (await u.me.get()).user.name == "异步"
        ws = await u.default_workspace()
        async with u.workspaces.events(ws) as events:
            page = await u.pages.create(ws, title="异步页面", markdown="# 异步\n\n正文")
            async for ev in events:
                if ev.name == "page.created" and ev.data.id == page.id:
                    break
        assert "正文" in ((await u.pages.content.get(page.id)).markdown or "")
        for i in range(3):
            await u.pages.content.replace(page.id, markdown=f"第 {i} 版")
            await u.pages.versions.create(page.id, label=f"v{i}")
        ids = [v.id async for v in u.pages.versions.list(page.id, limit=1)]
        assert len(ids) >= 3 and len(set(ids)) == len(ids)
        with pytest.raises(oad.NotFoundError):
            await u.pages.get("nope")
