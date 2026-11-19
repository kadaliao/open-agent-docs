# Configuration

[简体中文](../zh/configuration.md) · [Contents](README.md)

Everything is an environment variable; the first few `serve` settings are also command-line flags (`open-agent-docs serve --help`).
When starting in-process (Node's `@oad/server`, Python's `open_agent_docs`) the same names are options; `OAD_DATA_DIR` can also be written
`dataDir` / `data_dir`, and anything not given falls back to the environment (see [Embedding](embedding.md#in-process-node--python)).
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

## Storage

| Variable | Default | Meaning |
| --- | --- | --- |
| `OAD_DATABASE_URL` | none | `postgres://user:pass@host:5432/db`: use Postgres instead of SQLite in the data dir. Uploads and the search index stay in `OAD_DATA_DIR`. Add `?sslmode=require` for TLS (verified against public CAs); otherwise unencrypted |
| `OAD_BLOB_URL` | none | `s3://<bucket>/<prefix>?region=<region>&endpoint=<url>`: keep uploads and thumbnails in S3-compatible storage (AWS S3, MinIO, R2…). With `endpoint`, path-style addressing (MinIO-style); without, AWS virtual-hosted |
| `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` | none | Credentials for `OAD_BLOB_URL` (required); optional `AWS_SESSION_TOKEN`, `AWS_REGION` (when the URL has no region) |

## Multi-tenancy

| Variable | Default | Meaning |
| --- | --- | --- |
| `OAD_TENANTS` | none (single tenant) | `host:<domain>`: tenant by subdomain (`acme.docs.example.com` is tenant `acme`, `docs.example.com` itself is `main`); `header:<header name>`: by request header (e.g. `header:x-oad-tenant`; only behind a trusted reverse proxy). Each tenant's data lives in `<OAD_DATA_DIR>/tenants/<tenant>/`; with Postgres in schema `t_<tenant>`, with S3 under `<prefix>/<tenant>`. Tenant names: lowercase letters, digits, `-` |
| `OAD_TENANT_SECRET` | none | Optional for `header:`: also require `X-OAD-Tenant-Secret` to match |

Log level uses the standard `RUST_LOG` (default `info`), e.g. `RUST_LOG=warn` or `RUST_LOG=oad_server=debug`.

## Sign-in

| Variable | Flag | Default | Meaning |
| --- | --- | --- | --- |
| `OAD_AUTH` | `--auth` | `local` | `local`: built-in accounts; `header`: a host app signs people in and forwards user headers; `none`: single local user, no sign-in; `jwt`: the host signs JWTs (see [Embedding](embedding.md#jwt)) |
| `OAD_ADMIN_EMAIL`, `OAD_ADMIN_PASSWORD` | | none | `local` mode: when there are no users yet, create the admin (workspace owner) at startup. Ignored once any user exists |
| `OAD_SIGNUP` | | `invite` | `local` mode sign-up: `invite` only with an invite link; `open` anyone; `off` nobody |
| `OAD_USER_PASSWORD` | | none | Password for `open-agent-docs user add`; read from the terminal if unset |
| `OAD_HEADER_SECRET` | | none | Required in `header` mode: the `X-OAD-Secret` request header must match it |
| `OAD_JWT_SECRET` | | none | `jwt` mode: HS256 / HS384 / HS512 shared secret (give this and/or `OAD_JWT_JWKS_URL`) |
| `OAD_JWT_JWKS_URL` | | none | `jwt` mode: JWKS URL (RS256 / RS384 / RS512, ES256 / ES384, EdDSA). Cached by kid for 10 minutes; an unknown kid triggers a refetch (at most every 30 seconds) |
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
`docs/openapi.json`), `OAD_TEST_POSTGRES_URL` (run the whole server test suite on Postgres), `OAD_TEST_S3_URL` (test the blob store
against a real S3 / MinIO). Not used in deployments.

## Request origin (Host and Origin)

- **Host** guards against DNS rebinding. Always checked in `none` mode; in other modes only when `OAD_ALLOWED_HOSTS` or
  `OAD_PUBLIC_URL` is set. `localhost`, `127.0.0.1` and `[::1]` are always allowed. If you reach a `none`-mode server by another
  name or IP, add it to `OAD_ALLOWED_HOSTS`.
- **Origin**: requests that change data (POST / PUT / PATCH / DELETE) and WebSocket handshakes that carry an Origin must be
  same-origin, listed in `OAD_ALLOWED_ORIGINS`, or `OAD_PUBLIC_URL`. Requests without an Origin (agents, curl, server to server) pass.
- Behind a reverse proxy: set `OAD_PUBLIC_URL` and have the proxy pass the original `Host` (or `X-Forwarded-Host`). See [Deployment](deployment.md).
