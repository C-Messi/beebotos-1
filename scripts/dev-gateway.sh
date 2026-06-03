#!/bin/zsh
set -euo pipefail

cd "$(dirname "$0")/.."

set -a
source .env
set +a

export BEE_MODELS__DEFAULT_PROVIDER="${BEE_MODELS__DEFAULT_PROVIDER:-kimi}"
export KIMI_API_KEY="${KIMI_API_KEY:-${BEE__MODELS__KIMI__API_KEY:-}}"
export ZHIPU_API_KEY="${ZHIPU_API_KEY:-${BEE__MODELS__ZHIPU__API_KEY:-}}"
export DEEPSEEK_API_KEY="${DEEPSEEK_API_KEY:-dev-placeholder}"
export BEEBOTOS_SKIP_STALE_KILL=true

exec cargo run -p beebotos-gateway
