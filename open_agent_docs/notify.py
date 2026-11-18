"""给 Agent 的通知：用户在页面上写了要它处理的评论（pagecomments.post 选出来的）。

一条通知存一行（notifications），三种取法，宿主挑一种用：
- webhook：Agent 设了 webhook 地址就 POST 过去（JSON，带签名），失败按 10 秒、1 分钟、5 分钟重试，2xx 算送到。
  签名：X-OAD-Signature: sha256=<hex>，是用 Agent 的密钥对 "<X-OAD-Timestamp>.<请求体>" 做的 HMAC-SHA256。
- SSE：GET /api/agent/notifications/stream（带 token），先补发游标之后的，再实时推。适合宿主在 NAT 后面（本机的 dsh）、服务器在公网。
- 轮询：GET /api/agent/notifications?after=<游标>&wait=<秒>（长轮询）。
游标就是通知的编号，宿主记下最后处理的那个，重连时带上（SSE 也认 Last-Event-ID），不会重复也不会漏。
admin 的 token 收所有 Agent 的通知（每条带 agent），给替一群 Bot 管连接的宿主用。

通知里有 prompt：一段写给 Agent 的话（哪个页面、哪一段、评论原文、怎么回复），宿主可以直接把它当成一条消息发给 Bot。
处理完的（宿主 ack、webhook 送到）记下 done；网页的 Agent 设置里能看到还有几条没处理。通知留 30 天。
"""
import asyncio
import hashlib
import hmac
import json
import time
from . import agents, i18n, library
from .config import PUBLIC_URL
from .db import conn

conn.execute("""create table if not exists notifications(
  id integer primary key, agent_id integer, kind text, data text, created real, done real, attempts integer default 0, error text)""")
conn.execute("create index if not exists notifications_agent on notifications(agent_id, id)")
conn.commit()

KEEP_DAYS = 30
RETRY = (10, 60, 300)
TIMEOUT = 10
MAX_LIST = 100
_listeners: dict[int | None, set[asyncio.Queue]] = {}   # agent_id → 队列；None 是 admin（收全部）
client = None   # httpx.AsyncClient：测试里换成假的


def _row(r) -> dict:
    i, aid, kind, data, created, done, attempts, error = r
    return {**json.loads(data), "id": i, "agent_id": aid, "type": kind, "created": created, "done": done,
            "attempts": attempts, "error": error or ""}


_COLS = "id, agent_id, kind, data, created, done, attempts, error"


def url(path: str) -> str:
    return (PUBLIC_URL + path) if PUBLIC_URL else path


def _prompt(a: dict, p: dict, c: dict, root: int, quote: str) -> str:
    where = i18n.t("段落 ⟦{block}⟧", block=c["block_id"]) if c["block_id"] else i18n.t("整页")
    lines = [i18n.t("用户在页面 #{id}《{title}》的{where}上给你（{name}）写了评论 #{cid}：", id=p["id"], title=p["title"], where=where,
                    name=a["name"], cid=c["id"]),
             c["text"]]
    if quote:
        lines += ["", i18n.t("评论的那段原文：") + "\n" + quote]
    if c.get("parent_id"):
        lines += ["", i18n.t("这是评论串 #{root} 里的回复。", root=root)]
    lines += ["", i18n.t("页面链接：{url}", url=url(library.link(p["id"]))),
              i18n.t("先用 read_page(page_id={id}) 看页面和这串评论；要改页面用 edit_page；处理完用 comment_page(page_id={id}, reply_to={root}) 回复，"
                     "事情办完了加 resolve=true。", id=p["id"], root=root)]
    return "\n".join(lines)


async def comment(aid: int, p: dict, c: dict, root: int, quote: str) -> dict | None:
    """用户写的评论要 aid 处理：存一条通知、推给连着的宿主、有 webhook 就发过去。"""
    a = agents.get(aid)
    if not a or a["revoked"]:
        return None
    data = {"agent": {"id": a["id"], "name": a["name"]},
            "page": {"id": p["id"], "title": p["title"], "kind": p.get("kind") or "page", "version": p["version"],
                     "url": url(library.link(p["id"]))},
            "comment": {"id": c["id"], "root": root, "parent_id": c.get("parent_id"), "block": c["block_id"], "text": c["text"],
                        "quote": quote, "by": c["by"], "by_name": agents.name_of(c["by"]), "ts": c["ts"]},
            "prompt": _prompt(a, p, c, root, quote)}
    nid = conn.execute("insert into notifications(agent_id, kind, data, created) values(?,?,?,?)",
                       (aid, "comment", json.dumps(data, ensure_ascii=False), time.time())).lastrowid
    conn.commit()
    n = get(nid)
    assert n
    for q in list(_listeners.get(aid, ())) + list(_listeners.get(None, ())):
        q.put_nowait(n)
    if a["webhook"]:
        _spawn(_deliver(nid))
    return n


_tasks: set[asyncio.Task] = set()


def _spawn(coro):
    t = asyncio.get_running_loop().create_task(coro)
    _tasks.add(t)
    t.add_done_callback(_tasks.discard)


def get(nid: int) -> dict | None:
    r = conn.execute(f"select {_COLS} from notifications where id=?", (nid,)).fetchone()
    return _row(r) if r else None


def listing(aid: int | None, after: int = 0, pending: bool = False, limit: int = MAX_LIST) -> list[dict]:
    """aid 的通知（None：所有 Agent 的），编号在 after 之后的、按编号从小到大。pending：只要还没处理的。"""
    q = f"select {_COLS} from notifications where id > ?"
    args: list = [after]
    if aid is not None:
        q += " and agent_id=?"
        args.append(aid)
    if pending:
        q += " and done is null"
    return [_row(r) for r in conn.execute(q + " order by id limit ?", (*args, max(1, min(limit, MAX_LIST))))]


def ack(aid: int | None, ids: list[int] | None = None, upto: int | None = None) -> int:
    """标成处理完了：给 ids，或者 upto（这个编号和之前的全部）。aid 是 None（admin）时不限 Agent。返回改了几条。"""
    now = time.time()
    where, args = "done is null", []
    if aid is not None:
        where += " and agent_id=?"
        args.append(aid)
    if ids:
        ids = [i for i in ids if isinstance(i, int) and not isinstance(i, bool)][:500]
        n = conn.execute(f"update notifications set done=? where {where} and id in ({','.join('?' * len(ids))})", (now, *args, *ids)).rowcount
    elif isinstance(upto, int) and not isinstance(upto, bool):
        n = conn.execute(f"update notifications set done=? where {where} and id <= ?", (now, *args, upto)).rowcount
    else:
        return 0
    conn.commit()
    return n


def pending_counts() -> dict[int, int]:
    return {aid: n for aid, n in conn.execute("select agent_id, count(*) from notifications where done is null group by agent_id")}


def listen(aid: int | None) -> asyncio.Queue:
    q: asyncio.Queue = asyncio.Queue(1000)
    _listeners.setdefault(aid, set()).add(q)
    return q


def unlisten(aid: int | None, q: asyncio.Queue):
    _listeners.get(aid, set()).discard(q)


# ---------- webhook ----------
def sign(secret: str, ts: str, body: bytes) -> str:
    return "sha256=" + hmac.new(secret.encode(), ts.encode() + b"." + body, hashlib.sha256).hexdigest()


async def _post(url_: str, body: bytes, headers: dict) -> int:
    global client
    if client is None:
        import httpx
        client = httpx.AsyncClient(timeout=TIMEOUT, follow_redirects=False)
    r = await client.post(url_, content=body, headers=headers)
    return r.status_code


async def _deliver(nid: int):
    for k, delay in enumerate((0, *RETRY)):
        if delay:
            await asyncio.sleep(delay)
        n = get(nid)
        a = agents.get(n["agent_id"]) if n else None
        if not n or not a or a["revoked"] or not a["webhook"] or n["done"]:
            return
        body = json.dumps({k2: v for k2, v in n.items() if k2 not in ("attempts", "error", "done")}, ensure_ascii=False).encode()
        ts = str(int(time.time()))
        headers = {"content-type": "application/json; charset=utf-8", "user-agent": "open-agent-docs",
                   "x-oad-event": n["type"], "x-oad-delivery": str(nid), "x-oad-timestamp": ts,
                   "x-oad-signature": sign(agents.secret(a["id"]), ts, body)}
        try:
            status = await _post(a["webhook"], body, headers)
            err = "" if 200 <= status < 300 else f"HTTP {status}"
        except Exception as e:   # 连不上、超时
            err = f"{type(e).__name__}: {e}"[:300]
        conn.execute("update notifications set attempts=attempts+1, error=?, done=coalesce(done, ?) where id=?",
                     (err, None if err else time.time(), nid))
        conn.commit()
        if not err:
            return


def purge():
    conn.execute("delete from notifications where created < ?", (time.time() - KEEP_DAYS * 86400,))
    conn.commit()
