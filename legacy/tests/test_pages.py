"""页面按段读写（pagedoc.py）：段的编号和作者、按段修改和冲突、整页重写、锁定、版本历史、给 Agent 读的样子、接口。"""
import os
import time

import pytest
from conftest import asgi, png
from open_agent_docs import library, media, pagedoc, search
from open_agent_docs.db import conn


@pytest.fixture
def bot(make_agent):
    """建 Agent（OSA 里这里是 Bot）。"""
    def make(name: str = "") -> dict:
        return make_agent(name)[0]
    return make


def by(a: dict) -> str:
    return f"agent:{a['id']}"


@pytest.fixture
def pages():
    """测试里建的页面，结束时删掉。"""
    made: list[int] = []
    yield made
    for pid in made:
        library.delete_page(pid)


DOC = "# 周报\n\n本周完成了三件事。\n\n- [x] 整理定价\n- [ ] 联系销售\n\n| 项 | 数 |\n| - | - |\n| A | 1 |\n| B | 2 |"


def new(pages, b, content=DOC, title="周报") -> int:
    pid = library.save_page(None, title, content, by=by(b))["id"]
    pages.append(pid)
    return pid


def ids(pid):
    return [x["id"] for x in pagedoc.load(pid)[1]]


def md_of(pid):
    return [x["md"] for x in pagedoc.load(pid)[1]]


def test_new_page_has_blocks_and_history(bot, pages):
    b = bot()
    pid = new(pages, b)
    p, blks = pagedoc.load(pid)
    assert p["version"] == 1 and len(blks) == 7
    assert {x["by"] for x in blks} == {by(b)} and {x["v"] for x in blks} == {1}
    assert [x["md"] for x in blks][3:5] == ["- [ ] 联系销售", "| 项 | 数 |\n| - | - |"]
    assert len({x["id"] for x in blks}) == 7
    assert p["content"] == DOC
    assert [v["version"] for v in pagedoc.versions(pid)] == [1]


def test_legacy_page_gets_ids_lazily(pages):
    """从 OSA 导进来、还没有段编号的页面（bot_id 是 OSA 的 Bot）：整页算那个 Bot 写的。"""
    now = time.time()
    pid = conn.execute("insert into pages(bot_id, title, format, content, created, updated) values(?,?,?,?,?,?)",
                       (7, "旧页面", "markdown", "# 旧\n\n正文", now, now)).lastrowid
    conn.commit()
    pages.append(pid)
    first = ids(pid)
    assert len(first) == 2 and ids(pid) == first   # 第一次读时编上号，以后不变
    p, blks = pagedoc.load(pid)
    assert p["version"] == 0 and p["updated"] == now and {x["by"] for x in blks} == {"bot:7"}
    # 第一次改之前先记一版原样
    pagedoc.apply(pid, 0, [{"op": "insert", "end": True, "markdown": "新的一段"}], "user")
    vs = pagedoc.versions(pid)
    assert [v["label"] for v in vs] == ["", "base"] and pagedoc.version(pid, vs[1]["id"])["content"] == "# 旧\n\n正文"


def test_outside_edit_keeps_ids(bot, pages):
    """旧版本的程序直接改了正文：按指纹对上，没变的段编号不变，被改的那段保留编号但作者不明。"""
    pid = new(pages, bot())
    before = pagedoc.load(pid)[1]
    conn.execute("update pages set content=? where id=?", (DOC.replace("三件事", "四件事") + "\n\n结尾", pid))
    conn.commit()
    after = pagedoc.load(pid)[1]
    assert [x["id"] for x in after][:7] == [x["id"] for x in before] and len(after) == 8
    assert after[1]["by"] == "" and after[0]["by"] == before[0]["by"]


def test_ops(bot, pages):
    b = bot()
    pid = new(pages, b)
    i = ids(pid)
    res = pagedoc.apply(pid, 1, [
        {"op": "replace", "block": i[1], "markdown": "本周完成了**四件**事。"},
        {"op": "insert", "after": i[3], "markdown": "- [ ] 补截图"},
        {"op": "insert", "before": i[0], "markdown": "> [!NOTE]\n> 草稿"},
        {"op": "delete", "block": i[6]},
        {"op": "insert", "end": True, "markdown": "## 下周\n\n继续跟进"},
    ], "user")
    assert res["version"] == 2 and res["removed"] == [i[6]]
    p = library.get_page(pid)
    assert p["content"] == ("> [!NOTE]\n> 草稿\n\n# 周报\n\n本周完成了**四件**事。\n\n- [x] 整理定价\n- [ ] 联系销售\n- [ ] 补截图"
                            "\n\n| 项 | 数 |\n| - | - |\n| A | 1 |\n\n## 下周\n\n继续跟进")
    blks = pagedoc.load(pid)[1]
    assert blks[2]["id"] == i[1] and blks[2]["by"] == "user" and blks[2]["v"] == 2   # 被换掉的段编号不变
    assert blks[1]["by"] == by(b) and blks[1]["v"] == 1                   # 没动的段不变
    assert res["order"] == [x["id"] for x in blks]
    # 移动到最前面
    pagedoc.apply(pid, 2, [{"op": "move", "block": i[3], "after": None}], "user")
    assert md_of(pid)[0] == "- [ ] 联系销售"


def test_replace_with_several_blocks(bot, pages):
    pid = new(pages, bot())
    i = ids(pid)
    res = pagedoc.apply(pid, 1, [{"op": "replace", "block": i[1], "markdown": "第一段\n\n第二段\n\n- 列表"}], "user")
    assert md_of(pid)[1:4] == ["第一段", "第二段", "- 列表"] and ids(pid)[1] == i[1]
    # 新加的列表项后面接着原来的列表：拼成同一个紧凑列表，那一段的 gap 变了，也算改动
    assert [x["md"] for x in res["blocks"]] == ["第一段", "第二段", "- 列表", "- [x] 整理定价"]
    assert library.get_page(pid)["content"].startswith("# 周报\n\n第一段\n\n第二段\n\n- 列表\n- [x] 整理定价\n")


def test_merge_when_joined(bot, pages):
    """拼起来以后边界变了（这里：插进去的内容和后面的段拼成了一段），保留第一个编号。"""
    pid = new(pages, bot(), "正文")
    i = ids(pid)
    res = pagedoc.apply(pid, 1, [{"op": "insert", "before": i[0], "markdown": "<details>\n<summary>折叠</summary>"},
                                 {"op": "insert", "end": True, "markdown": "</details>"}], "user")
    blks = pagedoc.load(pid)[1]
    assert len(blks) == 1 and blks[0]["md"].startswith("<details>") and res["merged"]


def test_conflicts(bot, pages):
    pid = new(pages, bot())
    i = ids(pid)
    pagedoc.apply(pid, 1, [{"op": "replace", "block": i[1], "markdown": "别人先改了"}], "agent:999")
    before = library.get_page(pid)["content"]
    with pytest.raises(pagedoc.Conflict) as e:
        pagedoc.apply(pid, 1, [{"op": "replace", "block": i[0], "markdown": "# 新标题"},
                               {"op": "replace", "block": i[1], "markdown": "我也改"}], "user")
    assert e.value.version == 2 and e.value.items == [
        {"block": i[1], "op": "replace", "reason": "changed", "current": "别人先改了", "by": "agent:999", "v": 2}]
    assert library.get_page(pid)["content"] == before   # 一处冲突，整批都不写
    # 版本旧了但要改的段没变过：照样能改
    pagedoc.apply(pid, 1, [{"op": "replace", "block": i[0], "markdown": "# 新标题"}], "user")
    # 段不在了、锚点不在了
    with pytest.raises(pagedoc.Conflict) as e:
        pagedoc.apply(pid, 3, [{"op": "delete", "block": "zzzz"}, {"op": "insert", "after": "yyyy", "markdown": "x"}], "user")
    assert [c["reason"] for c in e.value.items] == ["missing", "anchor_missing"]
    with pytest.raises(ValueError):
        pagedoc.apply(pid, 99, [{"op": "delete", "block": i[0]}], "user")
    with pytest.raises(ValueError):
        pagedoc.apply(pid, 3, [{"op": "rename"}], "user")


def test_max_page_after_ops(bot, pages, monkeypatch):
    pid = new(pages, bot())
    monkeypatch.setattr(library, "MAX_PAGE", len(DOC) + 5)
    with pytest.raises(ValueError):
        pagedoc.apply(pid, 1, [{"op": "insert", "end": True, "markdown": "很长" * 10}], "user")
    assert library.get_page(pid)["version"] == 1


def test_save_page_by_other_agent(bot, pages):
    """整页保存（library.save_page 更新）：没变的段保留编号和作者；带了 base，盖掉别人在那之后改过的段就冲突。"""
    a, b = bot(), bot()
    pid = new(pages, a)
    i = ids(pid)
    library.save_page(None, "周报", "⟦" + i[0] + "⟧\n" + DOC + "\n\n补充", pid=pid, by=by(b), base=1)
    blks = pagedoc.load(pid)[1]
    assert [x["id"] for x in blks][:7] == i and blks[7]["md"] == "补充" and blks[7]["by"] == by(b)
    assert blks[0]["md"] == "# 周报"   # 照抄进来的段落编号去掉了
    assert library.get_page(pid, False)["version"] == 2
    library.save_page(None, "周报", DOC + "\n\n补充二", pid=pid, by=by(b))
    with pytest.raises(pagedoc.Conflict) as e:   # a 读到的是版本 1，之后 b 改过的段不能盖掉
        library.save_page(None, "周报", DOC, pid=pid, by=by(a), base=1)
    assert any(c["current"] == "补充二" for c in e.value.items)


def test_html_and_lock(bot, pages):
    a = bot()
    html = library.save_page(None, "面板", "<p>x</p>", "html", by=by(a))["id"]
    pages.append(html)
    assert library.get_page(html)["format"] == "html"
    out, ev = pagedoc.tool_edit(by(a), {"page_id": html, "version": 1, "ops": []})
    assert "html" in out and ev is None
    pid = new(pages, a)
    library.update_page(pid, locked=True)
    assert "锁定" in pagedoc.tool_edit(by(a), {"page_id": pid, "version": 1, "ops": [{"op": "append", "markdown": "x"}]})[0]
    pagedoc.apply(pid, 1, [{"op": "insert", "end": True, "markdown": "用户照样能改"}], "user")
    assert "锁定" in pagedoc.render(pid)


def test_tool_edit(bot, pages):
    a, b = bot("写手"), bot("分析")
    pid = new(pages, a)
    i = ids(pid)
    out, ev = pagedoc.tool_edit(by(b), {"page_id": pid, "version": 1, "ops": [
        {"op": "replace", "block": f"⟦{i[1]}⟧", "markdown": "本周完成了四件事。"},
        {"op": "insert_after", "block": "#" + i[3], "markdown": "- [ ] 补截图"},
        {"op": "append", "markdown": "结尾"}]})
    assert "已改页面" in out and "版本 2" in out and ev["by"] == by(b) and ev["version"] == 2
    assert set(ev["changed"]) >= {i[1]}
    # 冲突：告诉它现在的内容和改的人，按新版本重提
    pagedoc.apply(pid, 2, [{"op": "replace", "block": i[1], "markdown": "用户改的"}], "user")
    out, ev = pagedoc.tool_edit(by(b), {"page_id": pid, "version": 2, "ops": [{"op": "replace", "block": i[1], "markdown": "再改"}]})
    assert ev is None and "「用户」改了" in out and "用户改的" in out and "version=3" in out
    for bad in ([{"op": "replace", "markdown": "x"}], [{"op": "insert_after", "block": i[0]}], [{"op": "rename"}], "x"):
        out, ev = pagedoc.tool_edit(by(b), {"page_id": pid, "version": 3, "ops": bad})
        assert out.startswith("没改") and ev is None
    assert pagedoc.tool_edit(by(b), {"page_id": 999999, "version": 1, "ops": []})[0] == "没有页面 #999999"


def test_images_in_pages(bot, pages):
    """传上来的图片放进页面：只写了编号的补成 /api/media/编号；不存在的图片不让写；页面（含历史版本）引用着的不会被清掉。"""
    b = bot("写手")
    pid = new(pages, b)
    i = ids(pid)
    img = media.save(png(6, 6, (200, 10, 10)), "信息图.png")
    out, ev = pagedoc.tool_edit(by(b), {"page_id": pid, "version": 1, "ops": [
        {"op": "insert_after", "block": i[1], "markdown": f"![信息图]({img['id']})"}]})
    assert "已改页面" in out and ev
    assert md_of(pid)[2] == f"![信息图](/api/media/{img['id']})"
    gone = "0" * 24
    out, ev = pagedoc.tool_edit(by(b), {"page_id": pid, "version": 2, "ops": [{"op": "append", "markdown": f"![x](/api/media/{gone})"}]})
    assert out.startswith("没改") and gone in out and "不存在" in out and ev is None
    # 另一个页面也引用它（Agent 建页面时写 media:编号）
    new_id = new(pages, b, media.embed(f"# 配图\n\n![图](media:{img['id']})"), "配图")
    assert f"![图](/api/media/{img['id']})" in library.get_page(new_id)["content"]
    with pytest.raises(ValueError):
        media.embed(f"![x]({gone})")
    # 一天前存的、没有页面用到的图片会被清掉；页面里用着的留着，只剩历史版本里用着的也留着
    stray = media.save(png(5, 9, tuple(os.urandom(3))), "没人用.png")   # 编号是内容的哈希：别和别的测试的图撞上
    conn.execute("update media set created=0 where id in (?, ?)", (img["id"], stray["id"]))
    conn.commit()
    media.gc()
    assert media.info(img["id"]) and not media.info(stray["id"])
    pagedoc.apply(pid, 2, [{"op": "delete", "block": ids(pid)[2]}], "user")
    library.delete_page(new_id, trash=False)
    pages.remove(new_id)
    media.gc()
    assert media.info(img["id"])   # 页面 pid 的历史版本里还有它（能恢复回来）


async def test_shared_page_images(bot, pages):
    """分享出去的页面里的图片：从分享链接下面取，只给正文里引用着的。"""
    b = bot("写手")
    img = media.save(png(7, 7, (9, 9, 200)), "图.png")
    other = media.save(png(7, 7, (9, 200, 9)), "别的.png")
    pid = new(pages, b, f"看图\n\n![图](/api/media/{img['id']})")
    token = library.share(pid, True)["share_token"]
    async with asgi() as c:   # 不用登录
        r = await c.get(f"/api/shared/{token}/media/{img['id']}")
        assert r.status_code == 200 and r.content == media.get(img["id"])[1]
        assert (await c.get(f"/api/shared/{token}/media/{other['id']}")).status_code == 404
        assert (await c.get(f"/api/shared/nope/media/{img['id']}")).status_code == 404
        assert (await c.get(f"/api/shared/{token}")).json()["content"].startswith("看图")
    library.share(pid, False)
    async with asgi() as c:
        assert (await c.get(f"/api/shared/{token}/media/{img['id']}")).status_code == 404


def test_render(bot, pages):
    b = bot("写手")
    pid = new(pages, b)
    i = ids(pid)
    text = pagedoc.render(pid)
    assert text.startswith(f"《周报》/pages/{pid} · 版本 1 · 「写手」建的") and "段落编号" in text
    assert f"⟦{i[0]}⟧\n# 周报" in text and f"⟦{i[6]}⟧\n| B | 2 |" in text
    # 上限不同；太长时告诉它从哪接着读
    long = new(pages, b, "\n\n".join(f"第 {k} 段：" + "字" * 300 for k in range(60)), "长页面")
    text = library.tool_read_page({"page_id": long}, 7000)
    assert len(text) <= 7000 and "接着读：read_page(page_id=" in text
    start = text.rsplit('start="', 1)[1].split('"')[0]
    more = library.tool_read_page({"page_id": long, "start": start}, 7000)
    assert more.split("\n\n", 1)[1].startswith(f"⟦{start}⟧")
    assert "从头显示" in library.tool_read_page({"page_id": long, "start": "nope"})
    # 一段就超过上限：只显示前面一截
    huge = new(pages, b, "长" * 9000, "一大段")
    assert "这一段太长" in library.tool_read_page({"page_id": huge}, 3000)
    # 用户建的页面
    mine = library.save_page(None, "我的", "正文")["id"]
    pages.append(mine)
    assert "· 用户建的 ·" in pagedoc.render(mine)


def test_versions_group_and_restore(bot, pages, monkeypatch):
    b = bot()
    pid = new(pages, b)
    i = ids(pid)
    for n in range(3):   # 同一个人接着改：并进同一版
        pagedoc.apply(pid, 1 + n, [{"op": "replace", "block": i[1], "markdown": f"改第 {n} 次"}], "user")
    assert [v["by"] for v in pagedoc.versions(pid)] == ["user", by(b)]
    pagedoc.apply(pid, 4, [{"op": "delete", "block": i[0]}], "agent:999")
    vs = pagedoc.versions(pid)
    assert [v["version"] for v in vs] == [5, 4, 1]
    # 回到最早那一版：段的编号回来了，作为新的一版
    p = pagedoc.restore(pid, vs[-1]["id"])
    assert p["version"] == 6 and ids(pid) == i and library.get_page(pid)["content"] == DOC
    assert pagedoc.versions(pid)[0]["label"] == "restore:1"
    snap = pagedoc.version(pid, vs[1]["id"])
    assert snap["content"].startswith("# 周报\n\n改第 2 次") and [x["id"] for x in snap["blocks"]] == i
    with pytest.raises(LookupError):
        pagedoc.restore(pid, 999999)
    # 上限：最多留几版、总大小；至少留几版
    monkeypatch.setattr(pagedoc, "MAX_VERSIONS", 6)
    for n in range(8):
        pagedoc.apply(pid, 6 + n, [{"op": "insert", "end": True, "markdown": f"第 {n} 段"}], f"agent:{900 + n}")
    assert len(pagedoc.versions(pid)) == 6
    monkeypatch.setattr(pagedoc, "MAX_VERSION_BYTES", 1)
    pagedoc.apply(pid, 14, [{"op": "insert", "end": True, "markdown": "再一段"}], "agent:1")
    assert len(pagedoc.versions(pid)) == pagedoc.KEEP_VERSIONS


def test_title_and_rewrite_noop(bot, pages):
    b = bot()
    pid = new(pages, b)
    assert not pagedoc.rewrite(pid, "周报", DOC, by(b))   # 什么都没变：不记新版本
    assert library.get_page(pid, False)["version"] == 1
    library.update_page(pid, title="新标题")
    p = library.get_page(pid, False)
    assert p["title"] == "新标题" and p["version"] == 2 and pagedoc.last_by(pid) == "user"


async def test_search_follows_edits(bot, pages):
    """搜索每次先跟上页面的改动：改掉的字搜不到了，新写的能搜到。"""
    pid = new(pages, bot(), "\n\n".join(["甲" * 700, "猕猴桃供应链"]), "搜索")
    assert (await search.search("猕猴桃供应链"))[0]["ref"] == pid
    i = ids(pid)
    pagedoc.apply(pid, 1, [{"op": "replace", "block": i[1], "markdown": "火龙果的产地"}], "user")
    assert not any(h["ref"] == pid for h in await search.search("猕猴桃"))
    hit = (await search.search("火龙果"))[0]
    assert hit["ref"] == pid and hit["title"] == "搜索" and "火龙果" in hit["snippet"]
    assert await search.search("") == [] and await search.search("完全不存在的词zzqx") == []


async def test_rest(client, bot, pages, events):
    b = bot()
    c = client
    r = await c.post("/api/pages", json={"title": "空白页", "content": "", "allow_empty": True})
    assert r.status_code == 200 and r.json()["version"] == 1
    empty = r.json()["id"]
    pages.append(empty)
    pid = new(pages, b)
    r = (await c.get(f"/api/pages/{pid}/blocks")).json()
    assert r["version"] == 1 and len(r["order"]) == 7 and len(r["blocks"]) == 7 and r["blocks"][0]["md"] == "# 周报"
    i = r["order"]
    # 用户按段改：只发 page_changed，不发 pages_changed
    events.events.clear()
    r = await c.post(f"/api/pages/{pid}/blocks", json={"version": 1, "ops": [
        {"op": "replace", "block": i[1], "markdown": "改了"},
        {"op": "insert", "after": i[1], "new_id": "u1234567890", "markdown": "新段"}]})
    assert r.status_code == 200 and r.json()["version"] == 2 and "u1234567890" in r.json()["order"]
    ev = events.of("page_changed")
    assert ev and ev[0]["page_id"] == pid and ev[0]["by"] == "user" and set(ev[0]["changed"]) == {i[1], "u1234567890"}
    assert not events.of("pages_changed")
    since = (await c.get(f"/api/pages/{pid}/blocks", params={"since": 1})).json()
    assert {x["id"] for x in since["blocks"]} == {i[1], "u1234567890"} and len(since["order"]) == 8
    # 冲突：409，带现在的内容
    await c.post(f"/api/pages/{pid}/blocks", json={"version": 2, "ops": [{"op": "replace", "block": i[2], "markdown": "- [x] 改"}]})
    r = await c.post(f"/api/pages/{pid}/blocks", json={"version": 2, "ops": [{"op": "replace", "block": i[2], "markdown": "冲突"}]})
    assert r.status_code == 409 and r.json()["detail"]["version"] == 3
    assert r.json()["detail"]["conflicts"][0] == {"block": i[2], "op": "replace", "reason": "changed",
                                                  "current": "- [x] 改", "by": "user", "v": 3}
    assert (await c.post(f"/api/pages/{pid}/blocks", json={"version": "x", "ops": []})).status_code == 400
    # 锁定、改标题
    r = (await c.patch(f"/api/pages/{pid}", json={"locked": True, "title": "新标题"})).json()
    assert r["locked"] and r["title"] == "新标题" and r["version"] == 4
    # 历史：列表、看一版、回到那一版
    vs = (await c.get(f"/api/pages/{pid}/versions")).json()
    assert vs[0]["by"] == "user" and vs[-1]["version"] == 1
    one = (await c.get(f"/api/pages/{pid}/versions/{vs[-1]['id']}")).json()
    assert one["content"] == DOC and len(one["blocks"]) == 7
    assert (await c.get(f"/api/pages/{pid}/versions/999999")).status_code == 404
    events.events.clear()
    r = await c.post(f"/api/pages/{pid}/versions/{vs[-1]['id']}/restore")
    assert r.status_code == 200 and r.json()["version"] == 5 and r.json()["title"] == "周报"
    assert events.of("page_changed") and events.of("pages_changed")
    # html 页面没有段
    html = library.save_page(None, "面板", "<p>x</p>", "html", by=by(b))["id"]
    pages.append(html)
    assert (await c.get(f"/api/pages/{html}/blocks")).status_code == 400
    assert (await c.get("/api/pages/999999/blocks")).status_code == 404


def test_delete_removes_history(bot):
    pid = library.save_page(None, "临时", "内容", by=by(bot()))["id"]
    assert pagedoc.versions(pid)
    library.delete_page(pid)
    assert not pagedoc.versions(pid)


async def test_trash(client, bot, events):
    """删掉的进回收站：连版本历史、评论、子页面（cascade）、库的行一起放回去；原来的上级不在了放到最上层；30 天后清掉。"""
    from open_agent_docs import dbs, pagecomments
    b = bot()
    parent = library.save_page(None, "父", "# 父", by=by(b))["id"]
    pid = library.save_page(None, "要删的", "# 标题\n\n第一段", by=by(b), parent_id=parent)["id"]
    kid = library.save_page(None, "孙子", "内容", by=by(b), parent_id=pid)["id"]
    pagedoc.apply(pid, pagedoc.load(pid)[0]["version"], [{"op": "insert", "end": True, "markdown": "第二段"}], "user")
    bid = pagedoc.load(pid)[1][1]["id"]
    c = pagecomments.add(pid, "user", "这段再想想", bid)
    pagecomments.add(pid, by(b), "好", None, None, c["id"])
    library.delete_page(pid, cascade=True)
    assert library.get_page(pid, False) is None and library.get_page(kid, False) is None
    item = next(x for x in library.trash() if x["page_id"] == pid)
    assert item["title"] == "要删的" and item["count"] == 2
    assert any(x["id"] == item["id"] for x in (await client.get("/api/trash")).json())
    library.delete_page(parent)   # 原来的上级也删了
    r = await client.post(f"/api/trash/{item['id']}/restore")
    assert r.status_code == 200
    back = r.json()
    assert back["title"] == "要删的" and back["parent_id"] is None
    assert (await client.post(f"/api/trash/{item['id']}/restore")).status_code == 404
    nid = back["id"]
    assert "第二段" in library.get_page(nid)["content"] and len(pagedoc.versions(nid)) >= 2
    cs = pagecomments.comments(nid)
    assert [x["text"] for x in cs] == ["这段再想想", "好"] and cs[1]["parent_id"] == cs[0]["id"] and cs[0]["block_id"] == bid
    assert [k["title"] for k in library.family(nid)["children"]] == ["孙子"]
    assert events.of("pages_changed")
    # 数据库连行一起回来；单独删的行回到它的库
    db = dbs.create("库", "user", schema=dbs.default_schema())
    rows = [dbs.add_row(db["id"], {"状态": "进行中"}, "user", f"行{k}")["id"] for k in range(3)]
    library.delete_page(rows[0])
    library.restore_trash(next(x for x in library.trash() if x["page_id"] == rows[0])["id"])
    assert len(dbs.rows(db["id"])) == 3
    library.delete_page(db["id"])
    assert dbs.get_db(db["id"]) is None
    library.restore_trash(next(x for x in library.trash() if x["page_id"] == db["id"])["id"])
    restored = next(p for p in library.list_pages() if p["title"] == "库" and p["kind"] == "db")
    assert [r["title"] for r in dbs.rows(restored["id"])] == ["行0", "行1", "行2"]
    # 行所在的库不在了：恢复不了
    one = dbs.rows(restored["id"])[0]["id"]
    library.delete_page(one)
    library.delete_page(restored["id"], trash=False)
    with pytest.raises(ValueError, match="数据库已经不在"):
        library.restore_trash(next(x for x in library.trash() if x["page_id"] == one)["id"])
    # 放了 30 天的清掉
    conn.execute("update page_trash set deleted=deleted-31*86400 where page_id=?", (one,))
    library.purge_trash()
    assert not any(x["page_id"] == one for x in library.trash())
    library.delete_page(nid, cascade=True, trash=False)


async def test_hierarchy(client, bot, pages, events):
    b = bot("写手")
    a = new(pages, b, "# A", "A")
    child = library.save_page(None, "B", "# B", by=by(b), parent_id=a)["id"]
    grand = library.save_page(None, "C", "# C", by=by(b), parent_id=child)["id"]
    d = library.save_page(None, "D", "# D", by=by(b), parent_id=a)["id"]
    pages.extend([child, grand, d])
    assert library.family(grand)["ancestors"] == [{"id": a, "title": "A"}, {"id": child, "title": "B"}]
    assert [c["id"] for c in library.family(a)["children"]] == [child, d]
    html = library.save_page(None, "面板", "<p>x</p>", "html", by=by(b))["id"]
    pages.append(html)
    for bad in ((a, grand), (a, a), (child, html)):   # 挪到自己的子孙下面、自己下面、html 页面下面
        with pytest.raises(ValueError):
            library.move(bad[0], bad[1])
    with pytest.raises(ValueError):
        library.save_page(None, "X", "x", by=by(b), parent_id=html)
    library.move(d, a, after=0)                          # 排到最前
    assert [c["id"] for c in library.family(a)["children"]] == [d, child]
    library.move(d, a, after=child)                      # 排在 B 后面
    assert [c["id"] for c in library.family(a)["children"]] == [child, d]
    text = pagedoc.render(child)
    assert f"上级：#{a}《A》" in text and f"子页面：#{grand}《C》" in text
    e = library.save_page(None, "E", "# E", by=by(b), parent_id=a)["id"]
    pages.append(e)
    # 删 B：C 挪到 A 下面，站在 B 原来的位置（D 前面）
    library.delete_page(child)
    assert [c["id"] for c in library.family(a)["children"]] == [grand, d, e]
    c = client
    assert (await c.post(f"/api/pages/{a}/move", json={"parent_id": grand})).status_code == 400
    r = await c.post(f"/api/pages/{grand}/move", json={"parent_id": None})
    assert r.status_code == 200 and r.json()["parent_id"] is None
    got = (await c.get(f"/api/pages/{d}")).json()
    assert got["ancestors"] == [{"id": a, "title": "A"}] and got["children"] == []
    r = await c.post("/api/pages", json={"title": "子", "content": "", "allow_empty": True, "parent_id": a})
    sub = r.json()["id"]
    pages.append(sub)
    assert r.json()["parent_id"] == a
    assert (await c.delete(f"/api/pages/{a}?cascade=true")).json() == {"ok": True}   # 连子页面一起删
    assert not library.get_page(a, False) and not library.get_page(d, False) and not library.get_page(sub, False)
    assert library.get_page(grand, False)   # 已经挪走的不受影响


def test_replace_group_keeps_ids(bot, pages):
    """网页上整组改“Markdown 原文”块（含 HTML 的表格）：没变的行、改了的行都留着原来的编号，新加的行才发新编号。"""
    from open_agent_docs import pagecomments
    b = bot()
    table = "| 名 | 值 |\n| --- | --- |\n| a | <b>1</b> |\n| b | 2 |\n| c | 3 |"
    pid = new(pages, b, "# 表\n\n" + table)
    i = ids(pid)   # 标题、表头、三行
    assert len(i) == 5
    pagecomments.add(pid, "user", "这行对吗", i[3])
    v = pagedoc.load(pid)[0]["version"]
    changed = "| 名 | 值 |\n| --- | --- |\n| a | <b>1</b> |\n| b | 20 |\n| c | 3 |\n| d | 4 |"
    res = pagedoc.apply(pid, v, [{"op": "replace", "block": i[1], "markdown": changed, "group": i[2:]}], "user")
    after = ids(pid)
    assert after[:5] == i and len(after) == 6 and res["removed"] == []
    assert md_of(pid)[3] == "| b | 20 |" and pagecomments.comments(pid)[0]["block_id"] == i[3]
    who = {x["id"]: x["by"] for x in pagedoc.load(pid)[1]}
    assert who[i[2]] == by(b) and who[i[3]] == "user"   # 没改的行作者不变
    # 删掉一行：那一行的编号去掉
    v = pagedoc.load(pid)[0]["version"]
    res = pagedoc.apply(pid, v, [{"op": "replace", "block": i[1], "markdown": "| 名 | 值 |\n| --- | --- |\n| a | <b>1</b> |\n| c | 3 |\n| d | 4 |",
                                  "group": after[2:]}], "user")
    assert res["removed"] == [i[3]] and ids(pid) == [i[0], i[1], i[2], i[4], after[5]]
    with pytest.raises(pagedoc.Conflict):   # 组里的编号对不上（被别人删了一行）
        pagedoc.apply(pid, res["version"], [{"op": "replace", "block": i[1], "markdown": "x", "group": [i[3]]}], "user")
