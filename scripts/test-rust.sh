#!/usr/bin/env bash
# Run the Rust core suite the way CI runs it — all three feature sets.
#
# Why this exists (#649 follow-up): `nix flake check` builds `hyrr-core` three
# times, under **different feature sets**, and until now there was no local
# command that did the same. The natural local loop is
#
#     cargo test --manifest-path core/Cargo.toml
#
# which compiles only the *default* set. Everything gated on `mcp` or
# `embed-data` is then invisible: it is not "failing", it is not compiled.
#
# That is not hypothetical. Adding the `diagnostics` field to `StackResult`
# (#650) broke five initializers under `core/src/mcp/`, a full green local run
# said nothing, and it was `nix-check` on the PR that caught it. `nix flake
# check` is hermetic and slow enough that people reach for plain `cargo test`
# — so plain `cargo test` had better be the wrong habit to have, or there had
# better be a fast command that is right. This is that command.
#
#   scripts/test-rust.sh            # all three feature sets
#   scripts/test-rust.sh default    # just one (default | mcp | embed)
#   just test-rust
#
# The `embed` target list is parsed out of flake.nix rather than duplicated
# here: `scripts/tests/test_feature_gated_tests_run.sh` already asserts that
# list covers every embed-data-gated file, so deriving from it means one source
# of truth instead of a second list to forget.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/core/Cargo.toml"
FLAKE="$ROOT/flake.nix"

WHICH="${1:-all}"

# `rust-test` and `rust-test-mcp` set HYRR_DATA to the pinned nuclData; locally
# the submodule is the equivalent. Leave an existing HYRR_DATA alone.
if [ -z "${HYRR_DATA:-}" ] && [ -d "$ROOT/nucl-parquet/data" ]; then
  export HYRR_DATA="$ROOT/nucl-parquet/data"
fi

if [ -n "${HYRR_DATA:-}" ]; then
  echo "HYRR_DATA=$HYRR_DATA"
else
  echo "warning: no HYRR_DATA and no nucl-parquet/data submodule — data-backed tests will skip" >&2
fi

failed=0
run() {
  local name="$1"; shift
  echo
  echo "══ $name ══════════════════════════════════════════════════════════"
  if "$@"; then
    echo "── $name: PASS"
  else
    echo "── $name: FAIL" >&2
    failed=1
  fi
}

# ── default feature set (flake: rust-test) ──────────────────────────────────
# `live_*` tests need the network and are excluded in the sandbox too.
if [ "$WHICH" = "all" ] || [ "$WHICH" = "default" ]; then
  run "default features" \
    cargo test --manifest-path "$MANIFEST" \
    -- --include-ignored --test-threads=1 --skip live_
fi

# ── mcp (flake: rust-test-mcp) ──────────────────────────────────────────────
# Deliberately no --test filter, matching the flake: naming targets here would
# silently drop the mcp_*.rs binaries, which is the #652 trap.
if [ "$WHICH" = "all" ] || [ "$WHICH" = "mcp" ]; then
  run "--features mcp" \
    cargo test --manifest-path "$MANIFEST" --features mcp \
    -- --test-threads=1
fi

# ── embed-data (flake: rust-test-embed) ─────────────────────────────────────
if [ "$WHICH" = "all" ] || [ "$WHICH" = "embed" ]; then
  # Pull the `--test <name>` allow-list straight out of the derivation.
  # One token per line — `--test` and the name must stay separate argv
  # entries, so quoting `"${embed_targets[@]}"` below is correct.
  mapfile -t embed_targets < <(
    awk '/rust-test-embed = /,/\}\);/' "$FLAKE" \
      | grep -o '"--test [a-z0-9_]*"' \
      | tr -d '"' \
      | tr ' ' '\n'
  )
  n_targets=$(( ${#embed_targets[@]} / 2 ))
  if [ "$n_targets" -eq 0 ]; then
    echo "FAIL: could not parse embed-data test targets from $FLAKE" >&2
    echo "  (did rust-test-embed's cargoTestExtraArgs change shape?)" >&2
    failed=1
  else
    run "--features embed-data ($n_targets targets)" \
      cargo test --manifest-path "$MANIFEST" --features embed-data \
      "${embed_targets[@]}"
  fi
fi

echo
if [ "$failed" -eq 0 ]; then
  echo "Rust suite: all requested feature sets passed."
else
  echo "Rust suite: FAILURES above." >&2
fi
exit "$failed"
