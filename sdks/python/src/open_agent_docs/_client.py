"""Client 和 AsyncClient：方法一样，异步的全是 async（翻页、事件流除外，见各自的说明）。

```python
import open_agent_docs as oad

client = oad.Client(base_url="https://docs.example.com", token="oad_…")
client = oad.Client()                      # 读环境变量 OAD_BASE_URL、OAD_TOKEN
ws = client.default_workspace()
page = client.pages.create(ws, title="周报", markdown="# 本周\\n\\n- 上线了 SDK")
```
"""
from __future__ import annotations

from typing import Mapping, Optional

import httpx

from ._base_client import DEFAULT_MAX_RETRIES, DEFAULT_TIMEOUT
from ._generated.resources import AsyncResources, SyncResources

__all__ = ["Client", "AsyncClient"]


class Client(SyncResources):
    """同步客户端。

    参数（都可以不给）：
    - base_url：服务器地址，默认环境变量 OAD_BASE_URL，再没有就 http://127.0.0.1:8848（/api/v1 由 SDK 拼）
    - token：Agent token、会话令牌或 JWT（Authorization: Bearer），默认环境变量 OAD_TOKEN
    - header_secret / jwt_secret：OAD_AUTH=header / jwt 的服务器，配合 with_user 替用户调
    - jwt_issuer、jwt_audience、jwt_ttl：with_user 签 JWT 时写进去（ttl 默认 3600 秒）
    - timeout：单个请求的超时（秒，默认 60；事件流只管连接）
    - max_retries：429、502/503/504、连不上时重试几次（默认 2）
    - locale：zh / en，服务器错误信息的语言
    - headers：每个请求额外带的头
    - http_client：自己的 httpx.Client（代理、证书之类）
    """

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
        http_client: Optional[httpx.Client] = None,
    ) -> None:
        super().__init__(
            base_url=base_url,
            token=token,
            header_secret=header_secret,
            jwt_secret=jwt_secret,
            jwt_issuer=jwt_issuer,
            jwt_audience=jwt_audience,
            jwt_ttl=jwt_ttl,
            timeout=timeout,
            max_retries=max_retries,
            locale=locale,
            headers=headers,
            http_client=http_client,
        )


class AsyncClient(AsyncResources):
    """异步客户端，参数和 Client 一样（http_client 是 httpx.AsyncClient）。

    ```python
    async with oad.AsyncClient() as client:
        ws = await client.default_workspace()
        page = await client.pages.create(ws, title="周报")
        async for v in client.pages.versions.list(page.id):   # 翻页：直接 async for
            ...
    ```
    """

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
        http_client: Optional[httpx.AsyncClient] = None,
    ) -> None:
        super().__init__(
            base_url=base_url,
            token=token,
            header_secret=header_secret,
            jwt_secret=jwt_secret,
            jwt_issuer=jwt_issuer,
            jwt_audience=jwt_audience,
            jwt_ttl=jwt_ttl,
            timeout=timeout,
            max_retries=max_retries,
            locale=locale,
            headers=headers,
            http_client=http_client,
        )
