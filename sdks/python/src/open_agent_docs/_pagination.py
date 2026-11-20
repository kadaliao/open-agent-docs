"""翻页：有 next_cursor 的列表。

```python
page = client.pages.versions.list(pid, limit=20)
page.items              # 这一页
for v in page: ...      # 从这页起逐条取完所有页（自动带 cursor）
page.unread             # 响应里别的字段也能取（收件箱的 unread）

async for v in aclient.pages.versions.list(pid): ...   # 异步：直接 async for，或者 await 拿一页
```

停的条件和 JS 的 paginate 一样：next_cursor 是空、这页没有条目、或者游标没变。
"""
from __future__ import annotations

from typing import Any, AsyncIterator, Awaitable, Callable, Generator, Generic, Iterator, List, Optional, TypeVar

from ._errors import OADError
from ._models import BaseModel

__all__ = ["CursorPage", "AsyncCursorPage", "AsyncPaginator"]

_T = TypeVar("_T")


class _PageBase(Generic[_T]):
    items: List[_T]
    """这一页的条目"""
    next_cursor: Optional[str]
    """下一页的游标（没有就是最后一页）"""
    response: BaseModel
    """整个响应（items 以外的字段在这里，也可以直接 page.<字段> 取）"""

    def __init__(self, *, items: List[_T], next_cursor: Optional[str], response: BaseModel, cursor: Optional[str]) -> None:
        self.items = items
        self.next_cursor = next_cursor
        self.response = response
        self._cursor = cursor

    def has_next_page(self) -> bool:
        return bool(self.next_cursor) and bool(self.items) and self.next_cursor != self._cursor

    def __getattr__(self, name: str) -> Any:
        if name.startswith("_"):
            raise AttributeError(name)
        return getattr(self.response, name)

    def __repr__(self) -> str:
        return f"<{type(self).__name__} items={len(self.items)} next_cursor={self.next_cursor!r}>"


class CursorPage(_PageBase[_T]):
    """一页。`for x in page` 逐条取完所有页；`.items` 只是这一页。"""

    def __init__(
        self,
        *,
        items: List[_T],
        next_cursor: Optional[str],
        response: BaseModel,
        cursor: Optional[str],
        fetch: Callable[[str], "CursorPage[_T]"],
    ) -> None:
        super().__init__(items=items, next_cursor=next_cursor, response=response, cursor=cursor)
        self._fetch = fetch

    def next_page(self) -> "CursorPage[_T]":
        if not self.has_next_page() or self.next_cursor is None:
            raise OADError("没有下一页了（先看 has_next_page()）")
        return self._fetch(self.next_cursor)

    def iter_pages(self) -> Iterator["CursorPage[_T]"]:
        """从这页起一页一页地取"""
        page = self
        while True:
            yield page
            if not page.has_next_page():
                return
            page = page.next_page()

    def __iter__(self) -> Iterator[_T]:
        for page in self.iter_pages():
            yield from page.items


class AsyncCursorPage(_PageBase[_T]):
    """异步的一页。`async for x in page` 逐条取完所有页。"""

    def __init__(
        self,
        *,
        items: List[_T],
        next_cursor: Optional[str],
        response: BaseModel,
        cursor: Optional[str],
        fetch: Callable[[str], Awaitable["AsyncCursorPage[_T]"]],
    ) -> None:
        super().__init__(items=items, next_cursor=next_cursor, response=response, cursor=cursor)
        self._fetch = fetch

    async def next_page(self) -> "AsyncCursorPage[_T]":
        if not self.has_next_page() or self.next_cursor is None:
            raise OADError("没有下一页了（先看 has_next_page()）")
        return await self._fetch(self.next_cursor)

    async def iter_pages(self) -> AsyncIterator["AsyncCursorPage[_T]"]:
        page = self
        while True:
            yield page
            if not page.has_next_page():
                return
            page = await page.next_page()

    async def __aiter__(self) -> AsyncIterator[_T]:
        async for page in self.iter_pages():
            for x in page.items:
                yield x


class AsyncPaginator(Generic[_T]):
    """异步列表方法的返回：`await` 它拿第一页（AsyncCursorPage），`async for` 它逐条取完所有页。"""

    def __init__(self, first: Callable[[], Awaitable[AsyncCursorPage[_T]]]) -> None:
        self._first = first

    def __await__(self) -> Generator[Any, None, AsyncCursorPage[_T]]:
        return self._first().__await__()

    async def __aiter__(self) -> AsyncIterator[_T]:
        page = await self._first()
        async for x in page:
            yield x
