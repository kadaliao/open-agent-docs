# open-agent-docs-server

[English](https://github.com/kadaliao/open-agent-docs/blob/main/crates/oad-py/README.md) | 简体中文

[Open Agent Docs](https://github.com/kadaliao/open-agent-docs) 服务器（Rust，PyO3）做成的 Python 扩展模块：在 Python 进程里起，跑在自己的线程上，不占 GIL。

一般不用直接装它。装 SDK 时带上 `server` 这个 extra，`oad.start()` 发现它就在进程里跑服务器，不用下载二进制：

```sh
pip install "open-agent-docs[server]"
```

```python
import open_agent_docs as oad

with oad.start(data_dir="./data", auth="none") as server:
    client = oad.Client(base_url=server.url)
    ...
```

直接用也行，参数一样：

```python
import open_agent_docs_server

with open_agent_docs_server.start(data_dir="./data", auth="none") as server:
    print(server.url)   # http://127.0.0.1:<端口>
```

提供 macOS 11+（arm64、x86_64）和 Linux（x86_64、aarch64，glibc 2.34 起：Ubuntu 22.04+、Debian 12+、RHEL 9+）的 wheel。更老的系统和 Windows 请用 Docker 跑服务器，
见[系统要求](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/quickstart.md#系统要求)。

- [Python SDK](https://pypi.org/project/open-agent-docs/)（[English](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/python/README.md)）
- [配置](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/configuration.md)（[English](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/configuration.md)）
