# python-inprocess：在 FastAPI 里进程内跑 Open Agent Docs

`open-agent-docs`（PyO3 的 abi3 wheel，`crates/oad-py`）在 Python 进程里起服务器（Rust，自己的线程和运行时，不占 GIL）：

```python
import open_agent_docs as oad
with oad.start(data_dir="./data", auth="header", header_secret="...") as server:
    print(server.url)      # http://127.0.0.1:<端口>
```

这个例子在 FastAPI 的 lifespan 里起它，宿主用 header 登录（`X-OAD-Secret`、`X-OAD-User-*`）以某个用户的身份调接口；`POST /notes` 建页面写正文。

```sh
./smoke.sh                                  # 建 .venv、编 wheel、跑冒烟测试
.venv/bin/uvicorn app:app --port 3002       # 跑起来
```

配置项和命令行一样（`OAD_*`，写成 `data_dir=`、`auth=` 这样的关键字参数也行），见 docs/zh/configuration.md。
