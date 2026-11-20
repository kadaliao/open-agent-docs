"""上传：files.upload 的 file 可以是 bytes、路径（pathlib.Path）、打开的文件，或者 (文件名, bytes | 文件对象[, 类型])。"""
from __future__ import annotations

import mimetypes
import os
from typing import IO, Dict, Optional, Tuple, Union

__all__ = ["FileTypes", "file_part", "csv_body"]

FileContent = Union[bytes, IO[bytes]]
FileTypes = Union[
    bytes,
    "os.PathLike[str]",
    IO[bytes],
    Tuple[str, FileContent],
    Tuple[str, FileContent, str],
]


def _read(c: FileContent) -> bytes:
    # 先读成 bytes：重试时还能再发一次
    return c if isinstance(c, bytes) else c.read()


def file_part(file: FileTypes, filename: Optional[str] = None, content_type: Optional[str] = None) -> Dict[str, Tuple[str, bytes, str]]:
    """→ httpx 的 files（multipart 的 file 字段）"""
    name: str
    data: bytes
    ctype = content_type
    if isinstance(file, bytes):
        name, data = filename or "file", file
    elif isinstance(file, tuple):
        name, data = file[0], _read(file[1])
        if len(file) == 3 and not ctype:
            ctype = file[2]
        name = filename or name
    elif isinstance(file, os.PathLike):
        path = os.fspath(file)
        with open(path, "rb") as f:
            data = f.read()
        name = filename or os.path.basename(path)
    else:
        data = file.read()
        name = filename or os.path.basename(str(getattr(file, "name", "") or "")) or "file"
    if not ctype:
        ctype = mimetypes.guess_type(name)[0] or "application/octet-stream"
    return {"file": (name, data, ctype)}


def csv_body(csv: Union[str, bytes]) -> bytes:
    return csv.encode("utf-8") if isinstance(csv, str) else csv
