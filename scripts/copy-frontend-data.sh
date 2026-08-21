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
# With no library arguments the list is read from
# scripts/frontend-data-libraries.txt — the SSoT. Callers should almost always
# omit them: the previous default of "tendl-2023-iso" is why `dev.sh` and
# `just data` shipped one library while production and CI shipped three, which
# made the four neutron presets silently produce nothing in dev builds (#651).
#
# Libraries are specified as "catalog-name" (→ $DEST/xs/) or
# "catalog-name:subdir" (→ $DEST/subdir/) for non-default output paths.
#
# On success writes $DEST/MANIFEST.json describing what was actually populated,
# so the build and the running app can assert the shape rather than discovering
# a missing subdirectory as an empty results table.
set -euo pipefail

NP="${1:?Usage: $0 <nucl-parquet-data-dir> <frontend-public-dir> [library...]}"
DEST="${2:?Usage: $0 <nucl-parquet-data-dir> <frontend-public-dir> [library...]}"
shift 2

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB_LIST="$HERE/frontend-data-libraries.txt"

if [ "$#" -gt 0 ]; then
  LIBRARIES=("$@")
else
  [ -f "$LIB_LIST" ] || { echo "copy-frontend-data: missing $LIB_LIST" >&2; exit 1; }
  LIBRARIES=()
  while IFS= read -r line; do
    line="${line%%#*}"                     # strip comments
    line="$(echo "$line" | tr -d '[:space:]')"
    [ -n "$line" ] && LIBRARIES+=("$line")
  done < "$LIB_LIST"
  [ "${#LIBRARIES[@]}" -gt 0 ] || { echo "copy-frontend-data: no libraries in $LIB_LIST" >&2; exit 1; }
fi

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
# CatIMA stopping is federated upstream into ~399 per-beam-isotope shards
# (nucl-parquet #252/#254). The frontend only needs the beams HYRR offers, so
# copy just those rather than shipping ~58 MB of unused shards.
#   - He3: the active ³He ("h") light-ion beam now uses per-isotope CatIMA
#     instead of ASTAR×4/3 velocity-scaling (#194).
#   - C12…Fe56: the heavy-ion beams (currently UI-gated on #266, native-only).
# Keep in sync with core/src/stopping.rs BUNDLED_CATIMA_PROJECTILES.
for iso in He3 C12 O16 Ne20 Si28 Ar40 Fe56; do
  [ -f "$NP/stopping/catima_${iso}.parquet" ] && cp "$NP/stopping/catima_${iso}.parquet" "$DEST/stopping/"
done
# NIST compound stopping (PSTAR/ASTAR compounds)
cp "$NP/stopping/compounds/"*.parquet "$DEST/stopping/compounds/" 2>/dev/null || true

# ── Cross-section libraries ───────────────────────────────────────
# Format: "name" → xs/, or "name:subdir" → subdir/
#
# A requested library that isn't on disk is a HARD ERROR. It used to be skipped
# silently (`if [ -d ... ]`), so a sparse checkout missing `endfb-8.0` produced
# a bundle with no `neutron-xs/` and no complaint — the browser then 404'd every
# neutron cross-section and rendered an empty table (#651, epic #649).
manifest_entries=()
for spec in "${LIBRARIES[@]}"; do
  lib="${spec%%:*}"
  # Detect the separator explicitly. The old test was `[ "$subdir" = "$lib" ]
  # && subdir=xs`, which is also true for a `name:name` spec — so
  # `hi-xs-prod:hi-xs-prod` silently landed in `xs/` next to tendl's files
  # instead of its own `hi-xs-prod/` directory, in every build including
  # production (#651).
  if [[ "$spec" == *:* ]]; then
    subdir="${spec#*:}"
  else
    subdir="xs"
  fi

  if [ ! -d "$NP/$lib/xs" ]; then
    echo "copy-frontend-data: ERROR: requested library '$lib' has no xs/ dir at $NP/$lib/xs" >&2
    echo "  Either widen the nucl-parquet sparse-checkout to include data/$lib/xs," >&2
    echo "  or remove '$spec' from scripts/frontend-data-libraries.txt." >&2
    exit 1
  fi

  mkdir -p "$DEST/$subdir"
  cp "$NP/$lib/xs/"*.parquet "$DEST/$subdir/"
  count="$(find "$DEST/$subdir" -maxdepth 1 -name '*.parquet' | wc -l | tr -d ' ')"
  manifest_entries+=("{\"library\":\"$lib\",\"subdir\":\"$subdir\",\"files\":$count}")
done

# ── Shape descriptor ──────────────────────────────────────────────
# Not a checksum (data-version pinning lives in hyrr.json, #577/#645) — just
# "what got populated", so the build can assert the bundle has what the shipped
# presets need and the app can say so at startup instead of rendering nothing.
{
  printf '{\n  "generated_by": "scripts/copy-frontend-data.sh",\n  "libraries": [\n'
  for i in "${!manifest_entries[@]}"; do
    sep=","
    [ "$i" -eq $(( ${#manifest_entries[@]} - 1 )) ] && sep=""
    printf '    %s%s\n' "${manifest_entries[$i]}" "$sep"
  done
  printf '  ]\n}\n'
} > "$DEST/MANIFEST.json"

echo "copy-frontend-data: done → $DEST (${LIBRARIES[*]})"
echo "copy-frontend-data: wrote $DEST/MANIFEST.json"
