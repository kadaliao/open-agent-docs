"""网页实时刷新用的事件：写了什么就 emit，开着的网页经 /api/events（SSE）收到。

事件和 OSA 资料库的一样：
- page_changed {page_id, version, by, title, updated, changed}：这一页的段变了（changed 为空就整页重拉）；
- pages_changed：页面列表要刷新；
- page_comments {page_id}：评论变了；
- db_changed {db_id, rows, schema?, by}：开着的库重新取；
- agents_changed {agents}：作者名单变了。
"""
import asyncio
import json

_subs: set[asyncio.Queue] = set()
MAX_QUEUE = 500


def subscribe() -> asyncio.Queue:
    q: asyncio.Queue = asyncio.Queue(MAX_QUEUE)
    _subs.add(q)
    return q


def unsubscribe(q: asyncio.Queue):
    _subs.discard(q)


async def emit(kind: str, **data):
    msg = json.dumps({"type": kind, **data}, ensure_ascii=False)
    for q in list(_subs):
        try:
            q.put_nowait(msg)
        except asyncio.QueueFull:   # 收不过来的（网页卡住了）：断开，它重连时整个重拉
            _subs.discard(q)
            q.put_nowait(None) if q.empty() else None
