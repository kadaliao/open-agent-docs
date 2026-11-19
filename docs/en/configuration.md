# Configuration

[简体中文](../zh/configuration.md) · [Contents](README.md)

Everything is an environment variable; the first few `serve` settings are also command-line flags (`open-agent-docs serve --help`).
An empty value counts as unset. The tables list every `OAD_*` variable the code reads (under `crates/`).

## Server

| Variable | Flag | Default | Meaning |
| --- | --- | --- | --- |
| `OAD_PORT` | `--port` | `8848` | Port |
| `OAD_HOST` | `--host` | `127.0.0.1` | Listen address; `0.0.0.0` to serve others (already set in the Docker image) |
| `OAD_DATA_DIR` | `--data-dir` | `./data` | Data directory: `oad.db` (SQLite), `blobs/` (uploads), `search/` (search index). Also used by `user add` |
| `OAD_WEB_DIR` | `--web-dir` | none | Serve the web app from this directory (development); otherwise the embedded copy |
| `OAD_PUBLIC_URL` | `--public-url` | none | Public address, e.g. `https://docs.example.com`. Used in links in notifications, webhooks and shares; its Host and Origin are allowed automatically |
| `OAD_COOKIE_SECURE` | `--cookie-secure` | off | Mark the session cookie `Secure`. Turn on behind HTTPS (`true` / `1`) |
| `OAD_ALLOWED_HOSTS` | `--allowed-hosts` | none | Extra allowed Host values, comma-separated; `*` disables the check. See "Request origin" below |
| `OAD_ALLOWED_ORIGINS` | `--allowed-origins` | none | Origins allowed cross-origin (when embedding from another domain), comma-separated, e.g. `https://app.example.com` |
| `OAD_RATE_LIMIT` | | `1200` | Requests per minute per token / session (per IP without one); `0` for no limit |
| `OAD_MAX_UPLOAD_MB` | | `100` | Upload size limit (MB) |
| `OAD_TZ` | | system | Server time zone, IANA name such as `Europe/Berlin`. Used for "today", relative dates and time display in databases; falls back to the system zone if unknown |

Log level uses the standard `RUST_LOG` (default `info`), e.g. `RUST_LOG=warn` or `RUST_LOG=oad_server=debug`.

## Sign-in

| Variable | Flag | Default | Meaning |
| --- | --- | --- | --- |
| `OAD_AUTH` | `--auth` | `local` | `local`: built-in accounts; `header`: a host app signs people in and forwards user headers; `none`: single local user, no sign-in; `jwt`: the host signs JWTs (coming, see [Embedding](embedding.md#jwt-coming)) |
| `OAD_ADMIN_EMAIL`, `OAD_ADMIN_PASSWORD` | | none | `local` mode: when there are no users yet, create the admin (workspace owner) at startup. Ignored once any user exists |
| `OAD_SIGNUP` | | `invite` | `local` mode sign-up: `invite` only with an invite link; `open` anyone; `off` nobody |
| `OAD_USER_PASSWORD` | | none | Password for `open-agent-docs user add`; read from the terminal if unset |
| `OAD_HEADER_SECRET` | | none | Required in `header` mode: the `X-OAD-Secret` request header must match it |
| `OAD_JWT_SECRET` | | none | `jwt` mode: HS256 key (or use `OAD_JWT_JWKS_URL`) |
| `OAD_JWT_JWKS_URL` | | none | `jwt` mode: JWKS URL (RS256 / ES256 and so on) |
| `OAD_JWT_ISSUER`, `OAD_JWT_AUDIENCE` | | none | `jwt` mode: check `iss` / `aud` when set |
| `OAD_JWT_LEEWAY` | | `60` | `jwt` mode: allowed clock skew for expiry (seconds) |

## Notifications and fetching other sites

| Variable | Default | Meaning |
| --- | --- | --- |
| `OAD_WEBHOOK_ALLOW_PRIVATE` | off | `1` / `true`: allow webhooks to private addresses. Local development only; by default only public addresses (SSRF protection) |
| `OAD_FETCH_PROXY` | none | Proxy for fetching link previews (title, image, player), e.g. `http://127.0.0.1:7890` |
| `OAD_FETCH_ALLOW_CIDRS` | none | Private ranges link previews may fetch, comma-separated, e.g. `10.0.0.0/8`. Private addresses are refused by default |
| `OAD_GITHUB_TOKEN` | none | Token for the GitHub API when previewing GitHub links (higher rate limit) |

## Test-only

`OAD_UNFURL_SKIP` (skip link-preview steps), `OAD_FUZZ` (rounds of the Markdown fuzz test), `OAD_WRITE_OPENAPI=1` (regenerate
`docs/openapi.json`). Not used in deployments.

## Request origin (Host and Origin)

- **Host** guards against DNS rebinding. Always checked in `none` mode; in other modes only when `OAD_ALLOWED_HOSTS` or
  `OAD_PUBLIC_URL` is set. `localhost`, `127.0.0.1` and `[::1]` are always allowed. If you reach a `none`-mode server by another
  name or IP, add it to `OAD_ALLOWED_HOSTS`.
- **Origin**: requests that change data (POST / PUT / PATCH / DELETE) and WebSocket handshakes that carry an Origin must be
  same-origin, listed in `OAD_ALLOWED_ORIGINS`, or `OAD_PUBLIC_URL`. Requests without an Origin (agents, curl, server to server) pass.
- Behind a reverse proxy: set `OAD_PUBLIC_URL` and have the proxy pass the original `Host` (or `X-Forwarded-Host`). See [Deployment](deployment.md).
