"""所有测试共用的：重试、重连别真等那么久；环境变量清干净。"""
from __future__ import annotations

import pytest


@pytest.fixture(autouse=True)
def _fast(monkeypatch: pytest.MonkeyPatch) -> None:
    """重试、重连别真等那么久（S14 的那个测试自己改回来）"""
    import open_agent_docs._base_client as bc
    import open_agent_docs._sse as sse

    monkeypatch.setattr(bc, "BACKOFF_INITIAL", 0.001)
    monkeypatch.setattr(sse, "RECONNECT_INITIAL", 0.01)
    monkeypatch.delenv("OAD_BASE_URL", raising=False)
    monkeypatch.delenv("OAD_TOKEN", raising=False)
