"""资料库的数据库（多维表，参考 Notion）：一个库是一个页面（pages.kind = db），每一行是它下面的子页面（kind = row）。

- 表结构 pages.schema：[{key, name, type, options?, format?}]，第一项是名称（key = title，就是行页面的标题，不能删）。
  选项 {id, name, color, group?}（状态的选项多一个 group：todo / doing / done）。值里存选项 id，改名不用动每一行。
- 视图 pages.views：[{id, name, type: table | board, sort, filter, group_by?, hidden, widths}]。
- 行的属性 pages.props：{key: 值}，空的不存。props_meta：{key: {v, by, ts}}，每个键最后在哪个版本、被谁改的：
  给了版本号时只有要改的键在那之后被别人改过才冲突（和段的 block_meta 一个思路）。名称的改动也记在 props_meta.title 里。
- 行的正文还是 Markdown，按段编辑、评论、记历史都照旧（pagedoc.py）；属性改动也记一版历史（page_versions.props）。
- 值有两种写法：网页发的是存的样子（选项 id、agent:3、{start, end}）；工具（Agent）写名字（“进行中”、“Nova”、“2026-11-15”）。
  normalize 两种都认。工具读到的一律是名字。
- 改表结构不算冲突，按属性后写的为准；改类型时能转的值转过去（先变成文字再按新类型认），转不了的清空。
  每次改表结构记一条历史（谁、改了什么、清掉或转换了哪些格子的旧值），能撤销：删掉的列和值都放得回来。
- 筛选、排序在这里做，工具和网页用同一套。
- 算出来的属性（dbcalc.py）：公式、汇总、双向关联在对方库里的那一半（mirror_of）不存，读行的时候算好放在 row["calc"]。
  关联存对方库的行号列表；自动编号存数，加行时发号（表结构里记着下一个号 next）；按钮不存值，点了按配置改属性、写评论通知 Agent。
"""
import json
import re
import secrets
import time
from datetime import date, timedelta
from . import agents, dbcalc, i18n, library, media, pagedoc
from .db import conn

TYPES = {"title": "名称", "text": "文本", "number": "数字", "select": "单选", "multi_select": "多选", "status": "状态",
         "date": "日期", "person": "人员", "checkbox": "勾选", "url": "网址", "email": "邮箱", "phone": "电话",
         "created_time": "创建时间", "updated_time": "最后编辑时间", "created_by": "创建人", "updated_by": "最后编辑人",
         "relation": "关联", "rollup": "汇总", "formula": "公式", "files": "文件", "unique_id": "自动编号", "button": "按钮"}
COMPUTED = {"created_time", "updated_time", "created_by", "updated_by"}
CALCULATED = {"formula", "rollup"}            # 读的时候算，不存
READONLY = COMPUTED | CALCULATED | {"unique_id", "button"}   # 不能直接写值
MAX_LINKS = 100      # 一格最多关联几行
MAX_FILES = 20
OPTIONED = {"select", "multi_select", "status"}
TEXTUAL = {"title", "text", "url", "email", "phone"}
COLORS = ("default", "gray", "brown", "orange", "yellow", "green", "blue", "purple", "pink", "red")
GROUPS = ("todo", "doing", "done")
GROUP_NAMES = {"todo": "未开始", "doing": "进行中", "done": "已完成"}
FORMATS = ("number", "comma", "percent", "cny", "usd")
OPS = ("eq", "neq", "contains", "not_contains", "gt", "gte", "lt", "lte", "empty", "not_empty")
MAX_ROWS = 10000
MAX_PROPS = 50
MAX_OPTIONS = 100
MAX_VIEWS = 20
MAX_TEXT = 2000
NAME_LEN = 60
_BY_ZH = {v: k for k, v in TYPES.items()}


class Conflict(Exception):
    """要改的属性在调用方读过之后被别人改了。items：[{prop, name, current, by}]（current 是给人看的值）。"""

    def __init__(self, version: int, items: list[dict]):
        super().__init__("conflict")
        self.version = version
        self.items = items


# ---------- 读库 ----------
def _new_id(prefix: str, taken) -> str:
    while True:
        k = prefix + secrets.token_hex(2)
        if k not in taken:
            return k


def get_db(db_id) -> dict | None:
    """库页面加上 schema、views。不是库返回 None。"""
    p = library.get_page(db_id, False) if isinstance(db_id, int) and not isinstance(db_id, bool) else None
    if not p or p["kind"] != "db":
        return None
    r = conn.execute("select schema, views from pages where id=?", (db_id,)).fetchone()
    return {**p, "schema": json.loads(r[0] or "[]"), "views": json.loads(r[1] or "[]")}


def _row_of(r, last_by: dict) -> dict:
    i, title, version, pos, created, updated, bot_id, agent_id, props, meta, locked = r
    made = f"bot:{bot_id}" if bot_id else f"agent:{agent_id}" if agent_id else "user"
    return {"id": i, "title": title, "version": version or 0, "pos": pos or 0, "created": created, "updated": updated,
            "created_by": made, "updated_by": last_by.get(i, made), "props": json.loads(props or "{}"),
            "meta": json.loads(meta or "{}"), "locked": bool(locked)}


_ROW_COLS = "id, title, version, pos, created, updated, bot_id, agent_id, props, props_meta, locked"


def _last_by(ids: list[int]) -> dict:
    """每一行最后是谁改的（一次查完，不一行一查）。"""
    out = {}
    for k in range(0, len(ids), 500):
        part = ids[k:k + 500]
        q = (f"select page_id, by from page_versions where id in (select max(id) from page_versions where label != 'base' "
             f"and page_id in ({','.join('?' * len(part))}) group by page_id)")
        out |= {pid: by for pid, by in conn.execute(q, part)}
    return out


def rows(db_id: int, db: dict | None = None, depth: int = 0) -> list[dict]:
    """库里的所有行，按手动排的顺序；算出来的属性（公式、汇总、反向关联）在 row["calc"]。"""
    rs = conn.execute(f"select {_ROW_COLS} from pages where parent_id=? and kind='row' order by coalesce(pos, 0), id", (db_id,)).fetchall()
    last = _last_by([r[0] for r in rs])
    items = [_row_of(r, last) for r in rs]
    db = db or get_db(db_id)
    return _calc(db, items, depth) if db else items


def get_row(row_id) -> dict | None:
    if not isinstance(row_id, int) or isinstance(row_id, bool):
        return None
    r = conn.execute(f"select {_ROW_COLS}, parent_id from pages where id=? and kind='row'", (row_id,)).fetchone()
    if not r:
        return None
    row = {**_row_of(r[:-1], _last_by([r[0]])), "db_id": r[-1]}
    db = get_db(r[-1])
    return _calc(db, [row])[0] if db else row


def prop_of(db: dict, ref) -> dict | None:
    """按 key 或名字找属性（名字不分大小写）。"""
    ref = str(ref or "").strip()
    for p in db["schema"]:
        if p["key"] == ref:
            return p
    low = ref.lower()
    return next((p for p in db["schema"] if p["name"].lower() == low), None)


def is_mirror(prop: dict) -> bool:
    """双向关联在对方库里的那一半：值不存，从另一个库的关联反查。"""
    return prop["type"] == "relation" and bool(prop.get("mirror_of"))


def value_of(row: dict, prop: dict):
    """行在这一列的值（存的样子）。名称、自动算的从页面取，公式、汇总、反向关联从 calc 取。"""
    t = prop["type"]
    if t in CALCULATED or is_mirror(prop):
        return (row.get("calc") or {}).get(prop["key"])
    if t == "button":
        return None
    if t == "title":
        return row["title"]
    if t in ("created_time", "updated_time"):
        return row["created" if t == "created_time" else "updated"]
    if t in ("created_by", "updated_by"):
        return row[t]
    return row["props"].get(prop["key"])


# ---------- 算出来的属性：公式、汇总、反向关联（dbcalc.py） ----------
def titles(ids) -> dict[int, str]:
    """行号 → 标题（只给还在的行）。"""
    ids = [i for i in dict.fromkeys(ids) if isinstance(i, int) and not isinstance(i, bool)]
    out = {}
    for k in range(0, len(ids), 500):
        part = ids[k:k + 500]
        out |= {r[0]: r[1] or "" for r in conn.execute(
            f"select id, title from pages where kind='row' and id in ({','.join('?' * len(part))})", part)}
    return out


def _backlinks(src_db: int, key: str) -> dict[int, list[int]]:
    """另一个库（src_db）里关联属性 key 指向的行 → 哪几行指着它（按那边的顺序）。"""
    out: dict[int, list[int]] = {}
    for rid, props in conn.execute("select id, props from pages where parent_id=? and kind='row' order by coalesce(pos, 0), id", (src_db,)):
        for t in json.loads(props or "{}").get(key) or []:
            out.setdefault(t, []).append(rid)
    return out


class _Calc:
    """一次读行时算公式、汇总、反向关联：对方库的行、反查表、标题都只取一次。"""

    def __init__(self, db: dict, items: list[dict], depth: int):
        self.db, self.items, self.depth = db, items, depth
        self.by_name = {p["name"].lower(): p for p in db["schema"]}
        self.parsed: dict[str, object] = {}
        self.back: dict[str, dict] = {}
        self.other: dict[int, tuple[dict | None, dict[int, dict], "_Calc | None"]] = {}
        self.names: dict[int, str] = {}

    def links(self, r: dict, p: dict) -> list[int]:
        """一个关联属性（存的或反向的）在这一行指着哪些行。"""
        if is_mirror(p):
            if p["key"] not in self.back:
                self.back[p["key"]] = _backlinks(p["db"], p["mirror_of"])
            return list(self.back[p["key"]].get(r["id"], []))
        return [x for x in r["props"].get(p["key"]) or [] if isinstance(x, int)]

    def rows_of(self, db_id: int) -> tuple[dict | None, dict[int, dict], "_Calc | None"]:
        """关联到的库：它的表结构、所有行（算好了的）、在那些行上接着算的上下文。"""
        if db_id not in self.other:
            if db_id == self.db["id"] and len(self.items) == conn.execute(
                    "select count(*) from pages where parent_id=? and kind='row'", (db_id,)).fetchone()[0]:
                self.other[db_id] = (self.db, {r["id"]: r for r in self.items}, self)
            elif self.depth >= dbcalc.MAX_CHAIN:
                self.other[db_id] = (None, {}, None)
            else:
                odb = get_db(db_id)
                items = rows(db_id, odb, self.depth + 1) if odb else []
                self.other[db_id] = (odb, {r["id"]: r for r in items}, _Calc(odb, items, self.depth + 1) if odb else None)
        return self.other[db_id]

    def title_of(self, ids: list[int]) -> list[str]:
        missing = [i for i in ids if i not in self.names]
        if missing:
            self.names |= titles(missing)
        return [self.names[i] for i in ids if i in self.names]

    def value(self, r: dict, p: dict, stack: tuple = ()):
        calc = r.setdefault("calc", {})
        if p["key"] in calc:
            return calc[p["key"]]
        if p["key"] in stack:
            raise dbcalc.FormulaError("公式互相引用成了环")
        stack = stack + (p["key"],)
        v = None
        if is_mirror(p):
            v = self.links(r, p) or None
        elif p["type"] == "formula":
            src = p.get("formula") or ""
            if src:
                if p["key"] not in self.parsed:
                    try:
                        self.parsed[p["key"]] = dbcalc.parse(src)
                    except dbcalc.FormulaError:
                        self.parsed[p["key"]] = None
                node = self.parsed[p["key"]]

                def get(name: str, r=r, stack=stack):
                    q = self.by_name.get(str(name).strip().lower())
                    if not q:
                        raise dbcalc.FormulaError(f"没有属性「{name}」")
                    return self.formula_value(r, q, stack)
                v = dbcalc.run(node, get) if node else None
        elif p["type"] == "rollup":
            v = self.rollup(r, p, stack)
        calc[p["key"]] = v
        return v

    def rollup(self, r: dict, p: dict, stack: tuple):
        rel = next((q for q in self.db["schema"] if q["key"] == p.get("relation") and q["type"] == "relation"), None)
        if not rel or not p.get("target"):
            return None
        ids = self.links(r, rel)
        odb, others, sub = self.rows_of(rel["db"])
        tgt = next((q for q in (odb or {}).get("schema") or [] if q["key"] == p["target"]), None)
        if not tgt or not sub:
            return None
        got = [others[i] for i in ids if i in others]
        vals = [sub.formula_value(x, tgt, stack if sub is self else ()) for x in got]
        return dbcalc.rollup(p.get("fn") or "count", vals, len(got))

    def formula_value(self, r: dict, p: dict, stack: tuple = ()):
        """一个属性在公式、汇总里的样子：数、文字、true/false、datetime、文字的列表。"""
        t = p["type"]
        if t in CALCULATED or is_mirror(p):
            v = self.value(r, p, stack)
            if is_mirror(p):
                return self.title_of(v or [])
            if isinstance(v, dict):
                return dbcalc.to_dt(v)
            return v
        v = value_of(r, p)
        if t == "relation":
            return self.title_of([x for x in v or [] if isinstance(x, int)])
        if t in ("date", "created_time", "updated_time"):
            return dbcalc.to_dt(v)
        if t == "checkbox":
            return bool(v)
        if t == "number" or t == "unique_id":
            return v
        if t in ("multi_select", "person"):
            return [display(p, [x]) for x in v or []]
        if t == "files":
            return [f.get("name", "") for f in v or []]
        if v is None:
            return ""
        return display(p, v)


def _calc(db: dict, items: list[dict], depth: int = 0) -> list[dict]:
    props = [p for p in db["schema"] if p["type"] in CALCULATED or is_mirror(p)]
    for r in items:
        r.setdefault("calc", {})
    if props and items:
        ctx = _Calc(db, items, depth)
        for r in items:
            for p in props:
                try:
                    ctx.value(r, p)
                except dbcalc.FormulaError:
                    r["calc"][p["key"]] = None
    return items


def related_dbs(db_id: int) -> set[int]:
    """和这个库有关联的库（指向它的、它指向的）：这边的行变了，那边的汇总、反向关联也要重新取。"""
    out = set()
    for i, sch in conn.execute("select id, schema from pages where kind='db' and schema like '%\"relation\"%'"):
        for p in json.loads(sch or "[]"):
            if p.get("type") == "relation":
                if i == db_id and p.get("db") != db_id:
                    out.add(p.get("db"))
                elif p.get("db") == db_id and i != db_id:
                    out.add(i)
    return {x for x in out if isinstance(x, int)}


# ---------- 值：认、显示 ----------
def person_name(by: str) -> str:
    return pagedoc.name_of(by)


def _people() -> dict[str, str]:
    """名字（小写）→ 作者：用户、Agent。"""
    out = {"user": "user", "用户": "user", i18n.t("用户").lower(): "user"}
    for a in agents.names():
        out.setdefault(a["name"].lower(), f"agent:{a['id']}")
    return out


def _day(s: str) -> str | None:
    """日期：2026-11-15、2026/11/15、2026-11-15 14:00、2026-11-15T14:00，today / 今天 / 明天 / 昨天。"""
    s = str(s or "").strip()
    rel = {"today": 0, "今天": 0, "tomorrow": 1, "明天": 1, "yesterday": -1, "昨天": -1}
    if s.lower() in rel:
        return (date.today() + timedelta(days=rel[s.lower()])).isoformat()
    m = re.fullmatch(r"(\d{4})[-/.](\d{1,2})[-/.](\d{1,2})(?:[ T](\d{1,2}):(\d{2})(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?)?", s)
    if not m:
        return None
    y, mo, d, hh, mm = m.groups()
    try:
        day = date(int(y), int(mo), int(d)).isoformat()
    except ValueError:
        return None
    if hh is not None:
        if int(hh) > 23 or int(mm) > 59:
            return None
        return f"{day} {int(hh):02d}:{mm}"
    return day


def _truthy(v) -> bool | None:
    if isinstance(v, bool):
        return v
    s = str(v).strip().lower()
    if s in ("1", "true", "yes", "y", "是", "✓", "✔", "x", "[x]", "done", "对", "checked"):
        return True
    if s in ("0", "false", "no", "n", "否", "", "[ ]", "不是", "unchecked"):
        return False
    return None


def _option(prop: dict, v, create: bool) -> str:
    """选项：给的是 id 或名字。create：单选、多选里没有的名字自动加一个（状态不加）。"""
    opts = prop.setdefault("options", [])
    s = str(v).strip()
    for o in opts:
        if o["id"] == s:
            return o["id"]
    for o in opts:
        if o["name"].lower() == s.lower():
            return o["id"]
    if not create or prop["type"] == "status":
        names = "、".join(o["name"] for o in opts) or "（还没有选项）"
        raise ValueError(f"「{prop['name']}」没有选项「{s}」，可选：{names}")
    if not s or len(s) > NAME_LEN:
        raise ValueError(f"「{prop['name']}」的选项名不能是空的、也不能超过 {NAME_LEN} 字")
    if len(opts) >= MAX_OPTIONS:
        raise ValueError(f"「{prop['name']}」最多 {MAX_OPTIONS} 个选项")
    o = {"id": _new_id("o", {x["id"] for x in opts}), "name": s, "color": COLORS[1 + len(opts) % (len(COLORS) - 1)]}
    opts.append(o)
    return o["id"]


def normalize(prop: dict, v, create: bool = True):
    """把给的值变成存的样子；None / 空串 / 空列表是清空（返回 None）。认不出抛 ValueError（说清楚该怎么写）。
    create 时单选、多选会自动加没有的选项（改的是 prop 本身，调用方负责把表结构存回去）。"""
    t, name = prop["type"], prop["name"]
    if v is None or v == "" or v == []:
        return None
    if t in COMPUTED or t in CALCULATED or t == "unique_id":
        raise ValueError(f"「{name}」是自动算的，不能改")
    if t == "button":
        raise ValueError(f"「{name}」是按钮，没有值")
    if t == "relation":
        return _links(prop, v)
    if t == "files":
        return _files(prop, v)
    if t in ("title", "text", "url", "email", "phone"):
        if isinstance(v, (dict, list)):
            raise ValueError(f"「{name}」要写一段文字")
        s = str(v).strip()
        if t != "text":
            s = " ".join(s.split())
        if len(s) > MAX_TEXT:
            raise ValueError(f"「{name}」太长（最多 {MAX_TEXT} 字）")
        if t == "email" and s and not re.fullmatch(r"[^@\s]+@[^@\s]+\.[^@\s]+", s):
            raise ValueError(f"「{name}」不像邮箱：{s}")
        if t == "url" and s and not re.match(r"[a-z][a-z0-9+.-]*://|mailto:|/", s, re.I):
            s = "https://" + s
        return s or None
    if t == "number":
        if isinstance(v, bool):
            raise ValueError(f"「{name}」要写数字")
        if isinstance(v, (int, float)):
            n = v
        else:
            s = re.sub(r"[,，\s¥$￥%]", "", str(v))
            try:
                n = float(s)
            except ValueError:
                raise ValueError(f"「{name}」要写数字，不是「{v}」") from None
        if n != n or n in (float("inf"), float("-inf")):
            raise ValueError(f"「{name}」要写数字")
        return int(n) if isinstance(n, float) and n.is_integer() and abs(n) < 2 ** 53 else n
    if t in ("select", "status"):
        if isinstance(v, list):
            if len(v) != 1:
                raise ValueError(f"「{name}」只能选一个")
            v = v[0]
        return _option(prop, v, create)
    if t == "multi_select":
        items = v if isinstance(v, list) else [x for x in re.split(r"[,，、;；]", str(v))]
        out = []
        for x in items:
            if str(x).strip():
                oid = _option(prop, x, create)
                if oid not in out:
                    out.append(oid)
        return out or None
    if t == "date":
        if isinstance(v, dict):
            start, end = v.get("start"), v.get("end")
        else:
            parts = re.split(r"\s*(?:→|~|～|到|至| - | to )\s*", str(v).strip(), maxsplit=1)
            start, end = parts[0], parts[1] if len(parts) > 1 else None
        a = _day(start)
        if not a:
            raise ValueError(f"「{name}」的日期写成 2026-11-15 或 2026-11-15 14:00，不是「{start}」")
        out = {"start": a}
        if end not in (None, ""):
            b = _day(end)
            if not b:
                raise ValueError(f"「{name}」的结束日期写成 2026-11-15 或 2026-11-15 14:00，不是「{end}」")
            if b < a:
                raise ValueError(f"「{name}」的结束日期在开始日期前面")
            out["end"] = b
        return out
    if t == "person":
        items = v if isinstance(v, list) else [x for x in re.split(r"[,，、;；]", str(v))]
        people, out = _people(), []
        for x in items:
            s = str(x).strip()
            if not s:
                continue
            if re.fullmatch(r"(bot|agent):\d+", s) or s == "user":
                who = s
            else:
                who = people.get(s.lower())
                if not who:
                    raise ValueError(f"「{name}」没有叫「{s}」的人：写“用户”或者 Agent 的名字")
            if who not in out:
                out.append(who)
        return out or None
    if t == "checkbox":
        b = _truthy(v)
        if b is None:
            raise ValueError(f"「{name}」要写 true 或 false")
        return b or None   # 没勾就是空的
    raise ValueError(f"不认识的类型：{t}")


def _links(prop: dict, v) -> list[int] | None:
    """关联：行号（12、"#12"）或者对方库里行的名称，列表或逗号分开。只认对方库里的行。"""
    target, name = prop.get("db"), prop["name"]
    items = v if isinstance(v, list) else [x for x in re.split(r"[,，、;；]", str(v))]
    named, out = None, []
    for x in items:
        if isinstance(x, bool) or isinstance(x, (dict, list, float)):
            raise ValueError(f"「{name}」写行号或者那一行的名称")
        if isinstance(x, int):
            rid = x
        else:
            s = str(x).strip()
            if not s:
                continue
            if re.fullmatch(r"#?\d+", s):
                rid = int(s.lstrip("#"))
            else:
                if named is None:
                    named = {}
                    for i, title in conn.execute("select id, title from pages where parent_id=? and kind='row' order by coalesce(pos, 0), id", (target,)):
                        named.setdefault((title or "").strip().lower(), i)
                rid = named.get(s.lower())
                if rid is None:
                    raise ValueError(f"「{name}」关联的库里没有叫「{s}」的行")
        if rid not in out:
            out.append(rid)
    if len(out) > MAX_LINKS:
        raise ValueError(f"「{name}」最多关联 {MAX_LINKS} 行")
    if out:
        have = {r[0] for r in conn.execute(f"select id from pages where parent_id=? and kind='row' and id in ({','.join('?' * len(out))})",
                                           (target, *out))}
        if bad := [x for x in out if x not in have]:
            raise ValueError(f"「{name}」关联的库里没有行 #{bad[0]}")
    return out or None


def _files(prop: dict, v) -> list[dict] | None:
    """文件：网址，或者传上来的文件（/api/media/编号、编号），可以写成 {name, url}。"""
    name = prop["name"]
    out = []
    for x in v if isinstance(v, list) else [v]:
        url, nm = (x.get("url"), x.get("name")) if isinstance(x, dict) else (x, None)
        url = str(url or "").strip()
        if not url:
            continue
        if re.fullmatch(r"[0-9a-f]{24}", url):
            url = media.url(url)
        if m := media.EMBED.fullmatch(url):
            got = media.info(m.group(1))
            if not got:
                raise ValueError(f"「{name}」的文件 {m.group(1)} 不存在（可能被清理了）")
            nm = nm or got["name"]
        elif re.match(r"https?://[^\s]+$", url, re.I):
            nm = nm or (url.split("?")[0].rstrip("/").rsplit("/", 1)[-1] or url)
        else:
            raise ValueError(f"「{name}」要写网址（https://…）或者传上来的文件")
        out.append({"name": " ".join(str(nm).split())[:200] or url[:200], "url": url[:2000]})
    if len(out) > MAX_FILES:
        raise ValueError(f"「{name}」最多 {MAX_FILES} 个文件")
    return out or None


def _calc_text(prop: dict, v) -> str:
    """公式、汇总算出来的值给人看的样子。"""
    if v is None or v == "" or v == []:
        return ""
    if isinstance(v, bool):
        return "是" if v else "否"
    if isinstance(v, (int, float)):
        return _num(prop, v)
    if isinstance(v, dict):
        return v.get("start", "") + (f" → {v['end']}" if v.get("end") else "")
    if isinstance(v, list):
        return ", ".join(str(x) for x in v)
    return str(v)


def _num(prop: dict, n) -> str:
    f = prop.get("format") or "number"
    if f == "comma":
        return f"{n:,}"
    if f == "percent":
        return f"{n}%"
    if f == "cny":
        return f"¥{n:,}"
    if f == "usd":
        return f"${n:,}"
    return str(n)


def display(prop: dict, v) -> str:
    """给人和模型看的值（选项写名字、人写名字）。空的是空串。"""
    if v is None or v == "" or v == []:
        return ""
    t = prop["type"]
    if t in CALCULATED:
        return _calc_text(prop, v)
    if t == "relation":
        names = titles(v)
        return ", ".join(names[i] or "无标题" for i in v if i in names)
    if t == "files":
        return ", ".join(f.get("name", "") for f in v)
    if t == "unique_id":
        return f"{prop.get('prefix') or ''}{v}"
    if t == "button":
        return ""
    if t == "number":
        return _num(prop, v)
    if t in ("select", "status", "multi_select"):
        names = {o["id"]: o["name"] for o in prop.get("options") or []}
        ids = v if isinstance(v, list) else [v]
        return ", ".join(names.get(i, "?") for i in ids)
    if t == "date":
        return v["start"] + (f" → {v['end']}" if v.get("end") else "") if isinstance(v, dict) else str(v)
    if t in ("person", "created_by", "updated_by"):
        return ", ".join(person_name(x) for x in (v if isinstance(v, list) else [v]))
    if t == "checkbox":
        return "是" if v else ""
    if t in ("created_time", "updated_time"):
        return time.strftime("%Y-%m-%d %H:%M", time.localtime(v))
    return str(v)


# ---------- 表结构 ----------
def _clean_options(prop: dict, given, keep: list[dict]) -> list[dict]:
    """给的选项列表（名字，或者 {id?, name, color?, group?}）→ 存的样子。同名的沿用原来的 id 和颜色。"""
    if not isinstance(given, list):
        raise ValueError(f"「{prop['name']}」的 options 要是一个列表")
    if len(given) > MAX_OPTIONS:
        raise ValueError(f"「{prop['name']}」最多 {MAX_OPTIONS} 个选项")
    by_id = {o["id"]: o for o in keep}
    by_name = {o["name"].lower(): o for o in keep}
    out, seen = [], set()
    for k, g in enumerate(given):
        g = {"name": g} if isinstance(g, str) else g if isinstance(g, dict) else None
        if g is None:
            raise ValueError(f"「{prop['name']}」的选项要写名字")
        name = " ".join(str(g.get("name") or "").split())[:NAME_LEN]
        if not name or name.lower() in seen:
            raise ValueError(f"「{prop['name']}」的选项名不能是空的、也不能重复：{name or '（空）'}")
        seen.add(name.lower())
        old = by_id.get(g.get("id")) or by_name.get(name.lower())
        color = g.get("color") if g.get("color") in COLORS else old["color"] if old else COLORS[1 + k % (len(COLORS) - 1)]
        o = {"id": old["id"] if old else _new_id("o", {x["id"] for x in out} | set(by_id)), "name": name, "color": color}
        if prop["type"] == "status":
            o["group"] = g.get("group") if g.get("group") in GROUPS else old.get("group") if old and old.get("group") else (
                "done" if re.search(r"完成|done|关闭|closed", name, re.I) else "doing" if re.search(r"进行|doing|progress|处理中", name, re.I) else "todo")
        out.append(o)
    return out


def _status_default() -> list[dict]:
    return [{"id": "o" + secrets.token_hex(2), "name": GROUP_NAMES[g], "color": c, "group": g}
            for g, c in (("todo", "gray"), ("doing", "blue"), ("done", "green"))]


def _type(v) -> str:
    t = str(v or "").strip()
    t = _BY_ZH.get(t, t)
    if t not in TYPES or t == "title":
        raise ValueError(f"不认识的属性类型「{v}」，可以是：{'、'.join(k for k in TYPES if k != 'title')}")
    return t


def _new_prop(schema: list[dict], spec: dict, db_id: int | None = None) -> dict:
    name = " ".join(str(spec.get("name") or "").split())[:NAME_LEN]
    if not name:
        raise ValueError("属性要有名字")
    if any(p["name"].lower() == name.lower() for p in schema):
        raise ValueError(f"已经有叫「{name}」的属性了")
    t = _type(spec.get("type"))
    p = {"key": _new_id("p", {x["key"] for x in schema}), "name": name, "type": t}
    if t in OPTIONED:
        p["options"] = _clean_options(p, spec["options"], []) if spec.get("options") is not None else (_status_default() if t == "status" else [])
    if t == "number":
        p["format"] = spec.get("format") if spec.get("format") in FORMATS else "number"
    _config(p, spec, schema, db_id)
    return p


# 每种类型自己的设置（改类型时别的类型的设置去掉）
CONFIG = {"number": ("format",), "formula": ("formula", "format"), "relation": ("db", "mirror_of", "pair"),
          "rollup": ("relation", "target", "fn", "format"), "unique_id": ("prefix", "next"), "button": ("label", "set", "comment", "agent")}
_ALL_CONFIG = {k for v in CONFIG.values() for k in v}
_FN_BY_ZH = {v: k for k, v in dbcalc.ROLLUPS.items()}


def _db_ref(v, db_id) -> int | str:
    """关联指向哪个库：库的编号、名字，或者 self（自己）。"""
    if v in (None, "", "self", "自己") or v == db_id:
        return db_id if db_id is not None else "self"
    if isinstance(v, str) and re.fullmatch(r"#?\d+", v.strip()):
        v = int(v.strip().lstrip("#"))
    if isinstance(v, int) and not isinstance(v, bool):
        if get_db(v):
            return v
        raise ValueError(f"#{v} 不是数据库")
    found = [r[0] for r in conn.execute("select id from pages where kind='db' and lower(title)=lower(?)", (str(v).strip(),))]
    if len(found) == 1:
        return found[0]
    raise ValueError(f"找不到数据库「{v}」（写库的编号）" if not found else f"有 {len(found)} 个叫「{v}」的库，写库的编号")


def _check_formula(schema: list[dict], prop: dict, src) -> str:
    src = str(src or "").strip()
    if not src:
        return ""
    node = dbcalc.parse(src)
    names = {p["name"].lower(): p for p in schema}
    for n in dbcalc.refs(node):
        if n.strip().lower() == prop["name"].lower():
            raise ValueError("公式不能用到它自己")
        q = names.get(n.strip().lower())
        if not q:
            raise ValueError(f"公式里的属性「{n}」不存在，现在有：{'、'.join(p['name'] for p in schema)}")
        if q["key"] == prop["key"] or q is prop:
            raise ValueError("公式不能用到它自己")
    return src


def _formula_cycle(schema: list[dict]):
    """公式之间互相引用成环就报错。"""
    names = {p["name"].lower(): p for p in schema}
    graph = {}
    for p in schema:
        if p["type"] == "formula" and p.get("formula"):
            try:
                graph[p["key"]] = {names[n.lower()]["key"] for n in dbcalc.refs(dbcalc.parse(p["formula"])) if n.lower() in names}
            except dbcalc.FormulaError:
                graph[p["key"]] = set()
    state: dict = {}

    def visit(k):
        if state.get(k) == 1:
            raise ValueError(f"公式「{next(p['name'] for p in schema if p['key'] == k)}」和别的公式互相引用成了环")
        if state.get(k) == 2:
            return
        state[k] = 1
        for x in graph.get(k, ()):
            if x in graph:
                visit(x)
        state[k] = 2
    for k in graph:
        visit(k)


def _config(prop: dict, spec: dict, schema: list[dict], db_id) -> None:
    """按类型设好属性自己的设置：spec 里给了的才改（新建时补上默认的）。"""
    t = prop["type"]
    for k in _ALL_CONFIG - set(CONFIG.get(t, ())):
        prop.pop(k, None)
    if t in ("formula", "rollup") and "format" in spec:
        if spec["format"] not in FORMATS:
            raise ValueError(f"数字的格式只能是：{'、'.join(FORMATS)}")
        prop["format"] = spec["format"]
    if t == "formula":
        if "formula" in spec or "formula" not in prop:
            prop["formula"] = _check_formula(schema, prop, spec.get("formula"))
    elif t == "relation":
        if prop.get("mirror_of"):
            return
        if "db" in spec or "db" not in prop:
            target = _db_ref(spec.get("db"), db_id)
            if "db" in prop and prop["db"] != target:
                raise ValueError(f"「{prop['name']}」关联的库建好以后不能换：删了重建一个")
            prop["db"] = target
    elif t == "rollup":
        if "relation" in spec or "relation" not in prop:
            rel = prop_of({"schema": schema}, spec.get("relation"))
            if not rel or rel["type"] != "relation":
                rels = "、".join(p["name"] for p in schema if p["type"] == "relation")
                raise ValueError(f"汇总要选一个关联属性（relation）" + (f"，现在有：{rels}" if rels else "：这个库还没有关联属性，先加一个"))
            prop["relation"] = rel["key"]
        rel = next((p for p in schema if p["key"] == prop["relation"]), None)
        if not rel:
            raise ValueError("汇总用的关联属性不存在了")
        other = schema if rel.get("db") in (db_id, "self") else (get_db(rel["db"]) or {}).get("schema") or []
        if "target" in spec or "target" not in prop:
            tgt = prop_of({"schema": other}, spec.get("target") or other[0]["key"] if other else None)
            if not tgt:
                raise ValueError(f"关联的库里没有属性「{spec.get('target')}」，有：{'、'.join(p['name'] for p in other)}")
            if tgt["type"] == "button":
                raise ValueError("按钮不能汇总")
            prop["target"] = tgt["key"]
        if "fn" in spec or "fn" not in prop:
            fn = _FN_BY_ZH.get(str(spec.get("fn") or "").strip(), str(spec.get("fn") or "count").strip())
            if fn not in dbcalc.ROLLUPS:
                raise ValueError(f"汇总的算法只能是：{'、'.join(f'{k} {v}' for k, v in dbcalc.ROLLUPS.items())}")
            prop["fn"] = fn
    elif t == "unique_id":
        if "prefix" in spec:
            prefix = " ".join(str(spec.get("prefix") or "").split())[:12]
            if prefix and not re.fullmatch(r"[\w\u4e00-\u9fff -]+", prefix):
                raise ValueError("编号的前缀只能用字母、数字、汉字和 -")
            prop["prefix"] = prefix
        prop.setdefault("prefix", "")
        prop.setdefault("next", 1)
    elif t == "button":
        if "label" in spec or "label" not in prop:
            prop["label"] = " ".join(str(spec.get("label") or prop["name"]).split())[:30] or prop["name"]
        if "set" in spec or "set" not in prop:
            given = spec.get("set") or {}
            if not isinstance(given, dict):
                raise ValueError("按钮的 set 要是 {属性名: 值}")
            out = {}
            for ref, v in given.items():
                q = prop_of({"schema": schema}, ref)
                if not q or q is prop:
                    raise ValueError(f"按钮要改的属性「{ref}」不存在")
                if q["type"] in READONLY or is_mirror(q):
                    raise ValueError(f"按钮改不了「{q['name']}」（自动算的）")
                out[q["key"]] = v if q["type"] == "title" and isinstance(v, str) else normalize(q, v)
            prop["set"] = out
        if "comment" in spec or "comment" not in prop:
            prop["comment"] = str(spec.get("comment") or "").strip()[:500]
        if "agent" in spec or "agent" not in prop:
            b = spec.get("agent")
            if b in (None, "", 0):
                prop["agent"] = None
            else:
                found = agents.get(b) if isinstance(b, int) and not isinstance(b, bool) else next(
                    (x for x in agents.active() if x["name"].lower() == str(b).strip().lower()), None)
                if not found:
                    raise ValueError(f"没有 Agent「{b}」")
                prop["agent"] = found["id"]


def default_schema() -> list[dict]:
    return [{"key": "title", "name": "名称", "type": "title"},
            {"key": "status", "name": "状态", "type": "status", "options": _status_default()},
            {"key": "owner", "name": "负责人", "type": "person"},
            {"key": "due", "name": "日期", "type": "date"}]


def _default_views(schema: list[dict]) -> list[dict]:
    views = [{"id": "v1", "name": "表格", "type": "table"}]
    group = next((p["key"] for p in schema if p["type"] == "status"), None) or next((p["key"] for p in schema if p["type"] == "select"), None)
    if group:
        views.append({"id": "v2", "name": "看板", "type": "board", "group_by": group})
    return views


def create(title: str, by: str, schema: list[dict] | None = None, parent_id: int | None = None, bot_id: int | None = None,
           properties: list[dict] | None = None, title_name: str | None = None) -> dict:
    """建一个库。schema 是存的样子（网页用 default_schema）；properties 是工具给的 [{name, type, options?, format?}]。"""
    twoway = []
    if schema is None:
        schema = [{"key": "title", "name": " ".join(str(title_name or "").split())[:NAME_LEN] or "名称", "type": "title"}]
        for spec in properties or []:
            if len(schema) > MAX_PROPS:
                raise ValueError(f"最多 {MAX_PROPS} 个属性")
            spec = spec if isinstance(spec, dict) else {"name": spec}
            schema.append(_new_prop(schema, spec))
            if schema[-1]["type"] == "relation" and spec.get("twoway"):
                twoway.append((schema[-1], spec.get("pair_name")))
        _formula_cycle(schema)
    p = library.save_page(bot_id, title, "", "markdown", by=by, allow_empty=True, parent_id=parent_id, kind="db")
    for q in schema:   # 关联自己的：建好才知道自己的编号
        if q["type"] == "relation" and q.get("db") == "self":
            q["db"] = p["id"]
    for q, pair_name in twoway:
        _make_mirror({"id": p["id"], "title": p["title"]}, schema, q, pair_name, by)
    conn.execute("update pages set schema=?, views=? where id=?",
                 (json.dumps(schema, ensure_ascii=False), json.dumps(_default_views(schema), ensure_ascii=False), p["id"]))
    conn.commit()
    return get_db(p["id"])


def _touch(db_id: int, schema: list[dict] | None = None, views: list[dict] | None = None):
    sets, args = ["updated=?"], [time.time()]
    if schema is not None:
        sets.append("schema=?")
        args.append(json.dumps(schema, ensure_ascii=False))
    if views is not None:
        sets.append("views=?")
        args.append(json.dumps(views, ensure_ascii=False))
    conn.execute(f"update pages set {', '.join(sets)} where id=?", (*args, db_id))
    conn.commit()


def _rewrite_values(db_id: int, key: str, fn, cells: dict):
    """改表结构时改每一行这一列的值（fn(旧值) → 新值，None 是清空）。不涨行的版本号：这次改动记在库的表结构历史里，
    cells 收集改了的格子 {行号: {key: [旧值, 新值]}}，撤销时拿它放回去。"""
    for r in conn.execute("select id, props from pages where parent_id=? and kind='row'", (db_id,)).fetchall():
        props = json.loads(r[1] or "{}")
        if key not in props:
            continue
        old = props[key]
        new = fn(old)
        if new == old:
            continue
        if new is None:
            props.pop(key)
        else:
            props[key] = new
        conn.execute("update pages set props=? where id=?", (json.dumps(props, ensure_ascii=False), r[0]))
        first = cells.setdefault(str(r[0]), {}).get(key)
        cells[str(r[0])][key] = [first[0] if first else old, new]   # 同一批里改了两次：旧值留最早的


def _convert(old: dict, new: dict):
    """改类型时一个值怎么转：先变成文字再按新类型认，认不出就清空。"""
    def conv(v):
        if new["type"] in COMPUTED:
            return None
        if old["type"] == "multi_select" and new["type"] in ("select", "status") and isinstance(v, list):
            v = v[:1]   # 多选变单选：留第一个
        try:
            return normalize(new, display(old, v), create=True)
        except ValueError:
            return None
    return conv


def change_schema(db_id: int, ops: list[dict], by: str = "user") -> dict:
    """改表结构：{op: add, name, type, options?, format?} / {op: update, prop, name?, type?, options?, format?} / {op: delete, prop}。
    prop 是 key 或名字。按顺序做，有一个不行整批不做。每次改动记一条表结构历史（谁改的、改了什么、清掉或转换了哪些格子），
    能撤销（undo）。返回改好的库，多一个 last_change（这次改动的摘要，见 _changes）。"""
    db = get_db(db_id)
    if not db:
        raise LookupError(i18n.t("没有数据库 #{id}", id=db_id))
    if not isinstance(ops, list) or not ops:
        raise ValueError("没有要改的")
    schema = json.loads(json.dumps(db["schema"]))
    views = db["views"]
    later: list = []   # 表结构都通过了才改行里的值
    changes: list[dict] = []   # 给人看的摘要：[{kind, key, name, …}]
    twoway: list = []   # 新加的双向关联：(属性, 对方那一半叫什么)
    unpair: list = []   # 删掉的、改了类型的关联：对方库里的那一半也去掉
    for op in ops:
        if not isinstance(op, dict):
            raise ValueError("每个改动都要是一个对象")
        kind = op.get("op")
        if kind == "add":
            if len(schema) > MAX_PROPS:
                raise ValueError(f"最多 {MAX_PROPS} 个属性")
            schema.append(_new_prop(schema, op, db_id))
            changes.append({"kind": "add", "key": schema[-1]["key"], "name": schema[-1]["name"], "type": schema[-1]["type"]})
            if schema[-1]["type"] == "unique_id":
                later.append(("number", schema[-1]["key"], None, schema[-1]))
            if schema[-1]["type"] == "relation" and op.get("twoway"):
                twoway.append((schema[-1], op.get("pair_name")))
            continue
        prop = prop_of({"schema": schema}, op.get("prop"))
        if not prop:
            raise ValueError(f"没有属性「{op.get('prop')}」，现在有：{'、'.join(p['name'] for p in schema)}")
        if kind == "delete":
            if prop["type"] == "title":
                raise ValueError("名称不能删")
            schema.remove(prop)
            if prop["type"] == "relation":
                unpair.append(prop)
            later.append(("drop", prop["key"], None, None))
            changes.append({"kind": "delete", "key": prop["key"], "name": prop["name"]})
            continue
        if kind != "update":
            raise ValueError(f"不认识的改动：{kind}（只能是 add、update、delete）")
        if op.get("name") is not None:
            name = " ".join(str(op["name"]).split())[:NAME_LEN]
            if not name:
                raise ValueError("属性要有名字")
            if any(p is not prop and p["name"].lower() == name.lower() for p in schema):
                raise ValueError(f"已经有叫「{name}」的属性了")
            if name != prop["name"]:
                changes.append({"kind": "rename", "key": prop["key"], "name": name, "from": prop["name"]})
                _rename_refs(schema, prop["name"], name)
            prop["name"] = name
        old = json.loads(json.dumps(prop))
        if op.get("type") is not None and _type(op["type"]) != prop["type"]:
            if prop["type"] == "title":
                raise ValueError("名称的类型不能改")
            if is_mirror(prop):
                raise ValueError(f"「{prop['name']}」是另一个库的关联在这边的一半，不能改类型（不要了就删掉）")
            new_t = _type(op["type"])
            if old["type"] == "relation":
                unpair.append(old)
            prop["type"] = new_t
            if new_t in OPTIONED and old["type"] not in OPTIONED:
                prop["options"] = _status_default() if new_t == "status" else []
            elif new_t in OPTIONED and old["type"] in OPTIONED:   # 选项沿用；变成状态时补上分组
                prop["options"] = _clean_options(prop, old.get("options") or [], old.get("options") or [])
            elif new_t not in OPTIONED:
                prop.pop("options", None)
            if new_t == "number":
                prop["format"] = op.get("format") if op.get("format") in FORMATS else "number"
            else:
                prop.pop("format", None)
            _config(prop, op, schema, db_id)
            later.append(("number" if new_t == "unique_id" else "convert", prop["key"], old, prop))
            changes.append({"kind": "type", "key": prop["key"], "name": prop["name"], "from": old["type"], "to": new_t})
        if op.get("options") is not None:
            if prop["type"] not in OPTIONED:
                raise ValueError(f"「{prop['name']}」不是单选、多选或状态，没有选项")
            before = {o["id"] for o in prop.get("options") or []}
            was = json.dumps(prop.get("options") or [], ensure_ascii=False)
            prop["options"] = _clean_options(prop, op["options"], prop.get("options") or [])
            if gone := before - {o["id"] for o in prop["options"]}:
                later.append(("unoption", prop["key"], gone, None))
            if json.dumps(prop["options"], ensure_ascii=False) != was:
                changes.append({"kind": "options", "key": prop["key"], "name": prop["name"]})
        conf = {k: op[k] for k in _ALL_CONFIG & set(op) if k not in ("next", "mirror_of", "pair") and not (k == "format" and prop["type"] == "number")}
        if conf and op.get("type") is None or (conf and _type(op["type"]) == old["type"]):
            before_conf = json.dumps(prop, ensure_ascii=False, sort_keys=True)
            _config(prop, conf, schema, db_id)
            if json.dumps(prop, ensure_ascii=False, sort_keys=True) != before_conf:
                changes.append({"kind": "config", "key": prop["key"], "name": prop["name"]})
        if op.get("format") is not None and prop["type"] == "number":
            if op["format"] not in FORMATS:
                raise ValueError(f"数字的格式只能是：{'、'.join(FORMATS)}")
            if op["format"] != prop.get("format"):
                changes.append({"kind": "format", "key": prop["key"], "name": prop["name"]})
            prop["format"] = op["format"]
    _formula_cycle(schema)
    cells: dict = {}
    for what, key, a, b in later:
        if what == "number":
            _number_rows(db_id, b, cells)
        elif what == "drop":
            _rewrite_values(db_id, key, lambda v: None, cells)
        elif what == "unoption":
            _rewrite_values(db_id, key, lambda v, gone=a: ([x for x in v if x not in gone] or None) if isinstance(v, list)
                            else None if v in gone else v, cells)
        else:
            _rewrite_values(db_id, key, _convert(a, b), cells)
    views = [_fix_view(v, schema) for v in views] if views else _default_views(schema)
    if not changes and not cells and schema == db["schema"]:
        return get_db(db_id) | {"last_change": []}
    for prop, pair_name in twoway:
        _make_mirror(db, schema, prop, pair_name, by)
    _counts(changes, cells)
    _record(db, by, db["schema"], schema, db["views"], views, cells, changes)
    _touch(db_id, schema=schema, views=views)
    for prop in unpair:
        _drop_pair(db_id, prop, by)
    return get_db(db_id) | {"last_change": changes}


def _rename_refs(schema: list[dict], old: str, new: str):
    """属性改名：公式里的 prop("旧名") 跟着改。"""
    pat = re.compile(r"""prop\(\s*(["'“「])""" + re.escape(old) + r"""(["'”」])\s*\)""", re.I)
    for p in schema:
        if p["type"] == "formula" and p.get("formula"):
            p["formula"] = pat.sub(lambda m: f'prop("{new}")', p["formula"])


def _number_rows(db_id: int, prop: dict, cells: dict):
    """自动编号：按手动排的顺序给每一行发号，下一个号记在表结构里。"""
    n = 0
    for rid, props in conn.execute("select id, props from pages where parent_id=? and kind='row' order by coalesce(pos, 0), id", (db_id,)).fetchall():
        n += 1
        d = json.loads(props or "{}")
        old = d.get(prop["key"])
        d[prop["key"]] = n
        conn.execute("update pages set props=? where id=?", (json.dumps(d, ensure_ascii=False), rid))
        first = cells.setdefault(str(rid), {}).get(prop["key"])
        cells[str(rid)][prop["key"]] = [first[0] if first else old, n]
    prop["next"] = n + 1


def _make_mirror(db: dict, schema: list[dict], prop: dict, pair_name, by: str):
    """双向关联：在对方库（可以是自己）加一个反向的关联属性。"""
    target = prop["db"]
    if target == db["id"]:
        other = schema
    else:
        odb = get_db(target)
        if not odb:
            return
        other = json.loads(json.dumps(odb["schema"]))
    name = " ".join(str(pair_name or "").split())[:NAME_LEN] or (db["title"] if target != db["id"] else f"{prop['name']}（反向）")[:NAME_LEN]
    base, k = name, 2
    while any(p["name"].lower() == name.lower() for p in other):
        name = f"{base} {k}"
        k += 1
    m = {"key": _new_id("p", {x["key"] for x in other}), "name": name, "type": "relation", "db": db["id"], "mirror_of": prop["key"]}
    other.append(m)
    prop["pair"] = m["key"]
    if target != db["id"]:
        _record(odb, by, odb["schema"], other, odb["views"], odb["views"], {}, [{"kind": "add", "key": m["key"], "name": name, "type": "relation"}])
        _touch(target, schema=other)


def _drop_pair(db_id: int, prop: dict, by: str):
    """删了（或改了类型）一个关联：对方库里反向的那一半也去掉；删的是反向的那一半就把这边记的配对清掉。"""
    if prop.get("mirror_of"):
        odb = get_db(prop.get("db"))
        if odb:
            sch = json.loads(json.dumps(odb["schema"]))
            for q in sch:
                if q["key"] == prop["mirror_of"] and q.get("pair") == prop["key"]:
                    q.pop("pair", None)
            if sch != odb["schema"]:
                _touch(odb["id"], schema=sch)
        return
    if not prop.get("pair"):
        return
    odb = get_db(prop.get("db"))
    if not odb:
        return
    m = next((q for q in odb["schema"] if q["key"] == prop["pair"] and q.get("mirror_of") == prop["key"]), None)
    if not m:
        return
    sch = [q for q in odb["schema"] if q is not m]
    if odb["id"] == db_id:
        _touch(db_id, schema=sch)
        return
    _record(odb, by, odb["schema"], sch, odb["views"], [_fix_view(v, sch) for v in odb["views"]], {},
            [{"kind": "delete", "key": m["key"], "name": m["name"]}])
    _touch(odb["id"], schema=sch, views=[_fix_view(v, sch) for v in odb["views"]])


# ---------- 表结构的历史：每次改动一条，能撤销 ----------
HISTORY_LABEL = "schema"
MAX_HISTORY = 200
VIEW_MERGE = 600   # 同一个人 10 分钟内接着调视图（拖列宽、改筛选）并成一条


def _counts(changes: list[dict], cells: dict):
    """摘要里写上清掉了几行的值（删属性、改类型转不过去、去掉了用着的选项）。"""
    for c in changes:
        if c["kind"] in ("delete", "type", "options"):
            c["cleared"] = sum(1 for kv in cells.values() if c["key"] in kv and kv[c["key"]][1] is None and kv[c["key"]][0] is not None)


def _record(db: dict, by: str, before: list[dict], after: list[dict], views_before: list, views_after: list, cells: dict,
            changes: list[dict], undo_of: int | None = None):
    """记一条表结构历史（page_versions，label = schema，props 里是这次改动），库的版本号 +1。"""
    b = {p["key"]: p for p in before}
    a = {p["key"]: p for p in after}
    entry = {"props": {k: [b.get(k), a.get(k)] for k in [*b, *[k for k in a if k not in b]] if b.get(k) != a.get(k)},
             "order": [p["key"] for p in before], "cells": cells, "views": [views_before, views_after], "changes": changes}
    if undo_of is not None:
        entry["undo_of"] = undo_of
    now, newv = time.time(), db["version"] + 1
    conn.execute("update pages set version=? where id=?", (newv, db["id"]))
    conn.execute("insert into page_versions(page_id, version, title, content, meta, by, ts_start, ts, label, props) values(?,?,?,?,?,?,?,?,?,?)",
                 (db["id"], newv, db["title"], "", None, by, now, now, HISTORY_LABEL, json.dumps(entry, ensure_ascii=False)))
    old = conn.execute("select id from page_versions where page_id=? and label=? order by id desc limit -1 offset ?",
                       (db["id"], HISTORY_LABEL, MAX_HISTORY)).fetchall()
    conn.executemany("delete from page_versions where id=?", old)


def history(db_id: int) -> list[dict]:
    """库的表结构历史，新的在前：[{id, version, by, ts, changes, undo_of?, undone_by?}]。"""
    rows = conn.execute("select id, version, by, ts, props from page_versions where page_id=? and label=? order by id desc",
                        (db_id, HISTORY_LABEL)).fetchall()
    out = []
    for vid, v, by, ts, props in rows:
        e = json.loads(props or "{}")
        out.append({"id": vid, "version": v, "by": by, "ts": ts, "changes": e.get("changes") or [], "undo_of": e.get("undo_of")})
    undone = {x["undo_of"]: x["id"] for x in out if x["undo_of"]}
    for x in out:
        if x["id"] in undone:
            x["undone_by"] = undone[x["id"]]
    return out


def undo(db_id: int, vid: int, by: str = "user") -> dict:
    """撤销表结构历史里的一次改动：只动那次改到的属性（删掉的放回原来的位置，改了的换回原来的定义，加的删掉），
    那次清掉或转换的格子放回旧值；之后又被人改过的格子留着现在的值（类型换回去了的，按新类型转一下）。
    撤销本身也记一条，可以再撤销。返回改好的库，last_change 是 [{kind: undo, of, items, kept}]。"""
    db = get_db(db_id)
    if not db:
        raise LookupError(i18n.t("没有数据库 #{id}", id=db_id))
    r = conn.execute("select props from page_versions where id=? and page_id=? and label=?", (vid, db_id, HISTORY_LABEL)).fetchone()
    if not r:
        raise LookupError(i18n.t("没有这一版"))
    entry = json.loads(r[0])
    schema = json.loads(json.dumps(db["schema"]))
    keys = lambda: [p["key"] for p in schema]   # noqa: E731
    removed: list[str] = []
    retyped: dict[str, tuple[dict, dict]] = {}
    for key, (before, after) in entry["props"].items():
        at = keys().index(key) if key in keys() else None
        if before is None:   # 那次加的：删掉
            if at is not None and schema[at]["type"] != "title":
                removed.append(key)
                schema.pop(at)
            continue
        before = json.loads(json.dumps(before))
        if any(p["key"] != key and p["name"].lower() == before["name"].lower() for p in schema):
            before["name"] = (before["name"] + "（恢复）")[:NAME_LEN]   # 名字被别的属性占了
        if at is None:   # 那次删的（或者后来删了）：放回原来的位置
            order = entry.get("order") or []
            pos = next((keys().index(k) + 1 for k in reversed(order[:order.index(key)]) if k in keys()), 1) if key in order else len(schema)
            schema.insert(pos, before)
        else:
            if schema[at]["type"] != before["type"]:
                retyped[key] = (schema[at], before)
            schema[at] = before
    cells: dict = {}
    kept = 0
    rows_now = {str(x[0]): json.loads(x[1] or "{}") for x in conn.execute("select id, props from pages where parent_id=? and kind='row'", (db_id,))}
    for rid, props in rows_now.items():
        changed = False
        for key in removed:
            if key in props:
                cells.setdefault(rid, {})[key] = [props.pop(key), None]
                changed = True
        for key, (old, new) in (entry["cells"].get(rid) or {}).items():
            if key in removed:
                continue
            cur = props.get(key)
            if cur == new:
                val = old
            else:   # 之后被人改过：留着现在的值（类型换回去了就转一下）
                kept += 1
                val = _convert(*retyped[key])(cur) if key in retyped and cur is not None else cur
            if val != cur:
                cells.setdefault(rid, {})[key] = [cur, val]
                if val is None:
                    props.pop(key, None)
                else:
                    props[key] = val
                changed = True
        if changed:
            conn.execute("update pages set props=? where id=?", (json.dumps(props, ensure_ascii=False), int(rid)))
    views_before, views_after = entry.get("views") or [None, None]
    views, kept_views = _undo_views(db["views"], views_before, views_after, schema)
    kept += kept_views
    changes = [{"kind": "undo", "of": vid, "items": entry.get("changes") or [], "kept": kept}]
    _record(db, by, db["schema"], schema, db["views"], views, cells, changes, undo_of=vid)
    _touch(db_id, schema=schema, views=views)
    return get_db(db_id) | {"last_change": changes}


def change_text(c: dict) -> str:
    """一条改动给模型看的样子（网页上按 kind 自己翻译）。"""
    k = c["kind"]
    cleared = f"，清掉了 {c['cleared']} 行的值" if c.get("cleared") else ""
    if k == "add":
        return f"加了属性「{c['name']}」（{TYPES.get(c.get('type'), '')}）"
    if k == "delete":
        return f"删了属性「{c['name']}」{cleared}"
    if k == "rename":
        return f"把「{c['from']}」改名为「{c['name']}」"
    if k == "type":
        return f"把「{c['name']}」从{TYPES.get(c['from'], c['from'])}改成{TYPES.get(c['to'], c['to'])}" + (
            f"，{c['cleared']} 行的值转不过去，清掉了" if c.get("cleared") else "")
    if k == "options":
        return f"改了「{c['name']}」的选项" + (f"，{c['cleared']} 行用了去掉的选项，清掉了" if c.get("cleared") else "")
    if k == "format":
        return f"改了「{c['name']}」的数字格式"
    if k == "config":
        return f"改了「{c['name']}」的设置"
    if k == "views":
        parts = ([f"加了视图「{'」「'.join(c['added'])}」"] if c.get("added") else []) + (
            [f"删了视图「{'」「'.join(c['removed'])}」"] if c.get("removed") else []) + (
            [f"改了视图「{'」「'.join(c['changed'])}」"] if c.get("changed") else [])
        return "；".join(parts) or "改了视图"
    if k == "undo":
        return "撤销了：" + "；".join(change_text(x) for x in c.get("items") or [])
    return k


# ---------- 视图 ----------
def _fix_view(v: dict, schema: list[dict]) -> dict | None:
    """表结构变了以后：去掉指向已经删掉的属性的设置；看板分组的属性没了就退回表格。"""
    keys = {p["key"]: p for p in schema if p["type"] != "button"}
    v = dict(v)
    v["sort"] = [s for s in v.get("sort") or [] if s.get("prop") in keys]
    v["filter"] = [f for f in v.get("filter") or [] if f.get("prop") in keys]
    v["hidden"] = [k for k in v.get("hidden") or [] if k in keys]
    v["widths"] = {k: w for k, w in (v.get("widths") or {}).items() if k in keys}
    if v.get("type") == "board" and (keys.get(v.get("group_by")) or {}).get("type") not in ("select", "status"):
        group = next((p["key"] for p in schema if p["type"] in ("status", "select")), None)
        if group:
            v["group_by"] = group
        else:
            v["type"] = "table"
            v.pop("group_by", None)
    return v


def check_views(views, schema: list[dict]) -> list[dict]:
    if not isinstance(views, list) or not views:
        raise ValueError("至少要有一个视图")
    if len(views) > MAX_VIEWS:
        raise ValueError(f"最多 {MAX_VIEWS} 个视图")
    keys = {p["key"]: p for p in schema if p["type"] != "button"}
    out, ids = [], set()
    for v in views:
        if not isinstance(v, dict) or v.get("type") not in ("table", "board"):
            raise ValueError("视图要是表格（table）或看板（board）")
        vid = str(v.get("id") or "") if re.fullmatch(r"[A-Za-z0-9_-]{1,20}", str(v.get("id") or "")) else _new_id("v", ids)
        if vid in ids:
            vid = _new_id("v", ids)
        ids.add(vid)
        name = " ".join(str(v.get("name") or "").split())[:40] or ("看板" if v["type"] == "board" else "表格")
        clean = {"id": vid, "name": name, "type": v["type"],
                 "sort": [{"prop": s["prop"], "desc": bool(s.get("desc"))} for s in v.get("sort") or []
                          if isinstance(s, dict) and s.get("prop") in keys][:5],
                 "filter": [_check_filter(f, keys) for f in v.get("filter") or []][:10],
                 "hidden": [k for k in v.get("hidden") or [] if k in {p["key"] for p in schema} and k != "title"],
                 "widths": {k: max(60, min(800, int(w))) for k, w in (v.get("widths") or {}).items()
                            if k in keys and isinstance(w, (int, float)) and not isinstance(w, bool)}}
        if v["type"] == "board":
            if (keys.get(v.get("group_by")) or {}).get("type") not in ("select", "status"):
                raise ValueError("看板要按一个单选或状态属性分列")
            clean["group_by"] = v["group_by"]
        out.append(clean)
    return out


def set_views(db_id: int, views, by: str = "user") -> dict:
    """换掉视图列表，记进库的历史（同一个人 10 分钟内接着改的并成一条），能撤销。"""
    db = get_db(db_id)
    if not db:
        raise LookupError(i18n.t("没有数据库 #{id}", id=db_id))
    new = check_views(views, db["schema"])
    try:   # 比的时候两边都按规整过的样子（老的视图缺 sort、widths 这些空字段）
        db["views"] = check_views(db["views"], db["schema"])
    except ValueError:
        pass
    if new == db["views"]:
        return db
    last = conn.execute("select id, by, ts, props from page_versions where page_id=? and label=? order by id desc limit 1",
                        (db_id, HISTORY_LABEL)).fetchone()
    entry = json.loads(last[3]) if last else None
    undone = last and conn.execute("select 1 from page_versions where page_id=? and label=? and props like ?",
                                   (db_id, HISTORY_LABEL, f'%"undo_of": {last[0]}%')).fetchone()
    if (entry and last[1] == by and time.time() - last[2] < VIEW_MERGE and not undone and not entry.get("undo_of")
            and [c["kind"] for c in entry.get("changes") or []] == ["views"]):
        before = entry["views"][0]
        entry["views"] = [before, new]
        entry["changes"] = [_view_change(before, new)]
        conn.execute("update page_versions set ts=?, props=? where id=?", (time.time(), json.dumps(entry, ensure_ascii=False), last[0]))
    else:
        _record(db, by, db["schema"], db["schema"], db["views"], new, {}, [_view_change(db["views"], new)])
    _touch(db_id, views=new)
    return get_db(db_id)


def _view_change(before: list, after: list) -> dict:
    b, a = {v["id"]: v for v in before or []}, {v["id"]: v for v in after or []}
    return {"kind": "views", "added": [v["name"] for k, v in a.items() if k not in b],
            "removed": [v["name"] for k, v in b.items() if k not in a],
            "changed": [v["name"] for k, v in a.items() if k in b and b[k] != v] + (
                ["顺序"] if [k for k in b if k in a] != [k for k in a if k in b] else [])}


def _undo_views(cur: list, before: list | None, after: list | None, schema: list[dict]) -> tuple[list, int]:
    """撤销一次视图改动：那次改了的视图之后没再动过就换回去，加的去掉，删的放回原来的位置；之后又被改过的留着。"""
    b, a = {v["id"]: v for v in before or []}, {v["id"]: v for v in after or []}
    out, kept = [], 0
    for v in cur:
        k = v["id"]
        if k in a and k not in b:
            if v == a[k]:
                continue
            kept += 1
        elif k in a and k in b and a[k] != b[k]:
            if v == a[k]:
                v = b[k]
            else:
                kept += 1
        out.append(v)
    order = [v["id"] for v in before or []]
    for k, v in b.items():
        if k not in a and all(x["id"] != k for x in out):
            out.insert(min(order.index(k), len(out)), v)
    if before and [v["id"] for v in cur] == [v["id"] for v in after or []] and set(order) == {v["id"] for v in out}:
        out.sort(key=lambda v: order.index(v["id"]))   # 那次只调了顺序：顺序也换回去
    return [x for x in (_fix_view(v, schema) for v in out) if x] or _default_views(schema), kept


# ---------- 筛选、排序 ----------
def _check_filter(f, keys: dict) -> dict:
    if not isinstance(f, dict) or f.get("prop") not in keys:
        raise ValueError("筛选要指明属性")
    if f.get("op") not in OPS:
        raise ValueError(f"筛选的条件只能是：{'、'.join(OPS)}")
    out = {"prop": f["prop"], "op": f["op"]}
    if f["op"] not in ("empty", "not_empty"):
        out["value"] = f.get("value")
    return out


def _calc_key(v):
    """公式、汇总算出来的值排序：类型不一样的分开排（数在前，然后日期、文字、真假），不然没法比。"""
    if isinstance(v, bool):
        return (3, int(v), "")
    if isinstance(v, (int, float)):
        return (0, v, "")
    if isinstance(v, dict):
        return (1, 0, v.get("start", ""))
    return (2, 0, dbcalc.text(v).lower())


def _cmp_value(prop: dict, v):
    """排序、比较用的值：数字是数，选项按选项的顺序，日期、时间按字符串，其余小写的文字。"""
    t = prop["type"]
    if t in CALCULATED:
        return _calc_key(v)
    if t == "number" or t == "unique_id":
        return v
    if t in ("select", "status"):
        order = [o["id"] for o in prop.get("options") or []]
        return order.index(v) if v in order else len(order)
    if t == "date":
        return v["start"] if isinstance(v, dict) else str(v)
    if t in ("created_time", "updated_time"):
        return v
    if t == "checkbox":
        return 1 if v else 0
    return display(prop, v).lower()


def _target(prop: dict, raw):
    """筛选里给的值 → 和存的值比的样子（不自动加选项）。公式、汇总留着原样，比的时候按算出来的类型认。"""
    t = prop["type"]
    if t in CALCULATED:
        return raw
    if t == "unique_id":
        m = re.search(r"(\d+)\s*$", str(raw if raw is not None else ""))
        if not m:
            raise ValueError(f"「{prop['name']}」写编号，比如 {prop.get('prefix') or ''}12")
        return int(m.group(1))
    if t in ("relation", "files"):
        return str(raw if raw is not None else "").strip().lstrip("#").lower()
    if t in ("select", "status", "multi_select"):
        return {_option(prop, x, create=False) for x in (raw if isinstance(raw, list) else [raw])}
    if t in ("person", "created_by", "updated_by"):
        return set(normalize({**prop, "type": "person"}, raw) or [])
    if t == "date" or t in ("created_time", "updated_time"):
        d = _day(raw.get("start") if isinstance(raw, dict) else raw)
        if not d:
            raise ValueError(f"「{prop['name']}」的日期写成 2026-11-15，不是「{raw}」")
        return d
    if t == "number":
        return normalize(prop, raw)
    if t == "checkbox":
        b = _truthy(raw)
        if b is None:
            raise ValueError(f"「{prop['name']}」要写 true 或 false")
        return b
    return str(raw if raw is not None else "").lower()


def _match(prop: dict, v, op: str, target) -> bool:
    t = prop["type"]
    empty = v is None or v == "" or v == [] or (t == "checkbox" and not v)
    if op == "empty":
        return empty
    if op == "not_empty":
        return not empty
    if t in CALCULATED:   # 按算出来的值的类型比
        if isinstance(v, bool):
            b = _truthy(target)
            return False if b is None else (v == b if op in ("eq", "contains") else v != b if op in ("neq", "not_contains") else False)
        if isinstance(v, (int, float)):
            n = dbcalc.num(target)
            if n is None:
                return op in ("neq", "not_contains")
            return _match({"type": "number", "name": prop["name"]}, v, op, n)
        if isinstance(v, dict):
            d = _day(target.get("start") if isinstance(target, dict) else target)
            return op in ("neq", "not_contains") if not d else _match({"type": "date", "name": prop["name"]}, v, op, d)
        return _match({"type": "text", "name": prop["name"]}, dbcalc.text(v), op, str(target if target is not None else "").lower())
    if t == "relation":   # 比对方行的名称，也能写行号
        if target.isdigit():
            hit = int(target) in (v or [])
        else:
            hit = target in display(prop, v).lower() if op in ("contains", "not_contains") else target in [x.lower() for x in titles(v or []).values()]
        return hit if op in ("eq", "contains") else not hit if op in ("neq", "not_contains") else False
    if t == "files":
        hit = target in display(prop, v).lower()
        return hit if op in ("eq", "contains") else not hit if op in ("neq", "not_contains") else False
    if t == "checkbox":
        return bool(v) == target if op in ("eq", "contains") else bool(v) != target
    if t in ("select", "status", "multi_select", "person", "created_by", "updated_by"):
        have = set(v if isinstance(v, list) else [v]) if not empty else set()
        hit = bool(have & target)
        return hit if op in ("eq", "contains") else not hit if op in ("neq", "not_contains") else False
    if t in ("date", "created_time", "updated_time"):
        if empty:
            return op in ("neq", "not_contains")
        cur = (v["start"] if isinstance(v, dict) else time.strftime("%Y-%m-%d %H:%M", time.localtime(v)) if isinstance(v, (int, float)) else str(v))[:len(target)]
        return {"eq": cur == target, "contains": cur == target, "neq": cur != target, "not_contains": cur != target,
                "gt": cur > target, "gte": cur >= target, "lt": cur < target, "lte": cur <= target}[op]
    if t in ("number", "unique_id"):
        if empty:
            return op in ("neq", "not_contains")
        return {"eq": v == target, "contains": v == target, "neq": v != target, "not_contains": v != target,
                "gt": v > target, "gte": v >= target, "lt": v < target, "lte": v <= target}[op]
    s = display(prop, v).lower()
    return {"eq": s == target, "neq": s != target, "contains": target in s, "not_contains": target not in s,
            "gt": s > target, "gte": s >= target, "lt": s < target, "lte": s <= target}[op]


def query(db: dict, items: list[dict], filters: list[dict] | None = None, sorts: list[dict] | None = None) -> list[dict]:
    """筛选（都要满足）和排序（可以按几列；空的总排在最后）。prop 是 key 或名字。"""
    conds = []
    for f in filters or []:
        prop = prop_of(db, f.get("prop")) if isinstance(f, dict) else None
        if not prop:
            raise ValueError(f"筛选里没有属性「{f.get('prop') if isinstance(f, dict) else f}」")
        op = f.get("op") or "eq"
        if op not in OPS:
            raise ValueError(f"筛选的条件只能是：{'、'.join(OPS)}")
        conds.append((prop, op, None if op in ("empty", "not_empty") else _target(json.loads(json.dumps(prop)), f.get("value"))))
    out = [r for r in items if all(_match(p, value_of(r, p), op, tg) for p, op, tg in conds)]
    for s in reversed(sorts or []):
        prop = prop_of(db, s.get("prop")) if isinstance(s, dict) else None
        if not prop:
            raise ValueError(f"排序里没有属性「{s.get('prop') if isinstance(s, dict) else s}」")
        has = [r for r in out if value_of(r, prop) not in (None, "", [])]
        none = [r for r in out if value_of(r, prop) in (None, "", [])]
        has.sort(key=lambda r: _cmp_value(prop, value_of(r, prop)), reverse=bool(s.get("desc")))
        out = has + none
    return out


def view_rows(db: dict, view_id: str | None = None) -> tuple[dict, list[dict]]:
    """按视图筛选、排序后的行。没给或者找不到就用第一个视图。"""
    views = db["views"] or _default_views(db["schema"])
    view = next((v for v in views if v["id"] == view_id), views[0])
    try:
        return view, query(db, rows(db["id"]), view.get("filter"), view.get("sort"))
    except ValueError:   # 视图里的筛选值已经不成立了（比如选项被删）：不筛
        return view, rows(db["id"])


# ---------- 行 ----------
def _check_props(db: dict, given: dict, row: dict | None) -> tuple[dict, bool, str | None]:
    """给的属性 {key 或名字: 值} → ({key: 存的值或 None}, 表结构有没有变（加了选项）, 新标题或 None)。"""
    if not isinstance(given, dict):
        raise ValueError("props 要是一个对象：{属性名: 值}")
    changes, title = {}, None
    before = json.dumps(db["schema"], ensure_ascii=False)
    for ref, v in given.items():
        prop = prop_of(db, ref)
        if not prop:
            raise ValueError(f"没有属性「{ref}」，现在有：{'、'.join(p['name'] for p in db['schema'])}")
        if prop["type"] == "title":
            title = " ".join(str(v or "").split())[:120]
            continue
        changes[prop["key"]] = normalize(prop, v)
    return changes, json.dumps(db["schema"], ensure_ascii=False) != before, title


def add_row(db_id: int, props: dict | None, by: str, title: str = "", content: str = "", bot_id: int | None = None,
            after: int | None = None) -> dict:
    """加一行。props 是 {key 或名字: 值}（名称也能写在里面）。after：排在哪一行后面（0 是最前，None 是最后）。"""
    db = get_db(db_id)
    if not db:
        raise LookupError(i18n.t("没有数据库 #{id}", id=db_id))
    if conn.execute("select count(*) from pages where parent_id=? and kind='row'", (db_id,)).fetchone()[0] >= MAX_ROWS:
        raise ValueError(i18n.t("一个数据库最多 {n} 行", n=MAX_ROWS))
    changes, schema_changed, t2 = _check_props(db, props or {}, None)
    if len(content or "") > library.MAX_PAGE:
        raise ValueError(i18n.t("页面太大（最多 {n}K 字）", n=library.MAX_PAGE // 1000))
    mirrors = {k: changes.pop(k) for k in [k for k in changes if is_mirror(_key(db, k))]}
    stored = {k: v for k, v in changes.items() if v is not None}
    for q in db["schema"]:   # 自动编号：发下一个号
        if q["type"] == "unique_id":
            stored[q["key"]] = q.get("next") or 1
            q["next"] = stored[q["key"]] + 1
            schema_changed = True
    if schema_changed:
        _touch(db_id, schema=db["schema"])
    now = time.time()
    p = library.save_page(bot_id, t2 if t2 is not None else title, content or "", "markdown", by=by, allow_empty=True,
                          parent_id=db_id, kind="row", props=stored)
    conn.execute("update pages set props_meta=? where id=?",
                 (json.dumps({k: {"v": 1, "by": by, "ts": now} for k in stored}, ensure_ascii=False), p["id"]))
    conn.commit()
    if after is not None:
        library.move(p["id"], db_id, after)
    for k, v in mirrors.items():
        _set_mirror(db, p["id"], _key(db, k), v or [], by)
    _touch(db_id)
    return get_row(p["id"])


def _key(db: dict, key: str) -> dict:
    return next(p for p in db["schema"] if p["key"] == key)


def _set_mirror(db: dict, row_id: int, prop: dict, want: list[int], by: str) -> bool:
    """在双向关联反向的那一半改值：改的是另一个库里那些行的关联（加上或去掉这一行）。返回有没有变。"""
    src_key = prop["mirror_of"]
    now = _backlinks(prop["db"], src_key).get(row_id, [])
    changed = False
    for rid in [x for x in want if x not in now] + [x for x in now if x not in want]:
        other = get_row(rid)
        if not other:
            continue
        cur = [x for x in other["props"].get(src_key) or [] if isinstance(x, int)]
        new = [x for x in cur if x != row_id] if rid in now else cur + [row_id]
        update_row(rid, {src_key: new or None}, by)
        changed = True
    return changed


def update_row(row_id: int, props: dict, by: str, base: int | None = None) -> tuple[dict, list[str]]:
    """改一行的属性（名称也能改）。base：调用方读到的版本号，给了就按键查冲突。返回 (这一行, 改了哪些键)；什么都没变不涨版本。"""
    row = get_row(row_id)
    if not row:
        raise LookupError(i18n.t("没有这一行 #{id}", id=row_id))
    db = get_db(row["db_id"])
    if base is not None and (not isinstance(base, int) or isinstance(base, bool) or base < 0 or base > row["version"]):
        raise ValueError(i18n.t("版本号不对：页面现在是版本 {v}", v=row["version"]))
    changes, schema_changed, title = _check_props(db, props, row)
    mirrors = {k: changes.pop(k) for k in [k for k in changes if is_mirror(_key(db, k))]}
    cur = dict(row["props"])
    meta = dict(row["meta"])
    changed = [k for k, v in changes.items() if cur.get(k) != v]
    if title is not None and title != row["title"]:
        changed.append("title")
    if base is not None:
        stale = [k for k in changed if (meta.get(k) or {}).get("v", 0) > base and (meta.get(k) or {}).get("by") != by]
        if stale:
            keys = {p["key"]: p for p in db["schema"]}
            raise Conflict(row["version"], [{"prop": k, "name": keys[k]["name"] if k in keys else k,
                                             "current": display(keys[k], value_of(row, keys[k])) if k in keys else "",
                                             "by": meta[k].get("by", "")} for k in stale])
    if schema_changed:
        _touch(db["id"], schema=db["schema"])
    moved = [k for k, v in mirrors.items() if _set_mirror(db, row_id, _key(db, k), v or [], by)]
    if not changed:
        return (get_row(row_id) if moved else row), moved
    p, _ = pagedoc.load(row_id)
    now = time.time()
    for k in changed:
        if k != "title":
            if changes[k] is None:
                cur.pop(k, None)
            else:
                cur[k] = changes[k]
        meta[k] = {"v": row["version"] + 1, "by": by, "ts": now}
    try:
        pagedoc.commit_props(p, title if "title" in changed else row["title"], cur, meta, by)
    except pagedoc.Conflict:   # 正好被别人提交了一版：重新来一次（按键查冲突）
        return update_row(row_id, {k: v for k, v in props.items() if k not in mirrors}, by, base)
    _touch(db["id"])
    return get_row(row_id), changed + moved


def press(row_id: int, key: str, by: str = "user") -> tuple[dict, list[str], dict | None]:
    """点一行上的按钮：按设置改属性（已经删掉的属性、不认了的值跳过），返回 (这一行, 改了的键, 要写的评论 {text, agent} 或 None)。"""
    row = get_row(row_id)
    if not row:
        raise LookupError(i18n.t("没有这一行 #{id}", id=row_id))
    db = get_db(row["db_id"])
    btn = next((p for p in db["schema"] if p["key"] == key and p["type"] == "button"), None)
    if not btn:
        raise LookupError(i18n.t("没有这个按钮"))
    keys = {p["key"]: p for p in db["schema"]}
    todo = {}
    for k, v in (btn.get("set") or {}).items():
        q = keys.get(k)
        if q and q["type"] not in READONLY and (q["type"] == "title" or fits(q, v)):
            todo[k] = v
    changed = []
    if todo:
        row, changed = update_row(row_id, todo, by)
    note = {"text": btn["comment"], "agent": btn.get("agent")} if btn.get("comment") else None
    return row, changed, note


def fits(prop: dict, v) -> bool:
    """历史里存的值放到现在的表结构下还认不认得：后来改了类型、删了选项就不认得了（改表结构不给行记版本，旧版本里还是原来的样子）。"""
    if v is None or prop["type"] == "title":
        return True
    if prop["type"] in COMPUTED:
        return False
    try:
        return normalize({**prop}, v, create=False) == v
    except (ValueError, TypeError, AttributeError, KeyError):
        return False


def stale_keys(db_id: int, props: dict) -> list[str]:
    """一版历史的属性里，放到现在的表结构下认不出的键（列删了、类型改了、选项删了）。"""
    db = get_db(db_id)
    keys = {p["key"]: p for p in (db or {}).get("schema") or []}
    return [k for k, v in props.items() if k not in keys or not fits(keys[k], v)]


def restore_props(row_id: int, props: dict, by: str):
    """回到某一版时（pagedoc.restore）：属性换成那一版的，改了的键记成这次改的。版本号由接下来的提交来涨。
    只动现在表结构里有的列；那一版的值放到现在认不出（后来改了类型、删了选项）的留着现在的值，删掉的列不带回来（在库的历史里撤销）。"""
    row = get_row(row_id)
    if not row:
        return
    db = get_db(row["db_id"])
    cur, meta = dict(row["props"]), dict(row["meta"])
    for p in db["schema"]:
        k, old = p["key"], props.get(p["key"])
        if p["type"] == "title" or p["type"] in COMPUTED or old == cur.get(k) or not fits(p, old):
            continue
        if old is None:
            cur.pop(k, None)
        else:
            cur[k] = old
        meta[k] = {"v": row["version"] + 1, "by": by, "ts": time.time()}
    conn.execute("update pages set props=?, props_meta=? where id=?",
                 (json.dumps(cur, ensure_ascii=False), json.dumps(meta, ensure_ascii=False), row_id))


def public_row(r: dict) -> dict:
    return {k: r[k] for k in ("id", "title", "version", "pos", "created", "updated", "created_by", "updated_by", "props", "locked")} | {
        "calc": r.get("calc") or {}}


def refs(db: dict, items: list[dict]) -> dict[int, str]:
    """这些行的关联指着的行 → 标题（网页显示关联用）。"""
    ids = []
    for p in db["schema"]:
        if p["type"] == "relation":
            for r in items:
                ids += [x for x in value_of(r, p) or [] if isinstance(x, int)]
    return titles(ids)


PAGE = 200   # 网页一次取多少行（往下滚再取）


def public(db: dict, view_id: str | None = None, limit: int | None = None, include: list[int] | None = None) -> dict:
    """网页用：库、表结构、视图、按视图筛选排序后的前 limit 行（count 是符合条件的总数，total 是库里一共几行）。
    include：刚加的行，不在前 limit 行里也带上（放在最后），免得新建的行看不见。看板多给每一列有几张（groups）。"""
    view, items = view_rows(db, view_id)
    limit = max(1, min(int(limit or PAGE), MAX_ROWS))
    shown = items[:limit]
    if include:
        have = {r["id"] for r in shown}
        shown += [r for r in items[limit:] if r["id"] in set(include) - have]
    out = {"id": db["id"], "title": db["title"], "locked": db["locked"], "version": db["version"], "schema": db["schema"],
           "views": db["views"], "view": view["id"], "total": conn.execute(
               "select count(*) from pages where parent_id=? and kind='row'", (db["id"],)).fetchone()[0],
           "count": len(items), "rows": [public_row(r) for r in shown], "refs": refs(db, shown)}
    group = next((p for p in db["schema"] if p["key"] == view.get("group_by")), None) if view.get("type") == "board" else None
    if group:
        ids = {o["id"] for o in group.get("options") or []}
        counts: dict[str, int] = {}
        for r in items:
            v = value_of(r, group)
            k = v if v in ids else ""
            counts[k] = counts.get(k, 0) + 1
        out["groups"] = counts
    return out


def find_rows(db_id: int, q: str = "", limit: int = 30) -> list[dict]:
    """按名称找行（关联的选择框用）：[{id, title}]。"""
    q = " ".join(str(q or "").split()).lower()
    out = []
    for i, title in conn.execute("select id, title from pages where parent_id=? and kind='row' order by coalesce(pos, 0), id", (db_id,)):
        if not q or q in (title or "").lower() or q.lstrip("#") == str(i):
            out.append({"id": i, "title": title or ""})
            if len(out) >= limit:
                break
    return out


# ---------- 给模型看的文字 ----------
def schema_text(db: dict) -> str:
    parts = []
    names = {p["key"]: p["name"] for p in db["schema"]}
    for p in db["schema"]:
        t = TYPES[p["type"]]
        if p["type"] in OPTIONED:
            t += "：" + (" / ".join(o["name"] for o in p.get("options") or []) or "还没有选项")
        elif p["type"] == "number" and (p.get("format") or "number") != "number":
            t += f"（{p['format']}）"
        elif p["type"] == "formula":
            t += f"：{p.get('formula') or '还没写'}"
        elif p["type"] == "relation":
            odb = get_db(p.get("db"))
            t += f"到「{odb['title'] if odb else '已删的库'}」#{p.get('db')}" + ("，那边的关联在这边的一半，改这里改的是那边" if is_mirror(p) else "，双向" if p.get("pair") else "")
        elif p["type"] == "rollup":
            rel = next((q for q in db["schema"] if q["key"] == p.get("relation")), None)
            odb = get_db(rel["db"]) if rel else None
            tgt = next((q["name"] for q in (odb or {}).get("schema") or [] if q["key"] == p.get("target")), "?")
            t += f"：沿「{rel['name'] if rel else '?'}」取「{tgt}」{dbcalc.ROLLUPS.get(p.get('fn'), p.get('fn'))}"
        elif p["type"] == "unique_id" and p.get("prefix"):
            t += f"（{p['prefix']}）"
        elif p["type"] == "button":
            acts = [f"把{names.get(k, k)}改成{display(next((q for q in db['schema'] if q['key'] == k), {'type': 'text'}), v) or '空'}"
                    for k, v in (p.get("set") or {}).items()]
            if p.get("comment"):
                acts.append("写评论叫 Agent")
            t += "：" + ("、".join(acts) or "什么都不做") + "（只有用户能点）"
        parts.append(f"{p['name']}（{t}）")
    return "、".join(parts)


def _cell(s: str, width: int = 80) -> str:
    s = " ".join(str(s).split()).replace("|", "\\|")
    return s if len(s) <= width else s[:width - 1] + "…"


def table_text(db: dict, items: list[dict], limit: int = 50, cols: list[dict] | None = None) -> str:
    """紧凑的 Markdown 表：行号、版本、每个属性一列（cols 只要这几列，名称总在）。"""
    cols = cols or db["schema"]
    lines = ["| 行 | 版本 | " + " | ".join(_cell(p["name"], 30) for p in cols) + " |", "|" + " --- |" * (len(cols) + 2)]
    for r in items[:limit]:
        lines.append(f"| #{r['id']} | {r['version']} | " + " | ".join(_cell(display(p, value_of(r, p))) for p in cols) + " |")
    more = f"\n…还有 {len(items) - limit} 行没显示（用 db_query 的 filter / limit 缩小或调大）" if len(items) > limit else ""
    return "\n".join(lines) + more


def render_db(db_id: int, limit_chars: int = 12000) -> str:
    """read_page 读库：库的信息、表结构、第一个视图的前面几十行。"""
    db = get_db(db_id)
    if not db:
        return f"没有数据库 #{db_id}"
    view, items = view_rows(db)
    total = len(rows(db_id))
    head = (f"数据库《{db['title']}》{library.link(db_id)} · {total} 行 · {time.strftime('%Y-%m-%d', time.localtime(db['updated']))} 更新"
            + (" · 已锁定（只有用户能改，Agent 只能读）" if db["locked"] else ""))
    views = "视图：" + "、".join(f"{v['name']}（{'看板' if v['type'] == 'board' else '表格'}）" for v in db["views"])
    body = table_text(db, items, 30)
    if len(body) > limit_chars - 600:
        body = body[:limit_chars - 700] + "\n…（太长，截断了，用 db_query 只取要的列和行）"
    hint = ("（每行是一个页面：读某一行的正文用 read_page(行号)，改正文用 edit_page；改属性用 db_update（带版本），"
            "加行用 db_add，筛选、排序用 db_query，改表结构用 db_schema）")
    return f"{head}\n属性：{schema_text(db)}\n{views}\n按「{view['name']}」显示：\n{body}\n{hint}"


def row_text(row_id: int) -> str:
    """read_page 读一行：它在哪个库、每个属性的值。"""
    row = get_row(row_id)
    if not row:
        return ""
    db = get_db(row["db_id"])
    vals = [f"{p['name']}：{display(p, value_of(row, p)) or '（空）'}" for p in db["schema"] if p["type"] != "title"]
    return (f"数据库《{db['title']}》#{db['id']} 的一行" + (" · 库已锁定（只能读）" if db["locked"] else "")
            + "\n属性（改用 db_update，带这个版本号）：" + " · ".join(vals))


def conflict_text(e: Conflict, head: str = "没改") -> str:
    lines = [f"{head}：这一行现在是版本 {e.version}，你要改的属性在你读过之后被别人改了："]
    lines += [f"「{c['name']}」被「{person_name(c['by'])}」改成了：{c['current'] or '（空）'}" for c in e.items]
    lines.append(f"按现在的值决定要不要改，用 version={e.version} 再提交。")
    return "\n".join(lines)


# ---------- 工具（mcp.py 包一层发事件） ----------
_TYPE_ENUM = [k for k in TYPES if k != "title"]
_VALUES = ("值的写法：文本、网址、邮箱、电话写文字；数字写数；单选写选项名（没有的会自动加上）；多选写选项名的列表；"
           "状态只能写已有的选项名；日期写 2026-11-15 或 2026-11-15 14:00，有结束日期写 {\"start\": …, \"end\": …}；"
           "人员写名字的列表（“用户”或者 Agent 的名字）；勾选写 true / false；关联写对方库的行号或行的名称的列表；"
           "文件写网址的列表；写 null 是清空。自动算的（创建时间、公式、汇总、自动编号）和按钮不能写。")
_PROP_SPEC = {"type": "object", "properties": {
    "name": {"type": "string"},
    "type": {"type": "string", "enum": _TYPE_ENUM, "description": "、".join(f"{k} {v}" for k, v in TYPES.items() if k != "title")},
    "options": {"type": "array", "items": {"type": "string"}, "description": "单选、多选、状态的选项（按顺序）；状态不给就是 未开始 / 进行中 / 已完成"},
    "format": {"type": "string", "enum": list(FORMATS), "description": "数字（公式、汇总算出来是数时也算）的显示格式：number、comma 千分位、percent 百分比（存 50 显示 50%）、cny、usd"},
    "formula": {"type": "string", "description": '公式：prop("属性名") 取值，+ - * / % ^、== != > < >= <=、and or not、条件 ? 甲 : 乙，'
                                                 '函数 if、ifs、empty、length、concat、join、contains、replace、lower、upper、slice、toNumber、toText、round、floor、ceil、abs、'
                                                 'min、max、sum、average、now、today、dateAdd(日期, 数, "days")、dateBetween(日期1, 日期2, "days")、formatDate、year、month、day、weekday。'
                                                 '例：prop("单价") * prop("数量")、if(prop("完成"), "✓", "")、dateBetween(prop("截止"), today(), "days")'},
    "db": {"description": "关联：关联到哪个库（库的编号；self 是这个库自己）"},
    "twoway": {"type": "boolean", "description": "关联：双向（对方库里也加一个反向的关联属性，两边都能改）"},
    "pair_name": {"type": "string", "description": "关联：双向时对方那一半叫什么"},
    "relation": {"type": "string", "description": "汇总：沿哪个关联属性（名字）"},
    "target": {"type": "string", "description": "汇总：取对方库的哪个属性（名字）"},
    "fn": {"type": "string", "enum": list(dbcalc.ROLLUPS), "description": "汇总的算法：" + "、".join(f"{k} {v}" for k, v in dbcalc.ROLLUPS.items())},
    "prefix": {"type": "string", "description": "自动编号的前缀，比如 TASK-"},
    "label": {"type": "string", "description": "按钮上的字"},
    "set": {"type": "object", "description": "按钮：点了把哪些属性改成什么 {属性名: 值}"},
    "comment": {"type": "string", "description": "按钮：点了在这一行写一条评论（通知 Agent 处理，可以 @ 某个 Agent）"},
    "agent": {"type": ["integer", "string"], "description": "按钮：写了评论以后通知哪个 Agent（编号或名字；不填就按评论里的 @ 和行的作者选）"}},
    "required": ["name", "type"]}

DB_CREATE = {"type": "function", "function": {
    "name": "db_create",
    "description": "在资料库里建一个数据库（多维表：任务看板、会议列表、客户名单这类一条一行、每条带几个属性的东西），返回库的编号。"
                   "每一行也是一个页面，能写正文。网页上有表格和看板（按第一个状态或单选属性分列）两个视图。建好以后用 db_add 加行。",
    "parameters": {"type": "object", "properties": {
        "title": {"type": "string"},
        "properties": {"type": "array", "items": _PROP_SPEC,
                       "description": "名称以外的属性。不给就是：状态（未开始 / 进行中 / 已完成）、负责人（人员）、日期"},
        "title_name": {"type": "string", "description": "可选：名称这一列叫什么（默认“名称”）"},
        "parent_id": {"type": "integer", "description": "可选：放在哪个页面下面"}},
        "required": ["title"]}}}

DB_SCHEMA = {"type": "function", "function": {
    "name": "db_schema",
    "description": "改数据库的表结构：加属性、改名、改类型（能转的值会转过去，转不了的清空）、改选项（给全部选项，同名的保留，去掉的选项从各行清掉）、删属性。"
                   "按顺序做，有一个不行整批不做。删属性、改类型、去掉用着的选项会清掉各行里的值：每次改动都记进库的历史，用户能撤销，"
                   "但没让你删的属性、选项不要动，拿不准先问用户。",
    "parameters": {"type": "object", "properties": {
        "db_id": {"type": "integer"},
        "ops": {"type": "array", "items": {"type": "object", "properties": {
            "op": {"type": "string", "enum": ["add", "update", "delete"]},
            "prop": {"type": "string", "description": "update / delete：哪个属性（名字）"},
            "name": {"type": "string", "description": "add：属性名；update：新名字"},
            "type": {"type": "string", "enum": _TYPE_ENUM},
            "options": {"type": "array", "items": {"type": "string"}},
            "format": {"type": "string", "enum": list(FORMATS)},
            **{k: v for k, v in _PROP_SPEC["properties"].items() if k in ("formula", "db", "twoway", "pair_name", "relation", "target", "fn",
                                                                           "prefix", "label", "set", "comment")}},
            "required": ["op"]}}},
        "required": ["db_id", "ops"]}}}

DB_QUERY = {"type": "function", "function": {
    "name": "db_query",
    "description": "查数据库里的行：能筛选（条件都要满足）、排序、只要几列，返回表格（每行带行号和版本号，改属性用 db_update 时要带版本号）。",
    "parameters": {"type": "object", "properties": {
        "db_id": {"type": "integer"},
        "filter": {"type": "array", "items": {"type": "object", "properties": {
            "prop": {"type": "string", "description": "属性名"},
            "op": {"type": "string", "enum": list(OPS), "description": "eq 等于、neq 不等于、contains 包含（多选、人员是含有其中一个）、"
                                                                     "not_contains、gt / gte / lt / lte 比大小（日期按先后）、empty 是空的、not_empty；"
                                                                     "关联比对方行的名称；公式、汇总按算出来的值"},
            "value": {"description": "比的值，写法和 db_add 一样；日期能写 today / 今天 / 明天"}},
            "required": ["prop", "op"]}},
        "sort": {"type": "array", "items": {"type": "object", "properties": {
            "prop": {"type": "string"}, "desc": {"type": "boolean"}}, "required": ["prop"]}},
        "view": {"type": "string", "description": "可选：用这个视图（名字）的筛选和排序"},
        "props": {"type": "array", "items": {"type": "string"}, "description": "可选：只要这几列（名称总在）"},
        "limit": {"type": "integer", "description": "最多几行，默认 50，最多 200"}},
        "required": ["db_id"]}}}

DB_ADD = {"type": "function", "function": {
    "name": "db_add",
    "description": "往数据库里加行（一次最多 50 行），每行是 {title, props, content}。props 是 {属性名: 值}。" + _VALUES,
    "parameters": {"type": "object", "properties": {
        "db_id": {"type": "integer"},
        "rows": {"type": "array", "items": {"type": "object", "properties": {
            "title": {"type": "string", "description": "名称"},
            "props": {"type": "object", "description": "{属性名: 值}"},
            "content": {"type": "string", "description": "可选：这一行的正文（Markdown）"}}}}},
        "required": ["db_id", "rows"]}}}

DB_UPDATE = {"type": "function", "function": {
    "name": "db_update",
    "description": "改数据库里几行的属性（名称也能改，写在 props 里），一次最多 50 行，每行单独生效。带上 db_query / read_page 看到的版本号时，"
                   "你要改的属性在那之后被别人改了就不改那一行，告诉你现在的值。只改写到的属性，别的不动。改正文用 edit_page。" + _VALUES,
    "parameters": {"type": "object", "properties": {
        "rows": {"type": "array", "items": {"type": "object", "properties": {
            "row_id": {"type": "integer"},
            "props": {"type": "object", "description": "{属性名: 新值}"},
            "version": {"type": "integer", "description": "这一行的版本号"}},
            "required": ["row_id", "props"]}}},
        "required": ["rows"]}}}

TOOLS = (DB_CREATE, DB_SCHEMA, DB_QUERY, DB_ADD, DB_UPDATE)


def _who(who) -> tuple[str, int | None]:
    return (who, None) if isinstance(who, str) else (f"agent:{who}", None)


def _writable(db_id) -> tuple[dict | None, str]:
    db = get_db(db_id)
    if not db:
        p = library.get_page(db_id, False) if isinstance(db_id, int) and not isinstance(db_id, bool) else None
        return None, (f"#{db_id} 不是数据库" + ("（是数据库里的一行，库是 #" + str(p["parent_id"]) + "）" if p and p["kind"] == "row" else "")) if p else f"没有数据库 #{db_id}"
    if db["locked"]:
        return None, f"数据库 #{db_id}「{db['title']}」被用户锁定了，只能读"
    return db, ""


def tool_create(who, args: dict) -> tuple[str, dict | None]:
    """返回 (给模型的结果, 事件 {db_id} 或 None)。"""
    by, bot_id = _who(who)
    props = args.get("properties")
    try:
        if props is None:
            schema = default_schema()
            if args.get("title_name"):
                schema[0]["name"] = " ".join(str(args["title_name"]).split())[:NAME_LEN] or "名称"
            db = create(args.get("title") or "", by, schema=schema, parent_id=args.get("parent_id"), bot_id=bot_id)
        else:
            if not isinstance(props, list):
                return "没建：properties 要是一个列表", None
            db = create(args.get("title") or "", by, parent_id=args.get("parent_id"), bot_id=bot_id, properties=props,
                        title_name=args.get("title_name"))
    except (ValueError, TypeError, LookupError) as e:
        return f"没建：{e}", None
    return (f"已建数据库 #{db['id']}「{db['title']}」，链接 {library.link(db['id'])}。属性：{schema_text(db)}。"
            "用 db_add 加行；回复里用 Markdown 链接附上它。"), {"db_id": db["id"]}


def tool_schema(who, args: dict) -> tuple[str, dict | None]:
    by, _ = _who(who)
    db, why = _writable(args.get("db_id"))
    if not db:
        return f"没改：{why}", None
    try:
        db = change_schema(db["id"], args.get("ops"), by)
    except (ValueError, TypeError, LookupError) as e:
        return f"没改：{e}", None
    done = "；".join(change_text(c) for c in db["last_change"]) or "没有变化"
    lost = any(c.get("cleared") for c in db["last_change"])
    return (f"已改数据库 #{db['id']}「{db['title']}」的表结构：{done}。" + ("用户能在这个库的“历史”里撤销这次改动。" if lost else "")
            + f"\n现在的属性：{schema_text(db)}"), {"db_id": db["id"]}


def tool_query(args: dict) -> str:
    db = get_db(args.get("db_id"))
    if not db:
        return _writable(args.get("db_id"))[1]
    try:
        items = rows(db["id"])
        total = len(items)
        filters, sorts = args.get("filter") or [], args.get("sort") or []
        if args.get("view"):
            view = next((v for v in db["views"] if v["name"] == str(args["view"]).strip() or v["id"] == args["view"]), None)
            if not view:
                return f"没有视图「{args['view']}」，有：{'、'.join(v['name'] for v in db['views'])}"
            filters, sorts = list(view.get("filter") or []) + list(filters), list(sorts) or list(view.get("sort") or [])
        if not isinstance(filters, list) or not isinstance(sorts, list):
            return "没查：filter 和 sort 要是列表"
        items = query(db, items, filters, sorts)
        cols = None
        if args.get("props"):
            want = [prop_of(db, x) for x in args["props"]]
            if None in want:
                return f"没查：没有属性「{args['props'][want.index(None)]}」，有：{'、'.join(p['name'] for p in db['schema'])}"
            cols = [db["schema"][0]] + [p for p in want if p["type"] != "title"]
    except (ValueError, TypeError) as e:
        return f"没查：{e}"
    limit = args.get("limit") if isinstance(args.get("limit"), int) and not isinstance(args.get("limit"), bool) else 50
    limit = max(1, min(200, limit))
    head = f"数据库 #{db['id']}「{db['title']}」共 {total} 行" + (f"，符合条件的 {len(items)} 行" if filters else "") + (
        "（已锁定，只能读）" if db["locked"] else "")
    if not items:
        return head + "，没有符合条件的行。" if total else head + "，还没有行。"
    return f"{head}：\n{table_text(db, items, limit, cols)}"


def tool_add(who, args: dict) -> tuple[str, dict | None]:
    by, bot_id = _who(who)
    db, why = _writable(args.get("db_id"))
    if not db:
        return f"没加：{why}", None
    items = args.get("rows")
    if not isinstance(items, list) or not items:
        return "没加：rows 要是一个列表，每个是 {title, props, content}", None
    if len(items) > 50:
        return "没加：一次最多 50 行", None
    made, errors = [], []
    for k, it in enumerate(items, 1):
        if not isinstance(it, dict):
            errors.append(f"第 {k} 行：要是一个对象")
            continue
        try:
            r = add_row(db["id"], it.get("props") or {}, by, str(it.get("title") or ""), str(it.get("content") or ""), bot_id)
            made.append(r)
        except (ValueError, TypeError, LookupError) as e:
            errors.append(f"第 {k} 行没加：{e}")
    out = []
    if made:
        out.append(f"已在数据库 #{db['id']}「{db['title']}」加了 {len(made)} 行：" + "、".join(f"#{r['id']} {r['title']}" for r in made) + "（版本都是 1）")
    out += errors
    return "\n".join(out), ({"db_id": db["id"], "rows": [r["id"] for r in made]} if made else None)


def tool_update(who, args: dict) -> tuple[str, dict | None]:
    by, _ = _who(who)
    items = args.get("rows")
    if not isinstance(items, list) or not items:
        return "没改：rows 要是一个列表，每个是 {row_id, props, version}", None
    if len(items) > 50:
        return "没改：一次最多 50 行", None
    out, touched, dbs_ = [], [], set()
    for it in items:
        rid = it.get("row_id") if isinstance(it, dict) else None
        row = get_row(rid)
        if not row:
            out.append(f"#{rid}：没有这一行（行号是 db_query 里的 #数字）")
            continue
        db, why = _writable(row["db_id"])
        if not db:
            out.append(f"#{rid}：没改，{why}")
            continue
        if row["locked"]:
            out.append(f"#{rid}：没改，这一行被用户锁定了")
            continue
        try:
            new, changed = update_row(rid, it.get("props") or {}, by, it.get("version"))
        except Conflict as e:
            out.append(f"#{rid}：" + conflict_text(e))
            continue
        except (ValueError, TypeError, LookupError) as e:
            out.append(f"#{rid}：没改，{e}")
            continue
        if changed:
            touched.append(rid)
            dbs_.add(row["db_id"])
            keys = {p["key"]: p["name"] for p in db["schema"]}
            out.append(f"#{rid}「{new['title']}」改了 {'、'.join(keys.get(k, k) for k in changed)}，现在是版本 {new['version']}")
        else:
            out.append(f"#{rid}「{new['title']}」没有变化（版本 {new['version']}）")
    ev = {"db_id": next(iter(dbs_)), "rows": touched} if len(dbs_) == 1 else {"db_ids": sorted(dbs_), "rows": touched} if dbs_ else None
    return "\n".join(out), ev


def index_text(pid: int) -> str:
    """搜索索引用的文字：行是“属性名：值”，库是它的属性名。"""
    row = get_row(pid)
    if row:
        db = get_db(row["db_id"])
        return "\n".join(f"{p['name']}：{v}" for p in db["schema"] if p["type"] != "title" and p["type"] not in COMPUTED
                         and p["type"] not in ("button", "formula", "rollup") and not is_mirror(p) and (v := display(p, value_of(row, p))))
    db = get_db(pid)
    return ("数据库，属性：" + "、".join(p["name"] for p in db["schema"])) if db else ""


async def announce(ev: dict | None, by: str, created: bool = False):
    """工具改了库以后发事件：开着的库重新取（db_changed），新建的库、行的标题变了资料库列表也刷新（pages_changed）。"""
    if not ev:
        return
    from . import events
    done = set()
    for db_id in ev.get("db_ids") or [ev["db_id"]]:
        await events.emit("db_changed", db_id=db_id, rows=ev.get("rows") or [], by=by)
        done.add(db_id)
    for db_id in list(done):   # 关联着的库：汇总、反向关联跟着变
        for other in related_dbs(db_id) - done:
            await events.emit("db_changed", db_id=other, rows=[], by=by)
            done.add(other)
    await events.emit("pages_changed")


def share_markdown(db_id: int) -> str:
    """分享出去的库：第一个视图筛选、排序后的表格（隐藏的列不显示）。"""
    db = get_db(db_id)
    if not db:
        return ""
    view, items = view_rows(db)
    cols = [p for p in db["schema"] if p["key"] not in (view.get("hidden") or [])]

    def cell(s: str) -> str:
        return " ".join(str(s).split()).replace("|", "\\|") or " "
    lines = ["| " + " | ".join(cell(p["name"]) for p in cols) + " |", "|" + " --- |" * len(cols)]
    lines += ["| " + " | ".join(cell(display(p, value_of(r, p))) for p in cols) + " |" for r in items]
    return "\n".join(lines) if items else "\n".join(lines) + "\n\n（还没有行）"
