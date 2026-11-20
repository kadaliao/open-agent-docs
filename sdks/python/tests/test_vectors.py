"""sdks/conformance/vectors.json 的固定用例：webhook（S10）、embed_url 和 jwt（S11）。"""
from __future__ import annotations

import base64
import hashlib
import hmac
import json
from typing import Any, Dict

import pytest

import open_agent_docs as oad
from oad_test_helpers import VECTORS


def _b64d(s: str) -> bytes:
    return base64.urlsafe_b64decode(s + "=" * (-len(s) % 4))


@pytest.mark.parametrize("case", VECTORS["webhook"], ids=[c["name"] for c in VECTORS["webhook"]])
def test_s10_webhook(case: Dict[str, Any]) -> None:
    def verify() -> oad.AgentNotification:
        return oad.verify_webhook(case["secret"], case["headers"], case["body"], tolerance=case["tolerance_s"], now=case["now_ms"] / 1000)

    if case["expect"] == "ok":
        note = verify()
        assert isinstance(note, oad.AgentNotification)
        for k, v in json.loads(case["body"]).items():
            assert getattr(note, k) == v  # 测试用的小正文：缺字段也照样给出来
        # 正文给字节、头给 (名字, 值) 的列表也行
        pairs = list(case["headers"].items())
        oad.verify_webhook(case["secret"], pairs, case["body"].encode(), tolerance=case["tolerance_s"], now=case["now_ms"] / 1000)
    else:
        with pytest.raises(oad.WebhookVerificationError) as ei:
            verify()
        assert ei.value.kind == case["expect"]


def test_webhook_full_notification_is_validated() -> None:
    secret = "whsec_x"
    note = {
        "id": 3, "created_at": 1, "type": "comment", "agent": {"id": "a", "name": "bot"},
        "page": {"id": "p", "title": "t", "url": "/p/p"}, "thread_id": "t1",
        "comment": {"id": "c", "body": "@bot 看看", "author": {"kind": "user", "id": "u", "name": "甲"}}, "prompt": "看看",
    }
    body = json.dumps(note, ensure_ascii=False).encode()
    ts = "1700000000000"
    sig = "sha256=" + hmac.new(secret.encode(), ts.encode() + b"." + body, hashlib.sha256).hexdigest()
    got = oad.verify_webhook(secret, {"X-OAD-Timestamp": ts, "X-OAD-Signature": sig}, body, now=1_700_000_000)
    assert got.comment.body == "@bot 看看" and got.page.url == "/p/p"


@pytest.mark.parametrize("case", VECTORS["embed_url"])
def test_s11_embed_url(case: Dict[str, Any]) -> None:
    assert oad.embed_url(case["base_url"], case["page_id"], **case["options"]) == case["expect"]


def test_s11_jwt() -> None:
    v = VECTORS["jwt"]
    i = v["input"]
    tok = oad.sign_jwt(
        v["secret"], i["user_id"], name=i["name"], email=i["email"], workspaces=i["workspaces"],
        ttl=i["ttl_s"], issuer=i["issuer"], audience=i["audience"], now=v["now_s"],
    )
    head, claims, sig = tok.split(".")
    assert json.loads(_b64d(head)) == v["expect_header"]
    assert json.loads(_b64d(claims)) == v["expect_claims"]
    want = hmac.new(v["secret"].encode(), f"{head}.{claims}".encode(), hashlib.sha256).digest()
    assert _b64d(sig) == want
    assert "=" not in tok


def test_jwt_minimal_claims() -> None:
    tok = oad.sign_jwt("s", "u1", now=100)
    assert json.loads(_b64d(tok.split(".")[1])) == {"sub": "u1", "iat": 100, "exp": 3700}
    tok = oad.sign_jwt("s", "u1", workspaces=["w1", "w2"], now=100, ttl=5)
    assert json.loads(_b64d(tok.split(".")[1]))["workspaces"] == ["w1", "w2"]
