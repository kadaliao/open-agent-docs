#!/usr/bin/env bash
# 各语言 SDK 的统一场景测试（docs/sdk-contract.md §14）：编一次服务器，依次跑 Python、Go、Rust、JS 的场景测试。
#   sdks/conformance/run.sh              全部
#   sdks/conformance/run.sh python go    只跑这几个
# 已经有二进制时设 OAD_BINARY 跳过编译。
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT=$PWD

node sdks/check-operations.mjs

if [ -z "${OAD_BINARY:-}" ]; then
  cargo build -q -p oad-bin
  # 复制一份：跑测试期间别人重新编也不影响
  TMP=$(mktemp -d)
  trap 'rm -rf "$TMP"' EXIT
  cp target/debug/open-agent-docs "$TMP/open-agent-docs"
  export OAD_BINARY="$TMP/open-agent-docs"
fi
echo "服务器：${OAD_BINARY}（$("$OAD_BINARY" --version)）"

langs=("$@")
[ ${#langs[@]} -eq 0 ] && langs=(python go rust js)
failed=()

for lang in "${langs[@]}"; do
  echo
  echo "━━━━━━━━ $lang ━━━━━━━━"
  case "$lang" in
    python)
      (cd sdks/python && { [ -x .venv/bin/pytest ] || { uv venv -q && uv pip install -q -e . --group dev; }; } \
        && .venv/bin/pytest -q tests/conformance) || failed+=("$lang") ;;
    go)
      (cd sdks/go && go test -count=1 -run Conformance ./...) || failed+=("$lang") ;;
    rust)
      (cd sdks/rust && cargo test -q --all-features --test conformance) || failed+=("$lang") ;;
    js)
      (cd "$ROOT" && pnpm --filter @open-agent-docs/client exec vitest run test/conformance.test.ts) || failed+=("$lang") ;;
    *)
      echo "不认识的语言：$lang（python / go / rust / js）"; exit 2 ;;
  esac
done

echo
if [ ${#failed[@]} -gt 0 ]; then
  echo "✗ 没过：${failed[*]}"
  exit 1
fi
echo "✓ 全部通过：${langs[*]}"
