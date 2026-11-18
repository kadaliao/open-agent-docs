"""Agent：经 /mcp（或 /api/agent/…）读写文档的程序，每个 Agent 一个 token。

- token 只在建的时候显示一次，库里只存 sha256。scope：
  read（只读）、write（能写）、admin（能写，还能经接口管理 Agent、收所有 Agent 的通知：给 dsh Bots 这类“一群 Bot 的宿主”用，
  它替每个 Bot 建一个 Agent）。
- 作者记成 agent:<编号>。撤销了名字也留着（它写过的段、评论还要显示是谁），编号不复用。
- 通知（notify.py）：用户在页面上写评论、@ 到它、或者回复它说过话的那串评论时，给它发一条通知。
  webhook 设了就 POST 过去（带 HMAC 签名）；没设也会存着，宿主可以经 SSE 或轮询来取。
"""
import hashlib
import secrets
import time
from urllib.parse import urlparse
from . import i18n
from .db import conn

conn.execute("""create table if not exists agents(
  id integer primary key, name text, hash text unique, scope text, created real, last_used real, revoked real, hidden real,
  webhook text, secret text, about text default '')""")
conn.commit()

SCOPES = ("read", "write", "admin")
PREFIX = "oad_"
MAX_AGENTS = 200      # 没撤销的
MAX_NAME = 40
MAX_ABOUT = 300
TOUCH_EVERY = 60      # “最后用过”隔这么久才写一次库
_COLS = "id, name, scope, created, last_used, revoked, webhook, about"


def _hash(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()


def _row(r) -> dict:
    i, name, scope, created, last_used, revoked, webhook, about = r
    return {"id": i, "name": name, "scope": scope, "created": created, "last_used": last_used, "revoked": revoked,
            "webhook": webhook or "", "about": about or ""}


def all_agents() -> list[dict]:
    """设置页的列表：没撤销的在前，新的在前；从列表里删掉的不列。"""
    return [_row(r) for r in conn.execute(f"select {_COLS} from agents where hidden is null order by revoked is not null, id desc")]


def get(aid) -> dict | None:
    if not isinstance(aid, int) or isinstance(aid, bool):
        return None
    r = conn.execute(f"select {_COLS} from agents where id=?", (aid,)).fetchone()
    return _row(r) if r else None


def active() -> list[dict]:
    return [_row(r) for r in conn.execute(f"select {_COLS} from agents where revoked is null order by id")]


def _name(name) -> str:
    name = " ".join(str(name or "").split())[:MAX_NAME]
    if not name:
        raise ValueError(i18n.t("给它起个名字，比如 Claude Code"))
    if name.lower() in ("user", "用户", "everyone", "all", "所有人"):
        raise ValueError(i18n.t("这个名字留给别的用途了，换一个"))
    return name


def _webhook(url) -> str:
    url = str(url or "").strip()
    if not url:
        return ""
    u = urlparse(url)
    if u.scheme not in ("http", "https") or not u.netloc or len(url) > 500:
        raise ValueError(i18n.t("webhook 要是 http:// 或 https:// 开头的地址"))
    return url


def create(name: str, scope: str, webhook: str = "", about: str = "") -> tuple[dict, str, str]:
    """新建：返回 (记录, token 原文, webhook 签名用的密钥)。token 原文只有这一次，库里只存 sha256。"""
    name = _name(name)
    if scope not in SCOPES:
        raise ValueError(i18n.t("权限只能是只读、可写或管理"))
    if conn.execute("select count(*) from agents where revoked is null").fetchone()[0] >= MAX_AGENTS:
        raise ValueError(i18n.t("最多 {n} 个 Agent，先撤销一些", n=MAX_AGENTS))
    token = PREFIX + secrets.token_urlsafe(32)
    secret = secrets.token_urlsafe(24)
    aid = conn.execute("insert into agents(name, hash, scope, created, webhook, secret, about) values(?,?,?,?,?,?,?)",
                       (name, _hash(token), scope, time.time(), _webhook(webhook), secret, str(about or "").strip()[:MAX_ABOUT])).lastrowid
    conn.commit()
    return get(aid), token, secret


def update(aid: int, name=None, webhook=None, about=None) -> dict:
    a = get(aid)
    if not a:
        raise LookupError(i18n.t("没有这个 Agent"))
    if name is not None:
        conn.execute("update agents set name=? where id=?", (_name(name), aid))
    if webhook is not None:
        conn.execute("update agents set webhook=? where id=?", (_webhook(webhook), aid))
    if about is not None:
        conn.execute("update agents set about=? where id=?", (str(about).strip()[:MAX_ABOUT], aid))
    conn.commit()
    return get(aid)


def secret(aid: int) -> str:
    r = conn.execute("select secret from agents where id=?", (aid,)).fetchone()
    return (r[0] or "") if r else ""


def rotate_secret(aid: int) -> str:
    if not get(aid):
        raise LookupError(i18n.t("没有这个 Agent"))
    s = secrets.token_urlsafe(24)
    conn.execute("update agents set secret=? where id=?", (s, aid))
    conn.commit()
    return s


def revoke(aid: int) -> dict:
    a = get(aid)
    if not a:
        raise LookupError(i18n.t("没有这个 Agent"))
    if a["revoked"] is None:
        conn.execute("update agents set revoked=? where id=?", (time.time(), aid))
        conn.commit()
    return get(aid)


def hide(aid: int) -> None:
    """把撤销了的从设置的列表里删掉。名字还留着（它写过的段、评论、建的页面要显示是谁）。"""
    a = get(aid)
    if not a:
        raise LookupError(i18n.t("没有这个 Agent"))
    if a["revoked"] is None:
        raise ValueError(i18n.t("先撤销，再从列表里删掉"))
    conn.execute("update agents set hidden=? where id=? and hidden is null", (time.time(), aid))
    conn.commit()


def check(token: str) -> dict | None:
    """token 对得上、没撤销：返回记录，顺手记下最后用过的时间。"""
    if not token:
        return None
    r = conn.execute(f"select {_COLS} from agents where hash=? and revoked is null", (_hash(token),)).fetchone()
    if not r:
        return None
    a, now = _row(r), time.time()
    if not a["last_used"] or now - a["last_used"] > TOUCH_EVERY:
        conn.execute("update agents set last_used=? where id=?", (now, a["id"]))
        conn.commit()
        a["last_used"] = now
    return a


def names() -> list[dict]:
    """给网页显示作者用：撤销了的也在，它写过的段还要显示名字。"""
    return [{"id": r[0], "name": r[1], "revoked": r[2] is not None} for r in conn.execute("select id, name, revoked from agents order by id")]


def name(aid: int) -> str | None:
    r = conn.execute("select name from agents where id=?", (aid,)).fetchone()
    return r[0] if r else None


def id_of(by: str) -> int | None:
    return int(by[6:]) if isinstance(by, str) and by.startswith("agent:") and by[6:].isdigit() else None


def name_of(by: str) -> str:
    """作者名：user（网页上的用户）、agent:<编号>。"""
    if by == "user":
        return i18n.t("用户")
    if (aid := id_of(by)) is not None:
        return name(aid) or i18n.t("已删除的 Agent")
    return i18n.t("不知道是谁")
