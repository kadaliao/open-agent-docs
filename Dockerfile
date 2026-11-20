# syntax=docker/dockerfile:1.7
# Open Agent Docs 镜像：网页 + 服务器打成一个二进制，最终镜像只有它（distroless，带 CA 证书）。
#   docker build -t open-agent-docs .
#   docker run -p 8848:8848 -v oad-data:/data -e OAD_ADMIN_EMAIL=me@example.com -e OAD_ADMIN_PASSWORD=... open-agent-docs

ARG RUST_VERSION=1
ARG NODE_VERSION=24

# ---------- 1. 数据库引擎的 WASM（网页里的筛选、排序、公式和服务器同一份代码）----------
FROM rust:${RUST_VERSION}-bookworm AS wasm
ARG WASM_BINDGEN=0.2.130
RUN rustup target add wasm32-unknown-unknown
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install wasm-bindgen-cli --version ${WASM_BINDGEN} --locked
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY vendor vendor
COPY crates crates
# Cargo 工作区的成员（只用来编译检查，不进镜像）
COPY examples/rust-mount examples/rust-mount
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build -p oad-db --features wasm --target wasm32-unknown-unknown --release \
      --config 'profile.release.opt-level=3' --config 'profile.release.strip=false' \
 && wasm-bindgen --target web --remove-name-section --remove-producers-section \
      --out-dir /out/db-wasm --out-name oad_db target/wasm32-unknown-unknown/release/oad_db.wasm

# ---------- 2. 网页 ----------
FROM node:${NODE_VERSION}-bookworm-slim AS web
RUN corepack enable && corepack prepare pnpm@8.7.5 --activate
WORKDIR /src
COPY pnpm-lock.yaml pnpm-workspace.yaml package.json ./
COPY packages packages
COPY apps apps
COPY examples examples
COPY scripts scripts
COPY --from=wasm /out/db-wasm packages/db-wasm/dist
RUN --mount=type=cache,target=/root/.local/share/pnpm/store \
    pnpm install --frozen-lockfile --ignore-scripts
RUN pnpm --filter @open-agent-docs/web build

# ---------- 3. 服务器（把网页打进去）----------
FROM rust:${RUST_VERSION}-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY vendor vendor
COPY crates crates
# Cargo 工作区的成员（只用来编译检查，不进镜像）
COPY examples/rust-mount examples/rust-mount
COPY --from=web /src/apps/web/dist apps/web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p oad-bin \
 && cp target/release/open-agent-docs /open-agent-docs \
 && mkdir -p /data

# ---------- 4. 运行 ----------
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=server /open-agent-docs /usr/local/bin/open-agent-docs
COPY --from=server --chown=65532:65532 /data /data
ENV OAD_HOST=0.0.0.0 \
    OAD_PORT=8848 \
    OAD_DATA_DIR=/data
VOLUME /data
EXPOSE 8848
USER nonroot
ENTRYPOINT ["/usr/local/bin/open-agent-docs"]
CMD ["serve"]
