#!/usr/bin/env bash
# Populate frontend nuclear data from the nucl-parquet submodule or a GitHub release.
#
# Priority:
#   1. Local submodule at ../nucl-parquet (dev workflow)
#   2. GitHub release download (CI fallback)
#
# Usage:
#   ./scripts/fetch-data.sh                       # auto-detect
#   ./scripts/fetch-data.sh frontend-data-v1      # force release download
#
set -euo pipefail

DEST="public/data/parquet"
SUBMODULE="../nucl-parquet"

cd "$(dirname "$0")/.."

if [ -d "$DEST/xs" ] && [ "$(ls "$DEST/xs/" 2>/dev/null | wc -l)" -gt 100 ]; then
  echo "Data already present in $DEST ($(ls "$DEST/xs/" | wc -l) XS files). Skipping."
  echo "To re-download, remove $DEST first."
  exit 0
fi

mkdir -p "$DEST/meta" "$DEST/stopping" "$DEST/xs"

# Option 1: copy from local submodule
if [ -z "${1:-}" ] && [ -d "$SUBMODULE/tendl-2024/xs" ]; then
  echo "Copying nuclear data from submodule ($SUBMODULE) ..."
  cp "$SUBMODULE/meta/abundances.parquet" "$SUBMODULE/meta/decay.parquet" "$SUBMODULE/meta/elements.parquet" "$DEST/meta/"
  cp "$SUBMODULE/stopping/stopping.parquet" "$DEST/stopping/"
  cp "$SUBMODULE/tendl-2024/xs/"*.parquet "$DEST/xs/"
  echo "Done. $(find "$DEST" -name '*.parquet' | wc -l | tr -d ' ') parquet files in $DEST"
  exit 0
fi

# Option 2: download from GitHub release
TAG="${1:-frontend-data-v1}"
REPO="exoma-ch/nucl-parquet"
ARCHIVE="hyrr-frontend-data.tar.gz"

echo "Downloading nuclear data from $REPO@$TAG ..."

if command -v gh &>/dev/null; then
  gh release download "$TAG" --repo "$REPO" --pattern "$ARCHIVE" --dir /tmp --clobber
else
  curl -sL "https://github.com/$REPO/releases/download/$TAG/$ARCHIVE" -o "/tmp/$ARCHIVE"
fi

echo "Extracting to $DEST ..."
tar xzf "/tmp/$ARCHIVE" -C "$DEST"
rm -f "/tmp/$ARCHIVE"

echo "Done. $(find "$DEST" -name '*.parquet' | wc -l | tr -d ' ') parquet files in $DEST"
