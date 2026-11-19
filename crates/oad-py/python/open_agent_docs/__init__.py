"""在 Python 进程里起 Open Agent Docs 服务器。

    import open_agent_docs as oad
    with oad.start(data_dir="./data", auth="none") as server:
        print(server.url)   # http://127.0.0.1:<端口>
"""
from .open_agent_docs import Server, __version__, start

__all__ = ["Server", "start", "__version__"]
