#!/usr/bin/env bash
# Build (if needed) and start Wit at http://localhost:7700
set -euo pipefail
cd "$(dirname "$0")"
(cd web && { [ -d node_modules ] || npm install; } && npm run build >/dev/null)
exec cargo run --release
