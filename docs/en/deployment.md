# Deployment

[简体中文](../zh/deployment.md) · [Contents](README.md)

To serve others: run Open Agent Docs on a loopback or private address and put an HTTPS reverse proxy in front.

```sh
OAD_HOST=127.0.0.1 OAD_PORT=8848 OAD_DATA_DIR=/var/lib/oad \
OAD_PUBLIC_URL=https://docs.example.com OAD_COOKIE_SECURE=true \
  open-agent-docs serve
```

- `OAD_PUBLIC_URL`: the public address, used in links in notifications, webhooks and shares; its Host and Origin are allowed.
- `OAD_COOKIE_SECURE=true`: the sign-in cookie only travels over HTTPS.
- Everything else: [Configuration](configuration.md).

## Reverse proxy

Forward every path (web app, `/api`, `/mcp`, share pages at `/s/…`). In particular:

- `/api/v1/sync/*` is a WebSocket (collaborative editing) and needs Upgrade;
- `/api/v1/workspaces/*/events` and `/api/v1/agent/notifications/stream` are SSE and must not be buffered;
- `/mcp` is Streamable HTTP (which also uses SSE);
- pass the original `Host` (or `X-Forwarded-Host`); the server uses it for same-origin checks.

**Caddy** (gets certificates automatically; WebSocket and SSE need no extra settings):

```
docs.example.com {
    reverse_proxy 127.0.0.1:8848
}
```

**nginx**:

```nginx
server {
    listen 443 ssl;
    http2 on;
    server_name docs.example.com;
    ssl_certificate     /etc/letsencrypt/live/docs.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/docs.example.com/privkey.pem;

    client_max_body_size 100m;          # match OAD_MAX_UPLOAD_MB

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

# in the http block:
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

On SIGTERM the server finishes writing pending changes before it exits.

## Docker Compose

```yaml
services:
  oad:
    build: .                       # or a published image
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

## Backups

All data lives in `OAD_DATA_DIR`:

| Path | Contents | Back up |
| --- | --- | --- |
| `oad.db` (with `oad.db-wal`, `oad.db-shm`) | Pages, databases, comments, accounts, agents, history (SQLite, WAL mode) | Yes |
| `blobs/` | Uploaded images and files | Yes |
| `search/` | Search index | Optional: rebuilt from the database at startup if missing |

Don't copy `oad.db` while the server runs (you may catch it half-written). Use SQLite's online backup:

```sh
sqlite3 /var/lib/oad/oad.db ".backup '/backup/oad-$(date +%F).db'"
rsync -a /var/lib/oad/blobs/ /backup/blobs/
```

Or stop the server and copy the whole directory. To restore: stop the server, put `oad.db` and `blobs/` back (delete any old
`oad.db-wal` and `oad.db-shm`), start it.

Backing up a Docker volume:

```sh
docker run --rm -v oad-data:/data -v "$PWD":/backup alpine \
  sh -c 'apk add -q sqlite && sqlite3 /data/oad.db ".backup /backup/oad.db" && tar czf /backup/blobs.tgz -C /data blobs'
```

## Upgrading

Replace the binary (or image) and restart. Database schema upgrades run automatically at startup; back up first.
