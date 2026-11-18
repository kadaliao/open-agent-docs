"""页面的存储：新建、改标题、置顶、锁定、分享、子页面（挪动、上级和子页面）、回收站。

页面存在 pages 表里，正文是 Markdown（pagedoc.py 按段读写）。数据库是 kind=db 的页面，它的每一行是 kind=row 的子页面（dbs.py）。
分享就是生成一个不可猜的链接 /s/<token>，取消分享后链接失效。
bot_id、app、format=html 是从 OSA 带过来的列：这里不用（OSA 里 Bot 建的页面、小应用），留着方便两边的库互相导。
"""
import json
import re
import secrets
import time
from . import i18n
from .db import conn

conn.execute("""create table if not exists pages(
  id integer primary key, bot_id integer, title text, format text default 'markdown', content text,
  share_token text, pinned integer default 0, task_id integer, created real, updated real, app text)""")
conn.execute("create unique index if not exists pages_token on pages(share_token)")
_have = {r[1] for r in conn.execute("pragma table_info(pages)")}
for _col, _type in (("app", "text"), ("block_meta", "text"), ("version", "integer default 0"), ("locked", "integer default 0"),
                    ("parent_id", "integer"), ("pos", "real"), ("agent_id", "integer"),
                    ("kind", "text default 'page'"), ("schema", "text"), ("views", "text"), ("props", "text"), ("props_meta", "text")):
    if _col not in _have:   # block_meta / version / locked：按段编辑（pagedoc.py）；parent_id / pos：子页面和它们的顺序；
        # agent_id：建它的 Agent（agents.py），这时 bot_id 是空的；
        # kind：page / db（数据库）/ row（数据库的一行）；schema / views 是库的表结构和视图，props / props_meta 是行的属性（见 dbs.py）
        conn.execute(f"alter table pages add column {_col} {_type}")
conn.commit()

FORMATS = ("markdown", "html")
MAX_PAGE = 500_000
MAX_PAGES = 5000

_COLS = ("id, bot_id, title, format, share_token, pinned, task_id, created, updated, length(content), app, version, locked, parent_id, pos, "
         "agent_id, coalesce(kind, 'page')")
KINDS = ("page", "db", "row")


# ---------- 页面 ----------
def _page(r, content: str | None = None) -> dict:
    i, bot_id, title, fmt, token, pinned, task_id, created, updated, size, app, version, locked, parent_id, pos, agent_id, kind = r
    p = {"id": i, "bot_id": bot_id, "title": title, "format": fmt, "share_token": token, "pinned": bool(pinned),
         "task_id": task_id, "created": created, "updated": updated, "size": size or 0,
         "app": json.loads(app) if app else None, "version": version or 0, "locked": bool(locked), "parent_id": parent_id, "pos": pos or 0, "agent_id": agent_id, "kind": kind}
    if content is not None:
        p["content"] = content
    return p


def list_pages() -> list[dict]:
    return [_page(r) for r in conn.execute(f"select {_COLS} from pages order by pinned desc, updated desc")]


def get_page(pid: int, with_content: bool = True) -> dict | None:
    r = conn.execute(f"select {_COLS}, content from pages where id=?", (pid,)).fetchone()
    return _page(r[:-1], r[-1] if with_content else None) if r else None


def by_token(token: str) -> dict | None:
    if not token:
        return None
    r = conn.execute("select id from pages where share_token=?", (token,)).fetchone()
    return get_page(r[0]) if r else None


def save_page(bot_id: int | None, title: str, content: str, fmt: str = "markdown",
              pid: int | None = None, task_id: int | None = None, app=None, by: str | None = None,
              base: int | None = None, allow_empty: bool = False, parent_id: int | None = None,
              kind: str = "page", props: dict | None = None) -> dict:
    """新建，或者给 pid 时更新内容。app 不用（OSA 的小应用）。
    by 是谁改的（默认是用户；agent:<id> 是 Agent，新建时记进 agent_id）。markdown 页面更新时按段对齐（pagedoc.rewrite），没变的段保留编号和作者；
    给了 base（调用方读到的版本号），会覆盖掉 base 之后别人改过的段就抛 pagedoc.Conflict。allow_empty：用户新建的空白页面。
    parent_id 只在新建时有用：放在哪个页面下面（排在最后）。kind、props 也只在新建时有用：数据库（db）和它的行（row），见 dbs.py；
    行的属性先写好再记第一版历史，第一版里就有属性。"""
    from . import pagedoc
    title = " ".join((title or "").split())[:120] or ("" if kind == "row" else i18n.t("未命名页面"))   # 数据库的行可以没有名称
    fmt = fmt if fmt in FORMATS else "markdown"
    app = None
    by = by if by is not None else (f"bot:{bot_id}" if bot_id else "user")
    content = content or ""
    if not content.strip() and not allow_empty:
        raise ValueError(i18n.t("页面内容不能为空"))
    if len(content) > MAX_PAGE:
        raise ValueError(i18n.t("页面太大（最多 {n}K 字）", n=MAX_PAGE // 1000))
    if base is not None and (not isinstance(base, int) or isinstance(base, bool)):
        raise ValueError(i18n.t("版本号要是整数"))
    now = time.time()
    if pid is not None:
        if not (old := get_page(pid, False)):
            raise ValueError(i18n.t("没有页面 #{id}", id=pid))
        if fmt == "markdown" and old["format"] == "markdown":
            pagedoc.rewrite(pid, title, content, by, base)
        else:   # html 页面，或者换了格式：整页换掉，段的编号以后按需重建
            conn.execute("update pages set format=?, app=?, block_meta=null where id=?",
                         (fmt, json.dumps(app, ensure_ascii=False) if app else None, pid))
            pagedoc.snapshot_html(pid, by, title=title, content=content)
    else:
        if kind not in KINDS:
            raise ValueError(i18n.t("kind 只能是 {kinds}", kinds=" / ".join(KINDS)))
        if kind != "row" and conn.execute("select count(*) from pages where coalesce(kind, 'page') != 'row'").fetchone()[0] >= MAX_PAGES:
            raise ValueError(i18n.t("最多 {n} 个页面，先删掉一些", n=MAX_PAGES))   # 数据库的行另算（dbs.MAX_ROWS）
        if parent_id is not None or kind == "row":
            _check_parent(parent_id, kind=kind)
        agent_id = int(by[6:]) if by.startswith("agent:") and by[6:].isdigit() else None
        pid = conn.execute("insert into pages(bot_id, title, format, content, task_id, created, updated, app, parent_id, pos, agent_id, kind, props) "
                           "values(?,?,?,?,?,?,?,?,?,?,?,?,?)", (bot_id, title, fmt, content, task_id, now, now,
                                                                 json.dumps(app, ensure_ascii=False) if app else None,
                                                                 parent_id, _end_pos(parent_id), agent_id, kind,
                                                                 json.dumps(props, ensure_ascii=False) if props is not None else None)).lastrowid
        conn.commit()
        if fmt == "markdown":
            pagedoc.init(pid, by)
        else:
            pagedoc.snapshot_html(pid, by, created=True)
    conn.commit()
    return get_page(pid, False)


def update_page(pid: int, title: str | None = None, pinned: bool | None = None, locked: bool | None = None,
                by: str = "user") -> dict:
    """改标题（记一版历史）、置顶、锁定（锁定后 Agent 只能读、评论，用户照样能改）。"""
    from . import pagedoc
    if not get_page(pid, False):
        raise ValueError(i18n.t("没有页面 #{id}", id=pid))
    if title is not None:
        pagedoc.set_title(pid, " ".join(title.split())[:120] or i18n.t("未命名页面"), by)
    if pinned is not None:
        conn.execute("update pages set pinned=? where id=?", (int(pinned), pid))
    if locked is not None:
        conn.execute("update pages set locked=? where id=?", (int(locked), pid))
    conn.commit()
    return get_page(pid, False)


def share(pid: int, on: bool) -> dict:
    """打开分享时生成新链接；关掉后旧链接立即失效。"""
    conn.execute("update pages set share_token=? where id=?", (secrets.token_urlsafe(16) if on else None, pid))
    conn.commit()
    return get_page(pid, False)


def delete_page(pid: int, cascade: bool = False, trash: bool = True):
    """删页面。子页面默认挪到上一级、排在它原来的位置；cascade 就连子页面一起删。数据库总是连行一起删（行不能离开库）。
    trash：先整个（连版本历史、评论，库连同它的行）收进回收站，30 天内能恢复。"""
    from . import pagecomments, pagedoc   # noqa: F401  确保版本历史、评论的表已经建好
    p = get_page(pid, False)
    if not p:
        return
    kids = _children(pid)
    gone = [pid]
    if cascade or p["kind"] == "db":
        gone += sorted(descendants(pid), key=lambda i: (len(_ancestors_ids(i)), i))
    if trash:
        conn.execute("insert into page_trash(page_id, title, kind, format, deleted, data) values(?,?,?,?,?,?)",
                     (pid, p["title"], p["kind"], p["format"], time.time(), json.dumps({"pages": [_dump(i) for i in gone]}, ensure_ascii=False)))
    if not (cascade or p["kind"] == "db") and kids:
        after = _next_after(p["parent_id"], p["pos"])
        step = ((after if after is not None else p["pos"] + 1) - p["pos"]) / (len(kids) + 1)
        for n, (kid, _) in enumerate(kids, 1):
            conn.execute("update pages set parent_id=?, pos=? where id=?", (p["parent_id"], p["pos"] + step * n, kid))
    for i in gone:
        _purge(i)
    conn.commit()


def _purge(pid: int):
    from . import pagecomments
    conn.execute("delete from pages where id=?", (pid,))
    conn.execute("delete from page_versions where page_id=?", (pid,))
    pagecomments.drop_page(pid)


def _ancestors_ids(pid: int) -> list[int]:
    out, cur = [], pid
    while (r := conn.execute("select parent_id from pages where id=?", (cur,)).fetchone()) and r[0] is not None and r[0] not in out:
        out.append(r[0])
        cur = r[0]
    return out


def _rows(q: str, args) -> list[dict]:
    cur = conn.execute(q, args)
    cols = [d[0] for d in cur.description]
    return [dict(zip(cols, r)) for r in cur.fetchall()]


def _dump(pid: int) -> dict:
    """一个页面的全部：页面这一行、版本历史、评论。"""
    return {"page": _rows("select * from pages where id=?", (pid,))[0],
            "versions": _rows("select * from page_versions where page_id=? order by id", (pid,)),
            "comments": _rows("select * from page_comments where page_id=? order by id", (pid,))}


# ---------- 回收站：删掉的页面留 30 天 ----------
TRASH_DAYS = 30
conn.execute("""create table if not exists page_trash(
  id integer primary key, page_id integer, title text, kind text, format text, deleted real, data text)""")
conn.commit()


def trash() -> list[dict]:
    out = []
    for i, pid, title, kind, fmt, deleted, n in conn.execute(
            "select id, page_id, title, kind, format, deleted, json_array_length(data, '$.pages') from page_trash order by deleted desc"):
        out.append({"id": i, "page_id": pid, "title": title, "kind": kind or "page", "format": fmt, "deleted": deleted,
                    "count": n or 1, "expires": deleted + TRASH_DAYS * 86400})
    return out


def _insert(table: str, row: dict, drop_id: bool = False) -> int:
    have = {r[1] for r in conn.execute(f"pragma table_info({table})")}
    row = {k: v for k, v in row.items() if k in have and not (drop_id and k == "id")}
    return conn.execute(f"insert into {table}({', '.join(row)}) values({', '.join('?' * len(row))})", list(row.values())).lastrowid


def restore_trash(tid: int) -> dict:
    """从回收站放回去：原来的上级还在就放回原来的位置，不在了放到最上层。编号被别的页面占了就换一个新的。"""
    from . import pagecomments, pagedoc   # noqa: F401  确保版本历史、评论的表已经建好
    r = conn.execute("select data, kind from page_trash where id=?", (tid,)).fetchone()
    if not r:
        raise LookupError(i18n.t("回收站里没有这一项"))
    pages = json.loads(r[0])["pages"]
    top = pages[0]["page"]
    if top.get("kind") == "row" and not conn.execute("select 1 from pages where id=? and kind='db'", (top.get("parent_id"),)).fetchone():
        raise ValueError(i18n.t("这一行所在的数据库已经不在了，恢复不了"))
    if top.get("kind") != "row" and conn.execute("select count(*) from pages where coalesce(kind, 'page') != 'row'").fetchone()[0] >= MAX_PAGES:
        raise ValueError(i18n.t("最多 {n} 个页面，先删掉一些", n=MAX_PAGES))
    ids: dict[int, int] = {}
    for item in pages:
        pg = dict(item["page"])
        old = pg["id"]
        if conn.execute("select 1 from pages where id=?", (old,)).fetchone():
            pg.pop("id")
        if pg.get("parent_id") in ids:
            pg["parent_id"] = ids[pg["parent_id"]]
        elif item is pages[0] and pg.get("parent_id") is not None and not conn.execute("select 1 from pages where id=?", (pg["parent_id"],)).fetchone():
            pg["parent_id"] = None   # 原来的上级不在了：放到最上层的最后
            pg["pos"] = _end_pos(None)
        if pg.get("share_token") and conn.execute("select 1 from pages where share_token=?", (pg["share_token"],)).fetchone():
            pg["share_token"] = None
        ids[old] = _insert("pages", pg, drop_id="id" not in pg)
        for v in item["versions"]:
            _insert("page_versions", {**v, "page_id": ids[old]}, drop_id=True)
        cids: dict[int, int] = {}
        for c in item["comments"]:
            cids[c["id"]] = _insert("page_comments", {**c, "page_id": ids[old], "parent_id": cids.get(c.get("parent_id"), c.get("parent_id"))},
                                    drop_id=True)
    conn.execute("delete from page_trash where id=?", (tid,))
    conn.commit()
    return get_page(ids[top["id"]], False)


def drop_trash(tid: int | None = None):
    """彻底删掉回收站里的一项（不给就清空）。"""
    if tid is None:
        conn.execute("delete from page_trash")
    else:
        conn.execute("delete from page_trash where id=?", (tid,))
    conn.commit()


def purge_trash():
    """回收站里放了 30 天的清掉。"""
    conn.execute("delete from page_trash where deleted < ?", (time.time() - TRASH_DAYS * 86400,))
    conn.commit()


# ---------- 子页面 ----------
def _children(pid: int | None) -> list[tuple[int, float]]:
    q = "select id, coalesce(pos, 0) from pages where parent_id " + ("is null" if pid is None else "=?") + " order by coalesce(pos, 0), id"
    return list(conn.execute(q, () if pid is None else (pid,)))


def _end_pos(parent_id: int | None) -> float:
    kids = _children(parent_id)
    return (max(p for _, p in kids) + 1) if kids else 1.0


def _next_after(parent_id: int | None, pos: float) -> float | None:
    return next((p for _, p in _children(parent_id) if p > pos), None)


def descendants(pid: int) -> set[int]:
    out, todo = set(), [pid]
    while todo:
        for kid, _ in _children(todo.pop()):
            if kid not in out:
                out.add(kid)
                todo.append(kid)
    return out


def _check_parent(parent_id: int | None, pid: int | None = None, kind: str = "page"):
    """行只能在数据库下面；别的页面只能放在普通的 markdown 页面下面（数据库、行下面不放子页面）。"""
    parent = get_page(parent_id, False) if isinstance(parent_id, int) and not isinstance(parent_id, bool) else None
    if not parent:
        raise ValueError(i18n.t("没有页面 #{id}", id=parent_id))
    if kind == "row" or parent["kind"] != "page":
        if kind == "row" and parent["kind"] == "db":
            return
        raise ValueError(i18n.t("数据库的行只能在数据库里") if kind == "row" else i18n.t("数据库和它的行下面不能放子页面"))
    if parent["format"] != "markdown":
        raise ValueError(i18n.t("html 页面下面不能放子页面"))
    if pid is not None and (parent_id == pid or parent_id in descendants(pid)):
        raise ValueError(i18n.t("不能挪到它自己或者它的子页面下面"))


def move(pid: int, parent_id: int | None, after: int | None = None) -> dict:
    """挪到 parent_id 下面（None 是最上层），排在 after 后面（None 是排在最后，0 是排在最前）。
    数据库的行只能在自己的库里换顺序。"""
    p = get_page(pid, False)
    if not p:
        raise ValueError(i18n.t("没有页面 #{id}", id=pid))
    if p["kind"] == "row" and parent_id != p["parent_id"]:
        raise ValueError(i18n.t("数据库的行不能挪出数据库"))
    if parent_id is not None:
        _check_parent(parent_id, pid, p["kind"])
    kids = [(k, p) for k, p in _children(parent_id) if k != pid]
    if after is None:
        pos = (kids[-1][1] + 1) if kids else 1.0
    elif after == 0:
        pos = (kids[0][1] - 1) if kids else 1.0
    else:
        at = next((n for n, (k, _) in enumerate(kids) if k == after), None)
        if at is None:
            raise ValueError(i18n.t("页面 #{id} 不在那一层", id=after))
        pos = (kids[at][1] + kids[at + 1][1]) / 2 if at + 1 < len(kids) else kids[at][1] + 1
    conn.execute("update pages set parent_id=?, pos=? where id=?", (parent_id, pos, pid))
    conn.commit()
    return get_page(pid, False)


def family(pid: int) -> dict:
    """上级（从最上层往下）和子页面（按顺序），给页面顶上的路径和底下的子页面列表用。"""
    ancestors, seen = [], {pid}
    p = get_page(pid, False)
    while p and p["parent_id"] is not None and p["parent_id"] not in seen:
        seen.add(p["parent_id"])
        p = get_page(p["parent_id"], False)
        if p:
            ancestors.insert(0, {"id": p["id"], "title": p["title"]})
    kids = [get_page(k, False) for k, _ in _children(pid)] if (p0 := get_page(pid, False)) and p0["kind"] == "page" else []   # 库的行不算子页面
    return {"ancestors": ancestors, "children": [{"id": k["id"], "title": k["title"], "format": k["format"], "kind": k["kind"]} for k in kids if k]}


def link(pid: int) -> str:
    """页面在网页上的地址（相对的；给 Agent 的链接前面再拼上站点地址）。"""
    return f"/pages/{pid}"


def tool_read_page(args: dict, limit: int = 12000) -> str:
    from . import pagedoc, search
    p = get_page(args.get("page_id")) if isinstance(args.get("page_id"), int) else None
    if not p:
        return i18n.t("没有页面 #{id}", id=args.get("page_id"))
    if p["format"] == "markdown":
        return pagedoc.render(p["id"], args.get("start"), limit)
    text = search._plain(p["content"] or "", p["format"])
    if p["format"] == "html":
        text = re.sub(r"[ \t]+", " ", re.sub(r"\n\s*\n+", "\n\n", text)).strip()
    more = "\n" + i18n.t("…（还有 {n} 字没显示）", n=len(text) - limit) if len(text) > limit else ""
    return (i18n.t("《{title}》{link}，{when} 更新", title=p["title"], link=link(p["id"]), when=time.strftime('%Y-%m-%d', time.localtime(p['updated'])))
            + f"\n\n{text[:limit]}{more}")
