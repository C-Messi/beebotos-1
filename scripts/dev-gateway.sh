#!/bin/zsh
set -euo pipefail

cd "$(dirname "$0")/.."

set -a
source .env
set +a

export BEE_MODELS__DEFAULT_PROVIDER="${BEE_MODELS__DEFAULT_PROVIDER:-${BEE__MODELS__DEFAULT_PROVIDER:-deepseek}}"
export KIMI_API_KEY="${KIMI_API_KEY:-${BEE__MODELS__KIMI__API_KEY:-}}"
export ZHIPU_API_KEY="${ZHIPU_API_KEY:-${BEE__MODELS__ZHIPU__API_KEY:-}}"
export DEEPSEEK_API_KEY="${DEEPSEEK_API_KEY:-${BEE__MODELS__DEEPSEEK__API_KEY:-}}"
export DOUBAO_API_KEY="${DOUBAO_API_KEY:-${BEE__MODELS__DOUBAO__API_KEY:-${ARK_API_KEY:-}}}"
export IMAGE_GENERATION_BASE_URL="${IMAGE_GENERATION_BASE_URL:-https://ark.cn-beijing.volces.com/api/v3}"
export IMAGE_GENERATION_MODEL="${IMAGE_GENERATION_MODEL:-doubao-seedream-5-0-260128}"
export IMAGE_GENERATION_API_KEY="${IMAGE_GENERATION_API_KEY:-${ARK_API_KEY:-${VIDEO_GENERATION_API_KEY:-}}}"
export BEEBOTOS_SKIP_STALE_KILL=true

exec cargo run -p beebotos-gateway
