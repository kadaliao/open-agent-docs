"""嵌入用的：sign_jwt（宿主替用户签 JWT，HS256，只用标准库）、embed_url（iframe 的地址）。"""
from __future__ import annotations

import base64
import hashlib
import hmac
import json
import time
from typing import Any, Dict, Mapping, Optional, Sequence, Union

from ._utils import quote_path

__all__ = ["sign_jwt", "embed_url", "WorkspaceClaim"]

# 工作区 id，或者 {"id": ..., "role": ...}
WorkspaceClaim = Union[str, Mapping[str, Any]]


def _b64(b: bytes) -> str:
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode("ascii")


def _json(v: Any) -> bytes:
    return json.dumps(v, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def sign_jwt(
    secret: Union[str, bytes],
    user_id: str,
    name: Optional[str] = None,
    email: Optional[str] = None,
    workspaces: Optional[Sequence[WorkspaceClaim]] = None,
    ttl: int = 3600,
    issuer: Optional[str] = None,
    audience: Optional[str] = None,
    now: Optional[float] = None,
) -> str:
    """签一个 HS256 的 JWT，给 OAD_AUTH=jwt 的服务器（OAD_JWT_SECRET 是同一个密钥）。

    claims：sub、name、email、workspaces、iat、exp、iss、aud（没给的不写）。workspaces 是工作区 id 的数组，
    或者 [{"id": ..., "role": "viewer"}]；不给就进默认工作区。now 是“现在”（秒），测试用。
    要 RS256 / ES256 的用自己的 JWT 库签，claims 照这个写。
    """
    iat = int(time.time() if now is None else now)
    claims: Dict[str, Any] = {"sub": user_id}
    if name is not None:
        claims["name"] = name
    if email is not None:
        claims["email"] = email
    if workspaces is not None:
        claims["workspaces"] = [w if isinstance(w, str) else dict(w) for w in workspaces]
    claims["iat"] = iat
    claims["exp"] = iat + int(ttl)
    if issuer is not None:
        claims["iss"] = issuer
    if audience is not None:
        claims["aud"] = audience
    head = _b64(_json({"alg": "HS256", "typ": "JWT"}))
    body = _b64(_json(claims))
    key = secret.encode("utf-8") if isinstance(secret, str) else secret
    sig = hmac.new(key, f"{head}.{body}".encode("ascii"), hashlib.sha256).digest()
    return f"{head}.{body}.{_b64(sig)}"


def embed_url(
    base_url: str,
    page_id: str,
    theme: Optional[str] = None,
    lang: Optional[str] = None,
    token: Optional[str] = None,
) -> str:
    """iframe 里嵌一个页面的地址：`<base>/p/<page_id>?embed=1[&theme=][&lang=][#token=]`。

    token（JWT）放在 # 后面：不进服务器日志、不进 Referer，网页读到后马上从地址栏抹掉。
    """
    url = f"{base_url.rstrip('/')}/p/{quote_path(page_id)}?embed=1"
    if theme:
        url += "&theme=" + quote_path(theme)
    if lang:
        url += "&lang=" + quote_path(lang)
    if token:
        url += "#token=" + quote_path(token)
    return url
