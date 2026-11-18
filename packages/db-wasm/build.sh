#!/bin/sh
# 编 WASM 版 oad-db：在仓库根目录或这里跑都行。要 wasm32-unknown-unknown 和同版本的 wasm-bindgen-cli：
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.130 --locked
# 计算密集，用 opt-level 3（比 "s" 大一点，查询快不少）。
set -e
cd "$(dirname "$0")/../.."
cargo build -p oad-db --features wasm --target wasm32-unknown-unknown --release \
  --config 'profile.release.opt-level=3' --config 'profile.release.strip=false'
wasm-bindgen --target web --remove-name-section --remove-producers-section \
  --out-dir packages/db-wasm/dist --out-name oad_db \
  target/wasm32-unknown-unknown/release/oad_db.wasm
ls -l packages/db-wasm/dist
