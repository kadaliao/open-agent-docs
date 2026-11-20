# python-inprocess：在 FastAPI 里进程内跑 Open Agent Docs

Python SDK（`open-agent-docs`，[sdks/python](../../sdks/python)）加上进程内服务器（`open-agent-docs-server`，PyO3 的 abi3 wheel，`crates/oad-py`）：
`oad.start()` 发现装了服务器就在 Python 进程里跑（Rust，自己的线程和运行时，不占 GIL），没装就下载二进制跑子进程。

```sh
pip install "open-agent-docs[server]"
```

```python
import open_agent_docs as oad

with oad.start(data_dir="./data", auth="header", header_secret=SECRET) as server:
    admin = oad.AsyncClient(base_url=server.url, header_secret=SECRET)
    u = admin.with_user("alice", name="Alice 王")        # 以宿主当前用户的身份调
    page = await u.pages.create(await u.default_workspace(), title="周报", markdown="# 本周\n\n…")
```

这个例子在 FastAPI 的 lifespan 里起服务器和一个共用的 `AsyncClient`；宿主自己管登录（演示用 `x-demo-user` 头），
每个请求用 `with_user` 出一个“替当前用户调”的客户端（带 `X-OAD-Secret`、`X-OAD-User-*`，共享连接池）。
`POST /notes` 建页面写正文，`GET /notes` 列页面，`GET /notes/{id}` 读正文，`GET /me` 看 Open Agent Docs 眼里的当前用户。

```sh
./smoke.sh                                  # 建 .venv、编 wheel、装 SDK、跑冒烟测试
.venv/bin/uvicorn app:app --port 3002       # 跑起来
curl -X POST localhost:3002/notes -H 'x-demo-user: alice' -H 'content-type: application/json' \
     -d '{"title": "周报", "text": "# 本周\n\n- 上线了 SDK"}'
```

配置项和命令行一样（`OAD_*`，写成 `data_dir=`、`auth=` 这样的关键字参数也行），见 docs/zh/configuration.md。
