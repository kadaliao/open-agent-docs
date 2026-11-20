"""oad.start 的小零件：配置名转环境变量、找二进制、平台。真起服务器的在 conformance/。"""
from __future__ import annotations

from pathlib import Path

import pytest

import open_agent_docs as oad
from open_agent_docs import _server


def test_env_name_and_value() -> None:
    assert _server.env_name("data_dir") == "OAD_DATA_DIR"
    assert _server.env_name("headerSecret") == "OAD_HEADER_SECRET"
    assert _server.env_name("jwt-secret") == "OAD_JWT_SECRET"
    assert _server.env_name("OAD_PORT") == "OAD_PORT"
    assert _server.env_value(True) == "1" and _server.env_value(False) == "0"
    assert _server.env_value(["a", "b"]) == "a,b"
    assert _server.env_value(5) == "5"


def test_find_binary_order(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    a = tmp_path / "a"
    b = tmp_path / "b"
    a.write_text("")
    b.write_text("")
    monkeypatch.setenv("OAD_BINARY", str(b))
    assert _server.find_binary(str(a)) == a
    assert _server.find_binary() == b
    monkeypatch.setenv("OAD_BINARY", str(tmp_path / "nope"))
    with pytest.raises(oad.OADError, match="不存在"):
        _server.find_binary()
    monkeypatch.delenv("OAD_BINARY")
    monkeypatch.setattr(_server, "cache_dir", lambda: tmp_path / "cache")
    with pytest.raises(oad.OADError, match="缓存里没有"):
        _server.find_binary(version="9.9.9", download=False)
    cached = tmp_path / "cache" / "v9.9.9" / "open-agent-docs"
    cached.parent.mkdir(parents=True)
    cached.write_text("")
    assert _server.find_binary(version="9.9.9", download=False) == cached


def test_target(monkeypatch: pytest.MonkeyPatch) -> None:
    import platform
    import sys

    for plat, mach, want in [
        ("darwin", "arm64", "aarch64-apple-darwin"),
        ("darwin", "x86_64", "x86_64-apple-darwin"),
        ("linux", "x86_64", "x86_64-unknown-linux-gnu"),
        ("linux", "aarch64", "aarch64-unknown-linux-gnu"),
        ("linux", "arm64", "aarch64-unknown-linux-gnu"),
    ]:
        monkeypatch.setattr(sys, "platform", plat)
        monkeypatch.setattr(platform, "machine", lambda m=mach: m)
        assert _server._target() == want
    monkeypatch.setattr(sys, "platform", "win32")
    with pytest.raises(oad.OADError, match="Docker"):
        _server._target()


def test_start_reports_early_exit(tmp_path: Path) -> None:
    fake = tmp_path / "fake-oad"
    fake.write_text("#!/bin/sh\necho 'OAD_AUTH 只能是 local、header、jwt、none' >&2\nexit 3\n")
    fake.chmod(0o755)
    with pytest.raises(oad.OADError, match="(?s)退出码 3.*OAD_AUTH"):
        oad.start(binary=str(fake), auth="nope")
