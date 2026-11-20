# open-agent-docs (Python SDK)

English | [简体中文](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/python/README.zh-CN.md)

The Python client for [Open Agent Docs](https://github.com/kadaliao/open-agent-docs): sync and async clients with the same methods,
pydantic v2 models, event streams, webhook verification, JWT signing, embed URLs and a local server launcher.
Dependencies: just `httpx` and `pydantic`. Python 3.9+.

## Install

```sh
pip install open-agent-docs
pip install "open-agent-docs[server]"    # plus the in-process server (oad.start() won't need to download a binary)
```

## Create a client

```python
import open_agent_docs as oad

client = oad.Client(base_url="https://docs.example.com", token="oad_…")
client = oad.Client()            # reads OAD_BASE_URL (default http://127.0.0.1:8848) and OAD_TOKEN
```

`token` can be an agent token (`oad_…`), a session token, or a JWT signed by the host when the server runs with `OAD_AUTH=jwt`. Other options:

| Option | Default | Meaning |
| --- | --- | --- |
| `timeout` | 60 | Per-request timeout in seconds; event streams only use it for connecting |
| `max_retries` | 2 | Retries on 429, 502/503/504 and connection failures (POST/PATCH aren't retried on 5xx) |
| `locale` | none | `zh` / `en`: language of server error messages |
| `headers` | none | Extra headers on every request |
| `header_secret` / `jwt_secret` | none | Calling on behalf of users, see below |
| `jwt_issuer`, `jwt_audience`, `jwt_ttl` | none, none, 3600 | Put into the JWT that `with_user` signs |
| `http_client` | none | Your own `httpx.Client` (proxies, certificates…) |

Method names map one-to-one to the API (see the dotted names in [sdks/operations.json](https://github.com/kadaliao/open-agent-docs/blob/main/sdks/operations.json)): `client.pages.versions.list(...)`,
`client.me.get()`, `client.search(ws, q=...)`… Path parameters are positional, everything else is keyword-only.

## Create a page and write Markdown

```python
ws = client.default_workspace()                      # first workspace in /me (fetched once per client)
page = client.pages.create(ws, title="Weekly", markdown="# This week\n\n- Shipped the Python SDK")   # creates, then writes the body

content = client.pages.content.get(page.id)          # content.markdown, content.version
client.pages.content.replace(page.id, markdown=content.markdown + "\nMore", base=content.version)
client.pages.update(page.id, title="Week 47")
client.pages.update(page.id, after=None)             # explicitly send null: move to the top
for p in client.pages.list(ws):
    print(p.id, p.title)
```

Optional arguments you leave out aren't sent; pass `None` to send `null` (`oad.NOT_GIVEN` means "not given").

Paginated endpoints (`pages.versions.list`, `me.notifications.list`, `trash.list`…) return an `oad.CursorPage`; iterating it walks every page:

```python
page = client.pages.versions.list(page_id, limit=20)
page.items, page.next_cursor, page.has_next_page()    # this page
for v in page:                                        # follows the cursor automatically
    print(v.id, v.label)
```

Uploads and downloads:

```python
f = client.files.upload(ws, b"hello", "a.txt")       # also a pathlib.Path, an open file, or (name, bytes, type)
data = client.files.download(f.id)                   # bytes
db = client.databases.import_csv(ws, "name,qty\napple,3\n", title="Fruit")
```

## Async

```python
async with oad.AsyncClient() as client:
    ws = await client.default_workspace()
    page = await client.pages.create(ws, title="Weekly")
    async for v in client.pages.versions.list(page.id):    # pagination: async for directly (await it for one page)
        ...
```

## Calling on behalf of a user (host backends)

The host does its own login; its backend calls Open Agent Docs as the current user. Run the server with `OAD_AUTH=header` + `OAD_HEADER_SECRET`,
or `OAD_AUTH=jwt` + `OAD_JWT_SECRET`:

```python
admin = oad.Client(base_url=..., header_secret=SECRET)    # or jwt_secret=SECRET
u = admin.with_user("u-42", name="Ming Wang", email="x@example.com")
u.pages.list(u.default_workspace())
```

`with_user` returns a new client (sharing the connection pool and settings); the original is unchanged. Header mode sends `X-OAD-Secret`
and `X-OAD-User-Id / -Name / -Email`; JWT mode signs an HS256 JWT (cached until 60 seconds before it expires), and `workspaces=["ws1"]`
or `[{"id": "ws1", "role": "viewer"}]` goes into the claims.

With FastAPI:

```python
from fastapi import Depends, FastAPI, Request
import open_agent_docs as oad

app = FastAPI()
docs = oad.AsyncClient(base_url="http://127.0.0.1:8848", header_secret=SECRET)   # one for the whole app

async def docs_for(request: Request) -> oad.AsyncClient:
    user = await my_current_user(request)                # the host's own login
    return docs.with_user(user.id, name=user.name, email=user.email)

@app.post("/notes")
async def create_note(title: str, text: str, d: oad.AsyncClient = Depends(docs_for)):
    page = await d.pages.create(await d.default_workspace(), title=title, markdown=text)
    return {"id": page.id}
```

Full example (in-process server, two users): [examples/python-inprocess](https://github.com/kadaliao/open-agent-docs/tree/main/examples/python-inprocess).

## Embedding: sign a JWT, build the iframe URL

```python
token = oad.sign_jwt(SECRET, "u-42", name="Ming Wang", workspaces=["ws1"], ttl=3600)   # HS256, standard library only
url = oad.embed_url("https://docs.example.com", page_id, theme="dark", token=token)
# https://docs.example.com/p/<id>?embed=1&theme=dark#token=…   (the token is after #: it never reaches server logs)
```

For RS256 / ES256 use your own JWT library; claims: `sub`, `name`, `email`, `workspaces`, `iat`, `exp`, `iss`, `aud`.

## Events

```python
with client.workspaces.events(ws) as events:       # connected once inside `with`: nothing after this is missed
    for ev in events:                               # ev.name, ev.id, ev.data
        if ev.name == "page.created":
            print("new page", ev.data.title)        # data is parsed by event name (Page…); unknown events give raw JSON
        elif ev.name == "resync":
            ...                                     # fell behind: reload
```

Reconnects automatically (1 s doubling up to 30 s, with `Last-Event-ID`); 401, 403 and 404 don't reconnect and raise.
`events.close()` from another thread stops it. Async: `async with client.workspaces.events(ws) as events: async for ev in events: ...`.

Agent notifications (fetch → handle → ack):

```python
bot = oad.Client(token=AGENT_TOKEN)
with bot.agent.notifications.stream() as stream:   # replays unacked ones, then pushes live; each data is an AgentNotification
    for ev in stream:
        note = ev.data
        reply = handle(note.prompt)                 # prompt can be sent to your agent as a message
        bot.threads.reply(note.thread_id, body=reply)
        bot.agent.notifications.ack(ids=[note.id])  # ack after handling: unacked ones are replayed on reconnect
```

Without a long-lived connection: `bot.agent.notifications.list(wait=30)` (long polling, returns one page).

## Webhooks

When an agent has a webhook URL, the server POSTs notifications to it with `X-OAD-Timestamp` and `X-OAD-Signature`. Verify with the
`webhook_secret` you got when creating the agent:

```python
note = oad.verify_webhook(WEBHOOK_SECRET, headers, raw_body)    # returns an AgentNotification
# raises oad.WebhookVerificationError, e.kind is signature / stale / malformed; timestamps more than 300 s off are stale
```

Always pass the raw body bytes (don't parse and re-serialize). FastAPI:

```python
from fastapi import FastAPI, HTTPException, Request

@app.post("/oad-webhook")
async def webhook(request: Request):
    try:
        note = oad.verify_webhook(WEBHOOK_SECRET, request.headers, await request.body())
    except oad.WebhookVerificationError as e:
        raise HTTPException(401, e.kind)
    background_tasks.add_task(handle, note)          # answer 2xx quickly, do slow work in the background
    return {"ok": True}
```

Flask:

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

## Run a server locally

```python
with oad.start(data_dir="./data", auth="header", header_secret=SECRET) as server:
    client = oad.Client(base_url=server.url, header_secret=SECRET)
    ...
# stopped when leaving `with` (returns after pages are saved); or server = oad.start(...); ...; server.close()
```

- Keyword arguments become environment variables: `data_dir` → `OAD_DATA_DIR`, `auth` → `OAD_AUTH`… (same as the command line, see
  docs/en/configuration.md); without `port` a free port is picked.
- With `open-agent-docs[server]` installed it runs inside the Python process (Rust, its own threads, doesn't hold the GIL).
- Otherwise it runs the binary: `binary=` → `OAD_BINARY` → cache (`~/Library/Caches/open-agent-docs/v<version>/`,
  `~/.cache/open-agent-docs/v<version>/`) → download from the GitHub release (checked against that release's `SHA256SUMS`).
  macOS 11+ (arm64, x86_64) and Linux (x86_64, aarch64, glibc 2.34+: Ubuntu 22.04+, Debian 12+, RHEL 9+) are supported; use Docker on older systems and elsewhere ([system requirements](https://github.com/kadaliao/open-agent-docs/blob/main/docs/en/quickstart.md#system-requirements)). The `[server]` in-process server has the same requirements; on older Linux pip finds no wheel to install.

## Errors

```python
try:
    client.pages.get("nope")
except oad.NotFoundError as e:
    print(e.status, e.code, e.message)       # 404 not_found …
except oad.RateLimitError as e:
    time.sleep(e.retry_after or 1)
except oad.APIError as e:                    # any other server error: e.status, e.code, e.message, e.request_id
    ...
except oad.APIConnectionError:               # couldn't connect (oad.APITimeoutError for timeouts)
    ...
```

| Status | Class |
| --- | --- |
| 400 | `BadRequestError` |
| 401 | `AuthenticationError` |
| 403 | `PermissionDeniedError` |
| 404 | `NotFoundError` |
| 409 | `ConflictError` (e.g. a stale `base` version) |
| 410 | `GoneError` |
| 413 | `TooLargeError` |
| 429 | `RateLimitError` (`retry_after`) |
| 502 | `BadGatewayError` |
| other 5xx | `InternalServerError` (`BadGatewayError` is a subclass) |

Every error derives from `oad.OADError`; bad configuration raises `oad.ConfigurationError`.

## Development

```sh
cd sdks/python
uv venv .venv && uv pip install --python .venv/bin/python -e . pytest pytest-asyncio pyright
python codegen/generate.py            # regenerate src/open_agent_docs/_generated/ after changing docs/openapi.json or sdks/operations.json
python codegen/generate.py --check    # is the generated code up to date (tests/test_codegen.py checks too)
.venv/bin/python -m pytest            # unit tests; with OAD_BINARY set also the scenario tests (tests/conformance, S1–S15)
.venv/bin/pyright                     # type check (strict)
```

Models and resource classes are generated (don't edit them by hand); the hand-written runtime lives in `src/open_agent_docs/_*.py`.
