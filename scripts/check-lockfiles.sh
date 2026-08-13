#!/usr/bin/env bash
# check-lockfiles.sh — verify lockfiles are in sync with their manifests.
#
# Checks:
#   1. uv.lock matches pyproject.toml (uv lock --check)
#   2. every crate's Cargo.lock matches its manifest (cargo metadata --locked)
#   3. package-lock.json records each npm workspace's real package.json version
#   4. flake.lock's nucl-parquet input == the nucl-parquet submodule gitlink
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
# There's no workspace: core, hyrr-mcp, py, py-mcp, wasm, and desktop each carry
# their own Cargo.lock, and the hermetic nix-check builds each with --locked. Verify
# them all here so a stale lock (e.g. an unsynced release version bump) is caught
# before nix-check does. `cargo metadata --locked` resolves the graph and fails on a
# stale lock without a full compile, so this stays fast.
#
# py-mcp joined this list in #573. It had no committed lockfile at all, which
# mattered more than the others: it is the crate published to PyPI as the
# `hyrr-mcp` wheel, so every release re-resolved its whole graph against
# crates.io. Its lock was seeded from core's, so the shared subgraph pins
# exactly what the other crates (and therefore CI) build against.
#
# core/hyrr-mcp/py/py-mcp/desktop path-depend on the nucl-parquet Rust client
# (default features pull parquet-store; wasm doesn't). Without the submodule those
# resolve to a missing path and `cargo metadata` fails for the wrong reason — so
# skip them unless the client is checked out. The dedicated `lockfile-sync` CI job
# checks the submodule out and runs the full set.
if command -v cargo &>/dev/null; then
  if [ -f nucl-parquet/clients/rs/nucl-parquet/Cargo.toml ]; then
    manifests=(core/Cargo.toml hyrr-mcp/Cargo.toml py/Cargo.toml py-mcp/Cargo.toml wasm/Cargo.toml desktop/src-tauri/Cargo.toml)
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

# --- package-lock.json (npm workspaces) ---
# release-please bumps frontend/package.json through `extra-files`, but nothing
# regenerated package-lock.json — so the lockfile's recorded workspace version
# silently sat a release behind (0.17.0 vs 0.18.0). The next `npm install`
# "helpfully" corrects it, dirtying the working tree mid-build and blocking a
# checkout. Compare the lockfile's version for each LOCAL workspace against its
# package.json: offline, no registry round-trip, and it targets exactly that
# drift class (a dependency-graph check would need the network and is already
# covered by `npm ci` in the build jobs).
if [ -f package-lock.json ]; then
  if command -v python3 &>/dev/null; then
    if ! python3 - <<'PY'; then rc=1; fi
import json, os, sys

lock = json.load(open("package-lock.json"))
bad = []
for path, meta in (lock.get("packages") or {}).items():
    # "" is the root project; node_modules/* are resolved deps, not workspaces.
    if not path or path.startswith("node_modules") or "version" not in meta:
        continue
    manifest = os.path.join(path, "package.json")
    if not os.path.exists(manifest):
        continue
    actual = json.load(open(manifest)).get("version")
    if actual and actual != meta["version"]:
        bad.append((path, meta["version"], actual))

for path, locked, actual in bad:
    print(
        f"::error::package-lock.json records {path} at {locked}, "
        f"but {path}/package.json says {actual}. Run: npm install --package-lock-only"
    )
sys.exit(1 if bad else 0)
PY
  else
    echo "::warning::python3 not found — skipping package-lock.json check"
  fi
fi

# --- nucl-parquet: flake input rev == submodule gitlink ---
# There are TWO pins on the same nuclear-data tree and they are updated by
# different motions, so they drift silently:
#
#   * the submodule gitlink  — what the devshell checks out, what CI's submodule
#     job tests against, and what the release data tarball is cut from
#   * flake.lock's nucl-parquet input — what the hermetic `nix flake check`
#     gates build against (#484), because a sandboxed check cannot run
#     `git submodule update`
#
# THE FAILURE THIS PREVENTS. #574 bumped the gitlink and left flake.lock alone.
# The two pins then sat 10 commits and 4,634 data files apart — data_version
# 2026.7.2 vs 2026.8.1, across a BREAKING cross-section schema change
# (nucl-parquet#280). So the hermetic gate went green against nuclear data that
# was not the data anyone shipped, which is the one thing that gate exists to
# rule out. Nothing caught it: `nix flake check` is self-consistent, and the
# submodule path is self-consistent — only comparing them finds it.
#
# Reads the INDEX (`git ls-files -s`), not HEAD, so a `git add nucl-parquet`
# with no matching flake.lock bump fails at commit time rather than in CI.
#
# To fix a reported mismatch, move the flake input to the gitlink:
#   nix flake update nucl-parquet   (after pointing flake.nix's url at the rev)
if [ -f flake.lock ] && [ -f .gitmodules ]; then
  if command -v python3 &>/dev/null; then
    gitlink="$(git ls-files -s nucl-parquet 2>/dev/null | awk '{print $2}')"
    flakerev="$(python3 -c '
import json, sys
try:
    node = json.load(open("flake.lock"))["nodes"]["nucl-parquet"]["locked"]
except (KeyError, ValueError, OSError):
    sys.exit(0)
print(node.get("rev", ""))
' 2>/dev/null)"
    if [ -n "$gitlink" ] && [ -n "$flakerev" ] && [ "$gitlink" != "$flakerev" ]; then
      echo "::error::nucl-parquet pins disagree — the hermetic nix gates would"
      echo "::error::  test different nuclear data than the devshell and release ship."
      echo "::error::    submodule gitlink : $gitlink"
      echo "::error::    flake.lock input  : $flakerev"
      echo "::error::  Bump flake.nix's nucl-parquet url to $gitlink, then:"
      echo "::error::    nix flake update nucl-parquet"
      rc=1
    fi
  else
    echo "::warning::python3 not found — skipping nucl-parquet lockstep check"
  fi
fi

if [ "$rc" -eq 0 ]; then
  echo "Lockfiles in sync."
fi

exit "$rc"
