"""webhook 验签：服务器 POST 给 Agent 的通知带 X-OAD-Timestamp（毫秒）和
X-OAD-Signature: sha256=<hex(HMAC-SHA256(secret, "<时间戳>.<正文>"))>。"""
from __future__ import annotations

import hashlib
import hmac
import json
import re
import time
from typing import Any, Iterable, Mapping, Optional, Tuple, Union, cast

import pydantic

from ._errors import WebhookVerificationError
from ._generated.models import AgentNotification

__all__ = ["verify_webhook"]

_SIG = re.compile(r"sha256=([0-9a-fA-F]{64})")

Headers = Union[Mapping[str, str], Iterable[Tuple[str, str]]]


def _header(headers: Headers, name: str) -> Optional[str]:
    items: Iterable[Tuple[str, str]]
    if isinstance(headers, Mapping):
        m = cast(Mapping[str, str], headers)
        # Starlette、Flask、httpx 的头本身就不分大小写；普通 dict 这里自己找
        try:
            v = m.get(name)
        except Exception:  # noqa: BLE001
            v = None
        if v is not None:
            return v
        items = m.items()
    else:
        items = headers
    for k, v in items:
        if k.lower() == name:
            return v
    return None


def verify_webhook(
    secret: str,
    headers: Headers,
    body: Union[bytes, str],
    tolerance: float = 300,
    now: Optional[float] = None,
) -> AgentNotification:
    """验 webhook 的签名，对了返回解析好的 AgentNotification；不对抛 WebhookVerificationError（kind 是
    signature / stale / malformed）。

    - headers：请求头（任意映射，名字大小写不论）；body：原始字节（用框架给的原始正文，别解析了再序列化）
    - 时间戳和现在差超过 tolerance 秒（前后都算）算 stale，防重放
    - now：“现在”（秒，像 time.time()），测试用
    """
    ts = _header(headers, "x-oad-timestamp")
    sig = _header(headers, "x-oad-signature")
    if ts is None or sig is None:
        raise WebhookVerificationError("malformed", "缺 X-OAD-Timestamp 或 X-OAD-Signature 头")
    ts = ts.strip()
    if not re.fullmatch(r"-?[0-9]+", ts):
        raise WebhookVerificationError("malformed", "X-OAD-Timestamp 不是整数（毫秒）")
    m = _SIG.fullmatch(sig.strip())
    if not m:
        raise WebhookVerificationError("malformed", "X-OAD-Signature 应该是 sha256=<64 位十六进制>")
    raw = body.encode("utf-8") if isinstance(body, str) else bytes(body)
    want = hmac.new(secret.encode("utf-8"), ts.encode("ascii") + b"." + raw, hashlib.sha256).hexdigest()
    if not hmac.compare_digest(want, m.group(1).lower()):
        raise WebhookVerificationError("signature", "签名不对（密钥不对，或者正文被改过）")
    now_s = time.time() if now is None else now
    if abs(now_s - int(ts) / 1000.0) > tolerance:
        raise WebhookVerificationError("stale", f"时间戳和现在差了超过 {tolerance:g} 秒")
    try:
        data: Any = json.loads(raw)
    except ValueError as e:
        raise WebhookVerificationError("malformed", "正文不是 JSON") from e
    try:
        return AgentNotification.model_validate(data)
    except pydantic.ValidationError:
        # 签名是对的：内容就是服务器发的。缺字段（老服务器、测试用的小正文）也照样给出来，不认的字段留着
        if isinstance(data, dict):
            return AgentNotification.model_construct(**data)  # pyright: ignore[reportUnknownArgumentType]
        raise WebhookVerificationError("malformed", "正文不是 JSON 对象") from None
