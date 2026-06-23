#!/usr/bin/env bash
# SSoT script for copying nucl-parquet data into the frontend's public dir.
#
# Every workflow that builds the frontend (deploy-frontend.yml,
# tauri-build.yml, e2e.yml, promote-to-prod.yml) calls this instead
# of maintaining its own cp block. The data manifest lives HERE — if
# HYRR starts consuming a new view, add it once and every build picks
# it up.
#
# Usage:
#   scripts/copy-frontend-data.sh <nucl-parquet-data-dir> <frontend-public-dir> [library...]
#
# Libraries are specified as "catalog-name" (→ $DEST/xs/) or
# "catalog-name:subdir" (→ $DEST/subdir/) for non-default output paths.
#
# Example:
#   scripts/copy-frontend-data.sh nucl-parquet/data frontend/public/data/parquet \
#     tendl-2023-iso hi-xs-prod:hi-xs-prod endfb-8.1:neutron-xs
set -euo pipefail

NP="${1:?Usage: $0 <nucl-parquet-data-dir> <frontend-public-dir> [library...]}"
DEST="${2:?Usage: $0 <nucl-parquet-data-dir> <frontend-public-dir> [library...]}"
shift 2
LIBRARIES=("${@:-tendl-2023-iso}")

# ── Meta (shared across all libraries) ────────────────────────────
mkdir -p "$DEST/meta/ensdf/emissions"
for f in abundances decay elements dose_constants spectrum_xs compound_compositions; do
  [ -f "$NP/meta/${f}.parquet" ] && cp "$NP/meta/${f}.parquet" "$DEST/meta/"
done

# Per-element emission parquets (γ/α/β/CE/X-ray/Auger)
if [ -d "$NP/meta/ensdf/emissions" ]; then
  cp "$NP/meta/ensdf/emissions/"*.parquet "$DEST/meta/ensdf/emissions/" 2>/dev/null || true
fi

# ── Stopping ──────────────────────────────────────────────────────
mkdir -p "$DEST/stopping/compounds"
# NIST elemental sources (PSTAR/ASTAR/dSTAR/tSTAR …) — every non-catima shard.
for f in "$NP/stopping/"*.parquet; do
  [ -e "$f" ] || continue
  case "$(basename "$f")" in
    catima_*) : ;;  # catima handled selectively below
    *) cp "$f" "$DEST/stopping/" ;;
  esac
done
# CatIMA heavy-ion stopping is federated upstream into ~399 per-beam-isotope
# shards (nucl-parquet #252/#254). The frontend only needs the heavy-ion beams
# HYRR offers, so copy just those rather than shipping ~58 MB of unused shards.
# Keep in sync with core/src/stopping.rs BUNDLED_CATIMA_PROJECTILES.
for iso in C12 O16 Ne20 Si28 Ar40 Fe56; do
  [ -f "$NP/stopping/catima_${iso}.parquet" ] && cp "$NP/stopping/catima_${iso}.parquet" "$DEST/stopping/"
done
# NIST compound stopping (PSTAR/ASTAR compounds)
cp "$NP/stopping/compounds/"*.parquet "$DEST/stopping/compounds/" 2>/dev/null || true

# ── Cross-section libraries ───────────────────────────────────────
# Format: "name" → xs/, or "name:subdir" → subdir/
for spec in "${LIBRARIES[@]}"; do
  lib="${spec%%:*}"
  subdir="${spec#*:}"
  [ "$subdir" = "$lib" ] && subdir="xs"  # no colon → default to xs/
  if [ -d "$NP/$lib/xs" ]; then
    mkdir -p "$DEST/$subdir"
    cp "$NP/$lib/xs/"*.parquet "$DEST/$subdir/"
  fi
done

echo "copy-frontend-data: done → $DEST (${LIBRARIES[*]})"
