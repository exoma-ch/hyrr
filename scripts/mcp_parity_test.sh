#!/usr/bin/env bash
# Verify `hyrr --mcp` and `hyrr-mcp` produce byte-identical stdio responses.
#
# Both entry points call hyrr_core::mcp::transport::run_mcp_server, so parity
# is expected. This test guards against future drift (e.g. if someone adds a
# feature flag that diverges the codepath).
#
# Usage:
#   scripts/mcp_parity_test.sh
#
# Env:
#   HYRR_DATA              Required. Path to a nucl-parquet data directory.
#   HYRR_DESKTOP_BIN       Override for the desktop binary path.
#   HYRR_MCP_BIN           Override for the standalone hyrr-mcp binary path.
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
trap 'rm -f "$DESKTOP_OUT" "$MCP_OUT"' EXIT

# Strip server.version from initialize responses — desktop uses its own
# CARGO_PKG_VERSION, hyrr-mcp uses its own, and that differs even though
# every other byte matches. Filter that one field to a sentinel for the diff.
normalize() {
  sed 's/"version":"[^"]*"/"version":"<normalized>"/g'
}

cat "$FIXTURE" | "$HYRR_DESKTOP_BIN" --mcp 2>/dev/null | normalize > "$DESKTOP_OUT"
cat "$FIXTURE" | "$HYRR_MCP_BIN"            2>/dev/null | normalize > "$MCP_OUT"

if diff -u "$DESKTOP_OUT" "$MCP_OUT"; then
  echo "ok — hyrr --mcp and hyrr-mcp produce identical output for $(wc -l < "$FIXTURE" | tr -d ' ') requests"
else
  echo "fail — outputs diverged (see diff above)" >&2
  exit 1
fi
