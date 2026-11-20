"""事件流（SSE）：工作区事件（workspaces.events）和 Agent 的通知（agent.notifications.stream）。

```python
with client.workspaces.events(ws) as events:      # 进 with 就连上了：之后发生的事件不会漏
    for ev in events:                              # ev.name、ev.id、ev.data（按事件名解析成模型）
        if ev.name == "page.created":
            print(ev.data.title)

async with aclient.workspaces.events(ws) as events:
    async for ev in events: ...
```

- 标准 SSE：event、data（多行拼起来）、id；冒号开头的是心跳
- 断了自动重连：1 秒起翻倍、最多 30 秒，带 Last-Event-ID；连上后重置。401、403、404 不重连，抛错
- resync 事件照常给（意思是“落下了，重新拉一遍”）
"""
from __future__ import annotations

import asyncio
import json
import threading
from typing import Any, AsyncIterator, Awaitable, Callable, Dict, Iterator, List, Optional, cast

import httpx
import pydantic

from ._errors import APIConnectionError, APIError
from ._generated.models import EVENT_MODELS, AgentNotification

__all__ = ["Event", "EventStream", "AsyncEventStream", "SSEDecoder", "parse_event_data"]

RECONNECT_INITIAL = 1.0
"""第一次重连等几秒（之后翻倍）"""
RECONNECT_MAX = 30.0
FATAL_STATUS = (401, 403, 404)


class Event:
    """一条事件。data 按事件名解析成模型（page.created → Page……），不认识的给原始 JSON。"""

    __slots__ = ("name", "id", "data", "raw")

    def __init__(self, name: str, data: Any, id: Optional[str] = None, raw: str = "") -> None:
        self.name = name
        """事件名（event: 那一行，没有就是 message）"""
        self.data = data
        self.id = id
        """事件的 id（Agent 的流里是通知编号）"""
        self.raw = raw
        """data 的原文"""

    def __repr__(self) -> str:
        return f"Event(name={self.name!r}, id={self.id!r}, data={self.data!r})"


def parse_event_data(kind: str, name: str, raw: str) -> Any:
    """事件的 data → 模型（不是 JSON 给字符串，对不上模型给原始 JSON）"""
    try:
        data: Any = json.loads(raw) if raw else None
    except ValueError:
        return raw
    model = AgentNotification if kind == "agent" else EVENT_MODELS.get(name)
    if model is None or not isinstance(data, dict):
        return data
    obj = cast(Dict[str, Any], data)
    try:
        return model.model_validate(obj)
    except pydantic.ValidationError:
        return obj


class SSEDecoder:
    """一行一行喂进来，攒够一条（空行）吐一条 (name, data, id)。"""

    def __init__(self) -> None:
        self._name = ""
        self._data: List[str] = []
        self._id: Optional[str] = None
        self.last_id: Optional[str] = None
        """最近一条带 id 的事件的 id（重连时带成 Last-Event-ID）"""

    def feed(self, line: str) -> "Optional[tuple[str, str, Optional[str]]]":
        line = line.rstrip("\r\n")
        if line == "":
            if self._id is not None:
                self.last_id = self._id
            out = None
            if self._data:
                out = (self._name or "message", "\n".join(self._data), self._id if self._id is not None else self.last_id)
            self._name, self._data, self._id = "", [], None
            return out
        if line.startswith(":"):
            return None  # 心跳
        k, sep, v = line.partition(":")
        if sep and v.startswith(" "):
            v = v[1:]
        if k == "event":
            self._name = v
        elif k == "data":
            self._data.append(v)
        elif k == "id":
            if "\0" not in v:
                self._id = v
        # retry 和别的字段：不管
        return None


class _Lines:
    """文本块 → 行（\\n、\\r\\n、\\r 都算行尾）；没完的那段留到下一块"""

    def __init__(self) -> None:
        self._buf = ""

    def feed(self, chunk: str) -> List[str]:
        buf = self._buf + chunk
        out: List[str] = []
        start = 0
        i = 0
        n = len(buf)
        while i < n:
            ch = buf[i]
            if ch == "\n":
                out.append(buf[start:i])
                start = i = i + 1
            elif ch == "\r":
                if i + 1 == n:
                    break  # 可能是 \r\n：等下一块
                out.append(buf[start:i])
                i += 2 if buf[i + 1] == "\n" else 1
                start = i
            else:
                i += 1
        self._buf = buf[start:]
        return out

    def flush(self) -> List[str]:
        rest, self._buf = self._buf, ""
        return [rest.rstrip("\r")] if rest else []


def _is_fatal(e: BaseException) -> bool:
    return isinstance(e, APIError) and e.status in FATAL_STATUS


class _Base:
    def __init__(self, kind: str) -> None:
        self._kind = kind
        self._decoder = SSEDecoder()
        self._closed = False
        # 下次连之前要不要先等（连失败过、或者断过）
        self._wait_first = False

    @property
    def last_event_id(self) -> Optional[str]:
        """最近收到的事件 id（断了重连从这里接着收）"""
        return self._decoder.last_id

    @property
    def closed(self) -> bool:
        return self._closed

    def _event(self, got: "tuple[str, str, Optional[str]]") -> Event:
        name, raw, eid = got
        return Event(name, parse_event_data(self._kind, name, raw), eid, raw)


class EventStream(_Base):
    """同步的事件流：迭代器 + 上下文管理器。"""

    def __init__(self, connect: Callable[[Optional[str]], httpx.Response], kind: str) -> None:
        super().__init__(kind)
        self._connect = connect
        self._response: Optional[httpx.Response] = None
        self._stop = threading.Event()
        self._gen: Optional[Iterator[Event]] = None

    def connect(self) -> "EventStream":
        """现在就连上（进 with 时自动调）；连不上的话迭代时接着重连，401 / 403 / 404 直接抛。"""
        if self._response is None and not self._closed:
            try:
                self._response = self._connect(self.last_event_id)
            except (APIError, APIConnectionError) as e:
                if _is_fatal(e):
                    raise
                self._wait_first = True
        return self

    def __enter__(self) -> "EventStream":
        return self.connect()

    def __exit__(self, *exc: object) -> None:
        self.close()

    def __iter__(self) -> Iterator[Event]:
        return self

    def __next__(self) -> Event:
        if self._gen is None:
            self._gen = self._run()
        return next(self._gen)

    def _run(self) -> Iterator[Event]:
        backoff = RECONNECT_INITIAL
        while not self._closed:
            if self._response is None:
                if self._wait_first:
                    if self._stop.wait(backoff):
                        return
                    backoff = min(RECONNECT_MAX, backoff * 2)
                try:
                    self._response = self._connect(self.last_event_id)
                except (APIError, APIConnectionError) as e:
                    if _is_fatal(e):
                        raise
                    self._wait_first = True
                    continue
            # 连上了：等待时间重置；之后断了要先等一下再连
            backoff = RECONNECT_INITIAL
            self._wait_first = True
            resp = self._response
            lines = _Lines()
            try:
                for chunk in resp.iter_text():
                    for line in lines.feed(chunk):
                        got = self._decoder.feed(line)
                        if got is not None:
                            yield self._event(got)
                        if self._closed:
                            return
            except (httpx.TransportError, httpx.StreamError):
                pass  # 断了：重连
            finally:
                resp.close()
                self._response = None

    def close(self) -> None:
        """关掉（别的线程里调也行）"""
        self._closed = True
        self._stop.set()
        if self._response is not None:
            try:
                self._response.close()
            except Exception:  # noqa: BLE001
                pass


class AsyncEventStream(_Base):
    """异步的事件流：async 迭代器 + async 上下文管理器。"""

    def __init__(self, connect: Callable[[Optional[str]], Awaitable[httpx.Response]], kind: str) -> None:
        super().__init__(kind)
        self._connect = connect
        self._response: Optional[httpx.Response] = None
        self._gen: Optional[AsyncIterator[Event]] = None
        self._stop: Optional[asyncio.Event] = None

    async def connect(self) -> "AsyncEventStream":
        """现在就连上（进 async with 时自动调）"""
        if self._response is None and not self._closed:
            try:
                self._response = await self._connect(self.last_event_id)
            except (APIError, APIConnectionError) as e:
                if _is_fatal(e):
                    raise
                self._wait_first = True
        return self

    async def __aenter__(self) -> "AsyncEventStream":
        return await self.connect()

    async def __aexit__(self, *exc: object) -> None:
        await self.close()

    def __aiter__(self) -> AsyncIterator[Event]:
        return self

    async def __anext__(self) -> Event:
        if self._gen is None:
            self._gen = self._run()
        return await self._gen.__anext__()

    async def _sleep(self, s: float) -> bool:
        """等 s 秒；中间关了返回 True"""
        if self._stop is None:
            self._stop = asyncio.Event()
        if self._closed:
            return True
        try:
            await asyncio.wait_for(self._stop.wait(), s)
            return True
        except asyncio.TimeoutError:
            return self._closed

    async def _run(self) -> AsyncIterator[Event]:
        backoff = RECONNECT_INITIAL
        while not self._closed:
            if self._response is None:
                if self._wait_first:
                    if await self._sleep(backoff):
                        return
                    backoff = min(RECONNECT_MAX, backoff * 2)
                try:
                    self._response = await self._connect(self.last_event_id)
                except (APIError, APIConnectionError) as e:
                    if _is_fatal(e):
                        raise
                    self._wait_first = True
                    continue
            backoff = RECONNECT_INITIAL
            self._wait_first = True
            resp = self._response
            lines = _Lines()
            try:
                async for chunk in resp.aiter_text():
                    for line in lines.feed(chunk):
                        got = self._decoder.feed(line)
                        if got is not None:
                            yield self._event(got)
                        if self._closed:
                            return
            except (httpx.TransportError, httpx.StreamError):
                pass
            finally:
                await resp.aclose()
                self._response = None

    async def close(self) -> None:
        self._closed = True
        if self._stop is not None:
            self._stop.set()
        if self._response is not None:
            try:
                await self._response.aclose()
            except Exception:  # noqa: BLE001
                pass
