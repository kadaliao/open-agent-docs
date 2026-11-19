"""起服务器、用 httpx 打接口、关掉；上下文管理器；配置不对报错。"""
import httpx
import pytest

import open_agent_docs as oad


def test_start_and_api(tmp_path):
    server = oad.start(data_dir=str(tmp_path), auth="none")
    try:
        assert server.url.startswith("http://127.0.0.1:")
        assert server.port > 0
        me = httpx.get(f"{server.url}/api/v1/me").json()
        ws = me["workspaces"][0]["id"]
        r = httpx.post(f"{server.url}/api/v1/workspaces/{ws}/pages", json={"title": "从 Python 建的页面"})
        assert r.status_code == 201
        page = r.json()
        r = httpx.put(f"{server.url}/api/v1/pages/{page['id']}/content", json={"markdown": "# 你好\n\n正文"})
        assert r.status_code == 204
        content = httpx.get(f"{server.url}/api/v1/pages/{page['id']}/content").json()
        assert "正文" in content["markdown"]
        spec = httpx.get(f"{server.url}/api/v1/openapi.json").json()
        assert spec["openapi"].startswith("3.1")
    finally:
        server.close()
        server.close()
    assert server.closed
    with pytest.raises(httpx.ConnectError):
        httpx.get(f"{server.url}/api/v1/me")


def test_context_manager_and_options(tmp_path):
    with oad.start(OAD_DATA_DIR=str(tmp_path), auth="none", rate_limit=5) as s:
        r = httpx.get(f"{s.url}/api/v1/me")
        assert r.headers["ratelimit-limit"] == "5"
    assert s.closed


def test_bad_options():
    with pytest.raises(RuntimeError, match="OAD_AUTH"):
        oad.start(auth="nope")
