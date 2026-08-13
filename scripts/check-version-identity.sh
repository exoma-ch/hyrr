#!/usr/bin/env bash
# check-version-identity.sh — one version identity across the MCP surface (#603).
#
# ADR 0001 made hyrr-core the single source of truth for MCP tool logic, with
# thin entry points that never reimplement. This enforces the same rule for the
# version string, which was the one thing left outside it.
#
# THE FAILURE THIS PREVENTS (#599). `py-mcp/Cargo.toml` sits at 0.1.0 and is
# deliberately not bumped by release-please — that is fine, because maturin
# takes the wheel version from `py-mcp/pyproject.toml`. It stopped being fine
# the moment `py-mcp/src/lib.rs` did `env!("CARGO_PKG_VERSION")`, which reads
# that inert 0.1.0 and exported it as `hyrr_mcp.__version__`. The published
# wheel then told users 0.1.0 for four releases while every protocol surface
# correctly said 0.19.0. `.github/workflows/release-hyrr-mcp.yml` even asserted
# in a comment that "nothing reads it" — false since the crate's first commit.
#
# So: "inert" is only safe if nothing reads it. This makes that an invariant
# instead of a hope.
#
# THE RULE. `env!("CARGO_PKG_VERSION")` is allowed only inside `core/`. Anywhere
# else it resolves to a shim crate's own version, which is exactly the bug.
# Entry points must use `hyrr_core::VERSION`.
#
# `concat!` inside core/ still needs the literal `env!` (a const is not a
# literal), which is why the rule is scoped by directory rather than banning
# the macro outright.
#
# Usage:
#   scripts/check-version-identity.sh [files...]   — defaults to the whole repo
#
# Run by: prek (local hook) and the `lint` CI job.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

NEEDLE='env!("CARGO_PKG_VERSION")'

# Candidate files: those passed by the hook, else every tracked .rs file.
if [ "$#" -gt 0 ]; then
  files=("$@")
else
  mapfile -t files < <(git ls-files '*.rs')
fi

offenders=()
for f in "${files[@]}"; do
  [ -f "$f" ] || continue
  case "$f" in
    *.rs) ;;
    *) continue ;;
  esac
  # Allowed: anywhere under core/ (that IS the source of truth), and the
  # vendored nucl-parquet submodule, which is a separate project.
  case "$f" in
    core/*|*/core/*|nucl-parquet/*) continue ;;
  esac
  if grep -Fq "$NEEDLE" "$f"; then
    offenders+=("$f")
  fi
done

if [ "${#offenders[@]}" -eq 0 ]; then
  exit 0
fi

echo "ERROR: ${NEEDLE} outside core/ — this reads the SHIM crate's own" >&2
echo "       version, not the release version (#599 / #603)." >&2
echo "" >&2
for f in "${offenders[@]}"; do
  grep -Fn "$NEEDLE" "$f" | sed "s|^|         ${f}:|" >&2
done
echo "" >&2
echo "       Use hyrr_core::VERSION instead. Shim crates' Cargo.toml versions" >&2
echo "       are intentionally inert (release-please does not bump them), so" >&2
echo "       reading them ships a wrong version to users." >&2
exit 1
