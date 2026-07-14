#!/usr/bin/env bash
# check-lockfiles.sh — verify lockfiles are in sync with their manifests.
#
# Checks:
#   1. uv.lock matches pyproject.toml (uv lock --check)
#   2. every crate's Cargo.lock matches its manifest (cargo metadata --locked)
#
# Exits non-zero on the first mismatch with a clear fix suggestion.
# Used by prek (local git hook) and CI.

set -euo pipefail

rc=0

# --- uv.lock ---
if command -v uv &>/dev/null; then
  if ! uv lock --check 2>/dev/null; then
    echo "::error::uv.lock is out of sync with pyproject.toml. Run: uv lock"
    rc=1
  fi
else
  echo "::warning::uv not found — skipping uv.lock check"
fi

# --- Cargo.lock (every standalone crate) ---
# There's no workspace: core, hyrr-mcp, py, wasm, and desktop each carry their own
# Cargo.lock, and the hermetic nix-check builds each with --locked. Verify them all
# here so a stale lock (e.g. an unsynced release version bump) is caught before
# nix-check does. `cargo metadata --locked` resolves the graph and fails on a stale
# lock without a full compile, so this stays fast.
#
# core/hyrr-mcp/py/desktop path-depend on the nucl-parquet Rust client (default
# features pull parquet-store; wasm doesn't). Without the submodule those resolve
# to a missing path and `cargo metadata` fails for the wrong reason — so skip them
# unless the client is checked out. The dedicated `lockfile-sync` CI job checks
# the submodule out and runs the full set.
if command -v cargo &>/dev/null; then
  if [ -f nucl-parquet/clients/rs/nucl-parquet/Cargo.toml ]; then
    manifests=(core/Cargo.toml hyrr-mcp/Cargo.toml py/Cargo.toml wasm/Cargo.toml desktop/src-tauri/Cargo.toml)
  else
    manifests=(wasm/Cargo.toml)
    echo "::warning::nucl-parquet submodule not initialized — checking only wasm/Cargo.lock (init the submodule for the full set)"
  fi
  for manifest in "${manifests[@]}"; do
    [ -f "$manifest" ] || continue
    if ! cargo metadata --locked --manifest-path "$manifest" --format-version 1 >/dev/null 2>&1; then
      echo "::error::${manifest%/Cargo.toml}/Cargo.lock is out of sync with $manifest. Run: cargo update --manifest-path $manifest -p hyrr-core"
      rc=1
    fi
  done
else
  echo "::warning::cargo not found — skipping Cargo.lock checks"
fi

if [ "$rc" -eq 0 ]; then
  echo "Lockfiles in sync."
fi

exit "$rc"
