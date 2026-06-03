#!/bin/zsh
set -euo pipefail

cd "$(dirname "$0")/../apps/web"

export TRUNK_BUILD_CARGO_PROFILE=dev
export TRUNK_BUILD_RELEASE=false
export NO_COLOR=true

exec trunk serve \
  --port 8090 \
  --cargo-profile dev \
  --proxy-backend http://127.0.0.1:8000/api \
  --proxy-rewrite /api
