"""数据库（dbs.py）：值的写法、加行改行、按键冲突、历史、筛选排序、改表结构、视图、工具、read_page、接口和事件、分享、搜索。"""
from datetime import date, timedelta

import pytest
from conftest import png, settle
from open_agent_docs import dbs, library, media, notify, pagedoc, search

TODAY = date.today()


@pytest.fixture
def made():
    """测试里建的库，结束时删掉（连行）。"""
    ids: list[int] = []
    yield ids
    for i in ids:
        if library.get_page(i, False):
            library.delete_page(i)


@pytest.fixture
def bot(make_agent):
    """建 Agent（OSA 里这里是 Bot）。"""
    def make(name: str = "") -> dict:
        return make_agent(name)[0]
    return make


def new_db(made, props=None, title="任务") -> dict:
    db = dbs.create(title, "user", schema=dbs.default_schema()) if props is None else dbs.create(title, "user", properties=props)
    made.append(db["id"])
    return db


def P(db, name):
    return dbs.prop_of(dbs.get_db(db["id"]), name)


# ---------- 值 ----------
def test_values(made, bot):
    b = bot("Writer")
    db = new_db(made, [{"name": "数", "type": "number", "format": "cny"}, {"name": "标签", "type": "multi_select", "options": ["甲", "乙"]},
                       {"name": "状态", "type": "status"}, {"name": "人", "type": "person"}, {"name": "日", "type": "date"},
                       {"name": "好", "type": "checkbox"}, {"name": "网", "type": "url"}, {"name": "信", "type": "email"},
                       {"name": "选", "type": "select"}])
    n = lambda name, v: dbs.normalize(P(db, name), v)   # noqa: E731
    assert n("数", "¥1,200") == 1200 and n("数", 3.5) == 3.5 and n("数", None) is None
    with pytest.raises(ValueError):
        n("数", "很多")
    tags = P(db, "标签")
    assert dbs.normalize(tags, "甲、乙") == [o["id"] for o in tags["options"]]
    assert len(tags["options"]) == 2
    dbs.normalize(tags, ["丙"])
    assert [o["name"] for o in tags["options"]] == ["甲", "乙", "丙"]   # 没有的选项自动加（改的是 prop 本身）
    st = P(db, "状态")
    assert [(o["name"], o["group"]) for o in st["options"]] == [("未开始", "todo"), ("进行中", "doing"), ("已完成", "done")]
    assert dbs.normalize(st, "进行中") == st["options"][1]["id"] and dbs.normalize(st, st["options"][2]["id"]) == st["options"][2]["id"]
    with pytest.raises(ValueError, match="可选：未开始"):
        dbs.normalize(st, "搁置")   # 状态不自动加
    assert n("人", ["用户", "Writer"]) == ["user", f"agent:{b['id']}"] and n("人", f"agent:{b['id']}") == [f"agent:{b['id']}"]
    with pytest.raises(ValueError, match="没有叫"):
        n("人", "不存在的人")
    assert n("日", "2026/11/5") == {"start": "2026-11-05"} and n("日", "2026-11-15 9:30") == {"start": "2026-11-15 09:30"}
    assert n("日", "2026-11-15 → 2026-11-20") == {"start": "2026-11-15", "end": "2026-11-20"}
    assert n("日", {"start": "today"}) == {"start": TODAY.isoformat()}
    with pytest.raises(ValueError):
        n("日", "下周")
    with pytest.raises(ValueError, match="前面"):
        n("日", {"start": "2026-11-20", "end": "2026-11-01"})
    assert n("好", "是") is True and n("好", False) is None
    assert n("网", "example.com") == "https://example.com"
    with pytest.raises(ValueError):
        n("信", "not-an-email")
    with pytest.raises(ValueError, match="自动算"):
        dbs.normalize({"key": "x", "name": "创建时间", "type": "created_time"}, 1)
    assert dbs.display(P(db, "数"), 1200) == "¥1,200" and dbs.display(P(db, "日"), {"start": "2026-11-15", "end": "2026-11-20"}) == "2026-11-15 → 2026-11-20"
    assert dbs.display(P(db, "人"), ["user", f"agent:{b['id']}"]) == "用户, Writer"


# ---------- 行 ----------
def test_rows_and_conflicts(made):
    db = new_db(made)
    r = dbs.add_row(db["id"], {"名称": "写周报", "状态": "进行中", "负责人": ["用户"], "日期": "2026-11-15"}, "user")
    assert r["title"] == "写周报" and r["version"] == 1 and r["created_by"] == "user"
    st = P(db, "状态")
    assert r["props"] == {"status": st["options"][1]["id"], "owner": ["user"], "due": {"start": "2026-11-15"}}
    p = library.get_page(r["id"], False)
    assert p["kind"] == "row" and p["parent_id"] == db["id"]
    assert pagedoc.version(r["id"], pagedoc.versions(r["id"])[0]["id"])["props"] == r["props"]   # 第一版就有属性
    assert r["id"] not in [x["id"] for x in library.family(db["id"])["children"]]   # 行不算子页面

    r2, changed = dbs.update_row(r["id"], {"状态": "已完成"}, "agent:1", base=1)
    assert changed == ["status"] and r2["version"] == 2 and r2["props"]["status"] == st["options"][2]["id"]
    assert dbs.update_row(r["id"], {"状态": "已完成"}, "user")[1] == []   # 没变不涨版本
    # 用户拿着版本 1 改同一个键：冲突，告诉他现在的值；改别的键不冲突
    with pytest.raises(dbs.Conflict) as e:
        dbs.update_row(r["id"], {"状态": "未开始"}, "user", base=1)
    assert e.value.items[0]["name"] == "状态" and e.value.items[0]["current"] == "已完成" and e.value.version == 2
    assert "被「" in dbs.conflict_text(e.value) and "version=2" in dbs.conflict_text(e.value)
    r3, changed = dbs.update_row(r["id"], {"负责人": None, "名称": "写周报（改）"}, "user", base=1)
    assert set(changed) == {"owner", "title"} and "owner" not in r3["props"] and r3["title"] == "写周报（改）"
    assert r3["meta"]["title"]["by"] == "user" and r3["updated_by"] == "user"
    # 自己改过的键，自己拿旧版本再改不算冲突
    assert dbs.update_row(r["id"], {"状态": "进行中"}, "agent:1", base=1)[1] == ["status"]
    with pytest.raises(ValueError, match="版本号"):
        dbs.update_row(r["id"], {"状态": "进行中"}, "user", base=99)

    # 历史：回到第 2 版，属性和标题一起回去
    v2 = next(v for v in pagedoc.versions(r["id"]) if v["version"] == 2)
    pagedoc.restore(r["id"], v2["id"])
    back = dbs.get_row(r["id"])
    assert back["props"]["status"] == st["options"][2]["id"] and back["props"]["owner"] == ["user"] and back["title"] == "写周报"
    assert back["meta"]["status"]["v"] == back["version"]

    # 顺序：after=0 放最前；行不能挪出库
    a = dbs.add_row(db["id"], {}, "user", "第二行")
    first = dbs.add_row(db["id"], {}, "user", "最前面", after=0)
    assert [x["title"] for x in dbs.rows(db["id"])] == ["最前面", "写周报", "第二行"]
    with pytest.raises(ValueError, match="挪出"):
        library.move(a["id"], None)
    with pytest.raises(ValueError):
        library.save_page(None, "子页面", "x", parent_id=first["id"])   # 行下面不放子页面
    with pytest.raises(ValueError):
        library.save_page(None, "子页面", "x", parent_id=db["id"])   # 库下面也不放


def test_restore_after_schema_change(made):
    """表结构改了以后回到行的旧版本：现在认不出的旧值不带回来（版本接口标成 stale），删掉的列也不回来。"""
    db = new_db(made)
    dbs.change_schema(db["id"], [{"op": "add", "name": "备注", "type": "text"}])
    note = P(dbs.get_db(db["id"]), "备注")["key"]
    r = dbs.add_row(db["id"], {"名称": "a", "状态": "进行中", "负责人": ["用户"], "备注": "abc"}, "user")
    v1 = pagedoc.versions(r["id"])[0]["id"]
    dbs.update_row(r["id"], {"状态": "已完成", "备注": "def"}, "agent:1")   # 换个人改，不和第一版并成一版
    dbs.change_schema(db["id"], [{"op": "update", "prop": "备注", "type": "number"}, {"op": "delete", "prop": "负责人"}])
    snap = pagedoc.version(r["id"], v1)
    assert snap["db_id"] == db["id"] and set(snap["stale"]) == {note, "owner"}   # "abc" 不是数，负责人删了
    pagedoc.restore(r["id"], v1)
    back = dbs.get_row(r["id"])
    st = P(dbs.get_db(db["id"]), "状态")
    assert back["props"] == {"status": st["options"][1]["id"]}   # 状态回去了；备注（转数字时清掉了）不变成 "abc"，负责人不回来
    assert pagedoc.version(r["id"], pagedoc.versions(r["id"])[0]["id"])["stale"] == []
    assert not dbs.fits(st, "o-gone") and dbs.fits(st, st["options"][0]["id"]) and dbs.fits(st, None)


def test_delete_db_cascades(made):
    db = new_db(made)
    rs = [dbs.add_row(db["id"], {}, "user", f"r{k}")["id"] for k in range(3)]
    library.delete_page(db["id"])
    assert all(library.get_page(r, False) is None for r in rs)


# ---------- 筛选、排序 ----------
def test_query(made):
    db = new_db(made, [{"name": "状态", "type": "status"}, {"name": "分", "type": "number"}, {"name": "标签", "type": "multi_select"},
                       {"name": "截止", "type": "date"}, {"name": "完成", "type": "checkbox"}, {"name": "备注", "type": "text"},
                       {"name": "人", "type": "person"}])
    y, t = (TODAY - timedelta(days=1)).isoformat(), (TODAY + timedelta(days=1)).isoformat()
    for title, props in [("甲", {"状态": "进行中", "分": 3, "标签": "a, b", "截止": y, "完成": True, "备注": "Hello World", "人": "用户"}),
                         ("乙", {"状态": "未开始", "分": 10, "标签": "b", "截止": t}),
                         ("丙", {"状态": "已完成", "分": -1}),
                         ("丁", {})]:
        dbs.add_row(db["id"], props, "user", title)
    db = dbs.get_db(db["id"])
    items = dbs.rows(db["id"])
    q = lambda f=None, s=None: [r["title"] for r in dbs.query(db, items, f, s)]   # noqa: E731
    assert q([{"prop": "状态", "op": "eq", "value": "进行中"}]) == ["甲"]
    assert q([{"prop": "状态", "op": "neq", "value": "进行中"}]) == ["乙", "丙", "丁"]
    assert q([{"prop": "标签", "op": "contains", "value": "b"}]) == ["甲", "乙"]
    assert q([{"prop": "分", "op": "gt", "value": 0}]) == ["甲", "乙"]
    assert q([{"prop": "截止", "op": "lt", "value": "today"}]) == ["甲"]
    assert q([{"prop": "截止", "op": "gte", "value": "今天"}]) == ["乙"]
    assert q([{"prop": "完成", "op": "eq", "value": True}]) == ["甲"] and q([{"prop": "完成", "op": "eq", "value": False}]) == ["乙", "丙", "丁"]
    assert q([{"prop": "备注", "op": "contains", "value": "world"}]) == ["甲"]
    assert q([{"prop": "人", "op": "contains", "value": "用户"}]) == ["甲"]
    assert q([{"prop": "分", "op": "empty"}]) == ["丁"] and q([{"prop": "标签", "op": "not_empty"}]) == ["甲", "乙"]
    assert q([{"prop": "名称", "op": "eq", "value": "丙"}]) == ["丙"]
    assert q([{"prop": "分", "op": "gte", "value": 0}, {"prop": "标签", "op": "contains", "value": "a"}]) == ["甲"]   # 都要满足
    assert q(s=[{"prop": "分", "desc": True}]) == ["乙", "甲", "丙", "丁"]   # 空的在最后
    assert q(s=[{"prop": "状态"}]) == ["乙", "甲", "丙", "丁"]   # 按选项的顺序
    assert q(s=[{"prop": "名称", "desc": True}]) == ["甲", "乙", "丙", "丁"]   # 文字按码位
    with pytest.raises(ValueError, match="没有属性"):
        q([{"prop": "不存在", "op": "eq", "value": 1}])
    with pytest.raises(ValueError, match="没有选项"):
        q([{"prop": "状态", "op": "eq", "value": "搁置"}])

    out = dbs.tool_query({"db_id": db["id"], "filter": [{"prop": "分", "op": "gt", "value": 0}], "sort": [{"prop": "分"}],
                          "props": ["分"]})
    lines = out.splitlines()
    assert "共 4 行，符合条件的 2 行" in lines[0] and lines[1] == "| 行 | 版本 | 名称 | 分 |" and "| 甲 | 3 |" in lines[3]
    assert "没有符合条件的行" in dbs.tool_query({"db_id": db["id"], "filter": [{"prop": "分", "op": "gt", "value": 100}]})
    assert "没有属性" in dbs.tool_query({"db_id": db["id"], "props": ["不存在"]})
    assert "不是数据库" in dbs.tool_query({"db_id": items[0]["id"]})


# ---------- 表结构 ----------
def test_schema_changes(made):
    db = new_db(made, [{"name": "备注", "type": "text"}, {"name": "标签", "type": "multi_select", "options": ["a", "b"]},
                       {"name": "分", "type": "number"}, {"name": "级", "type": "select", "options": ["高", "低"]}])
    r = dbs.add_row(db["id"], {"备注": "紧急", "标签": "a, b", "分": 7, "级": "高"}, "user", "行")
    db = dbs.change_schema(db["id"], [{"op": "add", "name": "完成", "type": "checkbox"}, {"op": "update", "prop": "备注", "name": "说明"},
                                      {"op": "update", "prop": "说明", "type": "select"}, {"op": "update", "prop": "标签", "type": "select"},
                                      {"op": "update", "prop": "分", "type": "text"}])
    row = dbs.get_row(r["id"])
    vals = {p["name"]: dbs.display(p, dbs.value_of(row, p)) for p in db["schema"]}
    assert vals == {"名称": "行", "说明": "紧急", "标签": "a", "分": "7", "级": "高", "完成": ""}   # 文字变单选加了选项，多选变单选留第一个
    assert [o["name"] for o in dbs.prop_of(db, "说明")["options"]] == ["紧急"]
    # 改选项：去掉的选项从各行清掉；改成数字转不了的清空
    db = dbs.change_schema(db["id"], [{"op": "update", "prop": "级", "options": ["低", "中"]}, {"op": "update", "prop": "说明", "type": "number"}])
    row = dbs.get_row(r["id"])
    assert "级" not in [p["name"] for p in db["schema"] if dbs.value_of(row, p)] and dbs.value_of(row, dbs.prop_of(db, "说明")) is None
    assert [o["name"] for o in dbs.prop_of(db, "级")["options"]] == ["低", "中"]
    # 删属性：值也清掉；看板按它分列的退回别的单选
    views = [{"id": "v1", "name": "表格", "type": "table", "hidden": ["p0"]}, {"id": "b", "name": "看板", "type": "board", "group_by": dbs.prop_of(db, "标签")["key"]}]
    dbs.set_views(db["id"], views)
    db = dbs.change_schema(db["id"], [{"op": "delete", "prop": "标签"}])
    assert dbs.prop_of(db, "标签") is None and dbs.get_db(db["id"])["views"][1]["group_by"] == dbs.prop_of(db, "级")["key"]
    db = dbs.change_schema(db["id"], [{"op": "delete", "prop": "级"}])
    assert dbs.get_db(db["id"])["views"][1]["type"] == "table"   # 没有能分列的属性了
    for bad, msg in [([{"op": "delete", "prop": "名称"}], "不能删"), ([{"op": "add", "name": "说明", "type": "text"}], "已经有"),
                     ([{"op": "add", "name": "x", "type": "magic"}], "不认识"), ([{"op": "update", "prop": "完成", "options": ["a"]}], "没有选项"),
                     ([{"op": "add", "name": "ok", "type": "text"}, {"op": "delete", "prop": "nope"}], "没有属性")]:
        with pytest.raises(ValueError, match=msg):
            dbs.change_schema(db["id"], bad)
    assert dbs.prop_of(dbs.get_db(db["id"]), "ok") is None   # 有一个不行整批不做
    assert dbs.prop_of(dbs.change_schema(db["id"], [{"op": "add", "name": "人", "type": "人员"}]), "人")["type"] == "person"   # 中文的类型名也认


def test_views(made):
    db = new_db(made)
    assert [(v["name"], v["type"], v.get("group_by")) for v in db["views"]] == [("表格", "table", None), ("看板", "board", "status")]
    for bad in ([], [{"type": "gallery"}], [{"type": "board", "group_by": "due"}],
                [{"type": "table", "filter": [{"prop": "status", "op": "like"}]}]):
        with pytest.raises(ValueError):
            dbs.set_views(db["id"], bad)
    db = dbs.set_views(db["id"], [{"id": "v1", "name": "  我的  ", "type": "table", "sort": [{"prop": "due", "desc": True}, {"prop": "nope"}],
                                   "hidden": ["title", "owner"], "widths": {"due": 9999}}])
    v = db["views"][0]
    assert v["name"] == "我的" and v["sort"] == [{"prop": "due", "desc": True}] and v["hidden"] == ["owner"] and v["widths"] == {"due": 800}


def test_view_history(made):
    """视图的改动记进库的历史：同一个人接着改并成一条，能撤销（只撤那次改到的视图）。"""
    db = new_db(made)
    t, b = db["views"]
    dbs.set_views(db["id"], [{**t, "sort": [{"prop": "due", "desc": True}]}, b])
    dbs.set_views(db["id"], [{**t, "sort": [{"prop": "due", "desc": True}], "widths": {"due": 200}}, b])   # 接着改：并成一条
    h = dbs.history(db["id"])
    assert len(h) == 1 and h[0]["changes"][0]["kind"] == "views" and h[0]["changes"][0]["changed"] == ["表格"]
    assert "改了视图「表格」" in dbs.change_text(h[0]["changes"][0])
    cur = dbs.get_db(db["id"])["views"]
    dbs.set_views(db["id"], cur + [{"name": "我的看板", "type": "board", "group_by": "status"}], "agent:1")   # 别人加一个视图：另记一条
    h = dbs.history(db["id"])
    assert len(h) == 2 and h[0]["changes"][0]["added"] == ["我的看板"]
    dbs.undo(db["id"], h[1]["id"])   # 撤销最早那条：表格的排序、列宽回去，后加的视图留着
    views = dbs.get_db(db["id"])["views"]
    assert views[0]["sort"] == [] and views[0]["widths"] == {} and [v["name"] for v in views] == ["表格", "看板", "我的看板"]
    dbs.set_views(db["id"], [views[0], views[2]])   # 删一个再撤销：放回原来的位置
    dbs.undo(db["id"], dbs.history(db["id"])[0]["id"])
    assert [v["name"] for v in dbs.get_db(db["id"])["views"]] == ["表格", "看板", "我的看板"]
    assert dbs.set_views(db["id"], dbs.get_db(db["id"])["views"]) and len(dbs.history(db["id"])) == 5   # 没变不记


# ---------- 工具、锁定、read_page ----------
def test_tools(made, bot):
    b = bot("Nova2")
    out, ev = dbs.tool_create(b["id"], {"title": "会议", "properties": [{"name": "时间", "type": "date"}, {"name": "类型", "type": "select", "options": ["周会"]}]})
    db_id = ev["db_id"]
    made.append(db_id)
    assert out.startswith(f"已建数据库 #{db_id}") and "类型（单选：周会）" in out
    assert library.get_page(db_id, False)["agent_id"] == b["id"]
    out, ev = dbs.tool_add(b["id"], {"db_id": db_id, "rows": [{"title": "周会", "props": {"时间": "2026-11-16", "类型": "周会"}, "content": "## 议程"},
                                                             {"title": "复盘", "props": {"类型": "复盘会"}}, {"title": "坏的", "props": {"时间": "下周"}}]})
    assert "加了 2 行" in out and "第 3 行没加" in out and len(ev["rows"]) == 2
    rid = ev["rows"][0]
    assert [o["name"] for o in dbs.prop_of(dbs.get_db(db_id), "类型")["options"]] == ["周会", "复盘会"]   # 自动加的选项存下来了
    assert dbs.get_row(rid)["created_by"] == f"agent:{b['id']}"
    text = pagedoc.render(rid)
    assert f"数据库《会议》#{db_id} 的一行" in text and "时间：2026-11-16" in text and "## 议程" in text
    text = library.tool_read_page({"page_id": db_id})
    assert text.startswith("数据库《会议》") and "| 周会 |" in text and "db_update" in text
    out, ev = dbs.tool_update(b["id"], {"rows": [{"row_id": rid, "props": {"类型": "复盘会"}, "version": 1}, {"row_id": 999999, "props": {}}]})
    assert "改了 类型，现在是版本 2" in out and "没有这一行" in out and ev == {"db_id": db_id, "rows": [rid]}
    out, _ = dbs.tool_update("user", {"rows": [{"row_id": rid, "props": {"类型": "周会"}, "version": 1}]})
    assert "被「Nova2」改成了：复盘会" in out
    out, ev = dbs.tool_schema(b["id"], {"db_id": db_id, "ops": [{"op": "add", "name": "纪要人", "type": "person"}]})
    assert "纪要人（人员）" in out and ev
    # 编辑、整页保存数据库页面都不行；行的正文能 edit_page
    assert "数据库" in pagedoc.tool_edit(b["id"], {"page_id": db_id, "version": 1, "ops": [{"op": "append", "markdown": "x"}]})[0]
    v = dbs.get_row(rid)["version"]
    assert pagedoc.tool_edit(b["id"], {"page_id": rid, "version": v, "ops": [{"op": "append", "markdown": "- 第一项"}]})[0].startswith("已改")
    assert dbs.get_row(rid)["props"]   # 改正文不动属性
    # 锁定的库：Agent 只能读
    library.update_page(db_id, locked=True)
    assert "锁定" in dbs.tool_add(b["id"], {"db_id": db_id, "rows": [{"title": "x"}]})[0]
    assert "锁定" in dbs.tool_update(b["id"], {"rows": [{"row_id": rid, "props": {"类型": "周会"}}]})[0]
    assert "锁定" in dbs.tool_schema(b["id"], {"db_id": db_id, "ops": [{"op": "delete", "prop": "时间"}]})[0]
    assert "锁定" in pagedoc.tool_edit(b["id"], {"page_id": rid, "version": dbs.get_row(rid)["version"], "ops": [{"op": "append", "markdown": "x"}]})[0]
    assert "周会" in dbs.tool_query({"db_id": db_id}) and "已锁定" in dbs.tool_query({"db_id": db_id})
    # 默认的属性
    out, ev = dbs.tool_create(b["id"], {"title": "默认", "title_name": "任务"})
    made.append(ev["db_id"])
    assert "任务（名称）、状态（状态：未开始 / 进行中 / 已完成）、负责人（人员）、日期（日期）" in out


# ---------- 接口、事件、分享、搜索 ----------
async def test_api(client, made, events):
    c = client
    r = await c.post("/api/dbs", json={"title": "看板"})
    db = r.json()
    made.append(db["id"])
    assert [p["name"] for p in db["schema"]] == ["名称", "状态", "负责人", "日期"] and db["rows"] == [] and db["view"] == "v1"
    assert any(p["id"] == db["id"] and p["kind"] == "db" for p in (await c.get("/api/pages")).json())
    st = db["schema"][1]["options"]
    row = (await c.post(f"/api/dbs/{db['id']}/rows", json={"title": "做设计", "props": {"status": st[0]["id"]}})).json()
    assert row["props"] == {"status": st[0]["id"]} and row["version"] == 1
    assert not any(p["id"] == row["id"] for p in (await c.get("/api/pages")).json())   # 行不在页面列表里
    assert any(e["db_id"] == db["id"] and row["id"] in e["rows"] for e in events.of("db_changed"))
    r = await c.patch(f"/api/dbs/{db['id']}/rows/{row['id']}", json={"props": {"status": st[1]["id"], "title": "做设计稿"}, "version": 1})
    assert r.status_code == 200 and r.json()["version"] == 2 and r.json()["title"] == "做设计稿"
    dbs.update_row(row["id"], {"status": st[2]["id"]}, "agent:1")
    r = await c.patch(f"/api/dbs/{db['id']}/rows/{row['id']}", json={"props": {"status": st[0]["id"]}, "version": 2})
    assert r.status_code == 409 and r.json()["detail"]["conflicts"][0]["current"] == "已完成"
    assert (await c.patch(f"/api/dbs/{db['id']}/rows/{row['id']}", json={"props": {"nope": 1}})).status_code == 400
    r2 = (await c.post(f"/api/dbs/{db['id']}/rows", json={"title": "第二", "after": 0})).json()
    assert [x["id"] for x in (await c.get(f"/api/dbs/{db['id']}")).json()["rows"]] == [r2["id"], row["id"]]
    await c.post(f"/api/dbs/{db['id']}/rows/{r2['id']}/move", json={"after": row["id"]})
    got = (await c.get(f"/api/dbs/{db['id']}")).json()
    assert [x["id"] for x in got["rows"]] == [row["id"], r2["id"]] and got["total"] == 2
    r = await c.patch(f"/api/dbs/{db['id']}/schema", json={"ops": [{"op": "add", "name": "优先级", "type": "select", "options": ["高", "低"]}]})
    assert r.json()["schema"][-1]["name"] == "优先级" and any(e.get("schema") for e in events.of("db_changed"))
    views = got["views"] + [{"name": "进行中", "type": "table", "filter": [{"prop": "status", "op": "eq", "value": st[2]["id"]}]}]
    got = (await c.put(f"/api/dbs/{db['id']}/views", json={"views": views, "view": None})).json()
    vid = got["views"][-1]["id"]
    assert [x["id"] for x in (await c.get(f"/api/dbs/{db['id']}", params={"view": vid})).json()["rows"]] == [row["id"]]
    # 分享：只读的表格
    token = (await c.post(f"/api/pages/{db['id']}/share", json={"on": True})).json()["share_token"]
    shared = (await c.get(f"/api/shared/{token}")).json()
    assert shared["format"] == "markdown" and shared["content"].startswith("| 名称 | 状态 | 负责人 | 日期 | 优先级 |") and "| 做设计稿 | 已完成 |" in shared["content"]
    # 按段改的接口不收数据库；删一行
    assert (await c.post(f"/api/pages/{db['id']}/blocks", json={"version": 1, "ops": []})).status_code == 400
    assert (await c.delete(f"/api/dbs/{db['id']}/rows/{r2['id']}")).status_code == 200
    assert (await c.get(f"/api/dbs/{db['id']}")).json()["total"] == 1
    assert (await c.get(f"/api/dbs/{row['id']}")).status_code == 404
    assert (await c.delete(f"/api/dbs/{db['id']}/rows/{r2['id']}")).status_code == 404


async def test_search(made):
    db = new_db(made, [{"name": "客户", "type": "select"}])
    r = dbs.add_row(db["id"], {"客户": "猕猴桃果业"}, "user", "续约")
    hits = await search.search("猕猴桃果业")
    assert any(h["ref"] == r["id"] for h in hits)


# ---------- 表结构的历史和撤销 ----------
def test_schema_history_undo(made):
    db = new_db(made, [{"name": "级", "type": "select", "options": ["高", "低"]}, {"name": "备注", "type": "text"}, {"name": "分", "type": "number"}])
    rs = [dbs.add_row(db["id"], {"级": lv, "备注": f"第{k}行", "分": k}, "user", f"r{k}") for k, lv in enumerate(["高", "低", "高"])]
    key_of = lambda name: dbs.prop_of(dbs.get_db(db["id"]), name)["key"]   # noqa: E731
    lv_key, note_key = key_of("级"), key_of("备注")
    vals = lambda name: [dbs.display(dbs.prop_of(dbs.get_db(db["id"]), name), dbs.value_of(dbs.get_row(r["id"]), dbs.prop_of(dbs.get_db(db["id"]), name)))   # noqa: E731
                         for r in rs]

    # 删一列：历史里记着清掉了 3 行；撤销以后列回到原来的位置，值都回来
    out = dbs.change_schema(db["id"], [{"op": "delete", "prop": "级"}], "agent:7")
    assert out["last_change"] == [{"kind": "delete", "key": lv_key, "name": "级", "cleared": 3}]
    assert dbs.prop_of(dbs.get_db(db["id"]), "级") is None and all(lv_key not in dbs.get_row(r["id"])["props"] for r in rs)
    h = dbs.history(db["id"])
    assert h[0]["by"] == "agent:7" and h[0]["changes"][0]["cleared"] == 3 and "清掉了 3 行的值" in dbs.change_text(h[0]["changes"][0])
    back = dbs.undo(db["id"], h[0]["id"], "user")
    assert [p["name"] for p in back["schema"]] == ["名称", "级", "备注", "分"] and vals("级") == ["高", "低", "高"]
    h = dbs.history(db["id"])
    assert h[0]["undo_of"] == h[1]["id"] and h[1]["undone_by"] == h[0]["id"]
    assert dbs.change_text(h[0]["changes"][0]).startswith("撤销了：删了属性「级」")

    # 改类型（文字 → 数字，全都转不过去）；其中一格之后又被人改了：撤销时那一格留着现在的值（按文字存回去）
    dbs.change_schema(db["id"], [{"op": "update", "prop": "备注", "type": "number"}])
    assert vals("备注") == ["", "", ""]
    dbs.update_row(rs[1]["id"], {"备注": 42}, "user")
    dbs.undo(db["id"], dbs.history(db["id"])[0]["id"])
    assert dbs.prop_of(dbs.get_db(db["id"]), "备注")["type"] == "text" and vals("备注") == ["第0行", "42", "第2行"]
    assert dbs.history(db["id"])[0]["changes"][0]["kept"] == 1

    # 去掉一个用着的选项：那几行清掉；撤销以后选项和值都回来
    dbs.change_schema(db["id"], [{"op": "update", "prop": "级", "options": ["低"]}])
    assert vals("级") == ["", "低", ""]
    dbs.undo(db["id"], dbs.history(db["id"])[0]["id"])
    assert [o["name"] for o in dbs.prop_of(dbs.get_db(db["id"]), "级")["options"]] == ["高", "低"] and vals("级") == ["高", "低", "高"]

    # 撤销“加属性”就是删掉它；再撤销这次撤销，属性和值又回来
    dbs.change_schema(db["id"], [{"op": "add", "name": "新列", "type": "text"}])
    dbs.update_row(rs[0]["id"], {"新列": "有值"}, "user")
    dbs.undo(db["id"], dbs.history(db["id"])[0]["id"])
    assert dbs.prop_of(dbs.get_db(db["id"]), "新列") is None
    dbs.undo(db["id"], dbs.history(db["id"])[0]["id"])
    assert vals("新列") == ["有值", "", ""]

    # 只动那次改到的属性：之后别的改动（改名）不受影响；删掉的列名字被占了就加“（恢复）”
    dbs.change_schema(db["id"], [{"op": "delete", "prop": "分"}])
    gone = dbs.history(db["id"])[0]["id"]
    dbs.change_schema(db["id"], [{"op": "update", "prop": "备注", "name": "说明"}, {"op": "add", "name": "分", "type": "text"}])
    dbs.undo(db["id"], gone)
    names = [p["name"] for p in dbs.get_db(db["id"])["schema"]]
    assert "说明" in names and "分（恢复）" in names and vals("分（恢复）") == ["0", "1", "2"]
    assert dbs.change_schema(db["id"], [{"op": "update", "prop": "说明", "name": "说明"}])["last_change"] == []   # 没变不记
    with pytest.raises(LookupError):
        dbs.undo(db["id"], 999999)
    with pytest.raises(ValueError):   # 数据库不走页面的“回到这一版”
        pagedoc.restore(db["id"], dbs.history(db["id"])[0]["id"])


async def test_history_api(client, made, events):
    db = new_db(made)
    r = dbs.add_row(db["id"], {"负责人": "用户"}, "user", "x")
    out, _ = dbs.tool_schema("agent:1", {"db_id": db["id"], "ops": [{"op": "delete", "prop": "负责人"}]})
    assert "清掉了 1 行的值" in out and "撤销" in out
    c = client
    h = (await c.get(f"/api/dbs/{db['id']}/history")).json()
    assert h[0]["changes"][0]["kind"] == "delete" and h[0]["by"] == "agent:1"
    got = (await c.post(f"/api/dbs/{db['id']}/history/{h[0]['id']}/undo", json={})).json()
    assert any(p["name"] == "负责人" for p in got["schema"]) and got["last_change"][0]["kind"] == "undo"
    assert got["rows"][0]["props"]["owner"] == ["user"] and r["id"] == got["rows"][0]["id"]
    assert (await c.post(f"/api/dbs/{db['id']}/history/999999/undo", json={})).status_code == 404
    assert any(e["db_id"] == db["id"] and e.get("schema") for e in events.of("db_changed"))


# ---------- 算出来的属性、关联、文件、自动编号、按钮 ----------
def test_formula(made):
    db = new_db(made, [{"name": "单价", "type": "number", "format": "cny"}, {"name": "数量", "type": "number"},
                       {"name": "完成", "type": "checkbox"}, {"name": "截止", "type": "date"},
                       {"name": "金额", "type": "formula", "formula": 'prop("单价") * prop("数量")', "format": "cny"},
                       {"name": "标记", "type": "formula", "formula": 'if(prop("完成"), "✓", "还差 " + toText(10 - prop("数量")))'},
                       {"name": "剩几天", "type": "formula", "formula": 'dateBetween(prop("截止"), parseDate("2026-11-15"), "days")'}])
    a = dbs.add_row(db["id"], {"名称": "甲", "单价": 2.5, "数量": 4, "截止": "2026-11-20"}, "user")
    b = dbs.add_row(db["id"], {"名称": "乙", "单价": 10, "数量": 1, "完成": True}, "user")
    calc = lambda r, n: r["calc"][P(db, n)["key"]]   # noqa: E731
    assert calc(a, "金额") == 10 and calc(a, "标记") == "还差 6" and calc(a, "剩几天") == 5
    assert calc(b, "标记") == "✓" and calc(b, "剩几天") is None
    assert dbs.display(P(db, "金额"), 10) == "¥10"
    # 筛选、排序按算出来的值
    d = dbs.get_db(db["id"])
    assert [r["title"] for r in dbs.query(d, dbs.rows(db["id"]), [{"prop": "金额", "op": "gt", "value": 9.5}], [{"prop": "金额", "desc": True}])] == ["甲", "乙"]
    assert [r["title"] for r in dbs.query(d, dbs.rows(db["id"]), [{"prop": "标记", "op": "contains", "value": "还差"}])] == ["甲"]
    # 改名：公式跟着改；写不进值；写错了、自己引用自己、成环都报错
    dbs.change_schema(db["id"], [{"op": "update", "prop": "数量", "name": "件数"}])
    assert P(db, "金额")["formula"] == 'prop("单价") * prop("件数")'
    assert dbs.get_row(a["id"])["calc"][P(db, "金额")["key"]] == 10
    with pytest.raises(ValueError, match="自动算"):
        dbs.update_row(a["id"], {"金额": 3}, "user")
    for bad, msg in [('prop("没有")', "不存在"), ("1 +", "没写完"), ('prop("坏")', "它自己")]:
        with pytest.raises(ValueError, match=msg):
            dbs.change_schema(db["id"], [{"op": "add", "name": "坏", "type": "formula", "formula": bad}])
    dbs.change_schema(db["id"], [{"op": "add", "name": "甲式", "type": "formula", "formula": "1"},
                                 {"op": "add", "name": "乙式", "type": "formula", "formula": 'prop("甲式") + 1'}])
    with pytest.raises(ValueError, match="成了环"):
        dbs.change_schema(db["id"], [{"op": "update", "prop": "甲式", "formula": 'prop("乙式")'}])
    # read_page / 工具看到的是算出来的值
    assert "¥10" in dbs.render_db(db["id"]) and "prop(" in dbs.schema_text(dbs.get_db(db["id"]))


def test_relation_rollup(made):
    proj = new_db(made, [{"name": "预算", "type": "number"}], title="项目")
    task = new_db(made, [{"name": "工时", "type": "number"}, {"name": "完成", "type": "checkbox"},
                         {"name": "项目", "type": "relation", "db": proj["id"], "twoway": True, "pair_name": "任务"}], title="任务")
    mirror = P(proj, "任务")
    assert mirror and mirror["mirror_of"] == P(task, "项目")["key"] and P(task, "项目")["pair"] == mirror["key"]
    dbs.change_schema(proj["id"], [{"op": "add", "name": "总工时", "type": "rollup", "relation": "任务", "target": "工时", "fn": "sum"},
                                   {"op": "add", "name": "完成率", "type": "rollup", "relation": "任务", "target": "完成", "fn": "percent_checked"},
                                   {"op": "add", "name": "几个", "type": "rollup", "relation": "任务", "fn": "count"}])
    p1 = dbs.add_row(proj["id"], {"名称": "官网改版"}, "user")
    p2 = dbs.add_row(proj["id"], {"名称": "发布会"}, "user")
    t1 = dbs.add_row(task["id"], {"名称": "设计", "工时": 5, "完成": True, "项目": "官网改版"}, "user")   # 写名称
    t2 = dbs.add_row(task["id"], {"名称": "开发", "工时": 8, "项目": [f"#{p1['id']}"]}, "user")        # 写行号
    assert t1["props"][P(task, "项目")["key"]] == [p1["id"]]
    row = dbs.get_row(p1["id"])
    c = lambda r, n: r["calc"][P(proj, n)["key"]]   # noqa: E731
    assert c(row, "任务") == [t1["id"], t2["id"]] and c(row, "总工时") == 13 and c(row, "完成率") == 50.0 and c(row, "几个") == 2
    assert dbs.display(P(proj, "任务"), c(row, "任务")) == "设计, 开发"
    # 在反向那一半改：改的是任务那边
    dbs.update_row(p2["id"], {"任务": [t2["id"]]}, "user")
    assert dbs.get_row(t2["id"])["props"][P(task, "项目")["key"]] == [p1["id"], p2["id"]]
    dbs.update_row(p1["id"], {"任务": [t1["id"]]}, "user")
    assert dbs.get_row(t2["id"])["props"][P(task, "项目")["key"]] == [p2["id"]]
    assert c(dbs.get_row(p1["id"]), "总工时") == 5
    # 关联的库里没有的行不认；筛选按名称
    with pytest.raises(ValueError, match="没有叫"):
        dbs.update_row(t1["id"], {"项目": "不存在的项目"}, "user")
    with pytest.raises(ValueError, match="没有行"):
        dbs.update_row(t1["id"], {"项目": [t1["id"]]}, "user")   # 任务库的行不是项目
    d = dbs.get_db(task["id"])
    assert [r["title"] for r in dbs.query(d, dbs.rows(task["id"]), [{"prop": "项目", "op": "contains", "value": "发布"}])] == ["开发"]
    assert dbs.related_dbs(task["id"]) == {proj["id"]} and dbs.related_dbs(proj["id"]) == {task["id"]}
    # 网页拿到的标题表
    pub = dbs.public(dbs.get_db(proj["id"]))
    assert pub["refs"][t1["id"]] == "设计"
    # 删了这边的关联，对方那一半也去掉
    dbs.change_schema(task["id"], [{"op": "delete", "prop": "项目"}])
    assert P(proj, "任务") is None
    assert dbs.get_row(p1["id"])["calc"].get(P(proj, "总工时")["key"]) is None


def test_files_unique_id_button(made, bot):
    b = bot("Helper")
    db = new_db(made)
    mid = media.save(png(5, 3, (1, 2, 3)), "图.png")["id"]
    dbs.change_schema(db["id"], [{"op": "add", "name": "附件", "type": "files"}])
    r1 = dbs.add_row(db["id"], {"名称": "一", "附件": [mid, "https://example.com/a/报告.pdf"]}, "user")
    assert r1["props"][P(db, "附件")["key"]] == [{"name": "图.png", "url": f"/api/media/{mid}"}, {"name": "报告.pdf", "url": "https://example.com/a/报告.pdf"}]
    assert mid in media.used()   # 属性里引用着的不会被清掉
    with pytest.raises(ValueError, match="网址"):
        dbs.update_row(r1["id"], {"附件": "ftp://x"}, "user")
    r2 = dbs.add_row(db["id"], {"名称": "二"}, "user")
    # 加自动编号：已有的行按顺序发号，新行接着发
    dbs.change_schema(db["id"], [{"op": "add", "name": "编号", "type": "unique_id", "prefix": "T-"}])
    k = P(db, "编号")["key"]
    assert dbs.get_row(r1["id"])["props"][k] == 1 and dbs.get_row(r2["id"])["props"][k] == 2
    r3 = dbs.add_row(db["id"], {"名称": "三"}, "user")
    assert r3["props"][k] == 3 and dbs.display(P(db, "编号"), 3) == "T-3" and P(db, "编号")["next"] == 4
    with pytest.raises(ValueError, match="自动算"):
        dbs.update_row(r3["id"], {"编号": 9}, "user")
    d = dbs.get_db(db["id"])
    assert [r["title"] for r in dbs.query(d, dbs.rows(db["id"]), [{"prop": "编号", "op": "gte", "value": "T-2"}])] == ["二", "三"]
    # 按钮：改状态、写评论通知 Agent
    dbs.change_schema(db["id"], [{"op": "add", "name": "做完", "type": "button", "label": "标成完成", "set": {"状态": "已完成"},
                                  "comment": "这一行做完了，帮我写个总结", "agent": b["name"]}])
    btn = P(db, "做完")
    assert btn["set"] == {"status": P(db, "状态")["options"][2]["id"]} and btn["agent"] == b["id"]
    row, changed, note = dbs.press(r1["id"], btn["key"])
    assert changed == ["status"] and note == {"text": "这一行做完了，帮我写个总结", "agent": b["id"]}
    with pytest.raises(ValueError, match="没有 Agent"):
        dbs.change_schema(db["id"], [{"op": "update", "prop": "做完", "agent": "不存在的"}])
    with pytest.raises(ValueError, match="按钮"):
        dbs.update_row(r1["id"], {"做完": 1}, "user")
    assert "标成完成" not in dbs.render_db(db["id"]) and "只有用户能点" in dbs.schema_text(dbs.get_db(db["id"]))


async def test_press_and_related_events(client, made, bot, events):
    b = bot("Closer")
    proj = new_db(made, [], title="项目")
    task = new_db(made, title="任务")
    dbs.change_schema(task["id"], [{"op": "add", "name": "项目", "type": "relation", "db": proj["id"]},
                                   {"op": "add", "name": "做完", "type": "button", "set": {"状态": "已完成"}, "comment": "收尾", "agent": b["id"]}])
    t = dbs.add_row(task["id"], {"名称": "写文档"}, "user")
    key = P(task, "做完")["key"]
    c = client
    r = await c.post(f"/api/dbs/{task['id']}/rows/{t['id']}/press/{key}")
    assert r.status_code == 200 and r.json()["changed"] == ["status"] and r.json()["comment"]["text"] == "收尾"
    assert r.json()["comment"]["woke"] == [b["id"]]
    assert (await c.post(f"/api/dbs/{task['id']}/rows/{t['id']}/press/nope")).status_code == 404
    got = {e["db_id"] for e in events.of("db_changed")}
    assert {task["id"], proj["id"]} <= got   # 关联着的库也收到
    row = (await c.get(f"/api/dbs/{task['id']}/rows/{t['id']}")).json()
    assert "calc" in row["row"] and row["refs"] == {}
    # 文件属性：什么文件都能传，不是图片的只能下载
    up = await c.put("/api/media?file=1&name=说明.txt", content=b"hello")
    assert up.status_code == 200 and up.json()["mime"] == "application/octet-stream"
    got = await c.get(f"/api/media/{up.json()['id']}")
    assert got.headers["content-disposition"].startswith("attachment")
    assert (await c.put("/api/media?name=x.txt", content=b"hello")).status_code == 400   # 对话里还是只收图片
    await settle()
    n = notify.listing(b["id"])
    assert len(n) == 1 and n[0]["comment"]["text"] == "收尾" and n[0]["page"]["id"] == t["id"] and n[0]["page"]["kind"] == "row"


async def test_paging(client, made):
    db = new_db(made)
    ids = [dbs.add_row(db["id"], {"状态": "进行中" if k % 3 else "已完成"}, "user", f"行{k}")["id"] for k in range(7)]
    c = client
    d = (await c.get(f"/api/dbs/{db['id']}?limit=3")).json()
    assert [r["id"] for r in d["rows"]] == ids[:3] and d["count"] == 7 and d["total"] == 7
    d = (await c.get(f"/api/dbs/{db['id']}?limit=3&include={ids[6]}")).json()
    assert [r["id"] for r in d["rows"]] == ids[:3] + [ids[6]]   # 刚加的行带在最后
    board = next(v for v in d["views"] if v["type"] == "board")
    d = (await c.get(f"/api/dbs/{db['id']}?view={board['id']}&limit=2")).json()
    st = {o["name"]: o["id"] for o in P(db, "状态")["options"]}
    assert d["groups"] == {st["已完成"]: 3, st["进行中"]: 4} and len(d["rows"]) == 2
    found = (await c.get(f"/api/dbs/{db['id']}/find?q=行5")).json()
    assert found == [{"id": ids[5], "title": "行5"}]
