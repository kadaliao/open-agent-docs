"""冒烟测试：起 FastAPI 宿主（Open Agent Docs 跟着起来）、经宿主建页面、直接读回来、关掉。"""
import os
import tempfile

import httpx
from fastapi.testclient import TestClient


def test_smoke():
    with tempfile.TemporaryDirectory() as d:
        os.environ["DATA_DIR"] = d
        import app as host

        host.DATA_DIR = d
        with TestClient(host.app) as c:
            assert c.get("/host").json()["message"] == "宿主自己的接口"
            r = c.post("/notes", json={"title": "FastAPI 里建的页面", "text": "# 标题\n\n来自 Python 的正文"})
            assert r.status_code == 200, r.text
            page = r.json()
            base = host.app.state.oad.url
            got = httpx.get(f"{base}/api/v1/pages/{page['id']}/content", headers=host.oad_headers("demo-user", "演示用户"))
            assert "来自 Python 的正文" in got.json()["markdown"]
            # 不带宿主给的头：认不出是谁
            assert httpx.get(f"{base}/api/v1/me").status_code == 401
        assert host.app.state.oad.closed
