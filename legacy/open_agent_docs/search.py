"""搜页面：SQLite FTS5 关键词检索，不需要任何额外服务。

中文没有空格，入库前把每段中文切成相邻两个字一组（“今天天气” → 今天 天天 天气），英文、数字按词，统一小写；BM25 排序。
索引不靠到处埋钩子：每次检索前 sync() 比一遍页面的 (id, updated)，变了的重新切块、分词，删了的去掉。
页面按段切成约 600 字一块，最多索引前 200 块。数据库的行把“属性名：值”也放进去。
"""
import re
from .db import conn

conn.execute("""create table if not exists search_docs(
  id integer primary key, ref integer, chunk integer, src_updated real, title text default '', text text)""")
conn.execute("create index if not exists search_docs_ref on search_docs(ref)")
conn.execute("create virtual table if not exists search_fts using fts5(tokens)")
conn.commit()

CHUNK = 600
MAX_CHUNKS = 200
_CJK = re.compile(r"[㐀-䶿一-鿿豈-﫿]+|[a-z0-9_]+")


def tokens(text: str) -> list[str]:
    out = []
    for m in _CJK.finditer((text or "").lower()):
        s = m[0]
        if s[0] > "　":
            out += [s[i:i + 2] for i in range(len(s) - 1)] or [s]
        else:
            out.append(s)
    return out


def _match(q: str) -> str:
    """问句 → FTS5 查询：每个词加引号，用 OR 连起来。"""
    return " OR ".join(f'"{t}"' for t in dict.fromkeys(tokens(q)))


def _plain(content: str, fmt: str) -> str:
    if fmt == "html":
        content = re.sub(r"(?is)<(script|style)\b.*?</\1>", " ", content)
        content = re.sub(r"<[^>]+>", " ", content)
        content = re.sub(r"&nbsp;|&#160;", " ", content)
    return content


def chunks(text: str) -> list[str]:
    """按空行分段，凑到 CHUNK 字左右一块；太长的段硬切。"""
    out, cur = [], ""
    for para in re.split(r"\n\s*\n", text or ""):
        para = " ".join(para.split())
        if not para:
            continue
        while len(para) > CHUNK * 2:
            out.append(para[:CHUNK])
            para = para[CHUNK:]
        if cur and len(cur) + len(para) > CHUNK:
            out.append(cur)
            cur = ""
        cur = f"{cur} {para}".strip()
        if len(out) >= MAX_CHUNKS:
            break
    if cur and len(out) < MAX_CHUNKS:
        out.append(cur)
    return out[:MAX_CHUNKS]


def _drop(ids: list[int]):
    for i in ids:
        conn.execute("delete from search_fts where rowid=?", (i,))
    conn.executemany("delete from search_docs where id=?", [(i,) for i in ids])


def _add(ref: int, updated: float, title: str, parts: list[str]):
    for n, text in enumerate(parts):
        did = conn.execute("insert into search_docs(ref, chunk, src_updated, title, text) values(?,?,?,?,?)",
                           (ref, n, updated, title, text)).lastrowid
        conn.execute("insert into search_fts(rowid, tokens) values(?, ?)", (did, " ".join(tokens(f"{title} {text}"))))


def sync() -> int:
    """让索引跟上页面：返回改动了多少个页面。"""
    have: dict[int, tuple] = {}
    for ref, upd, did in conn.execute("select ref, src_updated, id from search_docs"):
        u, ids = have.get(ref, (upd, []))
        have[ref] = (u, ids + [did])
    src = {i: u for i, u in conn.execute("select id, updated from pages")}
    changed = 0
    for ref, (u, ids) in have.items():
        if ref not in src or src[ref] != u:
            _drop(ids)
            changed += 1
    for ref, u in src.items():
        if ref in have and have[ref][0] == u:
            continue
        r = conn.execute("select title, format, content, coalesce(kind, 'page') from pages where id=?", (ref,)).fetchone()
        if r:
            text = _plain(r[2] or "", r[1])
            if r[3] in ("db", "row"):   # 数据库：属性的文字也进索引（行的属性值、库的属性名）
                from . import dbs
                text = dbs.index_text(ref) + ("\n\n" + text if text else "")
            _add(ref, u, r[0] or "", chunks(text) or [r[0] or ""])
        changed += 1
    if changed:
        conn.commit()
    return changed


async def search(q: str, limit: int = 8) -> list[dict]:
    """返回 [{ref, title, snippet, score}]，同一个页面只出一次（取最相关的那块）。"""
    q = (q or "").strip()
    m = _match(q)
    if not m:
        return []
    sync()
    rows = conn.execute("select d.id, d.ref, d.title, d.text, bm25(search_fts) from search_fts f join search_docs d on d.id = f.rowid "
                        "where search_fts match ? order by bm25(search_fts) limit 200", (m,)).fetchall()
    out, seen = [], set()
    for _, ref, title, text, score in rows:
        if ref in seen:
            continue
        seen.add(ref)
        out.append({"kind": "page", "ref": ref, "title": title, "snippet": snippet(text, q), "score": round(-score, 4)})
        if len(out) >= limit:
            break
    return out


def snippet(text: str, q: str, width: int = 120) -> str:
    """命中词附近的一段。"""
    low = text.lower()
    pos = min((p for t in tokens(q) if (p := low.find(t)) >= 0), default=-1)
    if pos < 0 or len(text) <= width:
        return text[:width] + ("…" if len(text) > width else "")
    start = max(0, pos - width // 3)
    return ("…" if start else "") + text[start:start + width] + ("…" if start + width < len(text) else "")
