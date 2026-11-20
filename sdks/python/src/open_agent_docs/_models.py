"""模型的基类：pydantic v2，不认识的字段不报错（留着，属性能取到），名字和别名都能用来建。"""
from __future__ import annotations

from typing import Any, Dict

import pydantic
from pydantic import ConfigDict

__all__ = ["BaseModel"]


class BaseModel(pydantic.BaseModel):
    model_config = ConfigDict(extra="allow", populate_by_name=True, protected_namespaces=())

    def to_dict(self) -> Dict[str, Any]:
        """转成 JSON 那样的 dict（用接口里的字段名，没给的不出）。"""
        return self.model_dump(mode="json", by_alias=True, exclude_unset=True)
