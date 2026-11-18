"""HTTP 服务：网页用的接口（/api/…，登录后用）、Agent 的 MCP 端点（/mcp）和通知接口（/api/agent/…，Bearer token）、网页本身。

鉴权（guard 中间件）：
- 公开的：/api/bootstrap、/api/login、/api/shared/…（分享出去的页面）、/healthz。
- /mcp、/api/agent/…：Agent 的 Bearer token（mcpserver.py / 这里的 _agent）。
- /api/agents…：登录的用户，或者 admin 的 token（宿主替它的 Bot 建 Agent）。
- /api/media：登录的用户，或者任何 Agent 的 token（传图要能写的）。
- 别的 /api/…：登录的用户。
跨站的写请求（Origin 和 Host 对不上）一律拒掉；登录 cookie 是 SameSite=Lax。
"""
import asyncio
import contextlib
import json
import re
from pathlib import Path
from urllib.parse import quote, urlparse

from fastapi import Body, FastAPI, HTTPException, Request
from fastapi.responses import FileResponse, HTMLResponse, JSONResponse, Response, StreamingResponse

from . import agents, auth, dbs, events, i18n, library, media, mcpserver, notify, pagecomments, pagedoc, search
from .config import SECURE_COOKIE, VERSION

STATIC = Path(__file__).parent / "static"
PUBLIC = {"/api/bootstrap", "/api/login", "/healthz"}
PING = 20


@contextlib.asynccontextmanager
async def lifespan(app):
    pw = auth.ensure_password()
    if pw:
        print(f"\n  Open Agent Docs：第一次启动，网页登录密码是 {pw}\n  （只显示这一次；改密码用 open-agent-docs password）\n", flush=True)
    task = asyncio.create_task(_cleanup())
    yield
    task.cancel()


async def _cleanup():
    while True:
        try:
            library.purge_trash()
            media.gc()
            notify.purge()
        except Exception as e:
            print(f"清理出错：{e}", flush=True)
        await asyncio.sleep(86400)


app = FastAPI(title="Open Agent Docs", version=VERSION, lifespan=lifespan, docs_url=None, redoc_url=None, openapi_url=None)


def _err(e: Exception):
    raise HTTPException(400, str(e))


def _bearer(request: Request) -> dict | None:
    kind, _, token = (request.headers.get("authorization") or "").partition(" ")
    return agents.check(token.strip()) if kind.lower() == "bearer" and token.strip() else None


def _deny(status: int, detail: str) -> JSONResponse:
    return JSONResponse({"detail": detail}, status, headers={"www-authenticate": 'Bearer realm="open-agent-docs"'} if status == 401 else None)


@app.middleware("http")
async def guard(request: Request, call_next):
    path = request.url.path
    if request.method not in ("GET", "HEAD", "OPTIONS") and (origin := request.headers.get("origin")) and origin != "null":
        if urlparse(origin).netloc != request.headers.get("host"):
            return _deny(403, i18n.t("不接受别的网站发来的请求"))
    request.state.agent = None
    if path == mcpserver.PATH or path.startswith(mcpserver.PATH + "/") or not path.startswith("/api/") or path in PUBLIC \
            or path.startswith("/api/shared/"):
        return await call_next(request)
    if auth.check_session(request.cookies.get(auth.COOKIE)):
        return await call_next(request)
    a = _bearer(request)
    if path.startswith("/api/agent/"):
        if not a:
            return _deny(401, i18n.t("token 不对或者已经撤销了：在 设置 → Agent 里新建一个"))
    elif path == "/api/agents" or path.startswith("/api/agents/"):
        if not a or a["scope"] != "admin":
            return _deny(401 if not a else 403, i18n.t("要登录，或者用 admin 权限的 token"))
    elif path == "/api/media" or path.startswith("/api/media/"):
        if not a or (request.method != "GET" and a["scope"] == "read"):
            return _deny(401 if not a else 403, i18n.t("要登录，或者带上能写的 token"))
    else:
        return _deny(401, i18n.t("没有登录"))
    request.state.agent = a
    return await call_next(request)


def _by(request: Request) -> str:
    a = request.state.agent
    return f"agent:{a['id']}" if a else "user"


# ---------- 登录、设置 ----------
@app.get("/healthz")
async def healthz():
    return {"ok": True, "version": VERSION}


@app.get("/api/bootstrap")
async def bootstrap(request: Request):
    authed = auth.check_session(request.cookies.get(auth.COOKIE))
    lang = i18n.chosen() or i18n.from_header(request.headers.get("accept-language"))
    return {"version": VERSION, "authed": authed, "lang": lang, "language_set": bool(i18n.chosen()),
            **({"agents": agents.names()} if authed else {})}


@app.post("/api/login")
async def login(data: dict = Body(...)):
    if not auth.check_password(data.get("password")):
        await asyncio.sleep(1)
        raise HTTPException(401, i18n.t("密码不对"))
    r = JSONResponse({"ok": True})
    r.set_cookie(auth.COOKIE, auth.new_session(), max_age=auth.SESSION_DAYS * 86400, httponly=True, samesite="lax",
                 secure=SECURE_COOKIE, path="/")
    return r


@app.post("/api/logout")
async def logout(request: Request):
    auth.end_session(request.cookies.get(auth.COOKIE))
    r = JSONResponse({"ok": True})
    r.delete_cookie(auth.COOKIE, path="/")
    return r


@app.post("/api/password")
async def change_password(request: Request, data: dict = Body(...)):
    if not auth.check_password(data.get("old")):
        raise HTTPException(400, i18n.t("原来的密码不对"))
    try:
        auth.set_password(str(data.get("new") or ""))
    except ValueError:
        raise HTTPException(400, i18n.t("密码至少 8 个字符"))
    r = JSONResponse({"ok": True})   # 别的会话都失效了，这一个重新发
    r.set_cookie(auth.COOKIE, auth.new_session(), max_age=auth.SESSION_DAYS * 86400, httponly=True, samesite="lax",
                 secure=SECURE_COOKIE, path="/")
    return r


@app.put("/api/settings")
async def put_settings(data: dict = Body(...)):
    if "language" in data:
        try:
            i18n.set_lang(data["language"])
        except ValueError as e:
            _err(e)
    return {"lang": i18n.lang()}


@app.get("/api/events")
async def event_stream(request: Request):
    """网页实时刷新（SSE）：事件见 events.py。"""
    q = events.subscribe()

    async def gen():
        try:
            yield "retry: 3000\n\n"
            while True:
                try:
                    msg = await asyncio.wait_for(q.get(), PING)
                except asyncio.TimeoutError:
                    yield ": ping\n\n"
                    continue
                if msg is None:
                    return
                yield f"data: {msg}\n\n"
        finally:
            events.unsubscribe(q)
    return StreamingResponse(gen(), media_type="text/event-stream", headers={"Cache-Control": "no-store", "X-Accel-Buffering": "no"})


@app.get("/api/search")
async def search_pages(q: str = "", limit: int = 20):
    return await search.search(q, max(1, min(limit, 50)))


# ---------- 图片、文件 ----------
@app.put("/api/media")
async def upload_media(request: Request, name: str = "", file: bool = False):
    """传一张图片（请求体就是图片）。file=1：数据库的文件属性，什么文件都收。"""
    if int(request.headers.get("content-length") or 0) > media.MAX_BYTES:
        raise HTTPException(413, i18n.t("文件太大了（最多 {n} MB）", n=media.MAX_BYTES >> 20))
    data = await request.body()
    try:
        m = media.save(data, name, any_file=file)
    except ValueError as e:
        _err(e)
    return m | {"url": media.url(m["id"]), "markdown": media.image_md(m["id"], name) if m["mime"].startswith("image/") else ""}


def _byte_range(header: str | None, size: int) -> tuple[int, int] | None | bool:
    """Range 请求头 → (起, 止)，止也算在内。没有、看不懂、要好几段的返回 None（整个发）；超出文件的返回 False（回 416）。"""
    m = re.fullmatch(r"\s*bytes=(\d*)-(\d*)\s*", header or "")
    if not m or not (m[1] or m[2]) or (m[1] and m[2] and int(m[2]) < int(m[1])):
        return None
    if not m[1]:
        n = int(m[2])
        return (max(0, size - n), size - 1) if n and size else False
    start = int(m[1])
    if start >= size:
        return False
    return start, min(int(m[2]), size - 1) if m[2] else size - 1


def _media_response(mid: str, download: bool = False, rng: str | None = None) -> Response:
    got = media.info(mid)
    if not got:
        raise HTTPException(404)
    mime, size, name = got["mime"], got["size"], got["name"]
    download = download or not media.inline(mime)
    headers = {"Cache-Control": "private, max-age=31536000, immutable", "X-Content-Type-Options": "nosniff",
               "Content-Security-Policy": "default-src 'none'",
               "Content-Disposition": f"{'attachment' if download else 'inline'}; filename*=UTF-8''{quote(name)}"}
    if not mime.startswith("video/"):
        return Response(media.get(mid)[1], media_type=mime, headers=headers)
    headers["Accept-Ranges"] = "bytes"
    part = _byte_range(rng, size)
    if part is False:
        return Response(status_code=416, headers={"Content-Range": f"bytes */{size}", "Accept-Ranges": "bytes"})
    if part is None:
        return Response(media.read_range(mid, 0, size), media_type=mime, headers=headers)
    start, end = part
    return Response(media.read_range(mid, start, end - start + 1), status_code=206, media_type=mime,
                    headers=headers | {"Content-Range": f"bytes {start}-{end}/{size}"})


@app.get("/api/media/{mid}")
async def media_file(request: Request, mid: str, download: bool = False):
    return _media_response(mid, download, request.headers.get("range"))


# ---------- 页面 ----------
def _page_or_404(pid: int, content: bool = False) -> dict:
    p = library.get_page(pid, content)
    if not p:
        raise HTTPException(404, i18n.t("没有这个页面"))
    return p


@app.get("/api/pages")
async def list_pages():
    """页面列表（数据库的行不在这里，在 /api/dbs/<库>）。"""
    return [p for p in library.list_pages() if p["kind"] != "row"]


@app.post("/api/pages")
async def create_page(data: dict = Body(...)):
    try:
        p = library.save_page(None, data.get("title", ""), media.embed(data.get("content", "") or ""), "markdown",
                              allow_empty=True, parent_id=data.get("parent_id"))
    except (ValueError, TypeError) as e:
        _err(e)
    await events.emit("pages_changed")
    return p


@app.get("/api/pages/{pid}")
async def get_page(pid: int):
    return _page_or_404(pid, True) | library.family(pid)


@app.post("/api/pages/{pid}/move")
async def move_page(pid: int, data: dict = Body(...)):
    """{parent_id: 页面编号 | null（最上层）, after: 排在哪个页面后面 | null（最后）| 0（最前）}"""
    _page_or_404(pid)
    try:
        p = library.move(pid, data.get("parent_id"), data.get("after"))
    except (ValueError, TypeError) as e:
        _err(e)
    await events.emit("pages_changed")
    return p


@app.patch("/api/pages/{pid}")
async def update_page(pid: int, data: dict = Body(...)):
    old = _page_or_404(pid)
    p = library.update_page(pid, data.get("title"), data.get("pinned"), data.get("locked"))
    if p["version"] != old["version"]:   # 改了标题
        await events.emit("page_changed", page_id=pid, version=p["version"], by="user", title=p["title"], updated=p["updated"], changed=[])
    await _row_touched(p)
    await events.emit("pages_changed")
    return p


def _markdown_or_400(pid: int) -> dict:
    p = _page_or_404(pid)
    if p["format"] != "markdown" or p["kind"] == "db":
        raise HTTPException(400, i18n.t("只有 markdown 页面能按段改"))
    return p


@app.get("/api/pages/{pid}/blocks")
async def page_blocks(pid: int, since: int | None = None):
    """页面的段：{version, order, blocks}。给了 since 就只给那个版本之后改过的段（order 总是全的）。"""
    _markdown_or_400(pid)
    p, blks = pagedoc.load(pid)
    assert p
    return {**{k: p[k] for k in ("id", "title", "version", "locked", "bot_id", "agent_id", "updated")},
            "order": [b["id"] for b in blks],
            "blocks": [pagedoc.public(b) for b in blks if since is None or b["v"] > since]}


@app.post("/api/pages/{pid}/blocks")
async def edit_blocks(pid: int, data: dict = Body(...)):
    """按段改：{version, ops}。冲突时 409，detail 是 {version, conflicts}。"""
    _markdown_or_400(pid)
    try:
        res = pagedoc.apply(pid, data.get("version"), data.get("ops"), "user")
    except pagedoc.Conflict as e:
        raise HTTPException(409, {"version": e.version, "conflicts": e.items})
    except (ValueError, TypeError, LookupError) as e:
        _err(e)
    await events.emit("page_changed", **pagedoc.event(pid, res, "user"))
    return res


@app.get("/api/pages/{pid}/versions")
async def page_versions(pid: int, before: int | None = None, limit: int = 30):
    _page_or_404(pid)
    return pagedoc.versions(pid, before, limit)


@app.get("/api/pages/{pid}/versions/{vid}")
async def page_version(pid: int, vid: int):
    _page_or_404(pid)
    if not (v := pagedoc.version(pid, vid)):
        raise HTTPException(404, i18n.t("没有这一版"))
    return v


@app.post("/api/pages/{pid}/versions/{vid}/restore")
async def restore_page(pid: int, vid: int):
    _page_or_404(pid)
    try:
        p = pagedoc.restore(pid, vid)
    except LookupError as e:
        raise HTTPException(404, str(e))
    except ValueError as e:
        _err(e)
    await events.emit("page_changed", page_id=pid, version=p["version"], by="user", title=p["title"], updated=p["updated"], changed=[])
    await _row_touched(p)
    await events.emit("pages_changed")
    return p


@app.get("/api/pages/{pid}/comments")
async def list_page_comments(pid: int):
    _page_or_404(pid)
    return pagecomments.comments(pid)


@app.post("/api/pages/{pid}/comments")
async def add_page_comment(pid: int, data: dict = Body(...)):
    """{text, block?, quote?, reply_to?, wake?}：wake 不给是自动选（见 pagecomments.targets），false 是不通知，数字是通知这个 Agent。"""
    _page_or_404(pid)
    try:
        return await pagecomments.post(pid, str(data.get("text") or ""), data.get("block"), data.get("quote"), data.get("reply_to"), data.get("wake"))
    except LookupError as e:
        raise HTTPException(404, str(e))
    except (ValueError, TypeError) as e:
        _err(e)


@app.patch("/api/pages/{pid}/comments/{cid}")
async def update_page_comment(pid: int, cid: int, data: dict = Body(...)):
    try:
        c = pagecomments.update(pid, cid, data.get("text"), data.get("resolved"))
    except LookupError as e:
        raise HTTPException(404, str(e))
    except (ValueError, TypeError) as e:
        _err(e)
    await events.emit("page_comments", page_id=pid)
    return c


@app.delete("/api/pages/{pid}/comments/{cid}")
async def delete_page_comment(pid: int, cid: int):
    try:
        pagecomments.delete(pid, cid)
    except LookupError as e:
        raise HTTPException(404, str(e))
    await events.emit("page_comments", page_id=pid)
    return {"ok": True}


@app.post("/api/pages/{pid}/share")
async def share_page(pid: int, data: dict = Body(...)):
    _page_or_404(pid)
    p = library.share(pid, bool(data.get("on")))
    await events.emit("pages_changed")
    return p


@app.delete("/api/pages/{pid}")
async def delete_page(pid: int, cascade: bool = False):
    """删页面（先进回收站，30 天内能恢复）：子页面默认挪到上一级；cascade=true 连子页面一起删。数据库连行一起删。"""
    p = library.get_page(pid, False)
    library.delete_page(pid, cascade)
    if p:
        await _row_touched(p)
    await events.emit("pages_changed")
    return {"ok": True}


@app.get("/api/trash")
async def list_trash():
    return library.trash()


@app.post("/api/trash/{tid}/restore")
async def restore_trash(tid: int):
    try:
        p = library.restore_trash(tid)
    except LookupError as e:
        raise HTTPException(404, str(e))
    except ValueError as e:
        _err(e)
    await _row_touched(p)
    await events.emit("pages_changed")
    return p


@app.delete("/api/trash/{tid}")
async def drop_trash(tid: int):
    library.drop_trash(tid)
    return {"ok": True}


@app.delete("/api/trash")
async def empty_trash():
    library.drop_trash()
    return {"ok": True}


# ---------- 数据库（dbs.py）。用户改的不受锁定限制 ----------
async def _row_touched(p: dict, rows: list[int] | None = None):
    """数据库的行改了（标题、恢复、删除）：开着的库重新取。"""
    if p.get("kind") == "row" and p.get("parent_id") is not None:
        await events.emit("db_changed", db_id=p["parent_id"], rows=rows or [p["id"]], by="user")
        for other in dbs.related_dbs(p["parent_id"]):
            await events.emit("db_changed", db_id=other, rows=[], by="user")


def _db_or_404(db_id: int) -> dict:
    db = dbs.get_db(db_id)
    if not db:
        raise HTTPException(404, i18n.t("没有数据库 #{id}", id=db_id))
    return db


def _row_or_404(db_id: int, rid: int) -> dict:
    r = dbs.get_row(rid)
    if not r or r["db_id"] != db_id:
        raise HTTPException(404, i18n.t("没有这一行 #{id}", id=rid))
    return r


async def _db_saved(db_id: int, rows: list[int] | None = None, schema: bool = False):
    await events.emit("db_changed", db_id=db_id, rows=rows or [], schema=schema, by="user")
    for other in dbs.related_dbs(db_id):   # 关联着的库：汇总、反向关联跟着变，整个重新取
        await events.emit("db_changed", db_id=other, rows=[], schema=schema, by="user")


@app.post("/api/dbs")
async def create_db(data: dict = Body(...)):
    """新建数据库：{title, parent_id?}，带默认的属性（名称、状态、负责人、日期）和表格、看板两个视图。"""
    try:
        db = dbs.create(data.get("title") or "", "user", schema=dbs.default_schema(), parent_id=data.get("parent_id"))
    except (ValueError, TypeError) as e:
        _err(e)
    await events.emit("pages_changed")
    return dbs.public(db)


@app.get("/api/dbs/{db_id}")
async def get_db(db_id: int, view: str | None = None, limit: int | None = None, include: str = ""):
    try:
        return dbs.public(_db_or_404(db_id), view, limit, [int(x) for x in include.split(",") if x.strip().isdigit()][:50])
    except ValueError as e:
        _err(e)


@app.get("/api/dbs/{db_id}/find")
async def find_db_rows(db_id: int, q: str = ""):
    _db_or_404(db_id)
    return dbs.find_rows(db_id, q)


@app.patch("/api/dbs/{db_id}/schema")
async def change_db_schema(db_id: int, data: dict = Body(...)):
    _db_or_404(db_id)
    try:
        db = dbs.change_schema(db_id, data.get("ops"), "user")
    except (ValueError, TypeError) as e:
        _err(e)
    await _db_saved(db_id, schema=True)
    return dbs.public(db, data.get("view"), data.get("limit")) | {"last_change": db["last_change"]}


@app.get("/api/dbs/{db_id}/history")
async def db_history(db_id: int):
    _db_or_404(db_id)
    return dbs.history(db_id)


@app.post("/api/dbs/{db_id}/history/{vid}/undo")
async def db_undo(db_id: int, vid: int, data: dict = Body(default={})):
    _db_or_404(db_id)
    try:
        db = dbs.undo(db_id, vid, "user")
    except LookupError as e:
        raise HTTPException(404, str(e))
    except ValueError as e:
        _err(e)
    await _db_saved(db_id, schema=True)
    return dbs.public(db, data.get("view"), data.get("limit")) | {"last_change": db["last_change"]}


@app.put("/api/dbs/{db_id}/views")
async def set_db_views(db_id: int, data: dict = Body(...)):
    _db_or_404(db_id)
    try:
        db = dbs.set_views(db_id, data.get("views"), "user")
    except (ValueError, TypeError) as e:
        _err(e)
    await _db_saved(db_id, schema=True)
    return dbs.public(db, data.get("view"), data.get("limit"))


@app.post("/api/dbs/{db_id}/rows")
async def add_db_row(db_id: int, data: dict = Body(default={})):
    _db_or_404(db_id)
    try:
        r = dbs.add_row(db_id, data.get("props") or {}, "user", str(data.get("title") or ""), after=data.get("after"))
    except (ValueError, TypeError) as e:
        _err(e)
    await _db_saved(db_id, [r["id"]], schema=True)   # 可能顺手加了选项
    await events.emit("pages_changed")
    return dbs.public_row(r)


@app.get("/api/dbs/{db_id}/rows/{rid}")
async def get_db_row(db_id: int, rid: int):
    r = _row_or_404(db_id, rid)
    db = _db_or_404(db_id)
    return {"db": {"id": db["id"], "title": db["title"], "locked": db["locked"], "schema": db["schema"]}, "row": dbs.public_row(r),
            "refs": dbs.refs(db, [r])}


@app.post("/api/dbs/{db_id}/rows/{rid}/press/{key}")
async def press_db_button(db_id: int, rid: int, key: str):
    """点一行上的按钮：按设置改属性，设了评论就写一条（通知 Agent）。"""
    _row_or_404(db_id, rid)
    try:
        r, changed, note = dbs.press(rid, key, "user")
    except LookupError as e:
        raise HTTPException(404, str(e))
    except (ValueError, TypeError) as e:
        _err(e)
    if changed:
        await _db_saved(db_id, [rid], schema=True)
        if "title" in changed:
            await events.emit("pages_changed")
    comment = None
    if note:
        comment = await pagecomments.post(rid, note["text"], wake=note["agent"] if note.get("agent") else None)
    return {"row": dbs.public_row(r), "changed": changed, "comment": comment}


@app.patch("/api/dbs/{db_id}/rows/{rid}")
async def update_db_row(db_id: int, rid: int, data: dict = Body(...)):
    _row_or_404(db_id, rid)
    try:
        r, changed = dbs.update_row(rid, data.get("props") or {}, "user", data.get("version"))
    except dbs.Conflict as e:
        raise HTTPException(409, {"version": e.version, "conflicts": e.items})
    except (ValueError, TypeError, LookupError) as e:
        _err(e)
    if changed:
        await _db_saved(db_id, [rid], schema=True)
        if "title" in changed:
            await events.emit("page_changed", page_id=rid, version=r["version"], by="user", title=r["title"], updated=r["updated"], changed=[])
            await events.emit("pages_changed")
    return dbs.public_row(r)


@app.post("/api/dbs/{db_id}/rows/{rid}/move")
async def move_db_row(db_id: int, rid: int, data: dict = Body(...)):
    _row_or_404(db_id, rid)
    try:
        library.move(rid, db_id, data.get("after"))
    except (ValueError, TypeError) as e:
        _err(e)
    await _db_saved(db_id, [rid])
    return {"ok": True}


@app.delete("/api/dbs/{db_id}/rows/{rid}")
async def delete_db_row(db_id: int, rid: int):
    _row_or_404(db_id, rid)
    library.delete_page(rid)
    await _db_saved(db_id, [rid])
    await events.emit("pages_changed")
    return {"ok": True}


# ---------- Agent 的管理（网页的 设置 → Agent，或者 admin 的 token） ----------
def _agent_out(a: dict, pending: dict | None = None) -> dict:
    return a | {"pending": (pending if pending is not None else notify.pending_counts()).get(a["id"], 0)}


@app.get("/api/agents")
async def list_agents():
    pending = notify.pending_counts()
    return [_agent_out(a, pending) for a in agents.all_agents()]


@app.post("/api/agents")
async def create_agent(request: Request, data: dict = Body(...)):
    """{name, scope: read | write | admin, webhook?, about?}。返回记录、token 原文和 webhook 的密钥（token 只有这一次）。
    admin 的 token 不能再建 admin（只有网页上的用户能建）。"""
    scope = data.get("scope", "")
    if request.state.agent and scope == "admin":
        raise HTTPException(403, i18n.t("admin 的 Agent 只能在网页上建"))
    try:
        a, token, secret = agents.create(data.get("name", ""), scope, data.get("webhook", ""), data.get("about", ""))
    except (ValueError, TypeError) as e:
        _err(e)
    await events.emit("agents_changed", agents=agents.names())
    return _agent_out(a) | {"token": token, "secret": secret}


def _managed(request: Request, aid: int) -> dict:
    a = agents.get(aid)
    if not a or (request.state.agent and a["scope"] == "admin" and a["id"] != request.state.agent["id"]):
        raise HTTPException(404, i18n.t("没有这个 Agent"))
    return a


@app.patch("/api/agents/{aid}")
async def update_agent(request: Request, aid: int, data: dict = Body(...)):
    _managed(request, aid)
    try:
        a = agents.update(aid, data.get("name"), data.get("webhook"), data.get("about"))
    except (ValueError, TypeError) as e:
        _err(e)
    await events.emit("agents_changed", agents=agents.names())
    return _agent_out(a)


@app.get("/api/agents/{aid}/secret")
async def agent_secret(request: Request, aid: int):
    _managed(request, aid)
    return {"secret": agents.secret(aid)}


@app.post("/api/agents/{aid}/secret")
async def rotate_agent_secret(request: Request, aid: int):
    _managed(request, aid)
    return {"secret": agents.rotate_secret(aid)}


@app.delete("/api/agents/{aid}")
async def revoke_agent(request: Request, aid: int):
    _managed(request, aid)
    a = agents.revoke(aid)
    await events.emit("agents_changed", agents=agents.names())
    return _agent_out(a)


@app.post("/api/agents/{aid}/hide")
async def hide_agent(request: Request, aid: int):
    _managed(request, aid)
    try:
        agents.hide(aid)
    except ValueError as e:
        _err(e)
    return {"ok": True}


@app.get("/api/agents/{aid}/notifications")
async def agent_notifications(request: Request, aid: int, after: int = 0, pending: bool = False):
    _managed(request, aid)
    return notify.listing(aid, after, pending)


# ---------- Agent 自己（Bearer token）：通知 ----------
def _agent(request: Request) -> dict:
    a = request.state.agent or _bearer(request)
    if not a:
        raise HTTPException(401, i18n.t("要带 Agent 的 token"))
    return a


def _scope_of(a: dict, agent_id: int | None) -> int | None:
    """普通 Agent 只看自己的；admin 不给 agent_id 就是所有 Agent 的。"""
    if a["scope"] != "admin":
        return a["id"]
    return agent_id


@app.get("/api/agent/me")
async def agent_me(request: Request):
    a = _agent(request)
    return a | {"pending": notify.pending_counts().get(a["id"], 0)}


@app.get("/api/agent/notifications")
async def poll_notifications(request: Request, after: int = 0, wait: float = 0, pending: bool = False, agent_id: int | None = None):
    """编号在 after 之后的通知（从旧到新，最多 100 条）。wait：没有的话最多等这么多秒（长轮询，最多 60）。"""
    a = _agent(request)
    aid = _scope_of(a, agent_id)
    items = notify.listing(aid, after, pending)
    if items or wait <= 0:
        return items
    q = notify.listen(aid)
    try:
        with contextlib.suppress(asyncio.TimeoutError):
            await asyncio.wait_for(q.get(), min(wait, 60))
    finally:
        notify.unlisten(aid, q)
    return notify.listing(aid, after, pending)


@app.get("/api/agent/notifications/stream")
async def stream_notifications(request: Request, after: int | None = None, agent_id: int | None = None):
    """SSE：先补发游标之后的（after，或者 Last-Event-ID；都没有就是还没处理的），再实时推。每条 event: notification，id 是通知编号。"""
    a = _agent(request)
    aid = _scope_of(a, agent_id)
    last = request.headers.get("last-event-id")
    cursor = after if after is not None else int(last) if last and last.isdigit() else None
    q = notify.listen(aid)

    def fmt(n: dict) -> str:
        return f"id: {n['id']}\nevent: notification\ndata: {json.dumps(n, ensure_ascii=False)}\n\n"

    async def gen():
        try:
            yield "retry: 3000\n\n"
            sent = cursor or 0
            for n in notify.listing(aid, cursor or 0, pending=cursor is None, limit=100):
                sent = max(sent, n["id"])
                yield fmt(n)
            while True:
                try:
                    n = await asyncio.wait_for(q.get(), PING)
                except asyncio.TimeoutError:
                    yield ": ping\n\n"
                    continue
                if n["id"] > sent:
                    sent = n["id"]
                    yield fmt(n)
        finally:
            notify.unlisten(aid, q)
    return StreamingResponse(gen(), media_type="text/event-stream", headers={"Cache-Control": "no-store", "X-Accel-Buffering": "no"})


@app.post("/api/agent/notifications/ack")
async def ack_notifications(request: Request, data: dict = Body(...)):
    """{ids: [编号…]} 或 {upto: 编号}：标成处理完了（网页上的“还有几条没处理”就不算它们了）。"""
    a = _agent(request)
    return {"done": notify.ack(_scope_of(a, data.get("agent_id")), data.get("ids"), data.get("upto"))}


# ---------- 分享出去的页面（不用登录） ----------
@app.get("/api/shared/{token}")
async def shared_page(token: str):
    p = library.by_token(token)
    if not p:
        raise HTTPException(404, i18n.t("链接已失效"))
    if p["kind"] == "db":   # 分享的数据库：第一个视图的表格（只读）
        p = {**p, "content": dbs.share_markdown(p["id"])}
    return {k: p[k] for k in ("title", "format", "content", "updated")}


@app.get("/api/shared/{token}/media/{mid}")
async def shared_media(request: Request, token: str, mid: str):
    """分享出去的页面里的图片：只给这个页面正文里引用着的。"""
    p = library.by_token(token)
    if not p or p["format"] != "markdown" or mid not in media.EMBED.findall(p.get("content") or ""):
        raise HTTPException(404)
    return _media_response(mid, rng=request.headers.get("range"))


# ---------- MCP ----------
app.add_route(mcpserver.PATH, mcpserver.endpoint, include_in_schema=False)


# ---------- 网页 ----------
@app.get("/{path:path}", include_in_schema=False)
async def web(path: str):
    """网页（web/ 构建到 static/）：有这个文件就给文件，别的路径都给 index.html（前端自己按地址显示）。"""
    if path.startswith("api/"):
        raise HTTPException(404)
    f = (STATIC / path).resolve()
    if path and f.is_file() and STATIC.resolve() in f.parents:
        cache = "public, max-age=31536000, immutable" if path.startswith("assets/") else "no-cache"
        return FileResponse(f, headers={"Cache-Control": cache})
    index = STATIC / "index.html"
    if not index.is_file():
        return HTMLResponse("<p>Open Agent Docs 的网页还没构建：cd web && npm ci && npm run build</p>", 200)
    return FileResponse(index, headers={"Cache-Control": "no-cache"})
