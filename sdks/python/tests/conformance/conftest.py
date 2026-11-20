"""场景测试（docs/sdk-contract.md §14）：要真的服务器二进制，OAD_BINARY 指过去；没设就跳过（S14 用本地假服务器，照常跑）。

    OAD_BINARY=target/debug/open-agent-docs .venv/bin/python -m pytest tests/conformance
"""
from __future__ import annotations

import os
from pathlib import Path
from typing import Iterator

import pytest

import open_agent_docs as oad

SECRET = "conformance-header-secret"


@pytest.fixture
def need_binary() -> str:
    b = os.environ.get("OAD_BINARY")
    if not b:
        pytest.skip("没设 OAD_BINARY（服务器二进制的路径），场景测试跳过")
    return b


@pytest.fixture
def server(need_binary: str, tmp_path: Path) -> Iterator[oad.Server]:
    """每个场景自己的服务器和数据目录：header 登录"""
    with oad.start(data_dir=str(tmp_path / "data"), auth="header", header_secret=SECRET) as s:
        yield s


@pytest.fixture
def admin(server: oad.Server) -> Iterator[oad.Client]:
    with oad.Client(base_url=server.url, header_secret=SECRET) as c:
        yield c
