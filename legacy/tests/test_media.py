"""图片和文件（media.py）：认格式和宽高、传和取（要登录或 Agent 的 token）、视频按段取、清理。"""
import time

from conftest import asgi, png
from open_agent_docs import library, media
from open_agent_docs.db import conn

PNG = png(6, 4, (0, 128, 255))
GIF = b"GIF89a" + (12).to_bytes(2, "little") + (7).to_bytes(2, "little") + b"\x00" * 20
JPEG = (b"\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00"
        b"\xff\xc0\x00\x11\x08\x00\x64\x00\xc8\x03\x01\x22\x00\x02\x11\x01\x03\x11\x01" + b"\x00" * 20)
WEBP = b"RIFF\x00\x00\x00\x00WEBPVP8X\x0a\x00\x00\x00\x00\x00\x00\x00" + (299).to_bytes(3, "little") + (149).to_bytes(3, "little")


def test_sniff_and_dimensions():
    for data, mime, size in ((PNG, "image/png", (6, 4)), (GIF, "image/gif", (12, 7)),
                             (JPEG, "image/jpeg", (200, 100)), (WEBP, "image/webp", (300, 150))):
        assert media.sniff(data) == mime
        assert media.dimensions(data, mime) == size
    assert media.sniff(b"<svg onload=alert(1)>") is None


async def test_upload_and_serve(client):
    c = client
    r = await c.put("/api/media?name=shot.png", content=PNG)
    assert r.status_code == 200
    m = r.json()
    assert media.ID.match(m["id"]) and (m["w"], m["h"], m["mime"], m["name"]) == (6, 4, "image/png", "shot.png")
    assert m["url"] == f"/api/media/{m['id']}" and m["markdown"] == f"![shot.png](/api/media/{m['id']})"
    assert (await c.put("/api/media?name=again.png", content=PNG)).json()["id"] == m["id"]   # 同一张图只存一份
    got = await c.get(f"/api/media/{m['id']}")
    assert got.content == PNG and got.headers["content-type"] == "image/png"
    assert "immutable" in got.headers["cache-control"] and "private" in got.headers["cache-control"]
    assert got.headers["x-content-type-options"] == "nosniff"
    assert "attachment" in (await c.get(f"/api/media/{m['id']}?download=1")).headers["content-disposition"]
    assert (await c.put("/api/media", content=b"<svg onload=alert(1)></svg>")).status_code == 400
    assert (await c.get("/api/media/" + "0" * 24)).status_code == 404


async def test_video_ranges(client):
    video = b"\x00\x00\x00\x18ftypmp42" + bytes(range(256)) * 4
    mid = media.save(video, "clip.mp4", video=True)["id"]
    whole = await client.get(f"/api/media/{mid}")
    assert whole.status_code == 200 and whole.content == video and whole.headers["accept-ranges"] == "bytes"
    part = await client.get(f"/api/media/{mid}", headers={"range": "bytes=10-19"})
    assert part.status_code == 206 and part.content == video[10:20] and part.headers["content-range"] == f"bytes 10-19/{len(video)}"
    tail = await client.get(f"/api/media/{mid}", headers={"range": "bytes=-5"})
    assert tail.content == video[-5:]
    assert (await client.get(f"/api/media/{mid}", headers={"range": f"bytes={len(video)}-"})).status_code == 416
    assert (await client.put("/api/media?name=clip.mp4", content=video)).status_code == 400   # 页面里的图片不收视频


async def test_media_needs_login(make_agent):
    mid = media.save(PNG, "x.png")["id"]
    _, ro = make_agent("看图的", scope="read")
    _, rw = make_agent("传图的")
    async with asgi() as c:
        assert (await c.get(f"/api/media/{mid}")).status_code == 401
        assert (await c.put("/api/media", content=PNG)).status_code == 401
    async with asgi(headers={"authorization": f"Bearer {ro}"}) as c:   # 只读的 token 能看不能传
        assert (await c.get(f"/api/media/{mid}")).content == PNG
        assert (await c.put("/api/media", content=PNG)).status_code == 403
    async with asgi(headers={"authorization": f"Bearer {rw}"}) as c:
        assert (await c.put("/api/media?name=y.png", content=png(3, 3, (1, 1, 1)))).status_code == 200


def test_gc_keeps_referenced_and_fresh():
    used_old, free_old, fresh, trashed = (media.save(png(3, 3, c))["id"] for c in ((7, 7, 7), (8, 8, 8), (9, 9, 9), (6, 6, 6)))
    keep = library.save_page(None, "用着图", f"![a](/api/media/{used_old})")["id"]
    gone = library.save_page(None, "删了的", f"![b](/api/media/{trashed})")["id"]
    library.delete_page(gone)   # 进了回收站：图片还留着，能恢复
    for mid in (used_old, free_old, trashed):
        conn.execute("update media set created=? where id=?", (time.time() - media.KEEP_S - 10, mid))
    conn.commit()
    media.gc()
    assert media.get(used_old) and media.get(fresh) and media.get(trashed) and not media.get(free_old)
    library.delete_page(keep, trash=False)
    library.drop_trash(next(x["id"] for x in library.trash() if x["page_id"] == gone))
    media.gc()
    assert not media.get(used_old) and not media.get(trashed)
