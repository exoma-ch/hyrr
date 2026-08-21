#!/usr/bin/env bash
# Guard: the TS and Rust MATERIAL_CATALOGs agree.
#
# Why this is load-bearing (#652, epic #649). A layer whose `material` is a
# catalog key is sent to Rust *by name* — `expandCustomMaterials`
# (frontend/src/lib/compute/backend.ts) only expands user-defined custom
# materials, not catalog entries. If Rust's catalog lacks the key it cannot
# parse it as a formula either, and because the layer carries a catalog
# `density_override`, `resolve_material` (core/src/materials.rs) takes the
# density branch and returns **Ok with an empty element list** instead of an
# error. The result is a zero-mass layer that produces nothing, silently.
#
# That is exactly what happened: `o18-gas`, `xe124-gas` and `sr86-carbonate`
# shipped in packages/compute/src/materials.ts (#68/#106) and were never added
# to core/src/materials.rs. Probing Rust directly:
#
#     havar            OK  elements=8 density=8.3
#     o18-gas          OK  elements=0 density=1     <-- silent
#     xe124-gas        OK  elements=0 density=1     <-- silent
#     sr86-carbonate   OK  elements=0 density=1     <-- silent
#
# This lives in scripts/tests rather than core/tests because a Rust test that
# reads repo-root files is not provisioned inside the hermetic nix sandbox
# (#589).
#
#   scripts/tests/test_material_catalog_parity.sh   (also: just test-scripts)

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TS="$ROOT/packages/compute/src/materials.ts"
RS="$ROOT/core/src/materials.rs"

failed=0
report() {
  if [ "$2" = "pass" ]; then echo "PASS: $1"; else echo "FAIL: $1" >&2; failed=1; fi
}

for f in "$TS" "$RS"; do
  [ -f "$f" ] || { echo "FAIL: missing $f" >&2; exit 1; }
done

# TS keys: entries of the MATERIAL_CATALOG object literal, quoted or bare.
ts_keys="$(awk '/export const MATERIAL_CATALOG/,/^};/' "$TS" \
  | grep -oE '^  "?[A-Za-z0-9_-]+"?: \{' \
  | sed -E 's/^  "?([A-Za-z0-9_-]+)"?: \{/\1/' \
  | sort -u)"

# Rust keys: the m.insert("<key>", CatalogEntry { ... }) calls in the
# MATERIAL_CATALOG LazyLock initialiser.
rs_keys="$(awk '/pub static MATERIAL_CATALOG/,/^\}\);/' "$RS" \
  | grep -oE '^\s*"[A-Za-z0-9_-]+",' \
  | tr -d ' ",' \
  | sort -u)"

if [ -z "$ts_keys" ]; then
  report "extracted TS catalog keys" "fail"
  echo "  parsed zero keys from $TS — the literal's shape changed, fix this script" >&2
elif [ -z "$rs_keys" ]; then
  report "extracted Rust catalog keys" "fail"
  echo "  parsed zero keys from $RS — the initialiser's shape changed, fix this script" >&2
else
  report "extracted catalog keys from both sources" "pass"

  only_ts="$(comm -23 <(echo "$ts_keys") <(echo "$rs_keys") | tr '\n' ' ' | sed 's/ *$//')"
  only_rs="$(comm -13 <(echo "$ts_keys") <(echo "$rs_keys") | tr '\n' ' ' | sed 's/ *$//')"

  if [ -n "$only_ts" ]; then
    report "every TS catalog entry exists in Rust" "fail"
    echo "  in TS but NOT in Rust: $only_ts" >&2
    echo "  these resolve to an empty, zero-mass layer with NO error — add them to" >&2
    echo "  MATERIAL_CATALOG in core/src/materials.rs." >&2
  else
    report "every TS catalog entry exists in Rust ($(echo "$ts_keys" | wc -l | tr -d ' ') checked)" "pass"
  fi

  if [ -n "$only_rs" ]; then
    report "every Rust catalog entry exists in TS" "fail"
    echo "  in Rust but NOT in TS: $only_rs" >&2
    echo "  the browser will never offer these; add them to packages/compute/src/materials.ts." >&2
  else
    report "every Rust catalog entry exists in TS" "pass"
  fi
fi

# There must be exactly one live TS catalog. A second copy is a live hazard:
# importing the wrong module ships material names Rust cannot resolve.
dupes="$(grep -rl "export const MATERIAL_CATALOG" "$ROOT/packages" "$ROOT/frontend/src" 2>/dev/null \
  | grep -v node_modules | sort)"
n_dupes="$(echo "$dupes" | grep -c . )"
if [ "$n_dupes" -gt 1 ]; then
  report "exactly one TS MATERIAL_CATALOG definition" "fail"
  echo "  found $n_dupes:" >&2
  while IFS= read -r dupe; do echo "    $dupe" >&2; done <<< "$dupes"
  echo "  a second copy diverges silently — see epic #649." >&2
else
  report "exactly one TS MATERIAL_CATALOG definition" "pass"
fi

echo
if [ "$failed" -eq 0 ]; then echo "All catalog parity checks passed."; else echo "Catalog parity FAILED." >&2; fi
exit "$failed"
