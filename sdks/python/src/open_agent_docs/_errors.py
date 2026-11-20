"""错误。都从 OADError 派生：

- 服务器回了错误：APIError 和按状态码分的子类（NotFoundError……），带 status、code、message、request_id、retry_after
- 连不上、超时：APIConnectionError、APITimeoutError
- 配置不对：ConfigurationError；webhook 验签失败：WebhookVerificationError(kind)
"""
from __future__ import annotations

import json
from typing import Any, Dict, Optional, Type, cast

import httpx

__all__ = [
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


class OADError(Exception):
    """这个包抛的所有错误的基类。"""


class ConfigurationError(OADError):
    """配置不对（比如 with_user 时 header_secret 和 jwt_secret 都没配）。"""


class WebhookVerificationError(OADError):
    """webhook 验签失败。kind：signature（签名不对）/ stale（时间戳太旧或太新）/ malformed（缺头、格式不对）。"""

    def __init__(self, kind: str, message: str) -> None:
        super().__init__(message)
        self.kind = kind


class APIError(OADError):
    """服务器回了错误。code 是 `{ "error": { "code" } }` 里的（not_found……），不是 JSON 时是 http_<状态码>。"""

    status: int
    code: str
    message: str
    request_id: Optional[str]
    retry_after: Optional[float]
    response: Optional[httpx.Response]

    def __init__(
        self,
        status: int,
        code: str,
        message: str,
        request_id: Optional[str] = None,
        retry_after: Optional[float] = None,
        *,
        response: Optional[httpx.Response] = None,
    ) -> None:
        super().__init__(f"{status} {code}: {message}")
        self.status = status
        self.code = code
        self.message = message
        self.request_id = request_id
        self.retry_after = retry_after
        self.response = response

    @property
    def status_code(self) -> int:
        """同 status"""
        return self.status


class BadRequestError(APIError):
    """400"""


class AuthenticationError(APIError):
    """401：没登录、token 不对或过期"""


class PermissionDeniedError(APIError):
    """403：没权限"""


class NotFoundError(APIError):
    """404"""


class ConflictError(APIError):
    """409：比如 base 版本对不上"""


class GoneError(APIError):
    """410：邀请过期之类"""


class TooLargeError(APIError):
    """413"""


class RateLimitError(APIError):
    """429：太快了；retry_after 是服务器让等的秒数"""


class InternalServerError(APIError):
    """5xx"""


class BadGatewayError(InternalServerError):
    """502：服务器去外部网站取东西失败"""


class APIConnectionError(OADError):
    """连不上、连接被断。"""

    def __init__(self, message: str = "连不上服务器", *, request: Optional[httpx.Request] = None) -> None:
        super().__init__(message)
        self.request = request


class APITimeoutError(APIConnectionError):
    """超时。"""

    def __init__(self, message: str = "请求超时", *, request: Optional[httpx.Request] = None) -> None:
        super().__init__(message, request=request)


_BY_STATUS: Dict[int, Type[APIError]] = {
    400: BadRequestError,
    401: AuthenticationError,
    403: PermissionDeniedError,
    404: NotFoundError,
    409: ConflictError,
    410: GoneError,
    413: TooLargeError,
    429: RateLimitError,
    502: BadGatewayError,
}


def parse_retry_after(v: Optional[str]) -> Optional[float]:
    """Retry-After：秒数（HTTP 日期的写法也认）。"""
    if not v:
        return None
    try:
        return max(0.0, float(v.strip()))
    except ValueError:
        pass
    try:
        import email.utils
        import time

        t = email.utils.parsedate_to_datetime(v)
        return max(0.0, t.timestamp() - time.time())
    except (TypeError, ValueError, IndexError):
        return None


def error_from_response(resp: httpx.Response) -> APIError:
    """响应（已经读完）→ 对应的错误。"""
    status = resp.status_code
    code = f"http_{status}"
    message = ""
    try:
        body: Any = json.loads(resp.content)
    except ValueError:
        body = None
    if isinstance(body, dict):
        err: Any = cast(Dict[str, Any], body).get("error")
        if isinstance(err, dict):
            e = cast(Dict[str, Any], err)
            c: Any = e.get("code")
            m: Any = e.get("message")
            if isinstance(c, str) and c:
                code = c
            if isinstance(m, str):
                message = m
        elif isinstance(err, str):
            message = err
    if not message:
        try:
            message = resp.text.strip()[:500]
        except Exception:  # noqa: BLE001
            message = ""
    if not message:
        message = resp.reason_phrase or f"HTTP {status}"
    cls = _BY_STATUS.get(status) or (InternalServerError if status >= 500 else APIError)
    return cls(
        status,
        code,
        message,
        request_id=resp.headers.get("x-request-id"),
        retry_after=parse_retry_after(resp.headers.get("retry-after")),
        response=resp,
    )
