#!/bin/sh
# 冒烟测试：建 .venv（uv）、装 SDK（sdks/python）和进程内服务器（编 crates/oad-py 的 wheel：open-agent-docs-server）、跑 pytest。
set -e
cd "$(dirname "$0")"
UV_VENV_CLEAR=1 uv venv -q .venv
uv pip install -q --python .venv/bin/python fastapi httpx pytest uvicorn maturin
.venv/bin/maturin build -q -m ../../crates/oad-py/Cargo.toml -o .venv/wheels
uv pip install -q --python .venv/bin/python --reinstall .venv/wheels/open_agent_docs_server-*.whl
uv pip install -q --python .venv/bin/python --reinstall ../../sdks/python
.venv/bin/python -m pytest -q test_smoke.py
