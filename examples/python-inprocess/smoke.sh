#!/bin/sh
# 冒烟测试：建 .venv（uv）、编 open-agent-docs 的 wheel 装进去、跑 pytest。
set -e
cd "$(dirname "$0")"
uv venv -q .venv
uv pip install -q --python .venv/bin/python fastapi httpx pytest uvicorn maturin
.venv/bin/maturin build -q -m ../../crates/oad-py/Cargo.toml -o .venv/wheels
uv pip install -q --python .venv/bin/python --reinstall .venv/wheels/open_agent_docs-*.whl
.venv/bin/python -m pytest -q test_smoke.py
