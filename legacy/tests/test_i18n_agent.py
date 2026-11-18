"""给 Agent 的文字跟着设置里的语言走：英文时 read_page、edit_page 冲突、数据库、通知、MCP 的工具说明和服务说明里没有中文；
换回中文又是中文。输入两种语言都认（类型名、汇总算法、今天 / today、用户 / user）。"""
import re

import pytest
from conftest import asgi
from test_mcp import Agent, rpc
from open_agent_docs import dbs, i18n, library, mcpserver, notify, pagecomments, pagedoc
from open_agent_docs.db import conn

CJK = re.compile(r"[㐀-鿿　-〿＀-￯]")
DOC = "# Weekly\n\nWe shipped three things.\n\n- [ ] Call sales"


@pytest.fixture
def en():
    """切到英文；测试完还原（原来没设过语言就删掉设置）。"""
    was = i18n.chosen()
    i18n.set_lang("en")
    yield
    if was:
        i18n.set_lang(was)
    else:
        conn.execute("delete from settings where key='language'")
        conn.commit()
        i18n._lang = None


def no_cjk(text: str):
    assert not CJK.search(text), [m.group() for m in CJK.finditer(text)][:10] + [text]


@pytest.fixture
def made():
    ids: list[int] = []
    yield ids
    for pid in ids:
        if library.get_page(pid, False):
            library.delete_page(pid, cascade=True, trash=False)


def test_read_page_and_conflict(en, made, make_agent):
    a, _ = make_agent("Nova")
    p = library.save_page(None, "Weekly", DOC, by=f"agent:{a['id']}")
    made.append(p["id"])
    _, blks = pagedoc.load(p["id"])
    pagecomments.add(p["id"], "user", "Please add numbers", blks[1]["id"])
    out = library.tool_read_page({"page_id": p["id"]})
    assert "version 1" in out and "created by Nova" in out and "Open comments (1):" in out and f"⟦{blks[1]['id']}⟧" in out
    no_cjk(out)
    no_cjk(library.tool_read_page({"page_id": 999999}))
    # 用户先改了一段，Agent 拿旧版本号改同一段：冲突
    pagedoc.apply(p["id"], 1, [{"op": "replace", "block": blks[1]["id"], "markdown": "We shipped four things."}], "user")
    text, ev = pagedoc.tool_edit(f"agent:{a['id']}", {"page_id": p["id"], "version": 1,
                                                      "ops": [{"op": "replace", "block": blks[1]["id"], "markdown": "Five."}]})
    assert ev is None and text.startswith("Not changed: the page is now at version 2") and "We shipped four things." in text
    assert "was changed by User" in text and "version=2" in text
    no_cjk(text)
    text, ev = pagedoc.tool_edit(f"agent:{a['id']}", {"page_id": p["id"], "version": 2,
                                                      "ops": [{"op": "append", "markdown": "Done."}]})
    assert ev and text.startswith("Edited page") and "now at version 3: 1 block changed or added." in text
    no_cjk(text)
    i18n.set_lang("zh")
    assert "版本 3" in library.tool_read_page({"page_id": p["id"]}) and "未处理的评论（1）" in library.tool_read_page({"page_id": p["id"]})


def test_db(en, made, make_agent):
    a, _ = make_agent()
    who = f"agent:{a['id']}"
    text, ev = dbs.tool_create(who, {"title": "Tasks"})
    db_id = ev["db_id"]
    made.append(db_id)
    no_cjk(text)
    db = dbs.get_db(db_id)
    assert [p["name"] for p in db["schema"]] == ["Name", "Status", "Owner", "Date"]   # 默认的列按建的时候的语言
    assert [o["name"] for o in db["schema"][1]["options"]] == ["Not started", "In progress", "Done"]
    assert [o["group"] for o in db["schema"][1]["options"]] == ["todo", "doing", "done"]
    # 中文、英文的写法都认
    text, _ = dbs.tool_add(who, {"db_id": db_id, "rows": [
        {"title": "Write docs", "props": {"Status": "In progress", "Owner": "user", "Date": "today"}},
        {"title": "Ship", "props": {"Status": "Done", "Owner": "用户", "Date": "明天"}},
        {"title": "Bad", "props": {"Status": "Later"}}]})
    assert "Added 2 rows" in text and 'has no option "Later"' in text
    no_cjk(text)
    text, _ = dbs.tool_schema(who, {"db_id": db_id, "ops": [{"op": "add", "name": "Points", "type": "数字"},
                                                            {"op": "add", "name": "Tags", "type": "Multi-select"},
                                                            {"op": "add", "name": "Check", "type": "勾选"}]})
    assert 'Added property "Points" (Number)' in text and "Tags (Multi-select" in text
    no_cjk(text)
    assert dbs._FN_BY_NAME["求和"] == dbs._FN_BY_NAME["sum"] == "sum" and dbs._FN_BY_NAME["percent checked"] == "percent_checked"
    out = dbs.tool_query({"db_id": db_id, "filter": [{"prop": "Status", "op": "eq", "value": "In progress"}]})
    assert out.startswith("Database #") and "has 2 rows, 1 matches" in out and "| Row | Version |" in out and "Write docs" in out
    no_cjk(out)
    no_cjk(dbs.tool_query({"db_id": db_id, "filter": [{"prop": "Nope", "op": "eq", "value": 1}]}))
    rid = dbs.rows(db_id)[0]["id"]
    text, _ = dbs.tool_update(who, {"rows": [{"row_id": rid, "props": {"Check": "yes", "Points": "x"}}]})
    assert 'takes a number, not "x"' in text
    no_cjk(text)
    for out in (dbs.render_db(db_id), library.tool_read_page({"page_id": rid})):
        assert "Properties" in out
        no_cjk(out)
    i18n.set_lang("zh")
    assert dbs.render_db(db_id).startswith("数据库《Tasks》") and "属性：" in dbs.render_db(db_id)


def test_notify_prompt(en):
    a = {"id": 7, "name": "Nova"}
    p = {"id": 12, "title": "Weekly"}
    c = {"id": 30, "block_id": "k7qd", "text": "Please fix", "parent_id": 29}
    out = notify._prompt(a, p, c, 29, "We shipped three things.")
    assert "comment #30 for you (Nova) on page #12 «Weekly» (block ⟦k7qd⟧)" in out and "reply_to=29" in out
    no_cjk(out)
    i18n.set_lang("zh")
    assert "用户在页面 #12《Weekly》的段落 ⟦k7qd⟧上给你（Nova）写了评论 #30" in notify._prompt(a, p, c, 29, "")


def _descriptions(x):
    if isinstance(x, dict):
        for k, v in x.items():
            if k == "description" and isinstance(v, str):
                yield v
            else:
                yield from _descriptions(v)
    elif isinstance(x, list):
        for v in x:
            yield from _descriptions(v)


async def test_mcp_tools_and_instructions(en, make_agent):
    _, tok = make_agent("Claude Code", "admin")
    async with Agent(tok) as ag:
        tools = (await ag.client.list_tools()).tools
        assert len(tools) == 15
        for tool in tools:
            texts = [tool.description, *_descriptions(tool.input_schema)]
            assert len(texts) > 1
            for s in texts:
                no_cjk(s)
        assert "block by block" in next(x.description for x in tools if x.name == "edit_page")
        no_cjk(await ag.call("read_page", page_id=999999))
    async with asgi() as c:
        r = await rpc(c, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                        "clientInfo": {"name": "claude-code", "version": "1"}}, tok=tok)
        ins = r.json()["result"]["instructions"]
        assert ins.startswith("This is Open Agent Docs") and "edit_page" in ins
        no_cjk(ins)
    i18n.set_lang("zh")
    async with Agent(tok) as ag:
        tools = {x.name: x for x in (await ag.client.list_tools()).tools}
        assert tools["edit_page"].description.startswith("按段改")
        assert "{属性名: 值}" in list(_descriptions(tools["db_add"].input_schema))
    async with asgi() as c:
        r = await rpc(c, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                        "clientInfo": {"name": "claude-code", "version": "1"}}, tok=tok)
        assert r.json()["result"]["instructions"] == mcpserver.INSTRUCTIONS
