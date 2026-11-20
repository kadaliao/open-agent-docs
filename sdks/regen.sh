#!/usr/bin/env bash
# 服务器的接口或版本号变了以后：重新生成 docs/openapi.json 和各语言 SDK 里生成的代码。
#   sdks/regen.sh           生成
#   sdks/regen.sh --check   只检查都是最新的（CI 里用），过期了退出 1
set -euo pipefail
cd "$(dirname "$0")/.."

if [ "${1:-}" = "--check" ]; then
  cargo test -q -p oad-server --test openapi
  node sdks/check-operations.mjs
  pnpm --filter @open-agent-docs/client exec node scripts/gen-types.mjs --check
  (cd sdks/python && python3 codegen/generate.py --check)
  (cd sdks/go && go test -count=1 -run TestGeneratedUpToDate ./...)
  (cd sdks/rust && cargo run -q -p codegen -- --check)
  node scripts/versions.mjs
  echo "✓ openapi.json 和各 SDK 生成的代码都是最新的"
  exit 0
fi

OAD_WRITE_OPENAPI=1 cargo test -q -p oad-server --test openapi
node sdks/check-operations.mjs
pnpm --filter @open-agent-docs/client gen:types
(cd sdks/python && python3 codegen/generate.py)
(cd sdks/go && go generate ./...)
(cd sdks/rust && cargo run -q -p codegen)
echo "✓ 重新生成好了：git diff 看看改了什么"
