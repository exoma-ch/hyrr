#!/usr/bin/env bash
# Manual staging deploy — use when GitHub Actions runners are stuck.
#
# Builds WASM + frontend locally and pushes to gh-pages tst/ directory.
# Equivalent to what deploy-frontend.yml does in CI.
#
# Usage:
#   scripts/deploy-staging-manual.sh
#
# Prerequisites:
#   - wasm-pack installed (cargo install wasm-pack)
#   - npm dependencies installed (cd frontend && npm install)
#   - nucl-parquet submodule checked out
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "=== 1/4 Building WASM ==="
(cd wasm && wasm-pack build --target web --out-dir ../frontend/src/lib/compute/hyrr-wasm-pkg)

echo "=== 2/4 Copying nuclear data ==="
scripts/copy-frontend-data.sh nucl-parquet/data frontend/public/data/parquet tendl-2023-iso

echo "=== 3/4 Building frontend (staging base path) ==="
(cd frontend && VITE_BASE_PATH=/hyrr/tst/ npm run build)

echo "=== 4/4 Deploying to gh-pages tst/ ==="
TMPDIR="$(mktemp -d)"
git worktree add "$TMPDIR" origin/gh-pages
rm -rf "$TMPDIR/tst"
cp -r frontend/dist "$TMPDIR/tst"
(
  cd "$TMPDIR"
  git add tst
  VERSION=$(node -p "require('$REPO_ROOT/frontend/package.json').version")
  git commit -m "deploy(staging): tst/ v${VERSION} — manual" || echo "Nothing to commit"
  git push origin HEAD:gh-pages
)
git worktree remove --force "$TMPDIR"

echo "=== Done — /hyrr/tst/ will update in ~1-2 min (Pages CDN propagation) ==="
