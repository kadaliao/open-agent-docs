"""翻页：逐条取完、停止条件（游标空、这页空、游标没变）、响应里别的字段、异步。"""
from __future__ import annotations

from typing import Dict, List, Optional

import httpx
import pytest

import open_agent_docs as oad
from oad_test_helpers import make_async_client, make_client


def version(i: int) -> Dict[str, object]:
    return {"id": i, "page_id": "p1", "kind": "auto", "created_at": i, "authors": []}


def pager(pages: Dict[Optional[str], Dict[str, object]]):
    def h(r: httpx.Request) -> httpx.Response:
        return httpx.Response(200, json=pages[r.url.params.get("cursor")])

    return h


PAGES = {
    None: {"versions": [version(5), version(4)], "next_cursor": "4"},
    "4": {"versions": [version(3), version(2)], "next_cursor": "2"},
    "2": {"versions": [version(1)], "next_cursor": None},
}


def test_iterate_all_pages() -> None:
    c, rec = make_client(pager(PAGES))
    page = c.pages.versions.list("p1", limit=2)
    assert isinstance(page, oad.CursorPage)
    assert [v.id for v in page.items] == [5, 4]
    assert page.next_cursor == "4" and page.has_next_page()
    assert [v.id for v in page] == [5, 4, 3, 2, 1]
    assert len(rec.requests) == 3
    assert [r.url.params.get("cursor") for r in rec.requests] == [None, "4", "2"]
    assert all(r.url.params["limit"] == "2" for r in rec.requests)
    second = page.next_page()
    assert [v.id for v in second.items] == [3, 2]
    assert [len(p.items) for p in page.iter_pages()] == [2, 2, 1]


def test_stop_on_empty_page_and_same_cursor() -> None:
    # 这页空了但还给了游标：停
    c, rec = make_client(pager({None: {"versions": [version(1)], "next_cursor": "a"}, "a": {"versions": [], "next_cursor": "b"}}))
    assert [v.id for v in c.pages.versions.list("p1")] == [1]
    assert len(rec.requests) == 2
    # 游标没变（服务器的毛病）：停，不死循环
    c, rec = make_client(pager({None: {"versions": [version(2)], "next_cursor": "x"}, "x": {"versions": [version(1)], "next_cursor": "x"}}))
    assert [v.id for v in c.pages.versions.list("p1")] == [2, 1]
    assert len(rec.requests) == 2
    page = c.pages.versions.list("p1", cursor="x")
    assert not page.has_next_page()
    with pytest.raises(oad.OADError):
        page.next_page()


def test_other_response_fields() -> None:
    inbox = {"notifications": [{"id": 1, "workspace_id": "w", "type": "mention", "data": {}, "created_at": 1}], "unread": 7}
    c, _ = make_client(lambda r: httpx.Response(200, json=inbox))
    page = c.me.notifications.list()
    assert page.unread == 7
    assert page.response.unread == 7  # type: ignore[attr-defined]
    assert page.next_cursor is None and not page.has_next_page()
    assert [n.id for n in page] == [1]


async def test_async_pagination() -> None:
    c, rec = make_async_client(pager(PAGES))
    got: List[int] = []
    async for v in c.pages.versions.list("p1", limit=2):
        got.append(v.id)
    assert got == [5, 4, 3, 2, 1]
    assert len(rec.requests) == 3
    page = await c.pages.versions.list("p1")
    assert isinstance(page, oad.AsyncCursorPage)
    assert [v.id for v in page.items] == [5, 4]
    nxt = await page.next_page()
    assert [v.id for v in nxt.items] == [3, 2]
    got = [v.id async for v in nxt]
    assert got == [3, 2, 1]
    await c.close()
