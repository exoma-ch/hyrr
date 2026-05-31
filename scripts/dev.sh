#!/usr/bin/env bash
# Local development server — builds WASM, copies data, starts vite.
#
# Usage:
#   scripts/dev.sh              # full build + dev server
#   scripts/dev.sh --skip-wasm  # skip WASM rebuild (use existing)
#   scripts/dev.sh --port 3000  # custom port
#
# Prerequisites:
#   - wasm-pack installed (cargo install wasm-pack)
#   - npm dependencies installed (cd frontend && npm install)
#   - nucl-parquet submodule checked out (git submodule update --init)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

SKIP_WASM=false
PORT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-wasm) SKIP_WASM=true; shift ;;
    --port) PORT="$2"; shift 2 ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

# 1. Submodule
if [ ! -f nucl-parquet/data/catalog.json ]; then
  echo "==> Initializing nucl-parquet submodule..."
  git submodule update --init
fi

# 2. WASM
if [ "$SKIP_WASM" = false ]; then
  echo "==> Building WASM..."
  wasm-pack build wasm/ --target web --out-dir ../frontend/src/lib/compute/hyrr-wasm-pkg
else
  echo "==> Skipping WASM build (--skip-wasm)"
  if [ ! -f frontend/src/lib/compute/hyrr-wasm-pkg/hyrr_wasm_bg.wasm ]; then
    echo "ERROR: No WASM binary found. Run without --skip-wasm first."
    exit 1
  fi
fi

# 3. Nuclear data
echo "==> Copying nuclear data..."
scripts/copy-frontend-data.sh nucl-parquet/data frontend/public/data/parquet tendl-2023-iso

# 4. NPM deps
if [ ! -d frontend/node_modules ]; then
  echo "==> Installing npm dependencies..."
  (cd frontend && npm install)
fi

# 5. Dev server
echo "==> Starting vite dev server..."
PORT_ARG=()
[ -n "$PORT" ] && PORT_ARG=(--port "$PORT")
cd frontend && exec npx vite --host 0.0.0.0 "${PORT_ARG[@]}"
