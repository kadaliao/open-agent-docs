from __future__ import annotations

from urllib.parse import quote

__all__ = ["quote_path", "encode_uri_component"]


def quote_path(v: object) -> str:
    """路径里的一段：全都百分号编码（/ 也编）。"""
    return quote(str(v), safe="")


def encode_uri_component(s: str) -> str:
    """和 JS 的 encodeURIComponent 一样（UTF-8，A-Z a-z 0-9 - _ . ! ~ * ' ( ) 不编）。"""
    return quote(s, safe="-_.!~*'()")
