"""FastAPI 宿主：Open Agent Docs 在同一个进程里跑（oad.start，装了 open-agent-docs[server] 就是进程内），
宿主自己管登录，后端用 SDK 以“当前用户”的身份调 Open Agent Docs（header 登录：with_user）。

    uvicorn app:app --port 3002
    curl -X POST localhost:3002/notes -H 'x-demo-user: alice' -H 'content-type: application/json' \\
         -d '{"title": "周报", "text": "# 本周\\n\\n- 上线了 SDK"}'

这里用 x-demo-user 头冒充宿主的登录（真实的宿主从自己的会话里取用户）。
"""
import os
from contextlib import asynccontextmanager
from typing import List, Optional

from fastapi import Depends, FastAPI, Header, HTTPException, Request
from pydantic import BaseModel

import open_agent_docs as oad

SECRET = os.environ.get("OAD_HEADER_SECRET", "demo-secret-change-me")
DATA_DIR = os.environ.get("DATA_DIR", "./data-python-inprocess")

# 宿主自己的用户表（演示用）
USERS = {
    "alice": {"name": "Alice 王", "email": "alice@example.com"},
    "bob": {"name": "Bob 李", "email": "bob@example.com"},
}


@asynccontextmanager
async def lifespan(app: FastAPI):
    server = oad.start(data_dir=DATA_DIR, auth="header", header_secret=SECRET)
    app.state.oad = server
    # 一个客户端（连接池）给所有请求共用；每个请求 with_user 出一个“替这个用户调”的
    app.state.docs = oad.AsyncClient(base_url=server.url, header_secret=SECRET)
    try:
        yield
    finally:
        await app.state.docs.close()
        server.close()


app = FastAPI(lifespan=lifespan)


def current_user(x_demo_user: Optional[str] = Header(default=None)) -> str:
    """宿主的登录：认出是谁（演示：看 x-demo-user 头）"""
    if x_demo_user not in USERS:
        raise HTTPException(401, "没登录")
    return x_demo_user


def docs_for(request: Request, user_id: str = Depends(current_user)) -> oad.AsyncClient:
    """以当前用户的身份调 Open Agent Docs 的客户端（带 X-OAD-Secret、X-OAD-User-*）"""
    u = USERS[user_id]
    return request.app.state.docs.with_user(user_id, name=u["name"], email=u["email"])


@app.get("/host")
async def host():
    return {"message": "宿主自己的接口", "docs": app.state.oad.url}


class Note(BaseModel):
    title: str
    text: str


@app.post("/notes")
async def create_note(note: Note, docs: oad.AsyncClient = Depends(docs_for)):
    """建一个页面，正文写 Markdown（SDK 建完页面再写正文），返回它的 id 和网页地址。"""
    ws = await docs.default_workspace()
    page = await docs.pages.create(ws, title=note.title, markdown=note.text)
    return {"id": page.id, "created_by": page.created_by, "url": f"{app.state.oad.url}/p/{page.id}"}


@app.get("/notes")
async def list_notes(docs: oad.AsyncClient = Depends(docs_for)) -> List[dict]:
    ws = await docs.default_workspace()
    return [{"id": p.id, "title": p.title, "created_by": p.created_by} for p in await docs.pages.list(ws)]


@app.get("/notes/{page_id}")
async def get_note(page_id: str, docs: oad.AsyncClient = Depends(docs_for)):
    try:
        content = await docs.pages.content.get(page_id)
    except oad.NotFoundError:
        raise HTTPException(404, "没有这个页面") from None
    return {"id": page_id, "markdown": content.markdown}


@app.get("/me")
async def me(docs: oad.AsyncClient = Depends(docs_for)):
    """Open Agent Docs 眼里的当前用户"""
    got = await docs.me.get()
    return {"id": got.user.id, "name": got.user.name}
