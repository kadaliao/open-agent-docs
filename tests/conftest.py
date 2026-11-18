"""测试环境：临时数据目录、登录好的网页客户端、收集服务端发出的事件、建 Agent。

open_agent_docs 的模块在导入时就打开数据库，所以环境变量要在导入之前设好。
"""
import asyncio
import json
import os
import shutil
import struct
import sys
import tempfile
import zlib
from pathlib import Path

import httpx
import pytest

ROOT = Path(__file__).resolve().parent.parent
TMP = Path(tempfile.mkdtemp(prefix="oad-test-"))
os.environ["OAD_DATA_DIR"] = str(TMP)
os.environ["OAD_PASSWORD"] = "test-password"
for k in ("OAD_PUBLIC_URL", "OAD_LANG", "OAD_SECURE_COOKIE"):
    os.environ.pop(k, None)
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(Path(__file__).parent))

from open_agent_docs import agents, auth, events as events_mod, server  # noqa: E402

auth.ensure_password()
BASE = "http://127.0.0.1"


def pytest_sessionfinish(session, exitstatus):
    shutil.rmtree(TMP, ignore_errors=True)


class Sink:
    """冒充一个打开着的网页：events.emit 发出的事件都收在这里。"""

    def __init__(self):
        self.q = events_mod.subscribe()
        self._got: list[dict] = []

    @property
    def events(self) -> list[dict]:
        while not self.q.empty():
            m = self.q.get_nowait()
            if m is not None:
                self._got.append(json.loads(m))
        return self._got

    def of(self, type_: str) -> list[dict]:
        return [e for e in self.events if e.get("type") == type_]

    def close(self):
        events_mod.unsubscribe(self.q)


@pytest.fixture
async def events():
    sink = Sink()
    yield sink
    sink.close()
    await settle()


def asgi(**kw) -> httpx.AsyncClient:
    return httpx.AsyncClient(transport=httpx.ASGITransport(app=server.app), base_url=BASE, **kw)


@pytest.fixture
async def client():
    """登录好的网页客户端。"""
    async with asgi() as c:
        r = await c.post("/api/login", json={"password": "test-password"})
        assert r.status_code == 200, r.text
        yield c


_n = 0


@pytest.fixture
def make_agent():
    """建一个 Agent：返回 (记录, token)。名字不重复；测试完撤销。"""
    made: list[int] = []

    def make(name: str = "", scope: str = "write", webhook: str = "") -> tuple[dict, str]:
        global _n
        _n += 1
        a, token, _ = agents.create(name or f"Agent{_n}", scope, webhook)
        made.append(a["id"])
        return a, token
    yield make
    for aid in made:
        try:
            agents.revoke(aid)
        except LookupError:
            pass


def png(w: int, h: int, rgb: tuple) -> bytes:
    """一张纯色的小 PNG（测试图片用）。"""
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))

    def chunk(t: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + t + data + struct.pack(">I", zlib.crc32(t + data) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


async def settle(timeout: float = 5):
    """等后台的任务都跑完。"""
    me = asyncio.current_task()
    loop = asyncio.get_running_loop()
    end = loop.time() + timeout
    while loop.time() < end:
        if not [t for t in asyncio.all_tasks() if t is not me and not t.done()]:
            return
        await asyncio.sleep(0.02)


async def until(pred, timeout: float = 5, what: str = "条件"):
    """反复检查 pred()，成立就返回它的值；超时算失败。"""
    loop = asyncio.get_running_loop()
    end = loop.time() + timeout
    while True:
        if v := pred():
            return v
        if loop.time() > end:
            raise AssertionError(f"{timeout} 秒内没等到：{what}")
        await asyncio.sleep(0.02)
