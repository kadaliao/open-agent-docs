"""请求的形状、NOT_GIVEN、错误映射、重试（S14）、with_user 的头和 JWT 缓存、配置。"""
from __future__ import annotations

import base64
import json
import time
from typing import Any, Dict

import httpx
import pytest

import open_agent_docs as oad
from oad_test_helpers import body, make_async_client, make_client

PAGE = {"id": "p1", "workspace_id": "w1", "position": "a", "title": "周报", "created_at": 1, "updated_at": 2}


def ok(data: Any, status: int = 200) -> httpx.Response:
    return httpx.Response(status, json=data)


# ---------------------------------------------------------------- 配置


def test_defaults_and_env(monkeypatch: pytest.MonkeyPatch) -> None:
    c = oad.Client()
    assert c.base_url == "http://127.0.0.1:8848"
    assert c.token is None
    assert c.timeout == 60 and c.max_retries == 2
    monkeypatch.setenv("OAD_BASE_URL", "https://docs.example.com/")
    monkeypatch.setenv("OAD_TOKEN", "oad_env")
    c = oad.Client()
    assert c.base_url == "https://docs.example.com"
    assert c.token == "oad_env"
    assert oad.Client(token="oad_arg").token == "oad_arg"


def test_headers_on_every_request() -> None:
    c, rec = make_client(lambda r: ok(PAGE), token="oad_t", locale="zh", headers={"X-Trace": "1"})
    page = c.pages.get("p 1/x")
    assert isinstance(page, oad.Page) and page.title == "周报"
    req = rec.requests[0]
    assert str(req.url) == "http://oad.test/api/v1/pages/p%201%2Fx"
    assert req.headers["authorization"] == "Bearer oad_t"
    assert req.headers["user-agent"] == f"open-agent-docs-python/{oad.__version__}"
    assert req.headers["accept-language"] == "zh-CN"
    assert req.headers["x-trace"] == "1"


def test_unknown_fields_and_enum_values_are_kept() -> None:
    me = {
        "user": {"id": "u1", "name": "王小明", "kind": "robot", "new_field": 1},
        "workspaces": [{"id": "w1", "name": "默认", "role": "superadmin"}],
        "extra": True,
    }
    c, _ = make_client(lambda r: ok(me))
    got = c.me.get()
    assert got.user.kind == "robot"  # 服务器加了新的枚举值：不报错
    assert got.workspaces[0].role == "superadmin"
    assert got.user.new_field == 1  # type: ignore[attr-defined]  # 不认识的字段留着


# ---------------------------------------------------------------- NOT_GIVEN


def test_not_given_vs_none_in_body() -> None:
    c, rec = make_client(lambda r: ok(PAGE))
    c.pages.update("p1", title="新")
    assert body(rec.requests[-1]) == {"title": "新"}
    c.pages.update("p1", after=None)
    assert body(rec.requests[-1]) == {"after": None}
    c.pages.update("p1", after=oad.NOT_GIVEN, icon=None, extra_body={"x": 1})
    assert body(rec.requests[-1]) == {"icon": None, "x": 1}
    assert rec.requests[-1].method == "PATCH"
    assert rec.requests[-1].headers["content-type"] == "application/json"
    assert not oad.NOT_GIVEN and repr(oad.NOT_GIVEN) == "NOT_GIVEN"


def test_query_params_skip_not_given() -> None:
    c, rec = make_client(lambda r: ok({"results": []}))
    assert c.search("w1", q="周报") == []
    assert dict(rec.requests[-1].url.params) == {"q": "周报"}
    c.search("w1", q="x", limit=5, kind="page", extra_query={"debug": True})
    assert dict(rec.requests[-1].url.params) == {"q": "x", "limit": "5", "kind": "page", "debug": "true"}


def test_models_in_body_are_serialized() -> None:
    def h(r: httpx.Request) -> httpx.Response:
        return ok({"id": "t1", "page_id": "p1", "comments": [], "resolved": False, "created_at": 1}, 201)

    c, rec = make_client(h)
    try:
        c.threads.create("p1", body="看看", anchor=oad.Anchor(block_id="b1"), notify=False)
    except Exception:
        pass  # 只看请求
    assert body(rec.requests[-1]) == {"body": "看看", "anchor": {"block_id": "b1"}, "notify": False}


def test_create_page_with_markdown_is_two_requests() -> None:
    def h(r: httpx.Request) -> httpx.Response:
        if r.method == "POST":
            return ok(PAGE, 201)
        return httpx.Response(204)

    c, rec = make_client(h)
    page = c.pages.create("w1", title="周报", markdown="# 标题")
    assert page.id == "p1"
    assert [(r.method, r.url.path) for r in rec.requests] == [
        ("POST", "/api/v1/workspaces/w1/pages"),
        ("PUT", "/api/v1/pages/p1/content"),
    ]
    assert body(rec.requests[0]) == {"title": "周报"}
    assert body(rec.requests[1]) == {"markdown": "# 标题"}
    c.pages.create("w1", title="空的")
    assert len(rec.requests) == 3


def test_204_unwrap_null_on_404_bytes() -> None:
    def h(r: httpx.Request) -> httpx.Response:
        p = r.url.path
        if p.endswith("/share") and r.method == "GET":
            return httpx.Response(404, json={"error": {"code": "not_found", "message": "没分享"}})
        if p.endswith("/share"):
            return httpx.Response(204)
        if p.endswith("/pages"):
            return ok({"pages": [PAGE, PAGE]})
        if p.startswith("/api/v1/files/"):
            return httpx.Response(200, content=b"\x00bin")
        raise AssertionError(p)

    c, _ = make_client(h)
    assert c.pages.share.get("p1") is None
    assert c.pages.share.disable("p1") is None
    pages = c.pages.list("w1")
    assert [p.id for p in pages] == ["p1", "p1"] and isinstance(pages[0], oad.Page)
    assert c.files.download("f1") == b"\x00bin"


def test_default_workspace_is_cached() -> None:
    me = {"user": {"id": "u1", "name": "a", "kind": "user"}, "workspaces": [{"id": "w9", "name": "x", "role": "owner"}, {"id": "w2", "name": "y", "role": "owner"}]}
    c, rec = make_client(lambda r: ok(me))
    assert c.default_workspace() == "w9"
    assert c.default_workspace() == "w9"
    assert len(rec.requests) == 1


def test_default_workspace_empty() -> None:
    c, _ = make_client(lambda r: ok({"user": {"id": "u1", "name": "a", "kind": "user"}, "workspaces": []}))
    with pytest.raises(oad.OADError, match="工作区"):
        c.default_workspace()


# ---------------------------------------------------------------- 上传


def test_upload_shapes(tmp_path: Any) -> None:
    up = {"id": "f1", "url": "/api/v1/files/f1/a.txt", "name": "a.txt", "mime": "text/plain", "size": 5}
    c, rec = make_client(lambda r: ok(up, 201))
    c.files.upload("w1", b"hello", "a.txt")
    r = rec.requests[-1]
    assert r.headers["content-type"].startswith("multipart/form-data")
    assert b'name="file"; filename="a.txt"' in r.content and b"hello" in r.content and b"text/plain" in r.content
    f = tmp_path / "b.md"
    f.write_bytes(b"# x")
    c.files.upload("w1", f)
    assert b'filename="b.md"' in rec.requests[-1].content
    c.files.upload("w1", ("c.bin", b"\x01", "application/x-thing"))
    assert b"application/x-thing" in rec.requests[-1].content
    with open(f, "rb") as fh:
        c.files.upload("w1", fh)
    assert b'filename="b.md"' in rec.requests[-1].content


def test_import_csv() -> None:
    db = {"page": PAGE, "schema": {"props": []}, "views": [], "seq": 1}
    c, rec = make_client(lambda r: ok(db, 201))
    got = c.databases.import_csv("w1", "名字,数量\n苹果,3\n", title="水果")
    assert got.seq == 1 and got.schema.props == []
    r = rec.requests[-1]
    assert r.headers["content-type"].startswith("text/csv")
    assert r.content.decode() == "名字,数量\n苹果,3\n"
    assert dict(r.url.params) == {"title": "水果"}


# ---------------------------------------------------------------- 错误


@pytest.mark.parametrize(
    "status,cls",
    [
        (400, oad.BadRequestError),
        (401, oad.AuthenticationError),
        (403, oad.PermissionDeniedError),
        (404, oad.NotFoundError),
        (409, oad.ConflictError),
        (410, oad.GoneError),
        (413, oad.TooLargeError),
        (429, oad.RateLimitError),
        (500, oad.InternalServerError),
        (502, oad.BadGatewayError),
        (503, oad.InternalServerError),
        (418, oad.APIError),
    ],
)
def test_error_classes(status: int, cls: type) -> None:
    c, _ = make_client(
        lambda r: httpx.Response(status, json={"error": {"code": "some_code", "message": "出错了"}}, headers={"x-request-id": "req-1"}),
        max_retries=0,
    )
    with pytest.raises(cls) as ei:
        c.pages.get("p1")
    e = ei.value
    assert isinstance(e, oad.APIError) and isinstance(e, oad.OADError)
    assert type(e) is cls
    assert (e.status, e.code, e.message, e.request_id) == (status, "some_code", "出错了", "req-1")


def test_error_not_json() -> None:
    c, _ = make_client(lambda r: httpx.Response(502, text="Bad Gateway from nginx"), max_retries=0)
    with pytest.raises(oad.BadGatewayError) as ei:
        c.pages.get("p1")
    assert ei.value.code == "http_502"
    assert "nginx" in ei.value.message
    assert isinstance(ei.value, oad.InternalServerError)


def test_connection_error() -> None:
    def h(r: httpx.Request) -> httpx.Response:
        raise httpx.ConnectError("refused", request=r)

    c, rec = make_client(h, max_retries=1)
    with pytest.raises(oad.APIConnectionError):
        c.pages.get("p1")
    assert len(rec.requests) == 2  # 连不上：重试了一次


def test_timeout_error() -> None:
    def h(r: httpx.Request) -> httpx.Response:
        raise httpx.ReadTimeout("slow", request=r)

    c, rec = make_client(h)
    with pytest.raises(oad.APITimeoutError):
        c.pages.get("p1")
    assert len(rec.requests) == 1  # 读超时不重试
    assert isinstance(oad.APITimeoutError(), oad.APIConnectionError)


# ---------------------------------------------------------------- 重试（S14 的单元测试版；真起一个假服务器的在 conformance 里）


def test_s14_retry_after_then_ok(monkeypatch: pytest.MonkeyPatch) -> None:
    calls = {"n": 0}

    def h(r: httpx.Request) -> httpx.Response:
        calls["n"] += 1
        if calls["n"] == 1:
            return httpx.Response(429, json={"error": {"code": "too_many_requests", "message": "慢点"}}, headers={"retry-after": "1"})
        return ok({"mode": "header", "timezone": "UTC", "signup": "off"})

    c, _ = make_client(h)
    t0 = time.monotonic()
    cfg = c.auth.config()
    assert cfg.mode == "header"
    assert time.monotonic() - t0 >= 1.0
    assert calls["n"] == 2

    c0, _ = make_client(lambda r: httpx.Response(429, json={"error": {"code": "too_many_requests", "message": "慢点"}}, headers={"retry-after": "1"}), max_retries=0)
    with pytest.raises(oad.RateLimitError) as ei:
        c0.auth.config()
    assert ei.value.retry_after == 1


def test_retry_rules(monkeypatch: pytest.MonkeyPatch) -> None:
    sleeps: list = []
    import open_agent_docs._base_client as bc

    monkeypatch.setattr(bc.time, "sleep", lambda s: sleeps.append(s))
    # GET 遇到 503：重试到 max_retries
    c, rec = make_client(lambda r: httpx.Response(503), max_retries=3)
    with pytest.raises(oad.InternalServerError):
        c.pages.get("p1")
    assert len(rec.requests) == 4
    # 指数退避：0.5、1、2（测试里起点改小了），带 0–25% 随机
    assert len(sleeps) == 3 and sleeps[0] < sleeps[1] < sleeps[2]
    # POST 遇到 503：不重试（可能已经生效了）
    c, rec = make_client(lambda r: httpx.Response(503), max_retries=3)
    with pytest.raises(oad.InternalServerError):
        c.pages.create("w1", title="x")
    assert len(rec.requests) == 1
    # POST 遇到 429：重试
    c, rec = make_client(lambda r: httpx.Response(429), max_retries=2)
    with pytest.raises(oad.RateLimitError):
        c.pages.create("w1", title="x")
    assert len(rec.requests) == 3
    # 400 不重试
    c, rec = make_client(lambda r: httpx.Response(400), max_retries=2)
    with pytest.raises(oad.BadRequestError):
        c.pages.get("p1")
    assert len(rec.requests) == 1


def test_retry_after_is_capped(monkeypatch: pytest.MonkeyPatch) -> None:
    import open_agent_docs._base_client as bc

    sleeps: list = []
    monkeypatch.setattr(bc.time, "sleep", lambda s: sleeps.append(s))
    n = {"i": 0}

    def h(r: httpx.Request) -> httpx.Response:
        n["i"] += 1
        return httpx.Response(429, headers={"retry-after": "3600"}) if n["i"] == 1 else ok(PAGE)

    c, _ = make_client(h)
    c.pages.get("p1")
    assert sleeps == [60]


def test_connection_dropped_only_idempotent_retried() -> None:
    def h(r: httpx.Request) -> httpx.Response:
        raise httpx.RemoteProtocolError("peer closed", request=r)

    c, rec = make_client(h, max_retries=2)
    with pytest.raises(oad.APIConnectionError):
        c.pages.get("p1")
    assert len(rec.requests) == 3
    c, rec = make_client(h, max_retries=2)
    with pytest.raises(oad.APIConnectionError):
        c.pages.create("w1", title="x")
    assert len(rec.requests) == 1


async def test_async_retry_and_errors() -> None:
    n = {"i": 0}

    def h(r: httpx.Request) -> httpx.Response:
        n["i"] += 1
        if n["i"] == 1:
            return httpx.Response(429, headers={"retry-after": "0"})
        if r.url.path.endswith("/nope"):
            return httpx.Response(404, json={"error": {"code": "not_found", "message": "没有"}})
        return ok(PAGE)

    c, rec = make_async_client(h)
    page = await c.pages.get("p1")
    assert page.id == "p1" and len(rec.requests) == 2
    with pytest.raises(oad.NotFoundError):
        await c.pages.get("nope")
    await c.close()


# ---------------------------------------------------------------- with_user


def test_with_user_header_mode() -> None:
    me = {"user": {"id": "u-42", "name": "王小明", "kind": "user"}, "workspaces": [{"id": "w1", "name": "x", "role": "owner"}]}
    admin, rec = make_client(lambda r: ok(me), header_secret="hs", token="oad_admin")
    u = admin.with_user("u-42", name="王小明 (Ming)", email="x@example.com")
    assert u is not admin
    u.me.get()
    h = rec.requests[-1].headers
    assert h["x-oad-secret"] == "hs"
    assert h["x-oad-user-id"] == "u-42"
    assert h["x-oad-user-name"] == "%E7%8E%8B%E5%B0%8F%E6%98%8E%20(Ming)"  # 和 encodeURIComponent 一样
    assert h["x-oad-user-email"] == "x@example.com"
    assert "authorization" not in h
    # 原来的客户端不变
    admin.me.get()
    assert rec.requests[-1].headers["authorization"] == "Bearer oad_admin"
    assert "x-oad-user-id" not in rec.requests[-1].headers
    # 资源和默认工作区是各自的
    assert u.pages._client is u and admin.pages._client is admin
    assert u._http is admin._http  # 共享连接池
    u.close()  # 不关父客户端的连接池
    admin.me.get()
    with pytest.raises(oad.ConfigurationError):
        admin.with_user("u1", workspaces=["w1"])


def _claims(tok: str) -> Dict[str, Any]:
    p = tok.split(".")[1]
    return json.loads(base64.urlsafe_b64decode(p + "=" * (-len(p) % 4)))


def test_with_user_jwt_mode_caches(monkeypatch: pytest.MonkeyPatch) -> None:
    import open_agent_docs._base_client as bc

    now = [1_700_000_000.0]
    monkeypatch.setattr(bc.time, "time", lambda: now[0])
    me = {"user": {"id": "u9", "name": "小九", "kind": "user"}, "workspaces": []}
    admin, rec = make_client(lambda r: ok(me), jwt_secret="js", jwt_issuer="https://app", jwt_audience="oad", jwt_ttl=600)
    u = admin.with_user("u9", name="小九", workspaces=[{"id": "w1", "role": "viewer"}])
    u.me.get()
    t1 = rec.requests[-1].headers["authorization"].split(" ", 1)[1]
    c = _claims(t1)
    assert c == {"sub": "u9", "name": "小九", "workspaces": [{"id": "w1", "role": "viewer"}], "iat": 1_700_000_000, "exp": 1_700_000_600, "iss": "https://app", "aud": "oad"}
    now[0] += 500  # 离过期还有 100 秒：还用老的
    u.me.get()
    assert rec.requests[-1].headers["authorization"] == f"Bearer {t1}"
    now[0] += 50  # 只剩 50 秒（< 60）：换新的
    u.me.get()
    t2 = rec.requests[-1].headers["authorization"].split(" ", 1)[1]
    assert t2 != t1 and _claims(t2)["iat"] == 1_700_000_550


def test_with_user_config_errors() -> None:
    with pytest.raises(oad.ConfigurationError):
        oad.Client().with_user("u1")
    with pytest.raises(oad.ConfigurationError):
        oad.Client(header_secret="a", jwt_secret="b").with_user("u1")


async def test_async_with_user_and_default_workspace() -> None:
    me = {"user": {"id": "u1", "name": "a", "kind": "user"}, "workspaces": [{"id": "w7", "name": "x", "role": "owner"}]}
    admin, rec = make_async_client(lambda r: ok(me), header_secret="hs")
    async with admin:
        u = admin.with_user("u1", name="甲")
        assert await u.default_workspace() == "w7"
        assert rec.requests[-1].headers["x-oad-user-name"] == "%E7%94%B2"
