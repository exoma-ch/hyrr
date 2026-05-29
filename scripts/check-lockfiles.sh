#!/usr/bin/env bash
# check-lockfiles.sh — verify lockfiles are in sync with their manifests.
#
# Checks:
#   1. uv.lock matches pyproject.toml (uv lock --check)
#   2. wasm/Cargo.lock matches wasm/Cargo.toml (cargo check --locked)
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

# --- wasm/Cargo.lock ---
if [ -f wasm/Cargo.toml ]; then
  if command -v cargo &>/dev/null; then
    if ! cargo check --locked --manifest-path wasm/Cargo.toml 2>/dev/null; then
      echo "::error::wasm/Cargo.lock is out of sync with wasm/Cargo.toml. Run: cargo update --manifest-path wasm/Cargo.toml"
      rc=1
    fi
  else
    echo "::warning::cargo not found — skipping wasm/Cargo.lock check"
  fi
fi

if [ "$rc" -eq 0 ]; then
  echo "Lockfiles in sync."
fi

exit "$rc"
