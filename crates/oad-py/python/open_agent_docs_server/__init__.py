"""在 Python 进程里起 Open Agent Docs 服务器（Rust，PyO3）。

一般不直接用它：装 `open-agent-docs[server]`，SDK 的 `oad.start()` 发现它就在进程里跑，不用下载二进制。

    import open_agent_docs as oad
    with oad.start(data_dir="./data", auth="none") as server:
        print(server.url)   # http://127.0.0.1:<端口>

直接用也行（参数一样）：

    import open_agent_docs_server
    with open_agent_docs_server.start(data_dir="./data", auth="none") as server: ...
"""
from .open_agent_docs_server import Server, __version__, start

__all__ = ["Server", "start", "__version__"]
