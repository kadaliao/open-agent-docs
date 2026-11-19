#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["httpx>=0.27", "psutil>=5.9"]
# ///
"""把旧版基线（bench.py）用的那份搜索语料和查询原样导出成 JSON，新版的 bench-new.mjs 灌同一份数据、跑同样的查询。
   uv run bench/gen-corpus.py <输出.json>
同样的随机种子（2024 / 5），和 bench.py 的 bench_search 一模一样：500 页 × 约 200 段，50 个查询（中英各 25）。"""
import importlib.util
import json
import random
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location("bench", Path(__file__).with_name("bench.py"))
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)

pages, per_page = 500, 200
r = random.Random(2024)
out = []
for i in range(pages):
    content, nb = b.make_doc(r, per_page)
    title = f"{r.choices(b.ZH, b.ZW)[0]}{r.choice(['周报', '纪要', '方案', '复盘', '需求'])} {r.choice(b.EN)} #{i}"
    out.append({"title": title, "content": content, "blocks": nb})
queries = b.make_queries(random.Random(5))
Path(sys.argv[1]).write_text(json.dumps({"pages": out, "queries": queries}, ensure_ascii=False))
print(f"{len(out)} 页，约 {sum(p['blocks'] for p in out)} 段，{sum(len(p['content']) for p in out)} 字；{len(queries)} 个查询 → {sys.argv[1]}")
