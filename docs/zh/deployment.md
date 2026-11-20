# 部署

[English](../en/deployment.md) · [文档目录](README.md)

对外提供服务时：Open Agent Docs 只监听本机或内网，前面放一个做 HTTPS 的反向代理。

```sh
OAD_HOST=127.0.0.1 OAD_PORT=8848 OAD_DATA_DIR=/var/lib/oad \
OAD_PUBLIC_URL=https://docs.example.com OAD_COOKIE_SECURE=true \
  open-agent-docs serve
```

- `OAD_PUBLIC_URL`：对外的地址。通知、webhook、分享链接里用它；它的 Host 和 Origin 自动允许。
- `OAD_COOKIE_SECURE=true`：登录 cookie 只走 HTTPS。
- 别的设置见[配置](configuration.md)。

## 反向代理

要转发的：所有路径（网页、`/api`、`/mcp`、分享页 `/s/…`）。其中：

- `/api/v1/sync/*` 是 WebSocket（协作编辑），要支持 Upgrade；
- `/api/v1/workspaces/*/events`、`/api/v1/agent/notifications/stream` 是 SSE，不能缓冲；
- `/mcp` 是 Streamable HTTP（也会用 SSE）；
- 代理要传原来的 `Host`（或 `X-Forwarded-Host`），服务器靠它判断同源。

**Caddy**（自动申请证书，WebSocket、SSE 不用额外配置）：

```
docs.example.com {
    reverse_proxy 127.0.0.1:8848
}
```

**nginx**：

```nginx
server {
    listen 443 ssl;
    http2 on;
    server_name docs.example.com;
    ssl_certificate     /etc/letsencrypt/live/docs.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/docs.example.com/privkey.pem;

    client_max_body_size 100m;          # 和 OAD_MAX_UPLOAD_MB 对上

    location / {
        proxy_pass http://127.0.0.1:8848;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;     # WebSocket
        proxy_set_header Connection $connection_upgrade;
        proxy_buffering off;                        # SSE
        proxy_read_timeout 1h;
    }
}

# http 块里：
map $http_upgrade $connection_upgrade { default upgrade; '' close; }
```

## systemd

```ini
# /etc/systemd/system/open-agent-docs.service
[Unit]
Description=Open Agent Docs
After=network.target

[Service]
User=oad
Environment=OAD_DATA_DIR=/var/lib/oad
Environment=OAD_PUBLIC_URL=https://docs.example.com
Environment=OAD_COOKIE_SECURE=true
ExecStart=/usr/local/bin/open-agent-docs serve
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

收到 SIGTERM 时服务器先把内存里的改动写完再退出。

## Docker Compose

仓库根目录的 [docker-compose.yml](../../docker-compose.yml) 下载下来就能用（`curl -fsSLO https://raw.githubusercontent.com/kadaliao/open-agent-docs/main/docker-compose.yml`）。对外提供服务时大致是这样：

```yaml
services:
  oad:
    image: ghcr.io/kadaliao/open-agent-docs:latest   # 自己编的话换成 build: .
    restart: unless-stopped
    ports: ["127.0.0.1:8848:8848"]
    volumes: ["oad-data:/data"]
    environment:
      OAD_PUBLIC_URL: https://docs.example.com
      OAD_COOKIE_SECURE: "true"
      OAD_ADMIN_EMAIL: me@example.com
      OAD_ADMIN_PASSWORD: ${OAD_ADMIN_PASSWORD}
volumes:
  oad-data:
```

## 备份

所有数据都在 `OAD_DATA_DIR` 里：

| 路径 | 内容 | 备份 |
| --- | --- | --- |
| `oad.db`（和 `oad.db-wal`、`oad.db-shm`） | 页面、数据库、评论、账号、Agent、历史版本（SQLite，WAL 模式） | 要 |
| `blobs/` | 上传的图片和文件 | 要 |
| `search/` | 搜索索引 | 可以不备：丢了启动时按库重新建 |

服务器运行时不要直接复制 `oad.db`（可能复制到写了一半的状态）。用 SQLite 的在线备份：

```sh
sqlite3 /var/lib/oad/oad.db ".backup '/backup/oad-$(date +%F).db'"
rsync -a /var/lib/oad/blobs/ /backup/blobs/
```

或者先停服务再整个目录复制。恢复：停服务，把 `oad.db` 和 `blobs/` 放回去（删掉旧的 `oad.db-wal`、`oad.db-shm`），启动。

Docker 卷的备份：

```sh
docker run --rm -v oad-data:/data -v "$PWD":/backup alpine \
  sh -c 'apk add -q sqlite && sqlite3 /data/oad.db ".backup /backup/oad.db" && tar czf /backup/blobs.tgz -C /data blobs'
```

## 升级

换掉二进制（或镜像）重启，用 Docker Compose 的话是 `docker compose pull && docker compose up -d`。数据库结构的升级在启动时自动做；升级前先备份。
