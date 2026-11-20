#!/bin/sh
# 编 WASM 版 oad-markdown：在仓库根目录或这里跑都行。要 wasm32-unknown-unknown 和同版本的 wasm-bindgen-cli：
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.130 --locked
set -e
cd "$(dirname "$0")/../.."
cargo build -p oad-markdown --features wasm --target wasm32-unknown-unknown --release \
  --config 'profile.release.opt-level="s"' --config 'profile.release.strip=false'
wasm-bindgen --target web --remove-name-section --remove-producers-section \
  --out-dir packages/markdown-wasm/dist --out-name oad_markdown \
  target/wasm32-unknown-unknown/release/oad_markdown.wasm
# 只读 HTML 的样式表：唯一的来源是 crates/oad-markdown/content.css（服务器的 /embed/content.css 也是它），这里只是拷进包里
cp crates/oad-markdown/content.css packages/markdown-wasm/dist/content.css
ls -l packages/markdown-wasm/dist
