"""客户端的底子：配置、认证头（token / with_user 的 header、JWT）、重试、错误、翻页、事件流。
生成的资源类（_generated/resources.py）只调这里的 _call / _call_or_none / _call_page / _stream。"""
from __future__ import annotations

import asyncio
import json
import os
import random
import threading
import time
from functools import cached_property, lru_cache
from typing import (
    Any,
    Awaitable,
    Dict,
    List,
    Mapping,
    Optional,
    Sequence,
    Tuple,
    Type,
    TypeVar,
    Union,
    cast,
)

import httpx
import pydantic

from ._embed import WorkspaceClaim, sign_jwt
from ._errors import (
    APIConnectionError,
    APIError,
    APITimeoutError,
    ConfigurationError,
    NotFoundError,
    OADError,
    error_from_response,
    parse_retry_after,
)
from ._models import BaseModel
from ._pagination import AsyncCursorPage, AsyncPaginator, CursorPage
from ._sse import AsyncEventStream, EventStream
from ._types import NOT_GIVEN, NotGiven
from ._utils import encode_uri_component
from ._version import __version__

__all__ = ["SyncAPIClient", "AsyncAPIClient", "Headers", "Query", "Body", "TimeoutTypes"]

DEFAULT_BASE_URL = "http://127.0.0.1:8848"
DEFAULT_TIMEOUT = 60.0
DEFAULT_MAX_RETRIES = 2
RETRY_AFTER_MAX = 60.0
BACKOFF_INITIAL = 0.5
BACKOFF_MAX = 8.0
JWT_REFRESH_BEFORE = 60
USER_AGENT = f"open-agent-docs-python/{__version__}"

Headers = Mapping[str, str]
Query = Mapping[str, object]
Body = Mapping[str, object]
TimeoutTypes = Union[float, httpx.Timeout, None]

_T = TypeVar("_T")
_M = TypeVar("_M", bound=BaseModel)
_C = TypeVar("_C", bound="_BaseClient")

# 请求没发出去的错误：什么方法都能重试
_NOT_SENT = (httpx.ConnectError, httpx.ConnectTimeout, httpx.PoolTimeout)
# 连接被断：只有 GET、PUT、DELETE（幂等）重试
_IDEMPOTENT = {"GET", "HEAD", "OPTIONS", "PUT", "DELETE"}


@lru_cache(maxsize=None)
def _adapter(t: Any) -> pydantic.TypeAdapter[Any]:
    return pydantic.TypeAdapter(t)


def _validate(t: Any, data: Any) -> Any:
    if isinstance(t, type) and issubclass(t, pydantic.BaseModel):
        return t.model_validate(data)
    if t is object:
        return data
    adapter: pydantic.TypeAdapter[Any] = _adapter(cast(object, t))
    return adapter.validate_python(data)


def _jsonable(v: Any) -> Any:
    """请求体里的值：模型转 dict（用接口里的名字，没设的字段不发），NOT_GIVEN 去掉"""
    if isinstance(v, pydantic.BaseModel):
        return v.model_dump(mode="json", by_alias=True, exclude_unset=True)
    if isinstance(v, Mapping):
        m = cast(Mapping[Any, Any], v)
        return {k: _jsonable(x) for k, x in m.items() if not isinstance(x, NotGiven)}
    if isinstance(v, (list, tuple)):
        seq = cast(Sequence[Any], v)
        return [_jsonable(x) for x in seq]
    return v


def _query(q: Optional[Mapping[str, object]], extra: Optional[Mapping[str, object]]) -> Dict[str, str]:
    """查询参数：没给的（NOT_GIVEN、None）不带；布尔写成 true / false"""
    out: Dict[str, str] = {}
    for src in (q, extra):
        if not src:
            continue
        for k, v in src.items():
            if isinstance(v, NotGiven) or v is None:
                continue
            if isinstance(v, bool):
                out[k] = "true" if v else "false"
            elif isinstance(v, (list, tuple)):
                out[k] = ",".join(str(x) for x in cast(Sequence[Any], v))
            else:
                out[k] = str(v)
    return out


def _retry_delay(attempt: int, resp: Optional[httpx.Response]) -> float:
    """第 attempt 次重试（从 0 起）前等多久：Retry-After（秒，最多 60），没有就 0.5 × 2^n（最多 8）加 0–25% 随机"""
    if resp is not None:
        ra = parse_retry_after(resp.headers.get("retry-after"))
        if ra is not None:
            return min(ra, RETRY_AFTER_MAX)
    d = min(BACKOFF_INITIAL * (2**attempt), BACKOFF_MAX)
    return d * (1 + random.random() * 0.25)


def _retry_status(method: str, status: int) -> bool:
    if status == 429:
        return True
    if status in (502, 503, 504):
        # POST、PATCH 可能已经生效了：不重试
        return method not in ("POST", "PATCH")
    return False


def _retry_exc(method: str, e: Exception) -> bool:
    if isinstance(e, _NOT_SENT):
        return True
    if isinstance(e, (httpx.ReadTimeout, httpx.WriteTimeout)):
        return False
    if isinstance(e, (httpx.RemoteProtocolError, httpx.ReadError, httpx.WriteError, httpx.CloseError)):
        return method in _IDEMPOTENT
    return False


def _wrap_exc(e: Exception, req: Optional[httpx.Request]) -> APIConnectionError:
    if isinstance(e, httpx.TimeoutException):
        return APITimeoutError(f"请求超时：{e}", request=req)
    return APIConnectionError(f"连不上服务器：{e}", request=req)


class _User:
    __slots__ = ("id", "name", "email", "workspaces")

    def __init__(self, id: str, name: Optional[str], email: Optional[str], workspaces: Optional[Sequence[WorkspaceClaim]]) -> None:
        self.id = id
        self.name = name
        self.email = email
        self.workspaces = workspaces


class _BaseClient:
    base_url: str
    """服务器地址（不带 /api/v1）"""
    token: Optional[str]
    timeout: float
    max_retries: int
    locale: Optional[str]

    def __init__(
        self,
        *,
        base_url: Optional[str] = None,
        token: Optional[str] = None,
        header_secret: Optional[str] = None,
        jwt_secret: Optional[str] = None,
        jwt_issuer: Optional[str] = None,
        jwt_audience: Optional[str] = None,
        jwt_ttl: int = 3600,
        timeout: float = DEFAULT_TIMEOUT,
        max_retries: int = DEFAULT_MAX_RETRIES,
        locale: Optional[str] = None,
        headers: Optional[Mapping[str, str]] = None,
    ) -> None:
        url = base_url or os.environ.get("OAD_BASE_URL") or DEFAULT_BASE_URL
        self.base_url = url.rstrip("/")
        self.token = token if token is not None else (os.environ.get("OAD_TOKEN") or None)
        self._header_secret = header_secret
        self._jwt_secret = jwt_secret
        self._jwt_issuer = jwt_issuer
        self._jwt_audience = jwt_audience
        self._jwt_ttl = int(jwt_ttl)
        self.timeout = float(timeout)
        self.max_retries = max(0, int(max_retries))
        self.locale = locale
        self._headers: Dict[str, str] = dict(headers or {})
        self._user: Optional[_User] = None
        self._jwt: Optional[Tuple[str, float]] = None
        self._lock = threading.Lock()
        self._ws: Optional[str] = None

    # ---------- with_user

    def _child(self: _C, user: _User) -> _C:
        mode = self._auth_mode()
        if user.workspaces is not None and mode == "header":
            raise ConfigurationError("header 模式不支持 workspaces（要指定工作区用 jwt_secret）")
        c = self.__class__.__new__(self.__class__)
        c.__dict__.update(self.__dict__)
        # 每个客户端自己的：用户、JWT 缓存、默认工作区、资源（cached_property 存在 __dict__ 里，要重新建）
        for klass in type(self).__mro__:
            for name, attr in vars(klass).items():
                if isinstance(attr, cached_property):
                    c.__dict__.pop(name, None)
        c._user = user
        c._jwt = None
        c._ws = None
        c._lock = threading.Lock()
        c.__dict__["_owns_http"] = False  # 连接池是父客户端的
        return c

    def _auth_mode(self) -> str:
        if self._header_secret and self._jwt_secret:
            raise ConfigurationError("header_secret 和 jwt_secret 只能配一个")
        if self._header_secret:
            return "header"
        if self._jwt_secret:
            return "jwt"
        raise ConfigurationError("with_user 要先配 header_secret（OAD_AUTH=header）或 jwt_secret（OAD_AUTH=jwt）")

    def with_user(
        self: _C,
        user_id: str,
        name: Optional[str] = None,
        email: Optional[str] = None,
        workspaces: Optional[Sequence[WorkspaceClaim]] = None,
    ) -> _C:
        """以某个用户的身份调（宿主后端用）：返回一个新的客户端（共享连接池和配置），原来的不变。

        - 配了 header_secret：带 X-OAD-Secret、X-OAD-User-Id / -Name（百分号编码）/ -Email
        - 配了 jwt_secret：签一个 JWT（缓存到过期前 60 秒再换），workspaces 写进 claims
        """
        return self._child(_User(user_id, name, email, workspaces))

    def _jwt_token(self, user: _User) -> str:
        now = time.time()
        with self._lock:
            if self._jwt is not None and self._jwt[1] - JWT_REFRESH_BEFORE > now:
                return self._jwt[0]
            assert self._jwt_secret is not None
            tok = sign_jwt(
                self._jwt_secret,
                user.id,
                name=user.name,
                email=user.email,
                workspaces=user.workspaces,
                ttl=self._jwt_ttl,
                issuer=self._jwt_issuer,
                audience=self._jwt_audience,
                now=now,
            )
            self._jwt = (tok, int(now) + self._jwt_ttl)
            return tok

    def _auth_headers(self) -> Dict[str, str]:
        u = self._user
        if u is None:
            return {"authorization": f"Bearer {self.token}"} if self.token else {}
        if self._auth_mode() == "header":
            assert self._header_secret is not None
            h = {"x-oad-secret": self._header_secret, "x-oad-user-id": u.id}
            if u.name is not None:
                h["x-oad-user-name"] = encode_uri_component(u.name)
            if u.email is not None:
                h["x-oad-user-email"] = u.email
            return h
        return {"authorization": f"Bearer {self._jwt_token(u)}"}

    def _build_headers(self, extra: Optional[Mapping[str, str]], json_body: bool) -> Dict[str, str]:
        h: Dict[str, str] = {"user-agent": USER_AGENT}
        if self.locale:
            h["accept-language"] = {"zh": "zh-CN", "en": "en"}.get(self.locale, self.locale)
        if json_body:
            h["content-type"] = "application/json"
        h.update({k.lower(): v for k, v in self._headers.items()})
        h.update(self._auth_headers())
        if extra:
            h.update({k.lower(): v for k, v in extra.items()})
        return h

    def _timeout(self, t: Union[TimeoutTypes, NotGiven]) -> httpx.Timeout:
        if isinstance(t, NotGiven):
            return httpx.Timeout(self.timeout)
        if isinstance(t, httpx.Timeout):
            return t
        return httpx.Timeout(t)

    def _url(self, path: str) -> str:
        return f"{self.base_url}/api/v1{path}"

    def _prepare(
        self,
        method: str,
        path: str,
        *,
        query: Optional[Mapping[str, object]],
        body: object,
        extra_body: Optional[Mapping[str, object]],
        content: Optional[bytes],
        files: Optional[Mapping[str, Any]],
        headers: Optional[Mapping[str, str]],
        extra_headers: Optional[Mapping[str, str]],
        extra_query: Optional[Mapping[str, object]],
    ) -> Dict[str, Any]:
        """→ httpx.build_request 的参数"""
        kw: Dict[str, Any] = {"params": _query(query, extra_query)}
        is_json = False
        if not isinstance(body, NotGiven):
            data = _jsonable(body)
            if extra_body:
                if not isinstance(data, dict):
                    raise ConfigurationError("extra_body 只能用在正文是对象的接口上")
                cast(Dict[str, Any], data).update(_jsonable(extra_body))
            kw["content"] = json.dumps(data, ensure_ascii=False).encode("utf-8")
            is_json = True
        elif content is not None:
            kw["content"] = content
        if files is not None:
            kw["files"] = files
        h = dict(headers or {})
        if extra_headers:
            h.update(extra_headers)
        kw["headers"] = self._build_headers(h, is_json)
        return kw

    def _parse(self, resp: httpx.Response, cast_to: Any, unwrap: Optional[str]) -> Any:
        if cast_to is type(None):
            return None
        if cast_to is bytes:
            return resp.content
        if resp.status_code == 204 or not resp.content:
            data: Any = None
        else:
            try:
                data = json.loads(resp.content)
            except ValueError as e:
                raise APIError(resp.status_code, "invalid_json", f"响应不是 JSON：{resp.text[:200]}", response=resp) from e
        if unwrap is not None:
            if not isinstance(data, dict) or unwrap not in data:
                raise APIError(resp.status_code, "invalid_response", f"响应里没有 {unwrap}", response=resp)
            data = cast(Dict[str, Any], data)[unwrap]
        return _validate(cast_to, data)

    def _stream_params(self, query: Optional[Mapping[str, object]], last_id: Optional[str]) -> Tuple[Dict[str, str], Dict[str, str]]:
        q = _query(query, None)
        h: Dict[str, str] = {"accept": "text/event-stream", "cache-control": "no-cache"}
        if last_id is not None:
            # 服务器先看 ?after=：重连时去掉，用 Last-Event-ID 接着收
            q.pop("after", None)
            q.pop("cursor", None)
            h["last-event-id"] = last_id
        return q, h

    def __repr__(self) -> str:
        who = f" user={self._user.id!r}" if self._user else ""
        return f"<{type(self).__name__} {self.base_url}{who}>"


class SyncAPIClient(_BaseClient):
    _http: httpx.Client
    _owns_http: bool

    def __init__(self, *, http_client: Optional[httpx.Client] = None, **kw: Any) -> None:
        super().__init__(**kw)
        self._owns_http = http_client is None
        self._http = http_client or httpx.Client(timeout=self.timeout, follow_redirects=True)

    def close(self) -> None:
        """关掉连接池（自己建的才关；with_user 出来的客户端不关父客户端的）"""
        if self._owns_http:
            self._http.close()

    def __enter__(self: _C) -> _C:
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()

    def _send(self, method: str, path: str, timeout: Union[TimeoutTypes, NotGiven], stream: bool = False, **kw: Any) -> httpx.Response:
        attempt = 0
        while True:
            req = self._http.build_request(method, self._url(path), timeout=self._timeout(timeout), **kw)
            try:
                resp = self._http.send(req, stream=stream)
            except httpx.TransportError as e:
                if attempt < self.max_retries and _retry_exc(method, e):
                    time.sleep(_retry_delay(attempt, None))
                    attempt += 1
                    continue
                raise _wrap_exc(e, req) from e
            if resp.status_code >= 400:
                if attempt < self.max_retries and _retry_status(method, resp.status_code):
                    delay = _retry_delay(attempt, resp)
                    resp.close()
                    time.sleep(delay)
                    attempt += 1
                    continue
                try:
                    resp.read()
                finally:
                    resp.close()
                raise error_from_response(resp)
            return resp

    def _call(
        self,
        method: str,
        path: str,
        *,
        cast: Type[_T],
        query: Optional[Mapping[str, object]] = None,
        body: object = NOT_GIVEN,
        extra_body: Optional[Mapping[str, object]] = None,
        content: Optional[bytes] = None,
        files: Optional[Mapping[str, Any]] = None,
        headers: Optional[Mapping[str, str]] = None,
        unwrap: Optional[str] = None,
        extra_headers: Optional[Mapping[str, str]] = None,
        extra_query: Optional[Mapping[str, object]] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> _T:
        kw = self._prepare(
            method, path, query=query, body=body, extra_body=extra_body, content=content, files=files,
            headers=headers, extra_headers=extra_headers, extra_query=extra_query,
        )
        resp = self._send(method, path, timeout, **kw)
        return self._parse(resp, cast, unwrap)

    def _call_or_none(self, method: str, path: str, *, cast: Type[_T], **kw: Any) -> Optional[_T]:
        """404 不算错，返回 None"""
        try:
            return self._call(method, path, cast=cast, **kw)
        except NotFoundError:
            return None

    def _call_page(
        self,
        method: str,
        path: str,
        *,
        model: Type[_M],
        items: str,
        item: Type[_T],
        query: Optional[Mapping[str, object]] = None,
        extra_headers: Optional[Mapping[str, str]] = None,
        extra_query: Optional[Mapping[str, object]] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CursorPage[_T]:
        base = dict(query or {})

        def fetch(cursor: Optional[str]) -> CursorPage[_T]:
            q = dict(base)
            if cursor is not None:
                q["cursor"] = cursor
            r = self._call(method, path, cast=model, query=q, extra_headers=extra_headers, extra_query=extra_query, timeout=timeout)
            got = cast(List[_T], list(getattr(r, items) or []))
            return CursorPage(items=got, next_cursor=getattr(r, "next_cursor", None), response=r, cursor=cursor, fetch=fetch)

        start = base.get("cursor")
        return fetch(start if isinstance(start, str) else None)

    def _stream(
        self,
        path: str,
        *,
        kind: str,
        query: Optional[Mapping[str, object]] = None,
        extra_headers: Optional[Mapping[str, str]] = None,
        extra_query: Optional[Mapping[str, object]] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> EventStream:
        t = self._timeout(timeout)
        # 事件流没有总超时：只有连接超时
        t = httpx.Timeout(t.connect, read=None, write=t.write, pool=t.pool)
        merged = dict(query or {})
        merged.update(extra_query or {})

        def connect(last_id: Optional[str]) -> httpx.Response:
            q, h = self._stream_params(merged, last_id)
            h.update(extra_headers or {})
            req = self._http.build_request("GET", self._url(path), params=q, headers=self._build_headers(h, False), timeout=t)
            try:
                resp = self._http.send(req, stream=True)
            except httpx.TransportError as e:
                raise _wrap_exc(e, req) from e
            if resp.status_code >= 400:
                try:
                    resp.read()
                finally:
                    resp.close()
                raise error_from_response(resp)
            return resp

        return EventStream(connect, kind)

    def default_workspace(self) -> str:
        """/me 里第一个工作区的 id（每个客户端只取一次）"""
        if self._ws is None:
            me = self._call("GET", "/me", cast=object)
            self._ws = _first_ws(me)
        return self._ws


class AsyncAPIClient(_BaseClient):
    _http: httpx.AsyncClient
    _owns_http: bool

    def __init__(self, *, http_client: Optional[httpx.AsyncClient] = None, **kw: Any) -> None:
        super().__init__(**kw)
        self._owns_http = http_client is None
        self._http = http_client or httpx.AsyncClient(timeout=self.timeout, follow_redirects=True)

    async def close(self) -> None:
        if self._owns_http:
            await self._http.aclose()

    async def __aenter__(self: _C) -> _C:
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.close()

    async def _send(self, method: str, path: str, timeout: Union[TimeoutTypes, NotGiven], stream: bool = False, **kw: Any) -> httpx.Response:
        attempt = 0
        while True:
            req = self._http.build_request(method, self._url(path), timeout=self._timeout(timeout), **kw)
            try:
                resp = await self._http.send(req, stream=stream)
            except httpx.TransportError as e:
                if attempt < self.max_retries and _retry_exc(method, e):
                    await asyncio.sleep(_retry_delay(attempt, None))
                    attempt += 1
                    continue
                raise _wrap_exc(e, req) from e
            if resp.status_code >= 400:
                if attempt < self.max_retries and _retry_status(method, resp.status_code):
                    delay = _retry_delay(attempt, resp)
                    await resp.aclose()
                    await asyncio.sleep(delay)
                    attempt += 1
                    continue
                try:
                    await resp.aread()
                finally:
                    await resp.aclose()
                raise error_from_response(resp)
            return resp

    async def _call(
        self,
        method: str,
        path: str,
        *,
        cast: Type[_T],
        query: Optional[Mapping[str, object]] = None,
        body: object = NOT_GIVEN,
        extra_body: Optional[Mapping[str, object]] = None,
        content: Optional[bytes] = None,
        files: Optional[Mapping[str, Any]] = None,
        headers: Optional[Mapping[str, str]] = None,
        unwrap: Optional[str] = None,
        extra_headers: Optional[Mapping[str, str]] = None,
        extra_query: Optional[Mapping[str, object]] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> _T:
        kw = self._prepare(
            method, path, query=query, body=body, extra_body=extra_body, content=content, files=files,
            headers=headers, extra_headers=extra_headers, extra_query=extra_query,
        )
        resp = await self._send(method, path, timeout, **kw)
        return self._parse(resp, cast, unwrap)

    async def _call_or_none(self, method: str, path: str, *, cast: Type[_T], **kw: Any) -> Optional[_T]:
        try:
            return await self._call(method, path, cast=cast, **kw)
        except NotFoundError:
            return None

    def _call_page(
        self,
        method: str,
        path: str,
        *,
        model: Type[_M],
        items: str,
        item: Type[_T],
        query: Optional[Mapping[str, object]] = None,
        extra_headers: Optional[Mapping[str, str]] = None,
        extra_query: Optional[Mapping[str, object]] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AsyncPaginator[_T]:
        base = dict(query or {})

        async def fetch(cursor: Optional[str]) -> AsyncCursorPage[_T]:
            q = dict(base)
            if cursor is not None:
                q["cursor"] = cursor
            r = await self._call(method, path, cast=model, query=q, extra_headers=extra_headers, extra_query=extra_query, timeout=timeout)
            got = cast(List[_T], list(getattr(r, items) or []))
            return AsyncCursorPage(items=got, next_cursor=getattr(r, "next_cursor", None), response=r, cursor=cursor, fetch=fetch)

        start = base.get("cursor")

        def first() -> Awaitable[AsyncCursorPage[_T]]:
            return fetch(start if isinstance(start, str) else None)

        return AsyncPaginator(first)

    def _stream(
        self,
        path: str,
        *,
        kind: str,
        query: Optional[Mapping[str, object]] = None,
        extra_headers: Optional[Mapping[str, str]] = None,
        extra_query: Optional[Mapping[str, object]] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AsyncEventStream:
        t = self._timeout(timeout)
        t = httpx.Timeout(t.connect, read=None, write=t.write, pool=t.pool)
        merged = dict(query or {})
        merged.update(extra_query or {})

        async def connect(last_id: Optional[str]) -> httpx.Response:
            q, h = self._stream_params(merged, last_id)
            h.update(extra_headers or {})
            req = self._http.build_request("GET", self._url(path), params=q, headers=self._build_headers(h, False), timeout=t)
            try:
                resp = await self._http.send(req, stream=True)
            except httpx.TransportError as e:
                raise _wrap_exc(e, req) from e
            if resp.status_code >= 400:
                try:
                    await resp.aread()
                finally:
                    await resp.aclose()
                raise error_from_response(resp)
            return resp

        return AsyncEventStream(connect, kind)

    async def default_workspace(self) -> str:
        """/me 里第一个工作区的 id（每个客户端只取一次）"""
        if self._ws is None:
            me = await self._call("GET", "/me", cast=object)
            self._ws = _first_ws(me)
        return self._ws


def _first_ws(me: Any) -> str:
    try:
        return str(me["workspaces"][0]["id"])
    except (KeyError, IndexError, TypeError):
        raise OADError("这个用户不在任何工作区里（/me 的 workspaces 是空的）") from None

