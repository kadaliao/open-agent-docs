# open-agent-docs-server

English | [简体中文](https://github.com/kadaliao/open-agent-docs/blob/main/crates/oad-py/README.zh-CN.md)

The [Open Agent Docs](https://github.com/kadaliao/open-agent-docs) server (Rust, PyO3) as a Python extension module: it runs inside your Python process, on its own threads, without holding the GIL.

You normally don't use this package directly. Install the SDK with the `server` extra, and `oad.start()` finds it and runs the server in-process instead of downloading a binary:

```sh
pip install "open-agent-docs[server]"
```

```python
import open_agent_docs as oad

with oad.start(data_dir="./data", auth="none") as server:
    client = oad.Client(base_url=server.url)
    ...
```

Using it directly takes the same arguments:

```python
import open_agent_docs_server

with open_agent_docs_server.start(data_dir="./data", auth="none") as server:
    print(server.url)   # http://127.0.0.1:<port>
```

Wheels are built for macOS 11+ (arm64, x86_64) and Linux (x86_64, aarch64, glibc 2.34+: Ubuntu 22.04+, Debian 12+, RHEL 9+). On older systems and on Windows, run the server with Docker instead;
see the [system requirements](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/quickstart.md#system-requirements).

- [Python SDK](https://pypi.org/project/open-agent-docs/) ([简体中文](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/python/README.zh-CN.md))
- [Configuration](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/configuration.md) ([简体中文](https://github.com/kadaliao/open-agent-docs/blob/main/docs/zh/configuration.md))
