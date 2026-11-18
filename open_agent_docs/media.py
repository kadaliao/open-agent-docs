"""图片和文件：页面里的图片、数据库文件属性里的文件。

- 存在数据库的 media 表里，备份就是拷一个文件。id 是内容的哈希，同一个文件只存一份。
  图片只收 PNG / JPEG / WebP / GIF；数据库的文件属性什么都收（不是图片、视频、PDF 的打开时一律下载）。
- 用 /api/media/<id> 看，要登录（或者 Agent 的 token）。内容不会变，可以长缓存。
- 页面正文用 ![说明](/api/media/编号) 引用。分享出去的页面经 /api/shared/<token>/media/<编号> 看（只给正文里引用着的）。
- 没有页面（现在的、历史版本、回收站）引用的放一天后清掉：刚传上来、还没写进页面的不能删。
"""
import hashlib
import posixpath
import re
import time
from . import i18n
from .db import conn

conn.execute("""create table if not exists media(
  id text primary key, mime text, name text default '', size integer, w integer default 0, h integer default 0,
  data blob, created real)""")
conn.commit()

MAX_BYTES = 10 << 20
KEEP_S = 86400
ID = re.compile(r"^[0-9a-f]{24}$")


def sniff(b: bytes) -> str | None:
    if b.startswith(b"\x89PNG\r\n\x1a\n"):
        return "image/png"
    if b.startswith(b"\xff\xd8\xff"):
        return "image/jpeg"
    if b[:4] == b"RIFF" and b[8:12] == b"WEBP":
        return "image/webp"
    if b[:6] in (b"GIF87a", b"GIF89a"):
        return "image/gif"
    if b[4:8] == b"ftyp":
        return "video/quicktime" if b[8:12] == b"qt  " else "video/mp4"
    if b.startswith(b"\x1aE\xdf\xa3"):
        return "video/webm"
    return None


def dimensions(b: bytes, mime: str) -> tuple[int, int]:
    """从文件头里读宽高，读不出来是 (0, 0)。"""
    try:
        if mime == "image/png":
            return int.from_bytes(b[16:20], "big"), int.from_bytes(b[20:24], "big")
        if mime == "image/gif":
            return int.from_bytes(b[6:8], "little"), int.from_bytes(b[8:10], "little")
        if mime == "image/webp":
            kind = b[12:16]
            if kind == b"VP8 ":
                return int.from_bytes(b[26:28], "little") & 0x3FFF, int.from_bytes(b[28:30], "little") & 0x3FFF
            if kind == b"VP8L":
                w = 1 + (((b[22] & 0x3F) << 8) | b[21])
                h = 1 + (((b[24] & 0x0F) << 10) | (b[23] << 2) | ((b[22] & 0xC0) >> 6))
                return w, h
            if kind == b"VP8X":
                return 1 + int.from_bytes(b[24:27], "little"), 1 + int.from_bytes(b[27:30], "little")
        if mime == "image/jpeg":
            i = 2
            while i + 9 < len(b):
                if b[i] != 0xFF:
                    break
                marker = b[i + 1]
                if marker == 0xFF:   # 填充字节
                    i += 1
                    continue
                if marker in (0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF):
                    return int.from_bytes(b[i + 7:i + 9], "big"), int.from_bytes(b[i + 5:i + 7], "big")
                i += 2 + int.from_bytes(b[i + 2:i + 4], "big")
    except IndexError:
        pass
    return 0, 0


def _clean_name(name: str, mime: str) -> str:
    name = posixpath.basename((name or "").replace("\\", "/")).strip().replace("\0", "")[:200]
    return name or ("file" if mime == "application/octet-stream" else "image." + mime.split("/")[1].replace("jpeg", "jpg"))


INLINE = ("image/png", "image/jpeg", "image/webp", "image/gif", "application/pdf")


def inline(mime: str) -> bool:
    """能在浏览器里直接打开的（图片、视频、PDF）；别的一律下载。"""
    return mime in INLINE or mime.startswith("video/")


def save(data: bytes, name: str = "", limit: int = MAX_BYTES, video: bool = False, any_file: bool = False) -> dict:
    """存一个文件（video=True 时也收视频），返回它的信息（id、mime、name、size、w、h）。不是支持的格式就报错。
    any_file：数据库的文件属性，什么文件都收（认不出的记成 application/octet-stream，只能下载）。"""
    if not data:
        raise ValueError(i18n.t("没收到图片") if not any_file else i18n.t("没收到文件"))
    if len(data) > limit:
        raise ValueError(i18n.t("图片太大了（最多 {n} MB）", n=limit >> 20) if not any_file else i18n.t("文件太大了（最多 {n} MB）", n=limit >> 20))
    mime = sniff(data)
    if any_file and not mime:
        mime = "application/pdf" if data.startswith(b"%PDF-") else "application/octet-stream"
    elif not mime or (mime.startswith("video/") and not video and not any_file):
        raise ValueError(i18n.t("只支持 PNG、JPG、WebP、GIF 图片"))
    mid = hashlib.sha256(data).hexdigest()[:24]
    w, h = dimensions(data, mime)
    name = _clean_name(name, mime)
    # 已经有的同一张图：刷新时间，免得刚发出去就被当成没人用的清掉
    conn.execute("""insert into media(id, mime, name, size, w, h, data, created) values(?,?,?,?,?,?,?,?)
        on conflict(id) do update set created=excluded.created""", (mid, mime, name, len(data), w, h, data, time.time()))
    conn.commit()
    return {"id": mid, "mime": mime, "name": name, "size": len(data), "w": w, "h": h}


def get(mid) -> tuple[str, bytes, str] | None:
    """(mime, 内容, 文件名)。"""
    if not isinstance(mid, str) or not ID.match(mid):
        return None
    r = conn.execute("select mime, data, name from media where id=?", (mid,)).fetchone()
    return (r[0], r[1], r[2]) if r else None


def info(mid) -> dict | None:
    if not isinstance(mid, str) or not ID.match(mid):
        return None
    r = conn.execute("select mime, name, size, w, h from media where id=?", (mid,)).fetchone()
    return {"id": mid, "mime": r[0], "name": r[1], "size": r[2], "w": r[3], "h": r[4]} if r else None


def read_range(mid: str, start: int, length: int) -> bytes:
    """视频的一段（从 0 数），不用整个读出来。"""
    r = conn.execute("select substr(data, ?, ?) from media where id=?", (start + 1, length, mid)).fetchone()
    return bytes(r[0]) if r and r[0] is not None else b""


def url(mid: str) -> str:
    return f"/api/media/{mid}"


# ---------- 页面里的图片 ----------
EMBED = re.compile(r"/api/media/([0-9a-f]{24})")
_BARE = re.compile(r"(!\[[^\]\n]*\]\()\s*(?:media:)?([0-9a-f]{24})\s*\)")


def embed(md: str) -> str:
    """Agent 写进页面的 Markdown：图片地址只写了编号的（![说明](编号)）补成 /api/media/编号；引用的图片不存在时报 ValueError。"""
    md = _BARE.sub(lambda m: f"{m.group(1)}/api/media/{m.group(2)})", md or "")
    for mid in dict.fromkeys(EMBED.findall(md)):
        if not info(mid):
            raise ValueError(i18n.t("图片 {id} 不存在（可能已经被清理了），先用 upload_image 传上来", id=mid))
    return md


def image_md(mid: str, alt: str = "") -> str:
    alt = re.sub(r"[\[\]\n\r]+", " ", alt or "").strip()[:60]
    return f"![{alt}](/api/media/{mid})"


def used() -> set[str]:
    """页面（现在的和历史版本）里引用着的，数据库的文件属性里的，回收站里的。"""
    out: set[str] = set()
    for (c,) in conn.execute("""select content from pages where content like '%/api/media/%'
                                union all select content from page_versions where content like '%/api/media/%'
                                union all select props from pages where props like '%/api/media/%'
                                union all select props from page_versions where props like '%/api/media/%'
                                union all select data from page_trash where data like '%/api/media/%'"""):   # 数据库的文件属性、回收站
        out.update(EMBED.findall(c or ""))
    return out


def gc():
    """清掉一天前存的、现在没有页面在用的图片和文件。"""
    keep = used()
    old = [r[0] for r in conn.execute("select id from media where created < ?", (time.time() - KEEP_S,)) if r[0] not in keep]
    if old:
        conn.executemany("delete from media where id=?", [(x,) for x in old])
        conn.commit()
