"""本地起服务器：oad.start(...)。

```python
with oad.start(data_dir="./data", auth="header", header_secret="...") as server:
    client = oad.Client(base_url=server.url, header_secret="...")
```

- 装了 `open-agent-docs[server]`（能 import open_agent_docs_server）就在进程里跑（Rust，自己的线程）
- 否则跑二进制：binary= → 环境变量 OAD_BINARY → 缓存 → 从 GitHub 发布页下载（用 SHA256SUMS 验）
- 关键字参数转成环境变量：data_dir → OAD_DATA_DIR（和命令行一样）；不给 port 挑一个空端口
"""
from __future__ import annotations

import hashlib
import os
import platform
import re
import shutil
import signal
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
from pathlib import Path
from typing import Any, Dict, Optional, Protocol, Sequence, cast

import httpx

from ._errors import OADError
from ._version import __version__

__all__ = ["Server", "start", "find_binary"]

RELEASES = "https://github.com/kadaliao/open-agent-docs/releases/download"
READY_TIMEOUT = 30.0
STOP_TIMEOUT = 10.0
TARGETS = {
    ("darwin", "arm64"): "aarch64-apple-darwin",
    ("darwin", "x86_64"): "x86_64-apple-darwin",
    ("linux", "x86_64"): "x86_64-unknown-linux-gnu",
    ("linux", "aarch64"): "aarch64-unknown-linux-gnu",
}


class _InProcess(Protocol):
    url: str
    port: int

    @property
    def closed(self) -> bool: ...

    def close(self) -> None: ...


def env_name(k: str) -> str:
    """data_dir → OAD_DATA_DIR，headerSecret → OAD_HEADER_SECRET；OAD_ 开头的原样（和服务器的 key_name 一样）"""
    if k.startswith("OAD_"):
        return k
    out = "OAD_"
    for i, c in enumerate(k):
        if c.isupper() and i > 0 and not out.endswith("_"):
            out += "_"
        out += "_" if c == "-" else c.upper()
    return out


def env_value(v: Any) -> str:
    if isinstance(v, bool):
        return "1" if v else "0"
    if isinstance(v, (list, tuple)):
        return ",".join(str(x) for x in cast(Sequence[Any], v))
    return str(v)


class Server:
    """跑着的服务器：url、port、close()；可以当上下文管理器用（退出时关掉）。"""

    url: str
    """http://127.0.0.1:<端口>"""
    port: int
    mode: str
    """inprocess（进程内）或 binary（子进程）"""

    def __init__(
        self,
        url: str,
        port: int,
        *,
        inner: Optional[_InProcess] = None,
        process: Optional["subprocess.Popen[bytes]"] = None,
        log: Optional[Path] = None,
    ) -> None:
        self.url = url
        self.port = port
        self._inner = inner
        self._proc = process
        self._log = log
        self.mode = "inprocess" if inner is not None else "binary"
        self._closed = False

    @property
    def closed(self) -> bool:
        return self._closed

    def close(self) -> None:
        """关掉（等页面都存好再返回）；关过了再调什么也不做。"""
        if self._closed:
            return
        self._closed = True
        if self._inner is not None:
            self._inner.close()
        if self._proc is not None:
            _stop(self._proc)
        if self._log is not None:
            try:
                self._log.unlink()
            except OSError:
                pass

    def __enter__(self) -> "Server":
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()

    def __del__(self) -> None:
        try:
            if self._proc is not None and not self._closed:
                self.close()
        except Exception:  # noqa: BLE001
            pass

    def __repr__(self) -> str:
        return f"<open_agent_docs.Server {self.url} ({self.mode})>"


def _stop(p: "subprocess.Popen[bytes]") -> None:
    if p.poll() is not None:
        return
    try:
        p.send_signal(signal.SIGTERM)
        p.wait(STOP_TIMEOUT)
    except subprocess.TimeoutExpired:
        p.kill()
        p.wait()
    except OSError:
        pass


def _free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def _target() -> str:
    sysname = sys.platform
    machine = platform.machine().lower()
    machine = {"amd64": "x86_64", "x64": "x86_64", "aarch64": "aarch64", "arm64": "arm64"}.get(machine, machine)
    if sysname.startswith("linux"):
        machine = "aarch64" if machine == "arm64" else machine
        sysname = "linux"
    elif sysname == "darwin":
        machine = "arm64" if machine == "aarch64" else machine
    t = TARGETS.get((sysname, machine))
    if t is None:
        raise OADError(
            f"没有 {sys.platform}/{platform.machine()} 的预编译二进制：用 Docker（ghcr.io/kadaliao/open-agent-docs）"
            "，或者自己编了用 binary= / OAD_BINARY 指过去"
        )
    return t


def cache_dir() -> Path:
    """用户缓存目录（platformdirs 的规则）/open-agent-docs"""
    if sys.platform == "darwin":
        base = Path.home() / "Library" / "Caches"
    elif sys.platform.startswith("win"):
        base = Path(os.environ.get("LOCALAPPDATA") or (Path.home() / "AppData" / "Local"))
    else:
        base = Path(os.environ.get("XDG_CACHE_HOME") or (Path.home() / ".cache"))
    return base / "open-agent-docs"


def _download(version: str, dest: Path) -> Path:
    target = _target()
    name = f"open-agent-docs-v{version}-{target}"
    url = f"{RELEASES}/v{version}/{name}.tar.gz"
    sums_url = f"{RELEASES}/v{version}/SHA256SUMS"
    with httpx.Client(follow_redirects=True, timeout=httpx.Timeout(60.0, connect=15.0)) as http:
        try:
            sums = http.get(sums_url)
            sums.raise_for_status()
            want = None
            for line in sums.text.splitlines():
                m = re.match(r"^([0-9a-fA-F]{64})\s+\*?(.+)$", line.strip())
                if m and m.group(2).strip() == f"{name}.tar.gz":
                    want = m.group(1).lower()
            if want is None:
                raise OADError(f"SHA256SUMS 里没有 {name}.tar.gz（{sums_url}）")
            dest.mkdir(parents=True, exist_ok=True)
            with tempfile.TemporaryDirectory(dir=dest) as tmp:
                tgz = Path(tmp) / f"{name}.tar.gz"
                h = hashlib.sha256()
                with http.stream("GET", url) as r:
                    r.raise_for_status()
                    with open(tgz, "wb") as f:
                        for chunk in r.iter_bytes():
                            h.update(chunk)
                            f.write(chunk)
                if h.hexdigest() != want:
                    raise OADError(f"下载的 {name}.tar.gz 校验不对（SHA256 应该是 {want}，实际是 {h.hexdigest()}）")
                with tarfile.open(tgz, "r:gz") as tar:
                    member = tar.getmember(f"{name}/open-agent-docs")
                    src = tar.extractfile(member)
                    if src is None:
                        raise OADError(f"{name}.tar.gz 里没有 open-agent-docs")
                    out = Path(tmp) / "open-agent-docs"
                    with src, open(out, "wb") as f:
                        shutil.copyfileobj(src, f)
                out.chmod(0o755)
                final = dest / "open-agent-docs"
                os.replace(out, final)  # 原子地放进缓存：并发下载也不会拿到半个文件
                return final
        except httpx.HTTPError as e:
            raise OADError(f"下载 Open Agent Docs v{version} 失败：{e}（可以自己下载后用 OAD_BINARY 指过去）") from e


def find_binary(binary: Optional[str] = None, version: str = __version__, download: bool = True) -> Path:
    """服务器二进制在哪：binary= → OAD_BINARY → 缓存 → 下载（download=True 时）"""
    for p in (binary, os.environ.get("OAD_BINARY")):
        if p:
            path = Path(p).expanduser()
            if not path.is_file():
                raise OADError(f"服务器二进制不存在：{path}")
            return path
    dest = cache_dir() / f"v{version}"
    cached = dest / "open-agent-docs"
    if cached.is_file():
        return cached
    if not download:
        raise OADError(f"缓存里没有 Open Agent Docs v{version}（{cached}）")
    return _download(version, dest)


def _wait_ready(p: "subprocess.Popen[bytes]", url: str, log: Path) -> None:
    deadline = time.monotonic() + READY_TIMEOUT
    with httpx.Client(timeout=2.0) as http:
        while True:
            code = p.poll()
            if code is not None:
                tail = _tail(log)
                raise OADError(f"服务器启动失败（退出码 {code}）" + (f"：\n{tail}" if tail else ""))
            try:
                if http.get(f"{url}/api/v1/auth/config").status_code == 200:
                    return
            except httpx.HTTPError:
                pass
            if time.monotonic() > deadline:
                _stop(p)
                tail = _tail(log)
                raise OADError(f"服务器 {READY_TIMEOUT:g} 秒内没起来" + (f"：\n{tail}" if tail else ""))
            time.sleep(0.05)


def _tail(log: Path, n: int = 2000) -> str:
    try:
        data = log.read_bytes()[-n:]
    except OSError:
        return ""
    return data.decode("utf-8", "replace").strip()


def _start_binary(path: Path, env: Dict[str, str]) -> Server:
    host = env.get("OAD_HOST") or "127.0.0.1"
    if not env.get("OAD_PORT") or env["OAD_PORT"] == "0":
        env["OAD_PORT"] = str(_free_port())
    port = int(env["OAD_PORT"])
    url_host = "127.0.0.1" if host in ("0.0.0.0", "::", "") else host
    if ":" in url_host:
        url_host = f"[{url_host}]"
    url = f"http://{url_host}:{port}"
    fd, log_name = tempfile.mkstemp(prefix="oad-server-", suffix=".log")
    log = Path(log_name)
    with os.fdopen(fd, "wb") as out:
        p = subprocess.Popen([str(path), "serve"], env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=out)
    try:
        _wait_ready(p, url, log)
    except BaseException:
        _stop(p)
        try:
            log.unlink()
        except OSError:
            pass
        raise
    return Server(url, port, process=p, log=log)


def _server_module() -> Any:
    """装了 open-agent-docs[server] 就返回 open_agent_docs_server 模块"""
    try:
        import importlib

        return importlib.import_module("open_agent_docs_server")
    except ImportError:
        return None


def start(*, binary: Optional[str] = None, version: Optional[str] = None, **options: Any) -> Server:
    """起一个服务器，起好了（GET /api/v1/auth/config 回 200）再返回。

    关键字参数是配置项（data_dir、auth、header_secret、jwt_secret……，或者直接写 OAD_*），值转成字符串
    （列表用逗号连起来，True / False 是 1 / 0，None 不传）。binary= 指定二进制（这时不走进程内）；
    version= 下载哪个版本（默认和 SDK 一样）。
    """
    opts = {k: v for k, v in options.items() if v is not None}
    if binary is None and version is None:
        srv = _server_module()
        if srv is not None:
            inner = cast(_InProcess, srv.start(**opts))
            return Server(str(inner.url), int(inner.port), inner=inner)
    path = find_binary(binary, version or __version__)
    env = dict(os.environ)
    # 不让外面的 OAD_PORT 之类串进来：没给就挑空端口
    env.pop("OAD_PORT", None)
    for k, v in opts.items():
        env[env_name(k)] = env_value(v)
    return _start_binary(path, env)
