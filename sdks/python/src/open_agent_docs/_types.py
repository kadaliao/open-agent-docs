"""NOT_GIVEN 哨兵：区分“没给”（不发这个字段）和 None（发 null）。"""
from __future__ import annotations

from typing import Any, Optional

__all__ = ["NotGiven", "NOT_GIVEN"]


class NotGiven:
    """没给。可选参数的默认值：不发这个字段；要发 null 就明确传 None。

    ```python
    client.pages.update(pid, after=None)   # 发 {"after": null}：放到最前
    client.pages.update(pid, title="新")   # 只发 title
    ```
    """

    _instance: Optional["NotGiven"] = None

    def __new__(cls) -> "NotGiven":
        if cls._instance is None:
            cls._instance = super().__new__(cls)
        return cls._instance

    def __bool__(self) -> bool:
        return False

    def __repr__(self) -> str:
        return "NOT_GIVEN"

    def __reduce__(self) -> Any:
        return "NOT_GIVEN"


NOT_GIVEN = NotGiven()
