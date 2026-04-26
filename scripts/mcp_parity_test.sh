#!/usr/bin/env bash
# Verify the three MCP entry points produce byte-identical stdio responses
# on a fixture that exercises every tool in the surface.
#
# Entry points compared:
#   1. `hyrr --mcp`           — desktop binary in MCP mode
#   2. `hyrr-mcp`             — standalone Rust binary
#   3. `uvx hyrr-mcp` /
#      `python -m hyrr_mcp`   — PyO3 wheel (only if HYRR_MCP_PYTHON is set)
#
# All three dispatch through hyrr_core::mcp::transport::run_mcp_server, so
# parity is structurally guaranteed today. The test exists to catch future
# drift if anyone adds an entry-point-specific shim.
#
# Usage:
#   scripts/mcp_parity_test.sh
#
# Env:
#   HYRR_DATA              Required. Path to a nucl-parquet data directory.
#   HYRR_DESKTOP_BIN       Override for the desktop binary path.
#   HYRR_MCP_BIN           Override for the standalone hyrr-mcp binary path.
#   HYRR_MCP_PYTHON        Path to a Python that has hyrr_mcp installed
#                          (e.g. `py-mcp/.venv/bin/python`). When set, the
#                          `python -m hyrr_mcp` entry point is added to the
#                          parity comparison.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

HYRR_DESKTOP_BIN="${HYRR_DESKTOP_BIN:-$REPO_ROOT/desktop/src-tauri/target/debug/hyrr-desktop}"
HYRR_MCP_BIN="${HYRR_MCP_BIN:-$REPO_ROOT/hyrr-mcp/target/release/hyrr-mcp}"

if [ -z "${HYRR_DATA:-}" ]; then
  echo "error: HYRR_DATA must point to a nucl-parquet directory" >&2
  exit 2
fi
for bin in "$HYRR_DESKTOP_BIN" "$HYRR_MCP_BIN"; do
  if [ ! -x "$bin" ]; then
    echo "error: missing binary: $bin" >&2
    echo "build with: (cd hyrr-mcp && cargo build --release) && (cd desktop/src-tauri && cargo build)" >&2
    exit 2
  fi
done

FIXTURE="$REPO_ROOT/scripts/mcp_parity_fixture.jsonl"
if [ ! -f "$FIXTURE" ]; then
  echo "error: fixture not found: $FIXTURE" >&2
  exit 2
fi

DESKTOP_OUT="$(mktemp)"
MCP_OUT="$(mktemp)"
PY_OUT="$(mktemp)"
trap 'rm -f "$DESKTOP_OUT" "$MCP_OUT" "$PY_OUT"' EXIT

# Strip server.version from initialize responses — every entry point has
# its own CARGO_PKG_VERSION, and that legitimately differs even though
# every other byte should match. Filter to a sentinel for the diff.
normalize() {
  sed 's/"version":"[^"]*"/"version":"<normalized>"/g'
}

run_through() {
  # Stream stdin through the entry point, normalize the version field, and
  # write to the given output file. Stderr is suppressed.
  local out="$1"
  shift
  cat "$FIXTURE" | "$@" 2>/dev/null | normalize > "$out"
}

run_through "$DESKTOP_OUT" "$HYRR_DESKTOP_BIN" --mcp
run_through "$MCP_OUT"     "$HYRR_MCP_BIN"

fail=0
if ! diff -u --label "hyrr --mcp" --label "hyrr-mcp" "$DESKTOP_OUT" "$MCP_OUT"; then
  echo "fail — hyrr --mcp vs hyrr-mcp diverged (see diff above)" >&2
  fail=1
fi

if [ -n "${HYRR_MCP_PYTHON:-}" ]; then
  if [ ! -x "$HYRR_MCP_PYTHON" ]; then
    echo "error: HYRR_MCP_PYTHON is set but $HYRR_MCP_PYTHON is not executable" >&2
    exit 2
  fi
  run_through "$PY_OUT" "$HYRR_MCP_PYTHON" -m hyrr_mcp
  if ! diff -u --label "hyrr-mcp" --label "python -m hyrr_mcp" "$MCP_OUT" "$PY_OUT"; then
    echo "fail — hyrr-mcp vs python -m hyrr_mcp diverged (see diff above)" >&2
    fail=1
  fi
fi

n_requests=$(wc -l < "$FIXTURE" | tr -d ' ')
if [ "$fail" -eq 0 ]; then
  if [ -n "${HYRR_MCP_PYTHON:-}" ]; then
    echo "ok — all 3 entry points produce identical output for $n_requests requests"
  else
    echo "ok — hyrr --mcp and hyrr-mcp produce identical output for $n_requests requests (set HYRR_MCP_PYTHON to also test the wheel)"
  fi
else
  exit 1
fi
