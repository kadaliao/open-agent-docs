"""Agent 的 MCP 端点（mcpserver.py）和 Agent 的管理（agents.py）：token 的建、查、撤销，/mcp 的鉴权，每个工具，作者记成 agent:N，事件，锁定页面。

工具用 SDK 自带的 MCP 客户端调（走 httpx2 的 ASGITransport，和 Claude Code 一样经 HTTP），鉴权和老协议版本直接发 JSON-RPC。
"""
import base64

import httpx2
import pytest
from conftest import asgi, png, settle
from mcp import Client
from mcp.client.streamable_http import streamable_http_client
from open_agent_docs import agents, library, media, notify, pagecomments, pagedoc, server
from open_agent_docs.db import conn

DOC = "# 周报\n\n本周完成了三件事。\n\n- [ ] 联系销售"
RPC_HEADERS = {"accept": "application/json, text/event-stream", "content-type": "application/json"}
READ_TOOLS = {"search_pages", "list_pages", "read_page", "list_comments", "db_query", "my_notifications"}
WRITE_TOOLS = READ_TOOLS | {"create_page", "edit_page", "rewrite_page", "comment_page", "upload_image",
                            "db_create", "db_schema", "db_add", "db_update"}


@pytest.fixture
def token(make_agent):
    """建 Agent：make(name, scope) -> (记录, token 原文)，测试完撤销。"""
    def make(name: str = "Claude Code", scope: str = "write"):
        return make_agent(name, scope)
    return make


@pytest.fixture
def pages():
    made: list[int] = []
    yield made
    for pid in made:
        if library.get_page(pid, False):
            library.delete_page(pid)


def by(a: dict) -> str:
    return f"agent:{a['id']}"


class Agent:
    """用 SDK 的 MCP 客户端连 /mcp。"""

    def __init__(self, tok: str):
        self.hc = httpx2.AsyncClient(transport=httpx2.ASGITransport(app=server.app), base_url="http://127.0.0.1",
                                     headers={"authorization": f"Bearer {tok}"})
        self.client = Client(streamable_http_client("http://127.0.0.1/mcp", http_client=self.hc))

    async def __aenter__(self):
        await self.client.__aenter__()
        return self

    async def __aexit__(self, *exc):
        await self.client.__aexit__(*exc)
        await self.hc.aclose()

    async def tools(self) -> list[str]:
        return [t.name for t in (await self.client.list_tools()).tools]

    async def call(self, tool: str, /, **args) -> str:
        r = await self.client.call_tool(tool, args)
        text = "".join(c.text for c in r.content if getattr(c, "text", None))
        return ("ERROR " if r.is_error else "") + text


async def rpc(c, method: str, params: dict | None = None, tok: str | None = None, **headers):
    h = {**RPC_HEADERS, **headers}
    if tok is not None:
        h["authorization"] = f"Bearer {tok}"
    return await c.post("/mcp", headers=h, json={"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}})


# ---------- token ----------
def test_tokens(token):
    t, tok = token("  Claude   Code  ", "write")
    assert t["name"] == "Claude Code" and t["scope"] == "write" and t["revoked"] is None and t["last_used"] is None
    assert tok.startswith("oad_") and len(tok) > 40
    row = conn.execute("select hash from agents where id=?", (t["id"],)).fetchone()
    assert row[0] != tok and tok not in row[0]   # 库里只有 sha256
    assert agents.check(tok)["id"] == t["id"]
    assert agents.get(t["id"])["last_used"]   # 用过就记下时间
    assert agents.check(tok + "x") is None and agents.check("") is None
    assert {"id": t["id"], "name": "Claude Code", "revoked": False} in agents.names()
    assert not any("hash" in x or "token" in x or "secret" in x for x in agents.all_agents())
    agents.revoke(t["id"])
    assert agents.check(tok) is None
    assert {"id": t["id"], "name": "Claude Code", "revoked": True} in agents.names()   # 撤销了名字还在：写过的段要显示
    assert agents.revoke(t["id"])["revoked"]   # 撤销两次没事
    for bad in (("   ", "write"), ("x", "owner"), ("用户", "write"), ("x", "write", "ftp://hook")):
        with pytest.raises(ValueError):
            agents.create(*bad)
    with pytest.raises(LookupError):
        agents.revoke(999999)


async def test_settings_api(client, token, events):
    c = client
    r = await c.post("/api/agents", json={"name": "Codex", "scope": "read"})
    assert r.status_code == 200
    made = r.json()
    assert made["token"].startswith("oad_") and made["scope"] == "read" and made["secret"]
    listed = (await c.get("/api/agents")).json()
    mine = next(x for x in listed if x["id"] == made["id"])
    assert "token" not in mine and "hash" not in mine and "secret" not in mine   # 原文只在建的时候给一次
    assert mine["pending"] == 0
    assert any(e["agents"] for e in events.of("agents_changed"))
    assert (await c.post("/api/agents", json={"name": "", "scope": "read"})).status_code == 400
    assert (await c.delete(f"/api/agents/{made['id']}")).json()["revoked"]
    assert (await c.delete("/api/agents/999999")).status_code == 404
    boot = (await c.get("/api/bootstrap")).json()
    assert {"id": made["id"], "name": "Codex", "revoked": True} in boot["agents"]
    # 撤销了的能从列表里删掉，名字还在（写过的段要显示）；没撤销的不能删
    live, _ = token("还在用")
    assert (await c.post(f"/api/agents/{live['id']}/hide")).status_code == 400
    assert (await c.post(f"/api/agents/{made['id']}/hide")).json()["ok"]
    assert made["id"] not in [x["id"] for x in (await c.get("/api/agents")).json()]
    assert live["id"] in [x["id"] for x in (await c.get("/api/agents")).json()]
    assert {"id": made["id"], "name": "Codex", "revoked": True} in (await c.get("/api/bootstrap")).json()["agents"]
    assert agents.name(made["id"]) == "Codex"
    assert (await c.post("/api/agents/999999/hide")).status_code == 404
    # webhook 的密钥：看、换
    s1 = (await c.get(f"/api/agents/{live['id']}/secret")).json()["secret"]
    s2 = (await c.post(f"/api/agents/{live['id']}/secret")).json()["secret"]
    assert s1 and s2 != s1 and agents.secret(live["id"]) == s2


# ---------- 鉴权 ----------
async def test_auth(client, token):
    t, tok = token()
    async with asgi() as c:
        assert (await rpc(c, "tools/list")).status_code == 401
        r = await rpc(c, "tools/list", tok="oad_wrong")
        assert r.status_code == 401 and r.headers["www-authenticate"].startswith("Bearer")
        assert (await rpc(c, "tools/list", authorization=tok)).status_code == 401   # 不是 Bearer
        r = await rpc(c, "tools/list", tok=tok)
        assert r.status_code == 200 and "create_page" in r.text
        # Claude Code 这类客户端用的老协议版本：先 initialize
        r = await rpc(c, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                        "clientInfo": {"name": "claude-code", "version": "1"}}, tok=tok)
        assert r.json()["result"]["serverInfo"]["name"] == "open-agent-docs" and "read_page" in r.json()["result"]["instructions"]
        # 网页跨站发不来（来源不对）；反代后面的域名照样能用
        assert (await rpc(c, "tools/list", tok=tok, origin="https://evil.example")).status_code == 403
        assert (await rpc(c, "tools/list", tok=tok, host="docs.example.com")).status_code == 200
    # /mcp 不看登录 cookie，只认 token
    assert (await rpc(client, "tools/list")).status_code == 401
    agents.revoke(t["id"])
    async with asgi() as c:
        assert (await rpc(c, "tools/list", tok=tok)).status_code == 401   # 撤销马上生效


async def test_read_only(token, pages):
    _, tok = token("Reader", "read")
    pid = library.save_page(None, "周报", DOC)["id"]
    pages.append(pid)
    async with Agent(tok) as a:
        assert set(await a.tools()) == READ_TOOLS
        assert "周报" in await a.call("read_page", page_id=pid)
        out = await a.call("create_page", title="x", content="y")   # 看不到，硬调也不行
        assert out.startswith("ERROR") and "只读" in out
        assert "只读" in await a.call("comment_page", page_id=pid, text="hi")
        assert "只读" in await a.call("upload_image", data_base64=base64.b64encode(png(2, 2, (1, 2, 3))).decode())
    assert len(pagecomments.comments(pid)) == 0


async def test_admin_sees_write_tools(token):
    _, tok = token("宿主", "admin")
    async with Agent(tok) as a:
        assert set(await a.tools()) == WRITE_TOOLS


# ---------- 工具 ----------
async def test_create_read_edit(token, pages, events):
    t, tok = token("Claude Code")
    who = by(t)
    async with Agent(tok) as a:
        assert set(await a.tools()) == WRITE_TOOLS
        out = await a.call("create_page", title="调研：向量库", content="# 结论\n\n用 sqlite-vec。\n\n- 便宜\n- 够快")
        assert out.startswith("已建页面") and "http://127.0.0.1/pages/" in out
        pid = int(out.split("#")[1].split("「")[0])
        pages.append(pid)
        p = library.get_page(pid, False)
        assert p["agent_id"] == t["id"] and p["bot_id"] is None and p["version"] == 1
        _, blks = pagedoc.load(pid)
        assert {b["by"] for b in blks} == {who}
        assert events.of("pages_changed")

        text = await a.call("read_page", page_id=pid)
        assert "版本 1" in text and "「Claude Code」建的" in text
        ids = [b["id"] for b in blks]
        assert all(f"⟦{i}⟧" in text for i in ids)

        out = await a.call("edit_page", page_id=pid, version=1, ops=[
            {"op": "replace", "block": ids[1], "markdown": "用 sqlite-vec（已经在用 sqlite）。"},
            {"op": "append", "markdown": "- 不用另起服务"}])
        assert out.startswith("已改页面") and "版本 2" in out
        _, blks = pagedoc.load(pid)
        assert blks[1]["md"] == "用 sqlite-vec（已经在用 sqlite）。" and blks[1]["by"] == who and blks[-1]["md"] == "- 不用另起服务"
        ev = [e for e in events.of("page_changed") if e["page_id"] == pid]
        assert ev and ev[-1]["by"] == who and ids[1] in ev[-1]["changed"]

        # 用户在网页上改了第二段，Agent 还拿着版本 2 去改它：冲突，告诉它现在的内容
        pagedoc.apply(pid, 2, [{"op": "replace", "block": ids[1], "markdown": "用户改的"}], "user")
        out = await a.call("edit_page", page_id=pid, version=2, ops=[{"op": "replace", "block": ids[1], "markdown": "又改"}])
        assert "没改" in out and "用户改的" in out and "version=3" in out
        out = await a.call("edit_page", page_id=pid, version=2, ops=[{"op": "delete", "block": ids[0]}])   # 别的段照样能改
        assert out.startswith("已改页面")
        out = await a.call("edit_page", page_id=pid, version=4, ops=[{"op": "replace", "markdown": "x"}])
        assert "没改" in out and "block" in out
        out = await a.call("edit_page", page_id=pid, version=4, ops=[{"op": "bogus"}])
        assert out.startswith("ERROR")   # 不在 enum 里：参数检查就挡住
        sub = await a.call("create_page", title="子页面", content="正文", parent_id=pid)
        sid = int(sub.split("#")[1].split("「")[0])
        pages.insert(0, sid)
        assert library.get_page(sid, False)["parent_id"] == pid
        assert "没建" in await a.call("create_page", title="x", content="")
    await settle()


async def test_rewrite(token, pages, events):
    t, tok = token("Codex")
    b, _ = token("写手")
    pid = library.save_page(None, "周报", DOC, by=by(b))["id"]
    pages.append(pid)
    _, blks = pagedoc.load(pid)
    async with Agent(tok) as a:
        out = await a.call("rewrite_page", page_id=pid, version=1, content=DOC.replace("三件事", "四件事"), title="周报（改）")
        assert out.startswith("已保存页面") and "版本 2" in out
        p, now = pagedoc.load(pid)
        assert p["title"] == "周报（改）"
        assert [x["id"] for x in now] == [x["id"] for x in blks]   # 按位置保住编号
        assert now[0]["by"] == by(b) and now[1]["by"] == by(t)   # 没变的段还是原来的作者
        assert any(e["page_id"] == pid and e["changed"] == [] for e in events.of("page_changed"))
        assert "没有变化" in await a.call("rewrite_page", page_id=pid, version=2, content=p["content"])
        pagedoc.apply(pid, 2, [{"op": "replace", "block": blks[1]["id"], "markdown": "用户改的"}], "user")
        out = await a.call("rewrite_page", page_id=pid, version=2, content=DOC)
        assert "没保存" in out and "用户改的" in out
        assert "没有页面" in await a.call("rewrite_page", page_id=999999, version=1, content="x")


async def test_locked_and_html(token, pages):
    _, tok = token()
    pid = library.save_page(None, "周报", DOC)["id"]
    html = library.save_page(None, "看板", "<h1>hi</h1>", "html")["id"]
    pages.extend([pid, html])
    library.update_page(pid, locked=True)
    async with Agent(tok) as a:
        assert "已锁定" in await a.call("read_page", page_id=pid)
        assert "锁定" in await a.call("edit_page", page_id=pid, version=1, ops=[{"op": "append", "markdown": "x"}])
        assert "锁定" in await a.call("rewrite_page", page_id=pid, version=1, content="x")
        assert "已在页面" in await a.call("comment_page", page_id=pid, text="这里要改")   # 锁了还能评论
        assert "hi" in await a.call("read_page", page_id=html)
        assert "html" in await a.call("edit_page", page_id=html, version=1, ops=[{"op": "append", "markdown": "x"}])
        assert "html" in await a.call("rewrite_page", page_id=html, version=1, content="x")
    assert library.get_page(pid, False)["version"] == 1


async def test_list_and_search(token, pages):
    _, tok = token()
    top = library.save_page(None, "项目甲", "# 项目甲\n\n总览")["id"]
    kid = library.save_page(None, "甲的会议纪要", "讨论了猕猴桃供应链", parent_id=top)["id"]
    pages.extend([kid, top])
    async with Agent(tok) as a:
        out = await a.call("list_pages")
        line_top = next(ln for ln in out.splitlines() if f"#{top}《" in ln)
        line_kid = next(ln for ln in out.splitlines() if f"#{kid}《" in ln)
        assert line_kid.startswith("  - ") and not line_top.startswith(" ") and "版本 1" in line_kid
        out = await a.call("list_pages", parent_id=top)
        assert f"#{kid}《甲的会议纪要》" in out and f"#{top}《" in out.splitlines()[0] and "1 个页面" in out
        assert "没有页面" in await a.call("list_pages", parent_id=999999)
        out = await a.call("search_pages", query="猕猴桃")
        assert f"#{kid}《甲的会议纪要》" in out and f"http://127.0.0.1/pages/{kid}" in out
        assert await a.call("search_pages", query="完全不存在的词zzqx") == "没找到相关的页面"


async def test_comments(token, pages, events):
    """Agent 写的评论不通知别的 Agent；能回复用户的评论、标成已解决。"""
    t, tok = token("Claude Code")
    other, _ = token("写手")
    pid = library.save_page(None, "周报", DOC, by=by(other))["id"]
    pages.append(pid)
    ids = [x["id"] for x in pagedoc.load(pid)[1]]
    root = pagecomments.add(pid, "user", "数字对吗？", ids[1])
    async with Agent(tok) as a:
        out = await a.call("list_comments", page_id=pid)
        assert f"#{root['id']} ⟦{ids[1]}⟧ 用户：数字对吗？" in out
        out = await a.call("comment_page", page_id=pid, text="核对过了，是三件", reply_to=root["id"], resolve=True)
        assert "回复评论" in out and "已解决" in out
        assert "都解决了" in await a.call("list_comments", page_id=pid)
        out = await a.call("list_comments", page_id=pid, include_resolved=True)
        assert "（已解决）" in out and "Claude Code：核对过了" in out
        out = await a.call("comment_page", page_id=pid, text="@写手 第一段也看看", block=ids[0])
        assert "写了评论" in out
        assert "没写" in await a.call("comment_page", page_id=pid, text="x", block="nope")
    mine = [c for c in pagecomments.comments(pid) if c["by"] == by(t)]
    assert len(mine) == 2 and all(not c["woke"] for c in mine)
    assert notify.listing(other["id"]) == [] and notify.listing(t["id"]) == []
    assert any(e["page_id"] == pid for e in events.of("page_comments"))
    assert "Claude Code：@写手" in pagedoc.render(pid)   # 别的 Agent 读页面时也看到是谁说的


async def test_upload_image(token, pages):
    t, tok = token("画图的")
    img = png(4, 3, (10, 200, 30))
    async with Agent(tok) as a:
        out = await a.call("upload_image", data_base64="data:image/png;base64," + base64.b64encode(img).decode(), name="图.png", alt="示意图")
        assert out.startswith("已传上（4×3")
        md = out.split("写进页面：", 1)[1]
        mid = md.split("/api/media/")[1].rstrip(")")
        assert md == f"![示意图](/api/media/{mid})" and media.get(mid)[1] == img and media.info(mid)["name"] == "图.png"
        out = await a.call("create_page", title="配图", content=f"# 配图\n\n{md}")
        pages.append(int(out.split("#")[1].split("「")[0]))
        assert "没传上" in await a.call("upload_image", data_base64="不是 base64!!")
        assert "没传上" in await a.call("upload_image", data_base64=base64.b64encode(b"<svg/>").decode())
        out = await a.call("create_page", title="坏图", content="![x](/api/media/" + "0" * 24 + ")")
        assert "没建" in out and "upload_image" in out
    # 传上来的图 Agent 用 token 能取
    async with asgi(headers={"authorization": f"Bearer {tok}"}) as c:
        assert (await c.get(f"/api/media/{mid}")).content == img


async def test_my_notifications(token, pages):
    t, tok = token("收通知的")
    pid = library.save_page(None, "周报", DOC, by=by(t))["id"]
    pages.append(pid)
    async with Agent(tok) as a:
        assert await a.call("my_notifications") == "没有要你处理的评论"
        c1 = await pagecomments.post(pid, "第一段再短一点")
        await pagecomments.post(pid, "@收通知的 标题也改改")
        out = await a.call("my_notifications", mark_done=False)
        assert out.startswith("2 条通知") and "第一段再短一点" in out and "标题也改改" in out and f"reply_to={c1['id']}" in out
        assert len(notify.listing(t["id"], pending=True)) == 2   # 只看不标
        out = await a.call("my_notifications")
        assert out.startswith("2 条通知")
        assert notify.listing(t["id"], pending=True) == []
        assert await a.call("my_notifications") == "没有要你处理的评论"
        out = await a.call("my_notifications", include_done=True)
        assert out.startswith("2 条通知") and "已处理" in out
    await settle()


async def test_db_tools(token, pages, events):
    """数据库：建库、加行、查、改属性、改表结构，作者记成 agent:N，发 db_changed。"""
    from open_agent_docs import dbs
    t, tok = token("Codex")
    async with Agent(tok) as a:
        out = await a.call("db_create", title="任务", properties=[{"name": "状态", "type": "status"}, {"name": "分", "type": "number"}])
        db_id = int(out.split("#")[1].split("「")[0])
        pages.append(db_id)
        out = await a.call("db_add", db_id=db_id, rows=[{"title": "写文档", "props": {"状态": "进行中", "分": 3}}, {"title": "测试"}])
        assert "加了 2 行" in out
        rid = int(out.split("#")[2].split(" ")[0])
        assert dbs.get_row(rid)["created_by"] == by(t)
        out = await a.call("db_query", db_id=db_id, filter=[{"prop": "状态", "op": "eq", "value": "进行中"}])
        assert "符合条件的 1 行" in out and "| 写文档 | 进行中 | 3 |" in out
        out = await a.call("db_update", rows=[{"row_id": rid, "props": {"状态": "已完成"}, "version": 1}])
        assert "改了 状态，现在是版本 2" in out
        assert "优先级" in await a.call("db_schema", db_id=db_id, ops=[{"op": "add", "name": "优先级", "type": "select", "options": ["高"]}])
        assert "数据库《任务》" in await a.call("read_page", page_id=db_id)
        out = await a.call("db_schema", db_id=db_id, ops=[{"op": "add", "name": "双倍", "type": "formula", "formula": 'prop("分") * 2'},
                                                          {"op": "add", "name": "编号", "type": "unique_id", "prefix": "T-"}])
        assert "双倍" in out and "T-" in out
        out = await a.call("db_query", db_id=db_id, props=["分", "双倍", "编号"])
        assert "| 写文档 | 3 | 6 | T-1 |" in out
        out = await a.call("db_schema", db_id=db_id, ops=[{"op": "add", "name": "交给我", "type": "button", "comment": "接手", "agent": "Codex"}])
        assert "交给我" in out
        assert dbs.prop_of(dbs.get_db(db_id), "交给我")["agent"] == t["id"]
    assert any(e["db_id"] == db_id and e["by"] == by(t) for e in events.of("db_changed"))
    _, ro = token("Reader", "read")
    async with Agent(ro) as a:
        assert "只读" in await a.call("db_add", db_id=db_id, rows=[{"title": "x"}])
        assert "写文档" in await a.call("db_query", db_id=db_id)
