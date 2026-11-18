"""页面的评论：用户在某一段上写评论，通知负责的 Agent 来处理（改页面、在评论里回复）。

- 评论挂在段上（block_id，见 pagedoc.py）；不挂段的是对整页的评论。回复挂在第一条下面（parent_id），一串评论一起解决、一起重新打开。
- 只有用户在网页上写的评论会通知 Agent。Agent 写的评论不通知任何 Agent，免得 Agent 之间来回停不下来。
- 叫谁见 targets；怎么送到见 notify.py（webhook、SSE、轮询）。
"""
import json
import re
import time
from . import agents, blocks, events, i18n, library, pagedoc
from .db import conn

conn.execute("""create table if not exists page_comments(
  id integer primary key, page_id integer, block_id text, parent_id integer, by text, text text, quote text,
  ts real, resolved integer default 0, edited real, woke text)""")
conn.execute("create index if not exists page_comments_page on page_comments(page_id, id)")
conn.commit()

MAX_COMMENT = 4000
MAX_WAKE = 3
_COLS = "id, page_id, block_id, parent_id, by, text, quote, ts, resolved, edited, woke"


def _row(r) -> dict:
    i, pid, block, parent, by, text, quote, ts, resolved, edited, woke = r
    return {"id": i, "page_id": pid, "block_id": block, "parent_id": parent, "by": by, "text": text, "quote": quote or "",
            "ts": ts, "resolved": bool(resolved), "edited": edited, "woke": json.loads(woke) if woke else []}


def get(cid: int) -> dict | None:
    r = conn.execute(f"select {_COLS} from page_comments where id=?", (cid,)).fetchone()
    return _row(r) if r else None


def comments(pid: int, open_only: bool = False) -> list[dict]:
    out = [_row(r) for r in conn.execute(f"select {_COLS} from page_comments where page_id=? order by id", (pid,))]
    if open_only:
        live = {c["id"] for c in out if c["parent_id"] is None and not c["resolved"]}
        out = [c for c in out if (c["parent_id"] or c["id"]) in live]
    return out


def add(pid: int, by: str, text: str, block: str | None = None, quote: str | None = None, reply_to: int | None = None) -> dict:
    text = (text or "").strip()
    if not text:
        raise ValueError(i18n.t("评论不能是空的"))
    if len(text) > MAX_COMMENT:
        raise ValueError(i18n.t("评论太长（最多 {n} 字）", n=MAX_COMMENT))
    p, blks = pagedoc.load(pid)
    if not p:
        raise LookupError(i18n.t("没有页面 #{id}", id=pid))
    parent = None
    if reply_to is not None:
        root = get(reply_to) if isinstance(reply_to, int) else None
        if not root or root["page_id"] != pid:
            raise ValueError(i18n.t("没有评论 #{id}", id=reply_to))
        parent = root["parent_id"] or root["id"]
        block = get(parent)["block_id"]   # 回复跟着这串评论挂的段
        conn.execute("update page_comments set resolved=0 where id=?", (parent,))   # 解决过的又有人说话：重新打开
    elif block:
        block = blocks.clean_id(block)
        if block not in {b["id"] for b in blks}:
            raise ValueError(i18n.t("没有段落 ⟦{id}⟧", id=block))
    cid = conn.execute("insert into page_comments(page_id, block_id, parent_id, by, text, quote, ts) values(?,?,?,?,?,?,?)",
                       (pid, block or None, parent, by, text, (quote or "")[:500], time.time())).lastrowid
    conn.commit()
    return get(cid)


def update(pid: int, cid: int, text: str | None = None, resolved: bool | None = None) -> dict:
    c = get(cid)
    if not c or c["page_id"] != pid:
        raise LookupError(i18n.t("没有评论 #{id}", id=cid))
    if text is not None:
        text = text.strip()
        if not text or len(text) > MAX_COMMENT:
            raise ValueError(i18n.t("评论不能是空的") if not text else i18n.t("评论太长（最多 {n} 字）", n=MAX_COMMENT))
        conn.execute("update page_comments set text=?, edited=? where id=?", (text, time.time(), cid))
    if resolved is not None:
        conn.execute("update page_comments set resolved=? where id=?", (int(bool(resolved)), c["parent_id"] or cid))
    conn.commit()
    return get(cid)


def delete(pid: int, cid: int):
    """删一条；删的是一串的第一条就整串删掉。"""
    c = get(cid)
    if not c or c["page_id"] != pid:
        raise LookupError(i18n.t("没有评论 #{id}", id=cid))
    conn.execute("delete from page_comments where id=? or parent_id=?", (cid, cid if c["parent_id"] is None else -1))
    conn.commit()


def reanchor(pid: int, merged: list[list[str]]):
    """几段拼成了一段（见 pagedoc._commit）：挂在被并掉的段上的评论挪到留下的那段。"""
    for keep, *gone in merged:
        if gone:
            conn.execute(f"update page_comments set block_id=? where page_id=? and block_id in ({','.join('?' * len(gone))})",
                         (keep, pid, *gone))


def drop_page(pid: int):
    conn.execute("delete from page_comments where page_id=?", (pid,))


# ---------- 叫醒 Agent ----------
EVERYONE = ("all", "everyone", "所有人", "大家")


def mentions(text: str, alive: list[dict]) -> list[int]:
    """评论里 @ 到的 Agent，按出现顺序。名字可以带空格和中文，长名字优先匹配；@all / @所有人 是全部。"""
    low = text.lower()
    if any(f"@{w}" in low for w in EVERYONE):
        return [a["id"] for a in alive]
    names = sorted(((a["name"].lower(), a["id"]) for a in alive), key=lambda x: -len(x[0]))
    taken, found = [], []
    for name, aid in names:
        for m in re.finditer("@" + re.escape(name), low):
            x0, x1 = m.span()
            if any(x < x1 and x0 < y for x, y in taken):
                continue   # 已经被更长的名字占了
            if x1 < len(low) and low[x1].isascii() and low[x1].isalnum() and name[-1:].isascii():
                continue   # @Nova2 不算 @Nova
            taken.append((x0, x1))
            found.append((x0, aid))
            break
    return [aid for _, aid in sorted(found)]


def targets(page: dict, block: dict | None, root: int | None, text: str, wake) -> list[int]:
    """叫谁：明确指定的（wake）> 评论里 @ 到的（最多 3 个）> 回复的话是这串里最后说话的 Agent > 这一段的作者 > 建页面的 Agent。
    只叫能写的 Agent（只读的回不了评论）。都没有就谁也不叫：评论照样留着，Agent 读页面时会看到。"""
    alive = [a for a in agents.active() if a["scope"] in ("write", "admin")]
    ids = [a["id"] for a in alive]
    if wake is False:
        return []
    if isinstance(wake, int) and not isinstance(wake, bool):
        return [wake] if wake in ids else []
    if named := mentions(text, alive):
        return named[:MAX_WAKE]
    if root:
        said = [agents.id_of(c["by"]) for c in comments(page["id"]) if (c["parent_id"] or c["id"]) == root]
        if last := next((a for a in reversed(said) if a in ids), None):
            return [last]
    if block and (author := agents.id_of(block.get("by", ""))) in ids:
        return [author]
    if page.get("agent_id") in ids:
        return [page["agent_id"]]
    return []


async def post(pid: int, text: str, block: str | None = None, quote: str | None = None, reply_to: int | None = None, wake=None) -> dict:
    """用户写评论：存下来，给该处理的 Agent 发通知（notify.py）。返回这条评论（woke 里是通知了谁）。"""
    from . import notify
    c = add(pid, "user", text, block, quote, reply_to)
    p, blks = pagedoc.load(pid)
    blk = next((b for b in blks if b["id"] == c["block_id"]), None)
    root = c["parent_id"] or c["id"]
    who = targets(p, blk, c["parent_id"], c["text"], wake)
    if who:
        conn.execute("update page_comments set woke=? where id=?", (json.dumps(who), c["id"]))
        conn.commit()
        c["woke"] = who
    quote = (c["quote"] or (blk["md"] if blk else ""))[:300]
    for aid in who:
        await notify.comment(aid, p, c, root, quote)
    await events.emit("page_comments", page_id=pid)
    return c


# ---------- 工具：comment_page ----------
def tool_comment(who: int | str, args: dict, wrote: list | None = None) -> tuple[str, int | None]:
    """comment_page：返回 (给模型的结果, 改了评论的页面编号)。who 是作者（agent:<id>）。
    都走 add，不通知任何 Agent。wrote：给了就把写好的评论放进去。"""
    by = who if isinstance(who, str) else f"agent:{who}"
    pid = args.get("page_id")
    p = library.get_page(pid, False) if isinstance(pid, int) else None
    if not p:
        return i18n.t("没有页面 #{id}", id=pid), None
    text, reply_to = str(args.get("text") or "").strip(), args.get("reply_to")
    if reply_to is not None and not isinstance(reply_to, int):
        reply_to = int(str(reply_to).lstrip("#")) if str(reply_to).lstrip("#").isdigit() else -1
    if not text and not args.get("resolve"):
        return i18n.t("没写：评论内容是空的"), None
    try:
        out = []
        if text:
            c = add(pid, by, text, args.get("block"), None, reply_to)
            if wrote is not None:
                wrote.append(c)
            out.append(i18n.t("已在页面 #{id}「{title}」回复评论 #{parent}（#{cid}）", id=pid, title=p["title"], parent=c["parent_id"], cid=c["id"])
                       if c["parent_id"] else i18n.t("已在页面 #{id}「{title}」写了评论（#{cid}）", id=pid, title=p["title"], cid=c["id"]))
        if args.get("resolve"):
            if reply_to is None:
                return i18n.t("没标：resolve 要和 reply_to 一起用，说明是哪串评论"), None
            update(pid, reply_to, resolved=True)
            out.append(i18n.t("评论 #{id} 那一串已标成已解决", id=reply_to))
    except (ValueError, LookupError) as e:
        return i18n.t("没写：{error}", error=e), None
    return i18n.t("；").join(out), pid


def render(pid: int, limit_threads: int = 20, resolved: bool = False) -> str:
    """read_page 末尾附上的未处理评论（每条截 300 字，最多 limit_threads 串）。resolved：已经解决的也列出来（list_comments）。"""
    open_ = comments(pid, open_only=not resolved)
    roots = [c for c in open_ if c["parent_id"] is None]
    if not roots:
        return ""
    lines = [i18n.t("评论（{n} 串）：", n=len(roots)) if resolved else i18n.t("未处理的评论（{n}）：", n=len(roots))]
    for r in roots[:limit_threads]:
        where = f"⟦{r['block_id']}⟧ " if r["block_id"] else i18n.t("整页") + " "
        lines.append(f"#{r['id']} {where}" + (i18n.t("（已解决）") if r["resolved"] else "")
                     + i18n.t("{name}：{text}", name=pagedoc.name_of(r["by"]), text=r["text"][:300]))
        lines += ["  " + i18n.t("#{id} {name}：{text}", id=c["id"], name=pagedoc.name_of(c["by"]), text=c["text"][:300])
                  for c in open_ if c["parent_id"] == r["id"]]
    if len(roots) > limit_threads:
        lines.append(i18n.t("…还有 {n} 串没显示", n=len(roots) - limit_threads))
    return "\n".join(lines)
