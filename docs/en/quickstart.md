# Quick start

[简体中文](../zh/quickstart.md) · [Contents](README.md)

By default the server listens on `127.0.0.1:8848` and keeps its data in `./data`.

## Binary

Download `open-agent-docs` for your platform from the releases page (once published), or [build it from source](#build-from-source).

```sh
# When there are no users yet, these two create the admin (ignored once any user exists)
OAD_ADMIN_EMAIL=me@example.com OAD_ADMIN_PASSWORD='at least 8 chars' \
  ./open-agent-docs serve --data-dir ~/oad-data
```

Open http://127.0.0.1:8848 and sign in with that email and password.

Or create the account first:

```sh
./open-agent-docs user add --email me@example.com --name Me --role owner --data-dir ~/oad-data
# The password comes from OAD_USER_PASSWORD, or is read from the terminal
```

Just for yourself on your own machine, without signing in:

```sh
OAD_AUTH=none ./open-agent-docs serve
```

In `none` mode every request is the same local user. Keep it on a loopback address (the server warns if you don't: anyone who can
reach it can read and write).

## Docker

```sh
docker build -t open-agent-docs .
docker run -d --name oad -p 8848:8848 -v oad-data:/data \
  -e OAD_ADMIN_EMAIL=me@example.com -e OAD_ADMIN_PASSWORD='at least 8 chars' \
  open-agent-docs
```

The image sets `OAD_HOST=0.0.0.0` and `OAD_DATA_DIR=/data`, and runs as a non-root user (uid 65532). If you bind-mount your own
directory, `chown 65532:65532` it first. The image contains only the server binary, no shell. To add an account:

```sh
docker run --rm -it -v oad-data:/data open-agent-docs user add --email you@example.com --role editor
```

Before exposing it to the internet, read [Deployment](deployment.md): put it behind an HTTPS reverse proxy and set
`OAD_PUBLIC_URL` and `OAD_COOKIE_SECURE`.

## Build from source

Needs Node 22+, pnpm 8, Rust (stable), the `wasm32-unknown-unknown` target and `wasm-bindgen-cli 0.2.130`.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked

pnpm install
sh packages/db-wasm/build.sh          # database engine for the browser (WASM)
pnpm build                            # web app → apps/web/dist
cargo build --release -p oad-bin      # web app embedded → target/release/open-agent-docs
```

Development: `cargo run -p oad-bin -- serve` for the server, `pnpm dev` for the web app (Vite, proxies `/api` to 8848).
Tests: `pnpm -r test`, `cargo test`.

## Next

- Connect an agent: [Agents](agents.md)
- All settings: [Configuration](configuration.md)
