# Quick start

[简体中文](../zh/quickstart.md) · [Contents](README.md)

By default the server listens on `127.0.0.1:8848` and keeps its data in `./data`.

## System requirements

Every way of running the server locally (the release binaries, Homebrew, Python's `open-agent-docs[server]`, Node's
`@open-agent-docs/server`, and each SDK's `start()`) uses the same compiled program, so the requirements are the same:

| System | Requirement |
| --- | --- |
| macOS | 11 or later, Apple Silicon and Intel |
| Linux | x86_64 or arm64 with **glibc 2.34 or later**: Ubuntu 22.04+, Debian 12+, RHEL / Rocky / AlmaLinux 9+, Fedora 35+, Amazon Linux 2023 |
| Windows | No native build; use Docker or WSL2 (Ubuntu 22.04+) |

The first line of `ldd --version` shows your glibc version. Ubuntu 20.04, Debian 11, CentOS 7 / 8, RHEL 8 and Amazon Linux 2 ship an older glibc,
and Alpine uses musl; on those systems use [Docker](#docker) (the image brings its own runtime and runs anywhere).

This only applies to **running** the server. The SDKs themselves (the Python, Go, Rust and JS clients) talk HTTP and run anywhere.

## Homebrew

macOS and Linux (arm64 and x86_64):

```sh
brew install kadaliao/tap/open-agent-docs
# Create an admin first (you'll be asked for the password); the data directory matches the service below
open-agent-docs user add --email me@example.com --name Me --role owner --data-dir "$(brew --prefix)/var/open-agent-docs"
brew services start open-agent-docs      # runs as a service and starts at login; data lives in $(brew --prefix)/var/open-agent-docs
```

Open http://127.0.0.1:8848 and sign in. Logs go to `$(brew --prefix)/var/log/open-agent-docs.log`; upgrade with `brew upgrade open-agent-docs`.

## Binary

Download the archive for your platform from the [releases page](https://github.com/kadaliao/open-agent-docs/releases/latest)
(macOS and Linux, arm64 and x86_64), or [build it from source](#build-from-source).

```sh
tar xzf open-agent-docs-*.tar.gz
cd open-agent-docs-*/
# macOS only: the binary is not notarized, so clear the quarantine flag once
xattr -d com.apple.quarantine open-agent-docs 2>/dev/null
```

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

No source checkout needed: use the published image (`ghcr.io/kadaliao/open-agent-docs`, amd64 and arm64; `latest` is the newest release, or pin a
version such as `:0.1.0`):

```sh
docker run -d --name oad -p 8848:8848 -v oad-data:/data \
  -e OAD_ADMIN_EMAIL=me@example.com -e OAD_ADMIN_PASSWORD='at least 8 chars' \
  ghcr.io/kadaliao/open-agent-docs
```

With Docker Compose:

```sh
curl -fsSLO https://raw.githubusercontent.com/kadaliao/open-agent-docs/main/docker-compose.yml
# edit OAD_ADMIN_EMAIL in it
OAD_ADMIN_PASSWORD='at least 8 chars' docker compose up -d
```

To build the image yourself, run `docker build -t open-agent-docs .` in a source checkout.

The image sets `OAD_HOST=0.0.0.0` and `OAD_DATA_DIR=/data`, and runs as a non-root user (uid 65532). If you bind-mount your own
directory, `chown 65532:65532` it first. The image contains only the server binary, no shell. To add an account:

```sh
docker run --rm -it -v oad-data:/data ghcr.io/kadaliao/open-agent-docs user add --email you@example.com --role editor
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
