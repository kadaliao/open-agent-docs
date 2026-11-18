"""页面的评论（pagecomments.py）：一串评论、挂在段上、通知负责的 Agent（targets）、comment_page、接口。"""
import pytest
from conftest import settle
from open_agent_docs import agents, library, notify, pagecomments, pagedoc

DOC = "# 周报\n\n本周完成了三件事。\n\n- [ ] 联系销售"


@pytest.fixture
def bot(make_agent):
    def make(name: str = "", scope: str = "write") -> dict:
        return make_agent(name, scope)[0]
    return make


@pytest.fixture
def pages():
    made: list[int] = []
    yield made
    for pid in made:
        library.delete_page(pid)


def by(a: dict) -> str:
    return f"agent:{a['id']}"


def new(pages, b, content=DOC) -> int:
    pid = library.save_page(None, "周报", content, by=by(b) if b else None)["id"]
    pages.append(pid)
    return pid


def ids(pid):
    return [x["id"] for x in pagedoc.load(pid)[1]]


def thread(pid, root):
    return [c for c in pagecomments.comments(pid) if (c["parent_id"] or c["id"]) == root]


def test_threads(bot, pages):
    pid = new(pages, bot())
    i = ids(pid)
    root = pagecomments.add(pid, "user", "数字不对", i[1], "三件事")
    assert root["block_id"] == i[1] and root["quote"] == "三件事" and root["parent_id"] is None
    reply = pagecomments.add(pid, "agent:1", "已改", reply_to=root["id"])
    assert reply["parent_id"] == root["id"] and reply["block_id"] == i[1]   # 回复跟着那串挂的段
    pagecomments.update(pid, reply["id"], resolved=True)   # 标的是整串
    assert pagecomments.get(root["id"])["resolved"] and not pagecomments.comments(pid, open_only=True)
    pagecomments.add(pid, "user", "还是不对", reply_to=reply["id"])   # 回复的是回复：挂到第一条下面，重新打开
    assert len(thread(pid, root["id"])) == 3 and not pagecomments.get(root["id"])["resolved"]
    page = pagecomments.add(pid, "user", "整页的意见")
    assert page["block_id"] is None
    for bad in (dict(text=""), dict(text="x", block="zzzz"), dict(text="x" * (pagecomments.MAX_COMMENT + 1))):
        with pytest.raises(ValueError):
            pagecomments.add(pid, "user", bad["text"], bad.get("block"))
    with pytest.raises(ValueError):
        pagecomments.add(pid, "user", "x", reply_to=999999)
    pagecomments.delete(pid, root["id"])   # 删第一条：整串没了
    assert [c["id"] for c in pagecomments.comments(pid)] == [page["id"]]


def test_comments_follow_merged_blocks(bot, pages):
    pid = new(pages, bot(), "正文")
    i = ids(pid)
    pagedoc.apply(pid, 1, [{"op": "insert", "end": True, "markdown": "第二段"}], "user")
    second = ids(pid)[1]
    c = pagecomments.add(pid, "user", "这段", second)
    # 第一段换成一个没闭合的 <details>：和后面的段拼成了一段，评论挪到留下的那段上
    pagedoc.apply(pid, 2, [{"op": "replace", "block": i[0], "markdown": "<details>\n<summary>折叠</summary>"},
                           {"op": "insert", "end": True, "markdown": "</details>"}], "user")
    assert ids(pid) == [i[0]] and pagecomments.get(c["id"])["block_id"] == i[0]


def test_targets(bot, pages):
    a, b, c = bot("Writer8"), bot("Analyst8"), bot("Editor8")
    ro = bot("Reader8", "read")
    pid = new(pages, a)
    p, blks = pagedoc.load(pid)
    pagedoc.apply(pid, 1, [{"op": "replace", "block": blks[1]["id"], "markdown": "Analyst 改的"}], by(b))
    p, blks = pagedoc.load(pid)
    assert pagecomments.targets(p, blks[1], None, "看看", None) == [b["id"]]   # 这一段是谁写的找谁
    assert pagecomments.targets(p, blks[0], None, "看看", None) == [a["id"]]
    assert pagecomments.targets(p, None, None, "看看", None) == [a["id"]]     # 整页的：建页面的 Agent
    assert pagecomments.targets(p, blks[1], None, "@Editor8 你来", None) == [c["id"]]
    assert pagecomments.targets(p, blks[1], None, "@editor8 和 @Analyst8", None) == [c["id"], b["id"]]   # 不分大小写，按出现顺序
    assert pagecomments.targets(p, blks[1], None, "@Editor80 你来", None) == [b["id"]]   # @Editor80 不算 @Editor8
    assert pagecomments.targets(p, blks[1], None, "@Reader8 看看", None) == [b["id"]]   # 只读的不叫
    assert set(pagecomments.targets(p, blks[1], None, "@所有人 看看", None)) >= {a["id"], b["id"], c["id"]}
    assert ro["id"] not in pagecomments.targets(p, blks[1], None, "@all", None)
    assert pagecomments.targets(p, blks[1], None, "看看", False) == []
    assert pagecomments.targets(p, blks[1], None, "看看", c["id"]) == [c["id"]]
    assert pagecomments.targets(p, blks[1], None, "看看", ro["id"]) == []      # 指定只读的也不行
    root = pagecomments.add(pid, "user", "第一条", blks[0]["id"])
    pagecomments.add(pid, by(c), "我看看", reply_to=root["id"])
    assert pagecomments.targets(p, blks[0], root["id"], "再看看", None) == [c["id"]]   # 回复：这串里最后说话的 Agent
    agents.revoke(a["id"])
    p = pagedoc.load(pid)[0]
    assert pagecomments.targets(p, blks[0], None, "看看", None) == []   # 作者和建页面的都不在了：谁也不叫
    assert pagecomments.targets(p, blks[1], None, "看看", None) == [b["id"]]


async def test_comment_notifies_author(client, events, bot, pages):
    a = bot("Writer9")
    pid = new(pages, a)
    i = ids(pid)
    r = await client.post(f"/api/pages/{pid}/comments", json={"text": "应该是四件事", "block": i[1], "quote": "三件事"})
    assert r.status_code == 200 and r.json()["woke"] == [a["id"]]
    root = r.json()["id"]
    assert pagecomments.get(root)["woke"] == [a["id"]]
    n = notify.listing(a["id"])[-1]
    assert n["type"] == "comment" and n["comment"]["text"] == "应该是四件事" and n["comment"]["root"] == root
    assert n["comment"]["block"] == i[1] and n["comment"]["quote"] == "三件事" and n["page"]["id"] == pid
    assert f"页面 #{pid}《周报》" in n["prompt"] and f"reply_to={root}" in n["prompt"] and "三件事" in n["prompt"]
    assert any(e["page_id"] == pid for e in events.of("page_comments"))
    # Agent 回复以后，用户在这串里再说：还是通知它
    pagecomments.tool_comment(by(a), {"page_id": pid, "text": "改好了", "reply_to": root})
    r = await client.post(f"/api/pages/{pid}/comments", json={"text": "谢谢", "reply_to": root})
    assert r.json()["woke"] == [a["id"]] and r.json()["parent_id"] == root
    n = notify.listing(a["id"])[-1]
    assert n["comment"]["text"] == "谢谢" and n["comment"]["root"] == root and "评论串" in n["prompt"]
    await settle()


async def test_agent_comments_notify_nobody(events, bot, pages):
    a, b = bot("Writer12"), bot("Other12")
    pid = new(pages, a)
    before = len(notify.listing(a["id"]))
    out, got = pagecomments.tool_comment(by(b), {"page_id": pid, "text": "@Writer12 我觉得要改", "block": ids(pid)[1]})
    assert "已在页面" in out and got == pid
    out, got = pagecomments.tool_comment(b["id"], {"page_id": pid, "text": "编号也行"})   # who 给编号也认
    assert "写了评论" in out and pagecomments.comments(pid)[-1]["by"] == by(b)
    await settle()
    assert len(notify.listing(a["id"])) == before
    assert "没写" in pagecomments.tool_comment(by(b), {"page_id": pid})[0]
    assert "resolve 要和 reply_to 一起用" in pagecomments.tool_comment(by(b), {"page_id": pid, "resolve": True})[0]
    assert pagecomments.tool_comment(by(b), {"page_id": 999999, "text": "x"})[0] == "没有页面 #999999"
    # read_page 末尾带着未处理的评论
    assert "未处理的评论（2）" in pagedoc.render(pid) and "Other12：@Writer12 我觉得要改" in pagedoc.render(pid)


async def test_rest(client, events, bot, pages):
    a = bot("Writer14")
    pid = new(pages, a)
    h = client
    r = await h.post(f"/api/pages/{pid}/comments", json={"text": "不用叫 Agent", "wake": False})
    assert r.status_code == 200 and r.json()["woke"] == []
    cid = r.json()["id"]
    assert (await h.post(f"/api/pages/{pid}/comments", json={"text": ""})).status_code == 400
    assert (await h.post(f"/api/pages/{pid}/comments", json={"text": "x", "block": "zzzz"})).status_code == 400
    assert (await h.post("/api/pages/999999/comments", json={"text": "x"})).status_code == 404
    assert [c["id"] for c in (await h.get(f"/api/pages/{pid}/comments")).json()] == [cid]
    r = await h.patch(f"/api/pages/{pid}/comments/{cid}", json={"text": "改一下", "resolved": True})
    assert r.json()["text"] == "改一下" and r.json()["resolved"] and r.json()["edited"]
    assert (await h.delete(f"/api/pages/{pid}/comments/{cid}")).json() == {"ok": True}
    assert (await h.delete(f"/api/pages/{pid}/comments/{cid}")).status_code == 404
    await settle()
    assert notify.listing(a["id"]) == []
