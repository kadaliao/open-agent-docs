# Open Agent Docs 的镜像：后端 + 构建好的网页。数据在 /data（挂一个卷）。
#   docker build -t open-agent-docs .
#   docker run -d -p 8790:8790 -v oad-data:/data -e OAD_PASSWORD=... open-agent-docs
FROM node:24-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build    # 产物在 /src/open_agent_docs/static

FROM python:3.13-slim-bookworm
ENV PYTHONUNBUFFERED=1 UV_LINK_MODE=copy OAD_DATA_DIR=/data OAD_HOST=0.0.0.0 OAD_PORT=8790
RUN pip install --no-cache-dir --root-user-action=ignore uv==0.13.6
WORKDIR /app
COPY pyproject.toml uv.lock README.md ./
RUN uv sync --frozen --no-dev --no-install-project
COPY open_agent_docs/ open_agent_docs/
COPY --from=web /src/open_agent_docs/static/ open_agent_docs/static/
RUN uv sync --frozen --no-dev
VOLUME /data
EXPOSE 8790
HEALTHCHECK --interval=30s --timeout=5s CMD python -c "import urllib.request;urllib.request.urlopen('http://127.0.0.1:8790/healthz')"
CMD ["uv", "run", "--no-sync", "open-agent-docs", "serve"]
