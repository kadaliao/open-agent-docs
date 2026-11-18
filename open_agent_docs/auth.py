"""网页登录：一个主人密码（存 scrypt 哈希），登录后发一个会话 cookie（库里只存 sha256）。

- 没设过密码时：OAD_PASSWORD 有就用它，没有就随机生成一个、打印在终端（只打这一次）。之后用 `open-agent-docs password` 改，
  改了以后所有已经登录的会话都失效。
- Agent 不走这里，用 Bearer token（agents.py）。
"""
import hashlib
import hmac
import json
import secrets
import time
from .config import PASSWORD
from .db import conn

conn.execute("create table if not exists sessions(hash text primary key, created real, last_used real)")
conn.commit()

COOKIE = "oad_session"
SESSION_DAYS = 90
TOUCH_EVERY = 3600


def _get(key: str):
    r = conn.execute("select value from settings where key=?", (key,)).fetchone()
    return json.loads(r[0]) if r else None


def _put(key: str, value):
    conn.execute("insert or replace into settings values(?, ?)", (key, json.dumps(value)))
    conn.commit()


def _scrypt(password: str, salt: bytes) -> str:
    return hashlib.scrypt(password.encode(), salt=salt, n=2 ** 14, r=8, p=1).hex()


def set_password(password: str):
    if len(password or "") < 8:
        raise ValueError("密码至少 8 个字符")
    salt = secrets.token_bytes(16)
    _put("password", {"salt": salt.hex(), "hash": _scrypt(password, salt)})
    conn.execute("delete from sessions")
    conn.commit()


def ensure_password() -> str | None:
    """启动时：还没有密码就设一个。返回新生成的随机密码（要打印出来），用的是 OAD_PASSWORD 或者早就有了返回 None。"""
    if _get("password"):
        return None
    if PASSWORD:
        set_password(PASSWORD)
        return None
    pw = secrets.token_urlsafe(12)
    set_password(pw)
    return pw


def check_password(password: str) -> bool:
    rec = _get("password")
    if not rec or not isinstance(password, str):
        return False
    return hmac.compare_digest(_scrypt(password, bytes.fromhex(rec["salt"])), rec["hash"])


def _h(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()


def new_session() -> str:
    token = secrets.token_urlsafe(32)
    now = time.time()
    conn.execute("insert into sessions values(?,?,?)", (_h(token), now, now))
    conn.execute("delete from sessions where last_used < ?", (now - SESSION_DAYS * 86400,))
    conn.commit()
    return token


def check_session(token: str | None) -> bool:
    if not token:
        return False
    r = conn.execute("select last_used from sessions where hash=?", (_h(token),)).fetchone()
    if not r:
        return False
    now = time.time()
    if now - r[0] > SESSION_DAYS * 86400:
        return False
    if now - r[0] > TOUCH_EVERY:
        conn.execute("update sessions set last_used=? where hash=?", (now, _h(token)))
        conn.commit()
    return True


def end_session(token: str | None):
    if token:
        conn.execute("delete from sessions where hash=?", (_h(token),))
        conn.commit()
