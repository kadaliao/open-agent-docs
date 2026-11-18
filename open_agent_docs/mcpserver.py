"""给 Agent（Claude Code、Codex、dsh 的 Bot……）读写文档：/mcp 上的 MCP 端点（Streamable HTTP，无状态、JSON 响应）。

- 鉴权只认 Authorization: Bearer <token>（agents.py），不看登录 cookie。scope：read 只读，write / admin 能写；
  只读的 token 看不到写的工具。每个请求都重新查 token，撤销马上生效。
- 作者记成 agent:<编号>，名字是建 Agent 时起的。
- Agent 写的评论不通知别的 Agent（pagecomments.tool_comment 走 add，不走 post）：只有用户写的评论会通知。
  锁定的页面只能读、评论。
- 写了照样发 page_changed / pages_changed / page_comments，网页上开着的页面实时看到。
- 无状态：每个请求单独处理，不记会话，挂在反代后面、服务重启都不影响已经接上的 Agent。
"""
import base64
import binascii
import json
import logging
import time
from typing import Annotated, Any, Literal

from mcp.server.mcpserver import Context, MCPServer
from mcp.server.mcpserver.exceptions import ToolError
from mcp.server.streamable_http_manager import StreamableHTTPSessionManager
from mcp.server.transport_security import TransportSecuritySettings
from mcp_types import ListToolsResult, ToolAnnotations
from pydantic import BaseModel, Field

from . import agents, dbcalc, dbs, events, i18n, library, media, notify, pagecomments, pagedoc, search
from .config import PUBLIC_URL

PATH = "/mcp"
LIST_LIMIT = 300     # list_pages 最多列这么多行


# ---------- MCP 工具 ----------
INSTRUCTIONS = """这是 Open Agent Docs：用户和各个 Agent 一起写的文档（Markdown 页面，可以有子页面；还有数据库）。
你写的内容署你的名字（建 Agent 时起的），用户在网页上实时看到，也能在段落上给你写评论。

怎么用：
- 找页面用 search_pages（按意思查）或 list_pages（看目录）；读用 read_page，它会给出版本号和每段前面的段落编号 ⟦xxxx⟧。
- 改页面优先用 edit_page 按段改：只动要改的段，不会冲掉用户、Bot 刚改的地方。整页重写才用 rewrite_page。
- 都要带上 read_page 看到的版本号。你要改的段在你读过之后被别人改了，这一批都不写，结果里会告诉你它现在的内容，按新的版本号再提交就行。
- 段落编号只给 edit_page / comment_page 用，不是正文，写内容时不要带上。
- 用户在页面上的评论用 list_comments 看，用 comment_page 回复、标成已解决。你写的评论不会通知别的 Agent。
- 用户在评论里 @ 你、或者评论你写的段时会通知你：没有宿主帮你收通知的话，用 my_notifications 看有没有要你处理的评论。
- 页面里放图片：先用 upload_image 传上来，把它返回的 ![说明](/api/media/编号) 写进页面。
- 用户锁定的页面只能读和评论。
- 数据库（多维表：任务看板、会议列表这类一条一行、带属性的表）：read_page 读库会给出属性和前面的行；db_query 筛选、排序，
  db_add 加行，db_update 改属性（带行的版本号），db_schema 改表结构，db_create 建库。每一行也是页面，正文用 read_page / edit_page。"""

WRITE_TOOLS = {"create_page", "edit_page", "rewrite_page", "comment_page", "upload_image", "db_create", "db_schema", "db_add", "db_update"}
_RO = ToolAnnotations(read_only_hint=True, open_world_hint=False)
_RW = ToolAnnotations(read_only_hint=False, destructive_hint=False, open_world_hint=False)


class _Server(MCPServer):
    async def _handle_list_tools(self, ctx, params):
        res = await super()._handle_list_tools(ctx, params)
        if _scope_of(getattr(ctx, "request", None)) not in ("write", "admin"):   # 只读的 token 看不到写的工具
            res = ListToolsResult(tools=[t for t in res.tools if t.name not in WRITE_TOOLS])
        return res


def _scope_of(req) -> str:
    a = req.scope.get("oad.agent") if req is not None else None
    return a["scope"] if a else ""


def _agent(ctx: Context, write: bool = False) -> dict:
    req = ctx.request_context.request
    a = req.scope.get("oad.agent") if req is not None else None
    if not a:
        raise ToolError("没有带 token")
    if write and a["scope"] not in ("write", "admin"):
        raise ToolError("这个 token 只读，不能改文档。要写的话在网页的 设置 → Agent 里建一个可写的")
    return a


def _base(ctx: Context) -> str:
    """网页地址的开头（反代后面按 X-Forwarded-Proto 算 https），拼出能直接打开的页面链接。"""
    if PUBLIC_URL:
        return PUBLIC_URL
    req = ctx.request_context.request
    if req is None:
        return ""
    scheme = (req.headers.get("x-forwarded-proto") or req.url.scheme or "http").split(",")[0].strip()
    return f"{scheme}://{req.headers.get('host') or req.url.netloc}"


def _url(ctx: Context, pid: int) -> str:
    return _base(ctx) + library.link(pid)


def _by(a: dict) -> str:
    return f"agent:{a['id']}"


def _when(ts: float) -> str:
    return time.strftime("%Y-%m-%d %H:%M", time.localtime(ts))


def _quiet_server() -> _Server:
    """MCPServer 建的时候会 logging.basicConfig(INFO)：还原根日志器，不然所有库的 INFO 日志都会打出来。
    每个请求一行的“Terminating session”也压掉。"""
    root = logging.getLogger()
    level, handlers = root.level, list(root.handlers)
    s = _Server("open-agent-docs", title="Open Agent Docs", instructions=INSTRUCTIONS)
    root.setLevel(level)
    root.handlers[:] = handlers
    for name in ("mcp.server.streamable_http", "mcp.server.streamable_http_manager"):
        logging.getLogger(name).setLevel(logging.WARNING)
    return s


server = _quiet_server()


def _tool(description: str, read_only: bool):
    """结果只给文字（和 Bot 看到的一样），不再附一份 structuredContent：同样的内容发两遍，白占对方的上下文。"""
    return server.tool(description=description, annotations=_RO if read_only else _RW, structured_output=False)


@_tool("按关键词查页面（标题、正文、数据库行的属性），返回页面编号、标题、命中的那一小段。看全文用 read_page。", read_only=True)
async def search_pages(query: Annotated[str, Field(description="要找什么，用自然的说法；可以多写几个同义的词")],
                       ctx: Context,
                       limit: Annotated[int, Field(description="最多返回几个，默认 10", ge=1, le=30)] = 10) -> str:
    _agent(ctx)
    hits = await search.search(query, limit)
    if not hits:
        return "没找到相关的页面"
    return "\n".join(f"- #{h['ref']}《{h['title']}》{_url(ctx, h['ref'])}：{h['snippet']}" for h in hits)


@_tool("列出页面目录（缩进的是子页面），带页面编号、格式、版本号和更新时间。给 parent_id 只列那个页面下面的。", read_only=True)
async def list_pages(ctx: Context,
                     parent_id: Annotated[int | None, Field(description="可选：只列这个页面下面的子页面")] = None) -> str:
    _agent(ctx)
    pages = library.list_pages()   # 置顶的在前、新的在前：最上层就按这个顺序，和网页上的页面树一样
    by_id = {p["id"]: p for p in pages}
    kids: dict = {}
    for p in pages:
        up = p["parent_id"] if p["parent_id"] in by_id else None
        kids.setdefault(up, []).append(p)
    for k, v in kids.items():
        if k is not None:
            v.sort(key=lambda p: (p["pos"], p["id"]))
    if parent_id is not None and parent_id not in by_id:
        return f"没有页面 #{parent_id}"
    lines: list[str] = []
    total = [0]

    def walk(pid, depth: int, seen: frozenset):
        for p in kids.get(pid, []):
            total[0] += 1
            if len(lines) < LIST_LIMIT:
                extra = " · html" + (" 小应用" if p["app"] else "") if p["format"] == "html" else f" · 版本 {p['version']}"
                lines.append(f"{'  ' * depth}- #{p['id']}《{p['title']}》{extra}{' · 已锁定' if p['locked'] else ''}"
                             f" · {_when(p['updated'])} 更新")
            if p["id"] not in seen:
                walk(p["id"], depth + 1, seen | {p["id"]})

    walk(parent_id, 0, frozenset())
    if parent_id is not None:
        up = "".join(f"#{x['id']}《{x['title']}》 / " for x in library.family(parent_id)["ancestors"])
        head = f"{up}#{parent_id}《{by_id[parent_id]['title']}》下面有 {total[0]} 个页面" + ("：" if total[0] else "")
    else:
        head = f"共 {total[0]} 个页面" + ("（缩进的是子页面）：" if total[0] else "")
    more = f"\n…还有 {total[0] - len(lines)} 个没列出来，用 parent_id 分开看，或者用 search_pages 找" if total[0] > len(lines) else ""
    return head + ("\n" + "\n".join(lines) if lines else "") + more


@_tool("读一个页面（html 页面只给文字）。markdown 页面带版本号，每段前面一行是段落编号 ⟦xxxx⟧，"
       "改页面、写评论时用；末尾附上没处理的评论。页面太长时会告诉你从哪一段接着读。", read_only=True)
async def read_page(page_id: int, ctx: Context,
                    start: Annotated[str | None, Field(description="可选：从这个段落编号开始读（页面太长、上次没读完时）")] = None) -> str:
    _agent(ctx)
    return library.tool_read_page({"page_id": page_id, "start": start})


@_tool("新建一个 Markdown 页面，返回页面编号和网页链接。给 parent_id 就放在那个页面下面当子页面。", read_only=False)
async def create_page(title: str,
                      content: Annotated[str, Field(description="页面全文，Markdown")],
                      ctx: Context,
                      parent_id: Annotated[int | None, Field(description="可选：放在哪个页面下面当子页面")] = None) -> str:
    a = _agent(ctx, write=True)
    try:
        p = library.save_page(None, title, media.embed(content), "markdown", by=_by(a), parent_id=parent_id)
    except (ValueError, TypeError) as e:
        return f"没建：{e}"
    await events.emit("pages_changed")
    return (f"已建页面 #{p['id']}「{p['title']}」（版本 {p['version']}），网页上打开：{_url(ctx, p['id'])}。"
            "接着改用 read_page 看段落编号，再用 edit_page 按段改。")


class EditOp(BaseModel):
    op: Literal["replace", "insert_after", "insert_before", "delete", "append"] = Field(
        description="replace 换掉一段；insert_after / insert_before 加在某段后面 / 前面；delete 删掉一段；append 加到页面最后")
    block: str | None = Field(None, description="段落编号，比如 k7qd（append 不用填）")
    markdown: str | None = Field(None, description="新的内容，可以是好几段（delete 不用填）")


@_tool("按段改 markdown 页面：只动要改的段，不会冲掉别人刚改的地方。先用 read_page 读，拿到版本号和段落编号。"
       "一次可以改好几处，全部生效或者都不生效；要改的段在你读过之后被别人改了，会告诉你它现在的内容，按新版本号再提交就行。", read_only=False)
async def edit_page(page_id: int,
                    version: Annotated[int, Field(description="read_page 显示的版本号")],
                    ops: Annotated[list[EditOp], Field(description="按顺序执行的修改")],
                    ctx: Context) -> str:
    a = _agent(ctx, write=True)
    out, event = pagedoc.tool_edit(_by(a), {"page_id": page_id, "version": version,
                                            "ops": [o.model_dump(exclude_none=True) for o in ops]})
    if event:
        await events.emit("page_changed", **event)
        await events.emit("pages_changed")
    return out


@_tool("整页换成新的正文（markdown 页面）。没变的段保留原来的编号和作者，评论还挂着。要带 read_page 看到的版本号；"
       "会覆盖掉你读过之后别人改过的段时不写，告诉你那几段现在的内容。只改几处用 edit_page。", read_only=False)
async def rewrite_page(page_id: int,
                       version: Annotated[int, Field(description="read_page 显示的版本号")],
                       content: Annotated[str, Field(description="页面全文，Markdown（不要带段落编号）")],
                       ctx: Context,
                       title: Annotated[str | None, Field(description="可选：新标题，不给就不改")] = None) -> str:
    a = _agent(ctx, write=True)
    old = library.get_page(page_id, False)
    if not old:
        return f"没有页面 #{page_id}"
    if old["format"] != "markdown":
        return f"没保存：页面 #{page_id} 是 html 页面，只能读"
    if old["locked"]:
        return f"没保存：页面 #{page_id}「{old['title']}」被用户锁定了，只能读，有意见用 comment_page"
    try:
        p = library.save_page(None, title or old["title"], media.embed(content), "markdown", page_id, by=_by(a), base=version)
    except pagedoc.Conflict as e:
        return pagedoc.conflict_text(e, "没保存")
    except (ValueError, TypeError, LookupError) as e:
        return f"没保存：{e}"
    if p["version"] == old["version"]:
        return f"页面 #{page_id}「{p['title']}」没有变化（版本 {p['version']}）"
    await events.emit("page_changed", page_id=p["id"], version=p["version"], by=_by(a), title=p["title"],
                        updated=p["updated"], changed=[])   # changed 为空：网页上开着的整页重新拉
    await events.emit("pages_changed")
    return f"已保存页面 #{p['id']}「{p['title']}」，现在是版本 {p['version']}：{_url(ctx, p['id'])}"


@_tool("看一个页面上的评论：每串评论挂在哪一段（⟦段落编号⟧，没有就是对整页）、谁说的、回复。默认只看没解决的。", read_only=True)
async def list_comments(page_id: int, ctx: Context,
                        include_resolved: Annotated[bool, Field(description="也列出已经解决的")] = False) -> str:
    _agent(ctx)
    p = library.get_page(page_id, False)
    if not p:
        return f"没有页面 #{page_id}"
    text = pagecomments.render(page_id, 50, resolved=include_resolved)
    if text:
        return text
    if pagecomments.comments(page_id):
        return f"页面 #{page_id}「{p['title']}」上的评论都解决了（include_resolved=true 能看到）"
    return f"页面 #{page_id}「{p['title']}」上还没有评论"


@_tool("在页面上写评论、回复评论、把一串评论标成已解决。处理完用户的评论后用它回复（reply_to 填那条评论的编号），"
       "事情办完了加 resolve=true。也能对某一段（block 填段落编号）或整页提意见。你写的评论不会通知别的 Agent。", read_only=False)
async def comment_page(page_id: int, ctx: Context,
                       text: Annotated[str | None, Field(description="评论内容（只想标成已解决可以不填）")] = None,
                       block: Annotated[str | None, Field(description="可选：评论哪一段（read_page 里的段落编号）；不填就是整页")] = None,
                       reply_to: Annotated[int | None, Field(description="可选：回复哪条评论（#编号）")] = None,
                       resolve: Annotated[bool, Field(description="可选：把这串评论标成已解决（要和 reply_to 一起用）")] = False) -> str:
    a = _agent(ctx, write=True)
    out, pid = pagecomments.tool_comment(_by(a), {"page_id": page_id, "text": text, "block": block, "reply_to": reply_to,
                                                  "resolve": resolve})
    if pid:
        await events.emit("page_comments", page_id=pid)
    return out


# ---------- /mcp 入口 ----------
async def _reply(send, status: int, detail: str, headers: list | None = None):
    body = json.dumps({"detail": detail}, ensure_ascii=False).encode()
    await send({"type": "http.response.start", "status": status,
                "headers": [(b"content-type", b"application/json; charset=utf-8"), (b"content-length", str(len(body)).encode()),
                            *(headers or [])]})
    await send({"type": "http.response.body", "body": body})


class Endpoint:
    """ASGI 应用（不是函数：Starlette 的 Route 才会原样转给它，不包成普通的请求处理）。"""

    async def __call__(self, scope, receive, send):
        if scope["type"] != "http":
            return
        auth = next((v.decode("latin-1") for k, v in scope["headers"] if k == b"authorization"), "")
        kind, _, token = auth.partition(" ")
        a = agents.check(token.strip()) if kind.lower() == "bearer" else None
        if not a:
            return await _reply(send, 401, i18n.t("token 不对或者已经撤销了：在 设置 → Agent 里新建一个"),
                                [(b"www-authenticate", b'Bearer realm="open-agent-docs"')])
        # 无状态：每个请求起一个会话管理、处理完就收掉，不留常驻的任务（不用进 lifespan，测试里的 ASGITransport 也不跑 lifespan）。
        # 浏览器来的跨站请求带 Origin：server.py 的中间件已经拦了，这里不再查
        manager = StreamableHTTPSessionManager(app=server._lowlevel_server, stateless=True, json_response=True,
                                               security_settings=TransportSecuritySettings(enable_dns_rebinding_protection=False))
        async with manager.run():
            await manager.handle_request({**scope, "oad.agent": a}, receive, send)


endpoint = Endpoint()


# ---------- 数据库（dbs.py） ----------
_TypeName = Literal[tuple(dbs._TYPE_ENUM)]
_Format = Literal[tuple(dbs.FORMATS)]


_P = dbs._PROP_SPEC["properties"]


class _PropConfig(BaseModel):
    """公式、关联、汇总、自动编号、按钮自己的设置。"""
    formula: str | None = Field(None, description=_P["formula"]["description"])
    db: int | str | None = Field(None, description=_P["db"]["description"])
    twoway: bool | None = Field(None, description=_P["twoway"]["description"])
    pair_name: str | None = Field(None, description=_P["pair_name"]["description"])
    relation: str | None = Field(None, description=_P["relation"]["description"])
    target: str | None = Field(None, description=_P["target"]["description"])
    fn: Literal[tuple(dbcalc.ROLLUPS)] | None = Field(None, description=_P["fn"]["description"])
    prefix: str | None = Field(None, description=_P["prefix"]["description"])
    label: str | None = Field(None, description=_P["label"]["description"])
    set: dict[str, Any] | None = Field(None, description=_P["set"]["description"])
    comment: str | None = Field(None, description=_P["comment"]["description"])
    agent: int | str | None = Field(None, description=_P["agent"]["description"])


class DbProp(_PropConfig):
    name: str
    type: _TypeName = Field(description=dbs._PROP_SPEC["properties"]["type"]["description"])
    options: list[str] | None = Field(None, description="单选、多选、状态的选项（按顺序）；状态不给就是 未开始 / 进行中 / 已完成")
    format: _Format | None = Field(None, description=_P["format"]["description"])


class DbSchemaOp(_PropConfig):
    op: Literal["add", "update", "delete"]
    prop: str | None = Field(None, description="update / delete：哪个属性（名字）")
    name: str | None = Field(None, description="add：属性名；update：新名字")
    type: _TypeName | None = None
    options: list[str] | None = None
    format: _Format | None = None


class DbFilter(BaseModel):
    prop: str = Field(description="属性名")
    op: Literal[tuple(dbs.OPS)] = Field(description=dbs.DB_QUERY["function"]["parameters"]["properties"]["filter"]["items"]["properties"]["op"]["description"])
    value: Any = Field(None, description="比的值，写法和 db_add 一样；日期能写 today / 今天 / 明天")


class DbSort(BaseModel):
    prop: str
    desc: bool = False


class DbNewRow(BaseModel):
    title: str = Field("", description="名称")
    props: dict[str, Any] = Field(default_factory=dict, description="{属性名: 值}")
    content: str = Field("", description="可选：这一行的正文（Markdown）")


class DbRowChange(BaseModel):
    row_id: int
    props: dict[str, Any] = Field(description="{属性名: 新值}")
    version: int | None = Field(None, description="这一行的版本号")


def _desc(spec: dict) -> str:
    return spec["function"]["description"]


async def _db_write(ctx: Context, fn, args: dict) -> str:
    a = _agent(ctx, write=True)
    out, ev = fn(_by(a), args)
    await dbs.announce(ev, _by(a))
    return out


@_tool(_desc(dbs.DB_QUERY), read_only=True)
async def db_query(db_id: int, ctx: Context,
                   filter: list[DbFilter] | None = None, sort: list[DbSort] | None = None,
                   view: Annotated[str | None, Field(description="可选：用这个视图（名字）的筛选和排序")] = None,
                   props: Annotated[list[str] | None, Field(description="可选：只要这几列（名称总在）")] = None,
                   limit: Annotated[int, Field(description="最多几行，默认 50", ge=1, le=200)] = 50) -> str:
    _agent(ctx)
    return dbs.tool_query({"db_id": db_id, "filter": [f.model_dump() for f in filter or []], "sort": [x.model_dump() for x in sort or []],
                           "view": view, "props": props, "limit": limit})


@_tool(_desc(dbs.DB_CREATE), read_only=False)
async def db_create(title: str, ctx: Context,
                    properties: Annotated[list[DbProp] | None, Field(description="名称以外的属性。不给就是：状态、负责人（人员）、日期")] = None,
                    title_name: Annotated[str | None, Field(description="可选：名称这一列叫什么（默认“名称”）")] = None,
                    parent_id: Annotated[int | None, Field(description="可选：放在哪个页面下面")] = None) -> str:
    return await _db_write(ctx, dbs.tool_create, {"title": title, "title_name": title_name, "parent_id": parent_id,
                                                  "properties": None if properties is None else [x.model_dump(exclude_none=True) for x in properties]})


@_tool(_desc(dbs.DB_SCHEMA), read_only=False)
async def db_schema(db_id: int, ops: list[DbSchemaOp], ctx: Context) -> str:
    return await _db_write(ctx, dbs.tool_schema, {"db_id": db_id, "ops": [o.model_dump(exclude_none=True) for o in ops]})


@_tool(_desc(dbs.DB_ADD), read_only=False)
async def db_add(db_id: int, rows: list[DbNewRow], ctx: Context) -> str:
    return await _db_write(ctx, dbs.tool_add, {"db_id": db_id, "rows": [r.model_dump() for r in rows]})


@_tool(_desc(dbs.DB_UPDATE), read_only=False)
async def db_update(rows: list[DbRowChange], ctx: Context) -> str:
    return await _db_write(ctx, dbs.tool_update, {"rows": [r.model_dump() for r in rows]})


# ---------- 图片、通知 ----------
@_tool("传一张图片（PNG、JPG、WebP、GIF，最多 10 MB），返回能写进页面的 Markdown：![说明](/api/media/编号)。"
       "传完用 edit_page / create_page 把这一行放进页面。", read_only=False)
async def upload_image(data_base64: Annotated[str, Field(description="图片内容，base64（可以带 data:image/png;base64, 开头）")],
                       ctx: Context,
                       name: Annotated[str, Field(description="可选：文件名")] = "",
                       alt: Annotated[str, Field(description="可选：图片说明（写进 Markdown 的 ![说明]）")] = "") -> str:
    _agent(ctx, write=True)
    raw = data_base64.split(",", 1)[1] if data_base64.startswith("data:") and "," in data_base64 else data_base64
    try:
        data = base64.b64decode("".join(raw.split()), validate=True)
        m = media.save(data, name)
    except (binascii.Error, ValueError) as e:
        return f"没传上：{e}"
    return f"已传上（{m['w']}×{m['h']}，{m['size'] // 1024} KB）。写进页面：{media.image_md(m['id'], alt or name)}"


@_tool("看有没有用户在页面上要你处理的评论（@ 了你、评论了你写的段、回复了你说过话的评论串）。"
       "默认只列还没处理的，列出来就算处理过了（mark_done=false 只看不标）。有宿主替你收通知的话一般用不上。", read_only=True)
async def my_notifications(ctx: Context,
                           include_done: Annotated[bool, Field(description="也列出已经处理过的（最近 20 条）")] = False,
                           mark_done: Annotated[bool, Field(description="列出来的标成处理过了，默认 true")] = True) -> str:
    a = _agent(ctx)
    items = notify.listing(a["id"], pending=not include_done, limit=100)
    if include_done:
        items = items[-20:]
    if not items:
        return "没有要你处理的评论"
    if mark_done:
        notify.ack(a["id"], [n["id"] for n in items])
    out = [f"{len(items)} 条通知（从旧到新）："]
    for n in items:
        out.append(f"--- 通知 #{n['id']} · {_when(n['created'])}{' · 已处理' if n['done'] else ''}\n{n['prompt']}")
    return "\n\n".join(out)
