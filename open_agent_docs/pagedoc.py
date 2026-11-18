"""资料库页面按段读写：每段的编号和作者、按段修改和冲突检查、整页重写时保住没变的段、版本历史、给 Bot 读的样子。

- 正文只有 pages.content 一份（Markdown）。段从正文切出来（blocks.split）；pages.block_meta 只记每段的编号、谁写的（by）、
  在哪个版本改的（v），按位置和切出来的段一一对应，带着每段的指纹（h）。指纹对不上（旧版本改过正文、恢复了旧备份）
  就按指纹对齐重建：没变的段编号不变，评论还挂在上面。重建不涨版本号、不改更新时间，搜索索引不用重做。
- by：user（用户在网页上改的）、agent:<id>（Agent，经 /mcp，见 agents.py）、
  空串（不知道是谁：旧页面里被外面改过的段）。Agent 的编号不会复用。（bot:<id> 是 OSA 里的 Bot，从 OSA 导进来的页面里会有。）
- 改动按段检查冲突：要改的段在调用方读过之后（v > base）又被别人改过，就整批不写，把那几段现在的内容告诉调用方。
  没碰到的段不管：用户在上面改、Bot 在下面加，互不影响。
- 每次提交版本号 +1，同时记一版历史（page_versions）。同一个人 10 分钟内接着改就并进同一版，免得自动保存刷出几百版。
"""
import json
import time
from . import agents, blocks, i18n, library, media
from .db import conn

conn.execute("""create table if not exists page_versions(
  id integer primary key, page_id integer, version integer, title text, content text, meta text,
  by text, ts_start real, ts real, label text default '')""")
conn.execute("create index if not exists page_versions_page on page_versions(page_id, id)")
if "props" not in {r[1] for r in conn.execute("pragma table_info(page_versions)")}:   # 数据库的行：那一版的属性（dbs.py）
    conn.execute("alter table page_versions add column props text")
conn.commit()

GROUP_GAP = 600      # 同一个人隔这么久以内接着改，算同一版
GROUP_SPAN = 3600    # 一版最长覆盖这么久
MAX_VERSIONS = 50
MAX_VERSION_BYTES = 4 << 20
KEEP_VERSIONS = 5
READ_LIMIT = 12000


class Conflict(Exception):
    """要改的段被别人先改了 / 已经不在了。items：[{block, op, reason, current?, by?, v?}]，reason 是 changed / missing / anchor_missing。"""

    def __init__(self, version: int, items: list[dict]):
        super().__init__("conflict")
        self.version = version
        self.items = items


def name_of(by: str) -> str:
    """作者名（给 Agent 看的、网页上的都用它）。"""
    return agents.name_of(by)


# ---------- 读 ----------
def _meta(pid: int) -> list[dict] | None:
    r = conn.execute("select block_meta from pages where id=?", (pid,)).fetchone()
    return json.loads(r[0]) if r and r[0] else None


def load(pid: int) -> tuple[dict | None, list[dict]]:
    """页面和它的段：[{id, md, gap, by, v, ts}]。html 页面没有段。"""
    p = library.get_page(pid)
    if not p or p["format"] != "markdown":
        return p, []
    parts = blocks.split(p["content"] or "")
    hs = [blocks.digest(md) for md, _ in parts]
    meta = _meta(pid)
    if meta is None or [m.get("h") for m in meta] != hs:
        meta = _rebuild(p, meta, hs)
    return p, [{"id": m["id"], "md": md, "gap": gap, "by": m.get("by", ""), "v": m.get("v", 0), "ts": m.get("ts") or p["updated"]}
               for m, (md, gap) in zip(meta, parts)]


def _rebuild(p: dict, old: list[dict] | None, hs: list[str]) -> list[dict]:
    """按指纹把旧的段编号对到现在的段上。从来没有过编号的旧页面：整页都算建它的那个写的。"""
    who = _maker(p) if old is None else ""
    old = old or []
    taken: set = set()
    out = []
    for (oi, same), h in zip(blocks.align([m.get("h") for m in old], hs), hs):
        m = dict(old[oi]) if oi is not None else None
        if m is None or m.get("id") in taken or not blocks.valid_id(m.get("id")):
            m = {"id": blocks.new_id(taken), "by": who, "v": p["version"], "ts": p["updated"]}
        elif not same:
            m.update(by="", v=p["version"], ts=p["updated"])
        taken.add(m["id"])
        m["h"] = h
        out.append(m)
    conn.execute("update pages set block_meta=? where id=?", (json.dumps(out, ensure_ascii=False), p["id"]))
    conn.commit()
    return out


def _maker(p: dict) -> str:
    """建页面的：Agent、OSA 的 Bot（导进来的页面），都不是就不知道。"""
    return f"agent:{p['agent_id']}" if p.get("agent_id") else f"bot:{p['bot_id']}" if p.get("bot_id") else ""


def last_by(pid: int) -> str:
    """最后一次改这个页面的是谁（没有历史的旧页面：建它的 Bot）。"""
    r = conn.execute("select by from page_versions where page_id=? and label != 'base' order by id desc limit 1", (pid,)).fetchone()
    if r:
        return r[0] or ""
    p = library.get_page(pid, False)
    return _maker(p) if p else ""


def public(b: dict) -> dict:
    return {k: b[k] for k in ("id", "md", "gap", "by", "v", "ts")}


# ---------- 写 ----------
def _commit(p: dict, title: str, work: list[dict], by: str, label: str = "",
            created: bool = False) -> tuple[list[dict], list[list[str]]]:
    """拼成正文 → 再切一遍（拼接可能让相邻的段合并）→ 写正文、段的元数据、版本号 → 记一版历史。
    返回最后的段和合并情况（[[留下的编号, 被并进去的编号…]]）。"""
    newv, now = p["version"] + 1, time.time()
    parts = [(b["md"], b["gap"]) for b in work]
    content = blocks.join(parts)
    if len(content) > library.MAX_PAGE:
        raise ValueError(i18n.t("页面太大（最多 {n}K 字）", n=library.MAX_PAGE // 1000))
    merged: list[list[str]] = []
    if (canon := blocks.split(content)) != parts:
        work, merged = _regroup(work, canon, by, newv, now)
        content = blocks.join(canon)
    meta = [{"id": b["id"], "by": b["by"], "v": b["v"], "ts": b["ts"], "h": blocks.digest(b["md"])} for b in work]
    if not created and not conn.execute("select 1 from page_versions where page_id=? limit 1", (p["id"],)).fetchone():
        _baseline(p)
    cur = conn.execute("update pages set title=?, content=?, block_meta=?, version=?, updated=? where id=? and version=?",
                       (title, content, json.dumps(meta, ensure_ascii=False), newv, now, p["id"], p["version"]))
    if cur.rowcount != 1:
        conn.rollback()
        raise Conflict(p["version"], [])
    _snapshot(p["id"], newv, title, content, meta, by, label, now)
    if merged:
        from . import pagecomments   # 被并掉的段上的评论挪到留下的那段
        pagecomments.reanchor(p["id"], merged)
    conn.commit()
    p.update(title=title, content=content, version=newv, updated=now)
    return work, merged


def commit_props(p: dict, title: str, props: dict, props_meta: dict, by: str, label: str = ""):
    """数据库的行改了属性（和标题）：版本号 +1、记一版历史，正文和段不动。p 是 load 出来的页面，被别人先提交了抛 Conflict。"""
    newv, now = p["version"] + 1, time.time()
    if not conn.execute("select 1 from page_versions where page_id=? limit 1", (p["id"],)).fetchone():
        _baseline(p)
    cur = conn.execute("update pages set title=?, props=?, props_meta=?, version=?, updated=? where id=? and version=?",
                       (title, json.dumps(props, ensure_ascii=False), json.dumps(props_meta, ensure_ascii=False), newv, now, p["id"], p["version"]))
    if cur.rowcount != 1:
        conn.rollback()
        raise Conflict(p["version"], [])
    _snapshot(p["id"], newv, title, p["content"] or "", _meta(p["id"]), by, label, now)
    conn.commit()
    p.update(title=title, version=newv, updated=now)


def _regroup(work: list[dict], canon: list[tuple[str, str]], by: str, newv: int, now: float):
    out, merged = [], []
    gs = blocks.groups([(b["md"], b["gap"]) for b in work], canon)
    if gs is None:   # 不该发生：有段被切开了。按内容对齐，保住能对上的编号
        taken: set = set()
        for (oi, same), (md, gap) in zip(blocks.align([b["md"] for b in work], [md for md, _ in canon]), canon):
            b = work[oi] if oi is not None else None
            if b is None or b["id"] in taken:
                b = {"id": blocks.new_id(taken), "by": by, "v": newv, "ts": now}
            taken.add(b["id"])
            out.append({**b, "md": md, "gap": gap} if same else {**b, "md": md, "gap": gap, "by": by, "v": newv, "ts": now})
        return out, merged
    for g, (md, gap) in zip(gs, canon):
        first = work[g[0]]
        if len(g) == 1 and first["md"] == md:
            out.append({**first, "gap": gap})
            continue
        out.append({**first, "md": md, "gap": gap, "by": by, "v": newv, "ts": now})
        if len(g) > 1:
            merged.append([work[k]["id"] for k in g])
    return out, merged


def _must(pid: int, with_content: bool = True) -> dict:
    p = library.get_page(pid, with_content)
    if not p:
        raise LookupError(i18n.t("没有页面 #{id}", id=pid))
    return p


def init(pid: int, by: str):
    """新建的 markdown 页面：给每段编号，记第一版。"""
    p = _must(pid)
    parts = blocks.split(p["content"] or "")
    now, taken = time.time(), set()
    work = [{"id": blocks.new_id(taken), "md": md, "gap": gap, "by": by, "v": 1, "ts": now} for md, gap in parts]
    _commit(p, p["title"], work, by, created=True)


def _conflict(b: dict, op: str, reason: str) -> dict:
    return {"block": b["id"], "op": op, "reason": reason, "current": b["md"], "by": b["by"], "v": b["v"]}


def apply(pid: int, base, ops, by: str) -> dict:
    """按段修改。ops：
      {op: replace, block, markdown}（markdown 为空就是删掉；带 group: [后面几段的编号] 是连着的几段整组换掉，按内容对齐保住编号）、
      {op: delete, block, merged_into?}、
      {op: insert, after: 编号 | null（null 是最前面）| before: 编号 | end: true, markdown, new_id?}、
      {op: move, block, after: 编号 | null}
    markdown 可以是好几段。全部生效或者都不生效；冲突时抛 Conflict。返回 {version, order, blocks（改动的段）, merged, removed}。"""
    p, blks = load(pid)
    if not p:
        raise LookupError(i18n.t("没有页面 #{id}", id=pid))
    if p["format"] != "markdown":
        raise ValueError(i18n.t("只有 markdown 页面能按段改"))
    if not isinstance(base, int) or isinstance(base, bool) or base < 0 or base > p["version"]:
        raise ValueError(i18n.t("版本号不对：页面现在是版本 {v}", v=p["version"]))
    if not isinstance(ops, list) or not ops:
        raise ValueError(i18n.t("没有要做的修改"))
    newv, now = p["version"] + 1, time.time()
    work = [dict(b) for b in blks]
    prev0 = {b["id"]: (blks[k - 1]["id"] if k else None) for k, b in enumerate(blks)}
    kind0 = {b["id"]: blocks.kind(b["md"]) for b in blks}
    taken = {b["id"] for b in blks}
    touched, keep_gap, removed, conflicts = set(), set(), [], []

    def at(bid) -> int:
        bid = blocks.clean_id(bid)
        return next((k for k, b in enumerate(work) if b["id"] == bid), -1)

    def fresh(md: str, first_id=None) -> list[dict]:
        out = []
        for k, (m, gap) in enumerate(blocks.split(blocks.strip_ids(md or ""))):
            nid = first_id if k == 0 and blocks.valid_id(first_id) and first_id not in taken else blocks.new_id(taken)
            taken.add(nid)
            out.append({"id": nid, "md": m, "gap": gap, "by": by, "v": newv, "ts": now})
            if k:
                keep_gap.add(nid)   # 一次给的好几段之间，按它自己写的松紧
        return out

    for op in ops:
        if not isinstance(op, dict):
            raise ValueError(i18n.t("每个修改都要是一个对象"))
        t = op.get("op")
        if t == "replace" and op.get("group"):   # 网页上的“Markdown 原文”块整组改了：和原来的几段按内容对齐，没变的、改了的都留着编号
            k = at(op.get("block"))
            ids = [blocks.clean_id(x) for x in op["group"]] if isinstance(op["group"], list) else []
            if k < 0 or [b["id"] for b in work[k + 1:k + 1 + len(ids)]] != ids:
                conflicts.append({"block": blocks.clean_id(op.get("block")), "op": t, "reason": "missing"})
                continue
            olds = work[k:k + 1 + len(ids)]
            if stale := [b for b in olds if b["v"] > base and b["id"] not in touched]:
                conflicts += [_conflict(b, t, "changed") for b in stale]
                continue
            parts = blocks.split(blocks.strip_ids(op.get("markdown") or ""))
            new = []
            for (oi, same), (m, gap) in zip(blocks.align([b["md"] for b in olds], [m for m, _ in parts]), parts):
                if oi is not None:
                    new.append({**olds[oi], "gap": gap} if same else {**olds[oi], "md": m, "gap": gap, "by": by, "v": newv, "ts": now})
                else:
                    nid = blocks.new_id(taken)
                    taken.add(nid)
                    new.append({"id": nid, "md": m, "gap": gap, "by": by, "v": newv, "ts": now})
                keep_gap.add(new[-1]["id"])
            kept = {b["id"] for b in new}
            removed += [b["id"] for b in olds if b["id"] not in kept]
            work[k:k + len(olds)] = new
            touched.update(kept | {b["id"] for b in olds})
            continue
        if t in ("replace", "delete"):
            k = at(op.get("block"))
            if k < 0:
                conflicts.append({"block": blocks.clean_id(op.get("block")), "op": t, "reason": "missing"})
                continue
            b = work[k]
            if b["v"] > base and b["id"] not in touched:
                conflicts.append(_conflict(b, t, "changed"))
                continue
            new = fresh(op.get("markdown") or "") if t == "replace" else []
            touched.add(b["id"])
            if not new:
                del work[k]
                removed.append(b["id"])
                continue
            first = new[0]
            new[0] = {**b, "gap": b["gap"]} if first["md"] == b["md"] else {**b, "md": first["md"], "by": by, "v": newv, "ts": now}
            taken.discard(first["id"])
            work[k:k + 1] = new
            touched.update(x["id"] for x in new)
        elif t == "insert":
            if op.get("end"):
                k = len(work)
            elif op.get("before") is not None:
                k = at(op["before"])
                if k < 0:
                    conflicts.append({"block": blocks.clean_id(op["before"]), "op": t, "reason": "anchor_missing"})
                    continue
            elif op.get("after") is not None:
                k = at(op["after"])
                if k < 0:
                    conflicts.append({"block": blocks.clean_id(op["after"]), "op": t, "reason": "anchor_missing"})
                    continue
                k += 1
            else:
                k = 0
            new = fresh(op.get("markdown") or "", op.get("new_id"))
            work[k:k] = new
            touched.update(x["id"] for x in new)
        elif t == "move":
            k = at(op.get("block"))
            if k < 0:
                conflicts.append({"block": blocks.clean_id(op.get("block")), "op": t, "reason": "missing"})
                continue
            b = work.pop(k)
            if op.get("after") is None:
                work.insert(0, b)
            elif (j := at(op["after"])) < 0:
                work.insert(k, b)
                conflicts.append({"block": blocks.clean_id(op["after"]), "op": t, "reason": "anchor_missing"})
                continue
            else:
                work.insert(j + 1, b)
            touched.add(b["id"])
        else:
            raise ValueError(i18n.t("不认识的修改：{op}", op=t))
    if conflicts:
        raise Conflict(p["version"], conflicts)
    retyped = {b["id"] for b in work if b["id"] in kind0 and kind0[b["id"]] != blocks.kind(b["md"])}
    dirty = {k for k, b in enumerate(work) if k and b["id"] not in keep_gap and (
        prev0.get(b["id"], "?") != work[k - 1]["id"] or b["id"] in retyped or work[k - 1]["id"] in retyped
        or b["id"] not in kind0)}
    gaps = blocks.fix_gaps([b["md"] for b in work], [b["gap"] for b in work], dirty)
    for b, g in zip(work, gaps):
        b["gap"] = g
    old = {b["id"]: b for b in blks}
    work, merged = _commit(p, p["title"], work, by)
    changed = [b for b in work if b["id"] not in old or old[b["id"]]["md"] != b["md"] or old[b["id"]]["gap"] != b["gap"]]
    gone = [i for i in old if i not in {b["id"] for b in work}]
    return {"version": p["version"], "order": [b["id"] for b in work], "blocks": [public(b) for b in changed],
            "merged": merged, "removed": gone, "title": p["title"], "updated": p["updated"]}


def rewrite(pid: int, title: str, content: str, by: str, base: int | None = None) -> bool:
    """整页换成新的正文（save_page 更新时）：没变的段保留编号和作者，被改的段按位置保留编号（评论还挂着）。
    给了 base 时，会覆盖掉 base 之后别人改过的段就不写（Conflict）。什么都没变返回 False。"""
    p, old = load(pid)
    if not p:
        raise LookupError(i18n.t("没有页面 #{id}", id=pid))
    parts = blocks.split(blocks.strip_ids(content))
    pairs = blocks.align([b["md"] for b in old], [md for md, _ in parts])
    if base is not None:
        kept = {oi for oi, same in pairs if same}
        if stale := [_conflict(b, "replace", "changed") for k, b in enumerate(old) if b["v"] > base and k not in kept]:
            raise Conflict(p["version"], stale)
    newv, now, taken = p["version"] + 1, time.time(), {b["id"] for b in old}
    work = []
    for (oi, same), (md, gap) in zip(pairs, parts):
        if oi is not None and same:
            work.append({**old[oi], "gap": gap})
        elif oi is not None:
            work.append({**old[oi], "md": md, "gap": gap, "by": by, "v": newv, "ts": now})
        else:
            work.append({"id": blocks.new_id(taken), "md": md, "gap": gap, "by": by, "v": newv, "ts": now})
    if title == p["title"] and [(b["md"], b["gap"]) for b in work] == [(b["md"], b["gap"]) for b in old]:
        return False
    _commit(p, title, work, by)
    return True


def set_title(pid: int, title: str, by: str) -> bool:
    p, blks = load(pid)
    if not p or title == p["title"]:
        return False
    if p["format"] != "markdown":
        return snapshot_html(pid, by, title=title)
    _commit(p, title, blks, by)
    return True


def snapshot_html(pid: int, by: str, label: str = "", title: str | None = None, content: str | None = None,
                  created: bool = False) -> bool:
    """html 页面没有段：写入新的标题 / 正文并整页记一版，好回滚。改之前没有历史的，先把原样记一版。"""
    p = library.get_page(pid)
    if not p:
        return False
    now = time.time()
    title = p["title"] if title is None else title
    content = p["content"] if content is None else content
    if not created and not conn.execute("select 1 from page_versions where page_id=? limit 1", (pid,)).fetchone():
        _baseline(p)
    conn.execute("update pages set title=?, content=?, version=?, updated=? where id=?", (title, content, p["version"] + 1, now, pid))
    _snapshot(pid, p["version"] + 1, title, content, None, by, label, now)
    conn.commit()
    return True


# ---------- 版本历史 ----------
def _baseline(p: dict):
    """旧页面第一次被改之前，先把原样记一版（label=base，不和后面的合并）。"""
    r = conn.execute("select title, content, block_meta, updated, props from pages where id=?", (p["id"],)).fetchone()
    if r:
        conn.execute("insert into page_versions(page_id, version, title, content, meta, by, ts_start, ts, label, props) values(?,?,?,?,?,?,?,?,?,?)",
                     (p["id"], p["version"], r[0], r[1], r[2], _maker(p), r[3], r[3], "base", r[4]))


def _snapshot(pid: int, version: int, title: str, content: str, meta, by: str, label: str, now: float):
    """记一版（数据库的行连同页面现在的属性一起记）。调用前页面已经写好了。"""
    last = conn.execute("select id, by, label, ts_start, ts from page_versions where page_id=? order by id desc limit 1", (pid,)).fetchone()
    mj = json.dumps(meta, ensure_ascii=False) if meta is not None else None
    props = (conn.execute("select props from pages where id=?", (pid,)).fetchone() or (None,))[0]
    if last and not label and not last[2] and last[1] == by and now - last[4] < GROUP_GAP and now - last[3] < GROUP_SPAN:
        conn.execute("update page_versions set version=?, title=?, content=?, meta=?, ts=?, props=? where id=?",
                     (version, title, content, mj, now, props, last[0]))
        return
    conn.execute("insert into page_versions(page_id, version, title, content, meta, by, ts_start, ts, label, props) values(?,?,?,?,?,?,?,?,?,?)",
                 (pid, version, title, content, mj, by, now, now, label, props))
    rows = conn.execute("select id, length(content) from page_versions where page_id=? order by id desc", (pid,)).fetchall()
    total, drop = 0, []
    for k, (vid, size) in enumerate(rows):
        total += size or 0
        if k >= KEEP_VERSIONS and (k >= MAX_VERSIONS or total > MAX_VERSION_BYTES):
            drop.append((vid,))
    conn.executemany("delete from page_versions where id=?", drop)


def versions(pid: int, before: int | None = None, limit: int = 30) -> list[dict]:
    q = "select id, version, title, by, ts_start, ts, label, length(content) from page_versions where page_id=?"
    args: list = [pid]
    if before:
        q += " and id < ?"
        args.append(before)
    rows = conn.execute(q + " order by id desc limit ?", (*args, max(1, min(limit, 100)))).fetchall()
    return [{"id": r[0], "version": r[1], "title": r[2], "by": r[3], "ts_start": r[4], "ts": r[5], "label": r[6], "size": r[7] or 0}
            for r in rows]


def _snap_blocks(content: str, meta, taken: set | None = None) -> list[dict]:
    parts = blocks.split(content or "")
    meta = json.loads(meta) if meta else []
    if [m.get("h") for m in meta] != [blocks.digest(md) for md, _ in parts]:
        meta = []
    taken = (taken or set()) | {m["id"] for m in meta}
    return [{"id": m["id"], "md": md, "gap": gap, "by": m.get("by", ""), "v": m.get("v", 0), "ts": m.get("ts", 0)} if m else
            {"id": blocks.new_id(taken), "md": md, "gap": gap, "by": "", "v": 0, "ts": 0}
            for m, (md, gap) in zip(meta or [None] * len(parts), parts)]


def version(pid: int, vid: int) -> dict | None:
    r = conn.execute("select id, version, title, content, meta, by, ts_start, ts, label, props from page_versions where id=? and page_id=?",
                     (vid, pid)).fetchone()
    if not r:
        return None
    out = {"id": r[0], "version": r[1], "title": r[2], "content": r[3], "by": r[5], "ts_start": r[6], "ts": r[7], "label": r[8]}
    p = library.get_page(pid, False)
    if r[9] is not None:   # 数据库的行：那一版的属性，和所在的库（网页按库现在的表结构画）；stale 是放到现在的表结构下认不出的键
        out["props"] = json.loads(r[9])
        if p and p.get("kind") == "row" and p.get("parent_id"):
            from . import dbs
            out["db_id"] = p["parent_id"]
            out["stale"] = dbs.stale_keys(p["parent_id"], out["props"])
    if r[4] is not None or (p and p["format"] == "markdown"):
        out["blocks"] = [public(b) for b in _snap_blocks(r[3], r[4])]
    return out


def restore(pid: int, vid: int, by: str = "user") -> dict:
    """回到某一版：作为新的一版提交（label=restore:<版本>），当时的段编号回来，评论重新挂上。"""
    r = conn.execute("select version, title, content, meta, props from page_versions where id=? and page_id=?", (vid, pid)).fetchone()
    if not r:
        raise LookupError(i18n.t("没有这一版"))
    if (library.get_page(pid, False) or {}).get("kind") == "db":
        raise ValueError(i18n.t("数据库的表结构改动在库的“历史”里撤销"))
    if r[4] is not None:   # 数据库的行：属性也回到那一版（改了的键记成这次改的）
        from . import dbs
        dbs.restore_props(pid, json.loads(r[4]), by)
    label = f"restore:{r[0]}"
    p, cur = load(pid)
    if not p:
        raise LookupError(i18n.t("没有页面 #{id}", id=pid))
    if p["format"] != "markdown":
        snapshot_html(pid, by, label, r[1], r[2])
        return _must(pid, False)
    now, newv = time.time(), p["version"] + 1
    now_md = {b["id"]: b for b in cur}
    work = []
    for b in _snap_blocks(r[2], r[3], set(now_md)):
        same = now_md.get(b["id"])
        work.append({**same, "gap": b["gap"]} if same and same["md"] == b["md"] else {**b, "v": newv, "ts": now})
    _commit(p, r[1], work, by, label)
    return _must(pid, False)


# ---------- 给 Bot 读 ----------
def render(pid: int, start: str | None = None, limit: int = READ_LIMIT) -> str:
    p, blks = load(pid)
    if not p:
        return f"没有页面 #{pid}"
    from . import dbs
    if p.get("kind") == "db":   # 数据库：表结构和前面的行
        return dbs.render_db(pid, limit)
    maker = f"「{name_of('agent:' + str(p['agent_id']))}」建的" if p.get("agent_id") else "用户建的"
    head = (f"《{p['title'] or '无标题'}》{library.link(pid)} · 版本 {p['version']} · " + maker +
            f" · {time.strftime('%Y-%m-%d', time.localtime(p['updated']))} 更新" + (" · 已锁定（只有用户能改，有意见用 comment_page）" if p.get("locked") else ""))
    fam = library.family(pid)
    if fam["ancestors"] or fam["children"]:
        head += "\n" + "　".join(x for x in (
            "上级：" + " / ".join(f"#{a['id']}《{a['title']}》" for a in fam["ancestors"]) if fam["ancestors"] else "",
            "子页面：" + "、".join(f"#{c['id']}《{c['title']}》" for c in fam["children"][:30]) if fam["children"] else "") if x)
    if p.get("kind") == "row":   # 数据库的一行：属性放在正文前面
        head += "\n" + dbs.row_text(pid)
    from . import pagecomments
    tail = pagecomments.render(pid)
    tail = "\n\n" + tail if tail else ""
    if not blks:
        return head + "\n\n（空页面）" + tail
    k0 = 0
    note = ""
    if start:
        k0 = next((k for k, b in enumerate(blks) if b["id"] == blocks.clean_id(start)), -1)
        if k0 < 0:
            k0, note = 0, f"（没有段落 ⟦{blocks.clean_id(start)}⟧，从头显示）\n"
    legend = "（每段前一行的 ⟦xxxx⟧ 是段落编号，只给 edit_page / comment_page 用，不是正文，写内容时不要带上）"
    out, used = [], len(head) + len(legend) + len(note) + 4
    for k in range(k0, len(blks)):
        b = blks[k]
        piece = f"⟦{b['id']}⟧\n{b['md']}"
        if used + len(piece) > limit:
            if not out:   # 一段就超了：只显示前面一截
                room = max(200, limit - used - 200)
                out.append(f"⟦{b['id']}⟧\n{b['md'][:room]}\n…（这一段太长，只显示了前 {room} 字）")
                k += 1
            rest = sum(len(x["md"]) for x in blks[k:])
            if k < len(blks):
                out.append(f"…还有 {len(blks) - k} 段没显示（约 {rest} 字），接着读：read_page(page_id={pid}, start=\"{blks[k]['id']}\")")
            break
        out.append(piece)
        used += len(piece) + 2
    return f"{head}\n{note}{legend}\n\n" + "\n\n".join(out) + tail


# ---------- 工具：edit_page ----------
_BOT_OPS = {"replace": "replace", "delete": "delete", "insert_after": "after", "insert_before": "before", "append": "end"}


def _bot_ops(raw) -> list[dict]:
    if not isinstance(raw, list):
        raise ValueError("ops 要是一个列表")
    out = []
    for o in raw:
        if not isinstance(o, dict) or o.get("op") not in _BOT_OPS:
            raise ValueError(f"不认识的修改：{o.get('op') if isinstance(o, dict) else o}（只能是 {'、'.join(_BOT_OPS)}）")
        t, md = o["op"], o.get("markdown") or ""
        if t != "append" and not blocks.clean_id(o.get("block")):
            raise ValueError(f"{t} 要给 block（段落编号，read_page 里每段前面的 ⟦xxxx⟧）")
        if t in ("replace", "delete"):
            out.append({"op": t, "block": o.get("block"), "markdown": md})
        elif t == "append":
            out.append({"op": "insert", "end": True, "markdown": md})
        else:
            out.append({"op": "insert", _BOT_OPS[t]: o.get("block") or "", "markdown": md})
        if t != "delete" and not md.strip():
            raise ValueError(f"{t} 要给 markdown（删掉一段用 delete）")
        if md:
            out[-1]["markdown"] = media.embed(md)
    return out


def conflict_text(e: Conflict, head: str = "没改") -> str:
    lines = [f"{head}：页面现在是版本 {e.version}，你要动的段有变化，这一批修改都没写。"]
    for c in e.items:
        if c["reason"] == "changed":
            lines.append(f"⟦{c['block']}⟧ 在你读过之后被「{name_of(c['by'])}」改了，现在是：\n{c['current'][:800]}")
        elif c["reason"] == "missing":
            lines.append(f"⟦{c['block']}⟧ 已经不在了（可能被删了）")
        else:
            lines.append(f"⟦{c['block']}⟧ 不在了，没法在它前后加")
    if not e.items:
        lines.append("页面刚好被别人改了，用 read_page 重新读一下")
    else:
        lines.append(f"按现在的内容决定怎么改，用 version={e.version} 再提交（没列出来的段不用重读）。")
    return "\n".join(lines)


def tool_edit(who: int | str, args: dict) -> tuple[str, dict | None]:
    """edit_page：返回 (给模型的结果, 提交了就是页面改动事件的内容)。who 是作者（agent:<id>）。"""
    by = who if isinstance(who, str) else f"agent:{who}"
    pid = args.get("page_id")
    p = library.get_page(pid, False) if isinstance(pid, int) else None
    if not p:
        return f"没有页面 #{pid}", None
    if p["format"] != "markdown":
        return f"没改：页面 #{pid} 是 html 页面，没有段，只能读", None
    if p["kind"] == "db":
        return f"没改：#{pid} 是数据库，加行用 db_add、改属性用 db_update、改表结构用 db_schema；某一行的正文才用 edit_page", None
    if p.get("locked") or _db_locked(p):
        return f"没改：页面 #{pid}「{p['title']}」被用户锁定了，只能读", None
    pid = p["id"]
    try:
        res = apply(pid, args.get("version"), _bot_ops(args.get("ops")), by)
    except Conflict as e:
        return conflict_text(e), None
    except (ValueError, TypeError) as e:
        return f"没改：{e}", None
    added = [b["id"] for b in res["blocks"] if b["v"] == res["version"]]
    text = (f"已改页面 #{pid}「{p['title']}」，现在是版本 {res['version']}：改动或新加 {len(res['blocks'])} 段"
            + (f"、删掉 {len(res['removed'])} 段" if res["removed"] else "") + "。"
            + (f"涉及的段：{'、'.join(f'⟦{i}⟧' for i in added)}。" if added else "")
            + (f"有 {len(res['merged'])} 处相邻的段拼起来以后合成了一段。" if res["merged"] else ""))
    return text, event(pid, res, by)


def _db_locked(p: dict) -> bool:
    """数据库的行：库锁定了，行也只能读。"""
    if p.get("kind") != "row" or p.get("parent_id") is None:
        return False
    db = library.get_page(p["parent_id"], False)
    return bool(db and db["locked"])


def event(pid: int, res: dict, by: str) -> dict:
    return {"page_id": pid, "version": res["version"], "by": by, "title": res["title"], "updated": res["updated"],
            "changed": [b["id"] for b in res["blocks"]] + res["removed"]}
