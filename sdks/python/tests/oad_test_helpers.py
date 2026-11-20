"""单元测试的小工具：用 httpx.MockTransport 假装服务器；vectors.json 的固定用例。"""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Callable, Dict, List

import httpx

import open_agent_docs as oad

ROOT = Path(__file__).resolve().parents[3]
VECTORS = json.loads((ROOT / "sdks" / "conformance" / "vectors.json").read_text(encoding="utf-8"))

Handler = Callable[[httpx.Request], httpx.Response]


class Recorder:
    """记下收到的请求，按 handler 回"""

    def __init__(self, handler: Handler) -> None:
        self.handler = handler
        self.requests: List[httpx.Request] = []

    def __call__(self, req: httpx.Request) -> httpx.Response:
        self.requests.append(req)
        return self.handler(req)


def make_client(handler: Handler, **kw: Any) -> "tuple[oad.Client, Recorder]":
    rec = Recorder(handler)
    http = httpx.Client(transport=httpx.MockTransport(rec))
    kw.setdefault("base_url", "http://oad.test")
    return oad.Client(http_client=http, **kw), rec


def make_async_client(handler: Handler, **kw: Any) -> "tuple[oad.AsyncClient, Recorder]":
    rec = Recorder(handler)
    http = httpx.AsyncClient(transport=httpx.MockTransport(rec))
    kw.setdefault("base_url", "http://oad.test")
    return oad.AsyncClient(http_client=http, **kw), rec


def body(req: httpx.Request) -> Dict[str, Any]:
    return json.loads(req.content)
