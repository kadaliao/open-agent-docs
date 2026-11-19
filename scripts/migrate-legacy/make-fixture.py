"""用旧版（legacy/）自己的代码造一份真实形状的旧数据库，给迁移工具测试用。

    cd legacy && uv run python ../scripts/migrate-legacy/make-fixture.py /tmp/oad-legacy-fixture

数据放在给的目录里（OAD_DATA_DIR/docs.db）。内容：嵌套的页面（Markdown 和 HTML）、图片、几种块（标题、列表、待办、
代码、表格、引用、公式）、评论和回复（有解决了的、有 Agent 写的）、Agent（有 webhook 的）、分享的页面、回收站里的页面，
两个数据库（各种属性类型、选项、双向关联、汇总、公式、文件、自动编号、按钮、视图），行有正文。
"""
import os
import struct
import sys
import zlib
from pathlib import Path

out = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/oad-legacy-fixture").resolve()
out.mkdir(parents=True, exist_ok=True)
(out / "docs.db").unlink(missing_ok=True)
os.environ["OAD_DATA_DIR"] = str(out)
os.environ["OAD_LANG"] = "zh"
sys.path.insert(0, str(Path.cwd()))

from open_agent_docs import agents, dbs, library, media, pagecomments  # noqa: E402


def png(w, h, rgb):
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))

    def chunk(t, data):
        return struct.pack(">I", len(data)) + t + data + struct.pack(">I", zlib.crc32(t + data) & 0xFFFFFFFF)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


# ---------- Agent ----------
writer, _, _ = agents.create("Writer", "write", "https://example.com/hooks/writer", about="写周报、整理会议记录")
reader, _, _ = agents.create("Reader", "read", about="只读的检查员")
admin, _, _ = agents.create("Hub", "admin")
gone, _, _ = agents.create("Old", "write")
agents.revoke(gone["id"])

# ---------- 图片 ----------
red = media.save(png(32, 20, (220, 60, 60)), "red.png")
blue = media.save(png(40, 40, (60, 90, 220)), "blue.png")
doc = media.save(b"%PDF-1.4\n% fake pdf\n", "spec.pdf", any_file=True)

# ---------- 页面 ----------
home = library.save_page(None, "团队首页", f"""# 欢迎

这里是团队的**工作空间**，看 [项目说明](https://example.com/spec)。

{media.image_md(red['id'], '红色的图')}

## 本周

- 搜索上线
- 历史记录
  - 嵌套的一条

1. 第一步
2. 第二步

- [x] 写完方案
- [ ] 评审

> 先量，再改。

```python
def hello():
    return "世界"
```

| 名字 | 状态 |
| --- | --- |
| 甲 | 完成 |
| 乙 | 进行中 |

$$E = mc^2$$

---

行内公式 $a^2+b^2$ 和 `代码`。
""", by="user")
plan = library.save_page(None, "季度计划", "## 目标\n\n1. 提速\n2. 稳定\n\n" + media.image_md(blue["id"], "蓝"), by="user", parent_id=home["id"])
notes = library.save_page(None, "会议记录 11-15", "讨论了迁移方案。\n\n- 先导页面\n- 再导数据库", by=f"agent:{writer['id']}", parent_id=plan["id"])
library.save_page(None, "第二个子页面", "很短。", by="user", parent_id=home["id"])
html = library.save_page(None, "HTML 页面", "<h2>旧的 HTML 页面</h2><p>一段<b>粗体</b>和<a href='https://example.com'>链接</a>。</p><ul><li>一</li><li>二</li></ul>", fmt="html", by="user")
library.share(home["id"], True)
library.update_page(plan["id"], pinned=True)
trashed = library.save_page(None, "删掉的页面", "在回收站里", by="user")
library.delete_page(trashed["id"])

# ---------- 评论 ----------
p, blks = __import__("open_agent_docs.pagedoc", fromlist=["load"]).load(home["id"])
first = next(b for b in blks if "工作空间" in b.get("text", b.get("md", "")) ) if blks and isinstance(blks[0], dict) else None
bid = first["id"] if first else None
c1 = pagecomments.add(home["id"], "user", f"@Writer 这段要再写清楚一点", block=bid, quote="工作空间")
pagecomments.add(home["id"], f"agent:{writer['id']}", "好的，我改一下。", reply_to=c1["id"])
c2 = pagecomments.add(home["id"], "user", "整页的意见：标题太长。")
pagecomments.update(home["id"], c2["id"], resolved=True)
pagecomments.add(plan["id"], "user", "目标要量化")

# ---------- 数据库 ----------
proj = dbs.create("项目", "user", properties=[
    {"name": "负责人", "type": "person"},
    {"name": "预算", "type": "number", "format": "cny"},
])
proj_rows = [dbs.add_row(proj["id"], {"名称": n, "负责人": who, "预算": b}, "user") for n, who, b in
             [("搜索", "用户", 12000), ("历史", "Writer", 8000), ("迁移", "user", 3000)]]
tasks = dbs.create("任务", "user", parent_id=home["id"], properties=[
    {"name": "状态", "type": "status"},
    {"name": "优先级", "type": "select", "options": ["高", "中", "低"]},
    {"name": "标签", "type": "multi_select", "options": ["前端", "服务器", "文档"]},
    {"name": "截止", "type": "date"},
    {"name": "负责人", "type": "person"},
    {"name": "工时", "type": "number"},
    {"name": "完成", "type": "checkbox"},
    {"name": "链接", "type": "url"},
    {"name": "邮箱", "type": "email"},
    {"name": "电话", "type": "phone"},
    {"name": "备注", "type": "text"},
    {"name": "附件", "type": "files"},
    {"name": "项目", "type": "relation", "db": proj["id"], "twoway": True, "pair_name": "任务"},
    {"name": "创建时间", "type": "created_time"},
    {"name": "编号", "type": "unique_id", "prefix": "T-"},
])
dbs.change_schema(tasks["id"], [
    {"op": "add", "name": "人天", "type": "formula", "formula": 'prop("工时") / 8'},
    {"op": "add", "name": "做完", "type": "button", "label": "标成完成", "set": {"状态": "已完成"}},
])
dbs.change_schema(proj["id"], [
    {"op": "add", "name": "总工时", "type": "rollup", "relation": "任务", "target": "工时", "fn": "sum"},
    {"op": "add", "name": "任务数", "type": "rollup", "relation": "任务", "fn": "count"},
])
rows = [
    {"名称": "搜索框", "状态": "进行中", "优先级": "高", "标签": ["前端"], "截止": "2026-11-20", "负责人": "Writer", "工时": 16, "完成": False,
     "链接": "https://example.com/a", "邮箱": "a@example.com", "电话": "+86 138 0000 0000", "备注": "先做中文分词", "项目": ["搜索"],
     "附件": [media.url(doc["id"])]},
    {"名称": "索引", "状态": "已完成", "优先级": "中", "标签": ["服务器", "文档"], "截止": "2026-11-15 09:30 → 2026-11-18", "负责人": ["用户"], "工时": 24,
     "完成": True, "项目": ["搜索"]},
    {"名称": "版本对比", "状态": "未开始", "优先级": "低", "标签": [], "工时": 8, "项目": ["历史"], "附件": [{"name": "图", "url": media.url(red["id"])}]},
    {"名称": "", "备注": "没有名字的一行"},
]
for r in rows:
    dbs.add_row(tasks["id"], r, "user", content="行的正文：" + (r["名称"] or "空"))
sch = dbs.get_db(tasks["id"])["schema"]
key = {p["name"]: p["key"] for p in sch}
dbs.set_views(tasks["id"], [
    {"id": "v1", "name": "全部", "type": "table", "sort": [{"prop": key["工时"], "desc": True}], "filter": [{"prop": key["完成"], "op": "neq", "value": True}], "hidden": [key["电话"]]},
    {"id": "v2", "name": "看板", "type": "board", "group_by": key["状态"]},
])
print(f"旧版数据造好了：{out / 'docs.db'}")
