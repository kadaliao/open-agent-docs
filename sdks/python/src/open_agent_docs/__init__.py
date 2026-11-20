"""Open Agent Docs 的 Python SDK。

```python
import open_agent_docs as oad

client = oad.Client(base_url="http://127.0.0.1:8848", token="oad_…")
ws = client.default_workspace()
page = client.pages.create(ws, title="周报", markdown="# 本周\\n\\n正文")
```

- Client / AsyncClient：方法名见 sdks/operations.json（client.pages.versions.list……）
- with_user：宿主后端替用户调（header / JWT）
- workspaces.events、agent.notifications.stream：事件流
- verify_webhook、sign_jwt、embed_url：webhook 验签、签 JWT、嵌入地址
- start：本地起服务器
- 模型（Page、Content……）都在这里，也在 open_agent_docs.types
"""
from ._base_client import SyncAPIClient, AsyncAPIClient
from ._client import AsyncClient, Client
from ._embed import embed_url, sign_jwt
from ._errors import (
    APIConnectionError,
    APIError,
    APITimeoutError,
    AuthenticationError,
    BadGatewayError,
    BadRequestError,
    ConfigurationError,
    ConflictError,
    GoneError,
    InternalServerError,
    NotFoundError,
    OADError,
    PermissionDeniedError,
    RateLimitError,
    TooLargeError,
    WebhookVerificationError,
)
from ._generated.models import *  # noqa: F403
from ._generated import models as _models
from ._models import BaseModel
from ._pagination import AsyncCursorPage, AsyncPaginator, CursorPage
from ._server import Server, start
from ._sse import AsyncEventStream, Event, EventStream
from ._types import NOT_GIVEN, NotGiven
from ._version import __version__
from ._webhook import verify_webhook

__all__ = [
    "__version__",
    "Client",
    "AsyncClient",
    "SyncAPIClient",
    "AsyncAPIClient",
    "NOT_GIVEN",
    "NotGiven",
    "BaseModel",
    "CursorPage",
    "AsyncCursorPage",
    "AsyncPaginator",
    "Event",
    "EventStream",
    "AsyncEventStream",
    "Server",
    "start",
    "sign_jwt",
    "embed_url",
    "verify_webhook",
    "OADError",
    "ConfigurationError",
    "WebhookVerificationError",
    "APIError",
    "BadRequestError",
    "AuthenticationError",
    "PermissionDeniedError",
    "NotFoundError",
    "ConflictError",
    "GoneError",
    "TooLargeError",
    "RateLimitError",
    "InternalServerError",
    "BadGatewayError",
    "APIConnectionError",
    "APITimeoutError",
]
__all__ += _models.__all__
