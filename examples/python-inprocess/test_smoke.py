"""冒烟测试：起 FastAPI 宿主（Open Agent Docs 跟着起来）、两个用户各自经宿主建页面、按各自的身份读回来、关掉。"""
import os
import tempfile

import httpx
from fastapi.testclient import TestClient

import open_agent_docs as oad


def test_smoke():
    with tempfile.TemporaryDirectory() as d:
        os.environ["DATA_DIR"] = d
        import app as host

        host.DATA_DIR = d
        with TestClient(host.app) as c:
            server = host.app.state.oad
            assert server.mode == "inprocess"  # 装了 open-agent-docs[server]：进程内
            assert c.get("/host").json()["message"] == "宿主自己的接口"

            alice = {"x-demo-user": "alice"}
            bob = {"x-demo-user": "bob"}
            assert c.get("/me", headers=alice).json()["name"] == "Alice 王"
            assert c.get("/me", headers=bob).json()["name"] == "Bob 李"
            # 没登录：宿主自己就拒了
            assert c.post("/notes", json={"title": "x", "text": "y"}).status_code == 401

            r = c.post("/notes", headers=alice, json={"title": "Alice 的周报", "text": "# 标题\n\n来自 Python 的正文"})
            assert r.status_code == 200, r.text
            a_page = r.json()
            r = c.post("/notes", headers=bob, json={"title": "Bob 的笔记", "text": "Bob 写的"})
            b_page = r.json()
            # 按请求的用户建的：created_by 是各自的
            alice_id = c.get("/me", headers=alice).json()["id"]
            bob_id = c.get("/me", headers=bob).json()["id"]
            assert a_page["created_by"] == alice_id and b_page["created_by"] == bob_id != alice_id
            assert "来自 Python 的正文" in c.get(f"/notes/{a_page['id']}", headers=alice).json()["markdown"]
            titles = {n["title"] for n in c.get("/notes", headers=bob).json()}
            assert {"Alice 的周报", "Bob 的笔记"} <= titles  # 同一个默认工作区
            assert c.get("/notes/nope", headers=alice).status_code == 404

            # 直接用 SDK 读（宿主之外，比如后台任务）
            sdk = oad.Client(base_url=server.url, header_secret=host.SECRET).with_user("alice")
            assert "来自 Python 的正文" in (sdk.pages.content.get(a_page["id"]).markdown or "")
            # 不带宿主给的头：认不出是谁
            assert httpx.get(f"{server.url}/api/v1/me").status_code == 401
        assert host.app.state.oad.closed
