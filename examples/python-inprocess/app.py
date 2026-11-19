"""FastAPI 宿主：Open Agent Docs 在同一个进程里跑（open_agent_docs.start），宿主用 header 登录告诉它是谁，
宿主自己的接口（/notes）经 httpx 调 Open Agent Docs 的接口。

    uvicorn app:app --port 3002
"""
import os
from contextlib import asynccontextmanager

import httpx
from fastapi import FastAPI
from pydantic import BaseModel

import open_agent_docs as oad

SECRET = os.environ.get("OAD_HEADER_SECRET", "demo-secret-change-me")
DATA_DIR = os.environ.get("DATA_DIR", "./data-python-inprocess")


def oad_headers(user_id: str, name: str) -> dict[str, str]:
    """宿主知道用户是谁：带上 X-OAD-Secret 和 X-OAD-User-*（名字百分号编码，中文也行）。"""
    from urllib.parse import quote

    return {"x-oad-secret": SECRET, "x-oad-user-id": user_id, "x-oad-user-name": quote(name)}


@asynccontextmanager
async def lifespan(app: FastAPI):
    server = oad.start(data_dir=DATA_DIR, auth="header", header_secret=SECRET)
    app.state.oad = server
    app.state.http = httpx.AsyncClient(base_url=f"{server.url}/api/v1", headers=oad_headers("demo-user", "演示用户"))
    try:
        yield
    finally:
        await app.state.http.aclose()
        server.close()


app = FastAPI(lifespan=lifespan)


@app.get("/host")
async def host():
    return {"message": "宿主自己的接口", "docs": app.state.oad.url}


class Note(BaseModel):
    title: str
    text: str


@app.post("/notes")
async def create_note(note: Note):
    """建一个页面，正文写 Markdown，返回它的 id 和网页地址。"""
    http: httpx.AsyncClient = app.state.http
    me = (await http.get("/me")).raise_for_status().json()
    ws = me["workspaces"][0]["id"]
    page = (await http.post(f"/workspaces/{ws}/pages", json={"title": note.title})).raise_for_status().json()
    (await http.put(f"/pages/{page['id']}/content", json={"markdown": note.text})).raise_for_status()
    return {"id": page["id"], "url": f"{app.state.oad.url}/p/{page['id']}"}
