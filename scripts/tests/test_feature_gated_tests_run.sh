#!/usr/bin/env bash
# Guard: every feature-gated integration test in core/tests/ actually runs in CI.
#
# The trap this exists for (#652, epic #649): four files under core/tests/ are
# gated on `embed-data`, which is not in core's default feature set. The flake's
# `rust-test` derivation therefore compiled them to nothing — and
# `rust-test-embed` passed `--test embedded_data_store`, which filtered out the
# rest. Ten tests across four files were green by vacuum:
#
#   f18_production.rs    (1)  the classic PET F-18 production regression
#   nb_beam_stopper.rs   (1)  the #527 beam-stopper regression
#   compound_stopping.rs (2)  NIST compound stopping / #292
#   material_registry.rs (6)  density-fallback removal
#
# `cargo test --test compound_stopping` on the default feature set prints
# "running 0 tests" and exits 0 — which is why nobody noticed.
#
# A `--test` filter on a feature-gated derivation is an allow-list, and an
# allow-list nobody maintains silently drops tests. This asserts the allow-list
# is complete.
#
#   scripts/tests/test_feature_gated_tests_run.sh   (also: just test-scripts)

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FLAKE="$ROOT/flake.nix"
TESTS_DIR="$ROOT/core/tests"

failed=0
ran=0
report() {
  local name="$1" status="$2"
  ran=$((ran + 1))
  if [ "$status" = "pass" ]; then
    echo "PASS: $name"
  else
    echo "FAIL: $name" >&2
    failed=1
  fi
}

[ -f "$FLAKE" ] || { echo "FAIL: flake.nix not found at $FLAKE" >&2; exit 1; }
[ -d "$TESTS_DIR" ] || { echo "FAIL: core/tests not found at $TESTS_DIR" >&2; exit 1; }

# ── 1. Every embed-data-gated test target is named in rust-test-embed ────────
#
# Extract the cargoTestExtraArgs line(s) from the rust-test-embed derivation.
embed_args="$(awk '/rust-test-embed = /,/\}\);/' "$FLAKE" \
  | tr '\n' ' ' \
  | grep -o 'cargoTestExtraArgs *=[^;]*;' || true)"

if [ -z "$embed_args" ]; then
  # No filter at all means every embed-data test runs — that is also correct.
  echo "note: rust-test-embed has no cargoTestExtraArgs filter; all embed-data tests run"
  embed_args="__NO_FILTER__"
fi

missing=""
gated_count=0
for f in "$TESTS_DIR"/*.rs; do
  [ -f "$f" ] || continue
  grep -q 'cfg(feature *= *"embed-data")' "$f" || continue
  gated_count=$((gated_count + 1))
  stem="$(basename "$f" .rs)"
  if [ "$embed_args" = "__NO_FILTER__" ]; then
    continue
  fi
  case "$embed_args" in
    *"--test $stem"*) ;;
    *) missing="$missing $stem" ;;
  esac
done

if [ "$gated_count" -eq 0 ]; then
  report "found embed-data-gated tests to check" "fail"
  echo "  no core/tests/*.rs is gated on embed-data — did the feature get renamed?" >&2
elif [ -n "$missing" ]; then
  report "every embed-data test target is named in rust-test-embed" "fail"
  echo "  these are gated on embed-data but never run in CI:$missing" >&2
  echo "  add '--test <name>' to rust-test-embed's cargoTestExtraArgs in flake.nix" >&2
else
  report "every embed-data test target is named in rust-test-embed ($gated_count checked)" "pass"
fi

# ── 2. rust-test-mcp must NOT carry a --test allow-list ──────────────────────
#
# The mcp derivation deliberately runs the whole suite under `--features mcp`
# rather than naming targets, so mcp_*.rs files cannot be dropped the same way.
# If someone adds a --test filter there, this class of bug returns.
mcp_args="$(awk '/rust-test-mcp = /,/\}\);/' "$FLAKE" \
  | tr '\n' ' ' \
  | grep -o 'cargoTestExtraArgs *=[^;]*;' || true)"

# Match a real target filter (`--test <name>`), not `--test-threads`, which is a
# libtest runner flag and appears in this derivation legitimately.
case "$mcp_args" in
  *"--test "*)
    report "rust-test-mcp has no --test allow-list" "fail"
    echo "  rust-test-mcp gained a --test filter: $mcp_args" >&2
    echo "  that silently drops mcp_*.rs targets not named in it (see the embed-data trap above)." >&2
    echo "  either drop the filter or extend this guard to check the mcp list too." >&2
    ;;
  *)
    report "rust-test-mcp has no --test allow-list" "pass"
    ;;
esac

echo
if [ "$failed" -eq 0 ]; then
  echo "All $ran check(s) passed."
else
  echo "Some checks FAILED." >&2
fi
exit "$failed"
