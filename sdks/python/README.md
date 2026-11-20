# open-agent-docs（Python SDK）

[English](README.en.md)

[Open Agent Docs](https://github.com/kadaliao/open-agent-docs) 的 Python 客户端：同步、异步两套一样的方法，模型是 pydantic v2，
带事件流、webhook 验签、签 JWT、嵌入地址和本地起服务器。依赖只有 `httpx` 和 `pydantic`，Python 3.9 起。

## 安装

```sh
pip install open-agent-docs
pip install "open-agent-docs[server]"    # 再加上进程内的服务器（oad.start() 就不用下载二进制了）
```

## 建客户端

```python
import open_agent_docs as oad

client = oad.Client(base_url="https://docs.example.com", token="oad_…")
client = oad.Client()            # 读环境变量 OAD_BASE_URL（默认 http://127.0.0.1:8848）、OAD_TOKEN
```

`token` 可以是 Agent 的 token（`oad_…`）、登录后的会话令牌，或者 `OAD_AUTH=jwt` 时宿主签的 JWT。其它参数：

| 参数 | 默认 | 说明 |
| --- | --- | --- |
| `timeout` | 60 | 单个请求的超时（秒）；事件流只管连接 |
| `max_retries` | 2 | 429、502/503/504、连不上时重试几次（POST、PATCH 遇到 5xx 不重试） |
| `locale` | 无 | `zh` / `en`：服务器错误信息的语言 |
| `headers` | 无 | 每个请求额外带的头 |
| `header_secret` / `jwt_secret` | 无 | 替用户调，见下面 |
| `jwt_issuer`、`jwt_audience`、`jwt_ttl` | 无、无、3600 | `with_user` 签 JWT 时写进去 |
| `http_client` | 无 | 自己的 `httpx.Client`（代理、证书之类） |

方法名和接口一一对应（点路径见 [sdks/operations.json](../operations.json)）：`client.pages.versions.list(...)`、`client.me.get()`、`client.search(ws, q=...)`……
路径参数按顺序是位置参数，别的都是关键字参数。

## 建页面、写 Markdown

```python
ws = client.default_workspace()                      # /me 里的第一个工作区（每个客户端只取一次）
page = client.pages.create(ws, title="周报", markdown="# 本周\n\n- 上线了 Python SDK")   # 建完再写正文，两步

content = client.pages.content.get(page.id)          # content.markdown、content.version
client.pages.content.replace(page.id, markdown=content.markdown + "\n更多", base=content.version)
client.pages.update(page.id, title="第 47 周")
client.pages.update(page.id, after=None)             # 明确发 null：放到最前
for p in client.pages.list(ws):
    print(p.id, p.title)
```

可选参数不传就不发；要发 `null` 就传 `None`（`oad.NOT_GIVEN` 是“没给”）。

翻页的接口（`pages.versions.list`、`me.notifications.list`、`trash.list`……）返回一页 `oad.CursorPage`，直接 `for` 就会逐条取完所有页：

```python
page = client.pages.versions.list(page_id, limit=20)
page.items, page.next_cursor, page.has_next_page()    # 这一页
for v in page:                                        # 自动带 cursor 取后面的
    print(v.id, v.label)
```

上传、下载：

```python
f = client.files.upload(ws, b"hello", "a.txt")       # 也可以是 pathlib.Path、打开的文件、(文件名, bytes, 类型)
data = client.files.download(f.id)                   # bytes
db = client.databases.import_csv(ws, "名字,数量\n苹果,3\n", title="水果")
```

## 异步

```python
async with oad.AsyncClient() as client:
    ws = await client.default_workspace()
    page = await client.pages.create(ws, title="周报")
    async for v in client.pages.versions.list(page.id):    # 翻页：直接 async for（await 它拿第一页）
        ...
```

## 替用户调（宿主后端）

宿主自己做登录，后端以当前用户的身份调 Open Agent Docs。服务器配 `OAD_AUTH=header` + `OAD_HEADER_SECRET`，或者 `OAD_AUTH=jwt` + `OAD_JWT_SECRET`：

```python
admin = oad.Client(base_url=..., header_secret=SECRET)    # 或 jwt_secret=SECRET
u = admin.with_user("u-42", name="王小明", email="x@example.com")
u.pages.list(u.default_workspace())
```

`with_user` 返回一个新的客户端（共享连接池和配置），原来的不变。header 模式带 `X-OAD-Secret`、`X-OAD-User-Id / -Name / -Email`；
JWT 模式签一个 HS256 的 JWT（缓存到过期前 60 秒），`workspaces=["ws1"]` 或 `[{"id": "ws1", "role": "viewer"}]` 写进 claims。

FastAPI 里：

```python
from fastapi import Depends, FastAPI, Request
import open_agent_docs as oad

app = FastAPI()
docs = oad.AsyncClient(base_url="http://127.0.0.1:8848", header_secret=SECRET)   # 全局一个

async def docs_for(request: Request) -> oad.AsyncClient:
    user = await my_current_user(request)                # 宿主自己的登录
    return docs.with_user(user.id, name=user.name, email=user.email)

@app.post("/notes")
async def create_note(title: str, text: str, d: oad.AsyncClient = Depends(docs_for)):
    page = await d.pages.create(await d.default_workspace(), title=title, markdown=text)
    return {"id": page.id}
```

完整的例子（进程内起服务器、两个用户）：[examples/python-inprocess](../../examples/python-inprocess)。

## 嵌入：签 JWT、iframe 地址

```python
token = oad.sign_jwt(SECRET, "u-42", name="王小明", workspaces=["ws1"], ttl=3600)   # HS256，只用标准库
url = oad.embed_url("https://docs.example.com", page_id, theme="dark", token=token)
# https://docs.example.com/p/<id>?embed=1&theme=dark#token=…   （token 在 # 后面：不进服务器日志）
```

要 RS256 / ES256 的用自己的 JWT 库签，claims：`sub`、`name`、`email`、`workspaces`、`iat`、`exp`、`iss`、`aud`。

## 事件

```python
with client.workspaces.events(ws) as events:       # 进 with 就连上了：之后发生的事件不会漏
    for ev in events:                               # ev.name、ev.id、ev.data
        if ev.name == "page.created":
            print("新页面", ev.data.title)          # data 按事件名解析成模型（Page……），不认识的给原始 JSON
        elif ev.name == "resync":
            ...                                     # 落下了：重新拉一遍
```

断了自动重连（1 秒起翻倍、最多 30 秒，带 `Last-Event-ID`）；401、403、404 不重连，直接抛错。别的线程里 `events.close()` 能让它停下。
异步版：`async with client.workspaces.events(ws) as events: async for ev in events: ...`。

Agent 收通知（取 → 处理 → 确认）：

```python
bot = oad.Client(token=AGENT_TOKEN)
with bot.agent.notifications.stream() as stream:   # 先补发没确认的，再实时推；每条 data 是 AgentNotification
    for ev in stream:
        note = ev.data
        reply = handle(note.prompt)                 # prompt 可以直接当消息发给你的 Agent
        bot.threads.reply(note.thread_id, body=reply)
        bot.agent.notifications.ack(ids=[note.id])  # 处理完再确认：断了重连会补发没确认的
```

不想开长连接就轮询：`bot.agent.notifications.list(wait=30)`（长轮询，返回一页）。

## Webhook

Agent 配了 webhook 地址时，服务器把通知 POST 过去，带 `X-OAD-Timestamp`、`X-OAD-Signature`。用建 Agent 时给的 `webhook_secret` 验：

```python
note = oad.verify_webhook(WEBHOOK_SECRET, headers, raw_body)    # 返回 AgentNotification
# 不对抛 oad.WebhookVerificationError，e.kind 是 signature / stale / malformed；时间戳差超过 300 秒算 stale
```

一定用原始的正文字节（别解析了再序列化）。FastAPI：

```python
from fastapi import FastAPI, HTTPException, Request

@app.post("/oad-webhook")
async def webhook(request: Request):
    try:
        note = oad.verify_webhook(WEBHOOK_SECRET, request.headers, await request.body())
    except oad.WebhookVerificationError as e:
        raise HTTPException(401, e.kind)
    background_tasks.add_task(handle, note)          # 尽快回 2xx，慢活放后台
    return {"ok": True}
```

Flask：

```python
from flask import Flask, abort, request

@app.post("/oad-webhook")
def webhook():
    try:
        note = oad.verify_webhook(WEBHOOK_SECRET, request.headers, request.get_data())
    except oad.WebhookVerificationError:
        abort(401)
    queue.put(note)
    return {"ok": True}
```

## 本地起服务器

```python
with oad.start(data_dir="./data", auth="header", header_secret=SECRET) as server:
    client = oad.Client(base_url=server.url, header_secret=SECRET)
    ...
# 退出 with 时关掉（页面都存好再返回）；或者 server = oad.start(...); ...; server.close()
```

- 关键字参数转成环境变量：`data_dir` → `OAD_DATA_DIR`、`auth` → `OAD_AUTH`……（和命令行一样，见 docs/zh/configuration.md）；不给 `port` 挑一个空端口。
- 装了 `open-agent-docs[server]` 就在进程里跑（Rust，自己的线程，不占 GIL）。
- 没装就跑二进制：`binary=` → 环境变量 `OAD_BINARY` → 缓存（`~/Library/Caches/open-agent-docs/v<版本>/`、`~/.cache/open-agent-docs/v<版本>/`）→
  从 GitHub 发布页下载（用同一个 release 的 `SHA256SUMS` 校验）。支持 macOS（arm64、x86_64）和 Linux（x86_64、aarch64），别的平台用 Docker。

## 错误

```python
try:
    client.pages.get("nope")
except oad.NotFoundError as e:
    print(e.status, e.code, e.message)       # 404 not_found 页面 nope 不存在
except oad.RateLimitError as e:
    time.sleep(e.retry_after or 1)
except oad.APIError as e:                    # 服务器回的别的错误：e.status、e.code、e.message、e.request_id
    ...
except oad.APIConnectionError:               # 连不上（oad.APITimeoutError 是超时）
    ...
```

| 状态码 | 类 |
| --- | --- |
| 400 | `BadRequestError` |
| 401 | `AuthenticationError` |
| 403 | `PermissionDeniedError` |
| 404 | `NotFoundError` |
| 409 | `ConflictError`（比如 `base` 版本对不上） |
| 410 | `GoneError` |
| 413 | `TooLargeError` |
| 429 | `RateLimitError`（`retry_after`） |
| 502 | `BadGatewayError` |
| 其它 5xx | `InternalServerError`（`BadGatewayError` 是它的子类） |

所有错误的基类是 `oad.OADError`；配置不对是 `oad.ConfigurationError`。

## 开发

```sh
cd sdks/python
uv venv .venv && uv pip install --python .venv/bin/python -e . pytest pytest-asyncio pyright
python codegen/generate.py            # 改了 docs/openapi.json 或 sdks/operations.json 后重新生成（src/open_agent_docs/_generated/）
python codegen/generate.py --check    # 生成的是不是最新的（tests/test_codegen.py 也会查）
.venv/bin/python -m pytest            # 单元测试；设了 OAD_BINARY 还跑场景测试（tests/conformance，S1–S15）
.venv/bin/pyright                     # 类型检查（strict）
```

模型和资源类是生成的（别手改），手写的运行时在 `src/open_agent_docs/_*.py`。
